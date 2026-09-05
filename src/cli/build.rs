//! Compilation-driving commands: check, run, test, bench, build, domain, profile.

use super::{extract_target_arg, keyword_set, to_json_value, to_pretty_json};
use crate::driver::ForgenCompiler;
use crate::pgo::ProfileData;
use crate::project::{ProjectDiscovery, ProjectRunner};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `forgen check` — fast static verification, no binaries.
pub(crate) fn cmd_check(args: &[String]) -> bool {
    let target_opt = extract_target_arg(args, 2);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Check error: {}", e);
            std::process::exit(1);
        }
    };

    let compiler = ForgenCompiler::new("check");
    let start = Instant::now();

    let res = if layout.source_files.len() == 1 {
        compiler.check_file(&layout.source_files[0])
    } else {
        compiler.check_files(&layout.source_files)
    };

    let elapsed = start.elapsed().as_millis();

    if res.success {
        println!(
            "[Forgen check] Verified 100% OK in {}ms ({} modules, 0 errors, valid ownership & effects)",
            elapsed,
            layout.source_files.len()
        );
    } else {
        eprintln!("{}", res.diagnostics);
        std::process::exit(1);
    }
    true
}

/// `forgen run` / `forgen quick` / `forgen start`.
pub(crate) fn cmd_run(command: &str, args: &[String]) -> bool {
    let mut target_arg: Option<&str> = None;
    let mut run_args = Vec::new();
    let mut after_dash_dash = false;
    let mut skip_next = false;

    for arg in args.iter().skip(2) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if after_dash_dash {
            run_args.push(arg.clone());
        } else if arg == "--" {
            after_dash_dash = true;
        } else if arg == "-o" || arg == "--out" || arg == "--pgo" || arg == "--target" {
            skip_next = true;
        } else if arg.starts_with("-") {
            // compiler flag, e.g. --llvm, --domain, -g, --debug
        } else if target_arg.is_none() {
            target_arg = Some(arg.as_str());
        } else {
            run_args.push(arg.clone());
        }
    }
    let target_opt = target_arg.map(Path::new);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Run error: {}", e);
            std::process::exit(1);
        }
    };

    let mode = if command == "quick" || command == "start" {
        "quick"
    } else {
        "release"
    };
    let is_llvm = args.iter().any(|a| a == "--llvm");
    let target_triple = args
        .iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find(|a| a.starts_with("--target="))
                .and_then(|a| a.strip_prefix("--target=").map(|s| s.to_string()))
        });
    let debug_info = args.iter().any(|a| a == "-g" || a == "--debug");
    let compiler = ForgenCompiler::new(mode)
        .with_llvm(is_llvm)
        .with_debug(debug_info)
        .with_target(target_triple);

    // Cranelift in-memory JIT execution: zero disk artifacts, sub-millisecond launch
    if !is_llvm {
        match compiler.run_project_captured(&layout, &run_args, false) {
            Ok((stdout, stderr, code, _)) => {
                if !stdout.is_empty() {
                    print!("{}", stdout);
                }
                if !stderr.is_empty() {
                    eprint!("{}", stderr);
                }
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        return false;
    }

    // Incremental caching check: if target binary is newer than all source files, run directly
    let bin_name = layout.binary_name();
    let exe_target = if layout.source_files.len() == 1 && layout.manifest.is_none() {
        layout.entry_point.with_extension("exe")
    } else {
        layout.root.join(format!("{}.exe", bin_name))
    };

    let cache_dir = layout.root.join(".forgen_cache");
    let mut inc_cache = crate::incremental::IncrementalCache::load_from_dir(&cache_dir);
    let mut all_fresh = !inc_cache.fingerprints.is_empty();
    for sf in &layout.source_files {
        if let Ok(content) = fs::read_to_string(sf) {
            // Transitive check: a module is only fresh when its whole
            // recorded dependency closure is fresh as well.
            if !inc_cache.is_module_fresh_transitive(sf, &content) {
                all_fresh = false;
            }
        } else {
            all_fresh = false;
        }
    }

    let mut newest_source_mod = None;
    for sf in &layout.source_files {
        if let Ok(meta) = fs::metadata(sf)
            && let Ok(mod_time) = meta.modified()
        {
            newest_source_mod = Some(
                newest_source_mod
                    .map_or(mod_time, |curr: std::time::SystemTime| curr.max(mod_time)),
            );
        }
    }
    if let Ok(meta) = fs::metadata(layout.root.join("datara.toml"))
        && let Ok(mod_time) = meta.modified()
    {
        newest_source_mod = Some(
            newest_source_mod.map_or(mod_time, |curr: std::time::SystemTime| curr.max(mod_time)),
        );
    }

    let exe_mod = fs::metadata(&exe_target)
        .map(|m| m.modified().ok())
        .ok()
        .flatten();
    let need_recompile = is_llvm
        || !all_fresh
        || match (newest_source_mod, exe_mod) {
            (Some(s), Some(e)) => s > e,
            _ => true,
        };

    if !need_recompile && exe_target.exists() {
        // Execute cached artifact immediately
        if let Ok((stdout, stderr, code, _)) =
            compiler.codegen.run_executable(&exe_target, &run_args)
        {
            print!("{}", stdout);
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            if code != 0 {
                std::process::exit(code);
            }
            return false;
        }
    }

    match compiler.run_project(&layout, &run_args) {
        Ok((stdout, stderr, code, _)) => {
            for sf in &layout.source_files {
                if let Ok(content) = fs::read_to_string(sf) {
                    inc_cache.update_module(sf, &content, Vec::new());
                }
            }
            let _ = inc_cache.save_to_dir(&cache_dir);

            print!("{}", stdout);
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            if code != 0 {
                std::process::exit(code);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
    true
}

/// `forgen test` — run project integration tests.
pub(crate) fn cmd_test(args: &[String]) -> bool {
    let target_opt = extract_target_arg(args, 2);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Test discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let is_llvm = args.iter().any(|a| a == "--llvm");
    let target_triple = args
        .iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find(|a| a.starts_with("--target="))
                .and_then(|a| a.strip_prefix("--target=").map(|s| s.to_string()))
        });
    let debug_info = args.iter().any(|a| a == "-g" || a == "--debug");
    let compiler = ForgenCompiler::new("release")
        .with_llvm(is_llvm)
        .with_debug(debug_info)
        .with_target(target_triple);
    let rep = ProjectRunner::run_tests(&layout, &compiler);

    println!(
        "\nrunning {} test{}",
        rep.total,
        if rep.total == 1 { "" } else { "s" }
    );
    for item in &rep.results {
        if item.passed {
            println!("test {} ... ok ({}ms)", item.name, item.duration_ms);
        } else {
            println!("test {} ... FAILED ({}ms)", item.name, item.duration_ms);
            if let Some(ref err) = item.error {
                println!("  Error: {}", err);
            }
            if !item.output.is_empty() {
                println!("  Output: {}", item.output.trim());
            }
        }
    }

    let status_str = if rep.failed == 0 { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {}. {} passed; {} failed; finished in {:.2}s\n",
        status_str,
        rep.passed,
        rep.failed,
        (rep.total_duration_ms as f64) / 1000.0
    );

    if rep.failed > 0 {
        std::process::exit(1);
    }
    true
}

/// `forgen bench` — run benchmarks.
pub(crate) fn cmd_bench(args: &[String]) -> bool {
    let target_opt = extract_target_arg(args, 2);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Benchmark discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let is_llvm = args.iter().any(|a| a == "--llvm");
    let target_triple = args
        .iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find(|a| a.starts_with("--target="))
                .and_then(|a| a.strip_prefix("--target=").map(|s| s.to_string()))
        });
    let compiler = ForgenCompiler::new("release")
        .with_llvm(is_llvm)
        .with_target(target_triple);
    if let Err(e) = ProjectRunner::run_benches(&layout, &compiler) {
        eprintln!("Benchmark failed: {}", e);
        std::process::exit(1);
    }
    true
}

/// `forgen build` / `release` / `debug` / `verify` — AOT native compilation.
pub(crate) fn cmd_build(command: &str, args: &[String]) -> bool {
    let target_opt = extract_target_arg(args, 2);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Build discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let pgo_profile = args
        .iter()
        .position(|a| a == "--pgo")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let target_triple = args
        .iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find(|a| a.starts_with("--target="))
                .and_then(|a| a.strip_prefix("--target=").map(|s| s.to_string()))
        });
    let debug_info = command == "debug" || args.iter().any(|a| a == "-g" || a == "--debug");
    let mode = if args.iter().any(|a| a == "--domain") {
        "domain"
    } else if command == "build" {
        if debug_info { "debug" } else { "release" }
    } else {
        command
    };
    let is_llvm = args.iter().any(|a| a == "--llvm");
    let compiler = ForgenCompiler::new(mode)
        .with_llvm(is_llvm)
        .with_pgo(pgo_profile)
        .with_debug(debug_info)
        .with_target(target_triple);

    let start = Instant::now();
    let bin_name = layout.binary_name();
    let is_wasm_target = args
        .iter()
        .any(|a| a == "--wasm" || a == "--target=wasm32" || a == "wasm32")
        || args
            .windows(2)
            .any(|w| w[0] == "--target" && w[1] == "wasm32")
        || args
            .windows(2)
            .any(|w| (w[0] == "-o" || w[0] == "--out") && w[1].ends_with(".wasm"));
    let is_python_target = args.iter().any(|a| a == "--python");
    let output_exe = if let Some(pos) = args.iter().position(|a| a == "-o" || a == "--out") {
        if let Some(val) = args.get(pos + 1) {
            PathBuf::from(val)
        } else if is_wasm_target {
            layout.root.join(format!("{}.wasm", bin_name))
        } else if is_python_target {
            layout.root.join(format!("{}.dll", bin_name))
        } else {
            layout.root.join(format!("{}.exe", bin_name))
        }
    } else if is_wasm_target {
        layout.root.join(format!("{}.wasm", bin_name))
    } else if is_python_target {
        layout.root.join(format!("{}.dll", bin_name))
    } else {
        layout.root.join(format!("{}.exe", bin_name))
    };

    if is_wasm_target {
        let dmir_res = if layout.source_files.len() == 1 {
            compiler.compile_file_to_dmir(&layout.source_files[0])
        } else {
            compiler.compile_files_to_dmir(&layout.source_files)
        };
        match dmir_res {
            Ok(dmir_mod) => {
                match crate::codegen::wasm::WasmEmitter::emit_wasm_binary(&dmir_mod, &output_exe) {
                    Ok(wasm_path) => {
                        let elapsed = start.elapsed().as_millis();
                        println!(
                            "[Forgen WASM] WebAssembly compilation succeeded in {}ms",
                            elapsed
                        );
                        println!("[Forgen WASM] Target Architecture: wasm32-unknown-wasi");
                        println!("[Forgen WASM] Output: {}", wasm_path.display());
                        println!(
                            "[Forgen WASM] WAT:    {}",
                            wasm_path.with_extension("wat").display()
                        );
                        println!(
                            "[Forgen WASM] JS:     {}",
                            wasm_path.with_extension("js").display()
                        );
                        return false;
                    }
                    Err(e) => {
                        eprintln!("[Forgen WASM] Codegen error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("[Forgen WASM] Compilation error:\n{}", e);
                std::process::exit(1);
            }
        }
    }

    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], Some(&output_exe))
    } else {
        compiler.compile_files(&layout.source_files, Some(&output_exe))
    };
    let elapsed = start.elapsed().as_millis();

    if res.success {
        if is_llvm {
            println!("[Forgen LLVM] Ultra-optimized AOT LLVM pipeline completed.");
        }
        println!("[Forgen] Build succeeded in {}ms ({} mode)", elapsed, mode);
        println!(
            "[Forgen] Project: {} ({} source files)",
            layout.name,
            layout.source_files.len()
        );
        if let Some(exe_p) = res.exe_path.as_ref() {
            println!("[Forgen] Output:  {}", exe_p.display());

            if is_python_target {
                let py_path = exe_p.with_extension("py");
                // Paths and identifiers are embedded verbatim into
                // generated Python source; escape them so a path with
                // a quote or a foreign identifier cannot break or
                // inject into the generated module.
                let escape_py = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
                let mut py_code = format!(
                    "# Auto-generated Datara Python Bridge for {}\nimport ctypes\nimport os\n\n_dll_path = os.path.abspath(r\"{}\")\n_lib = ctypes.CDLL(_dll_path)\n\n",
                    escape_py(&bin_name),
                    escape_py(&exe_p.display().to_string())
                );

                if let Some(ref dmir) = res.dmir_module {
                    for fn_name in dmir.functions.keys() {
                        // Only valid Python identifiers are exported;
                        // everything else would produce a syntax
                        // error (or worse) in the generated module.
                        let is_ident = !fn_name.is_empty()
                            && fn_name
                                .chars()
                                .next()
                                .map(|c| c.is_ascii_alphabetic() || c == '_')
                                .unwrap_or(false)
                            && fn_name
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                            && !keyword_set().contains(fn_name.as_str());
                        if !is_ident {
                            continue;
                        }
                        py_code.push_str(&format!(
                                "def {}(*args):\n    _fn = getattr(_lib, \"{}\")\n    _fn.restype = ctypes.c_int64\n    return _fn(*args)\n\n",
                                fn_name, fn_name
                            ));
                    }
                }

                if let Err(e) = fs::write(&py_path, py_code) {
                    eprintln!("Warning: Failed to write Python wrapper: {}", e);
                } else {
                    println!(
                        "[Forgen Python FFI] Synthesized Python module: {}",
                        py_path.display()
                    );
                }
            }

            if args.iter().any(|a| a == "--ledger")
                && let Some(report) = &res.optimization_report
            {
                let ledger_path = exe_p.with_extension("ledger.json");
                let ledger_data = serde_json::json!({
                    "version": "1.0",
                    "compiler": "forgen",
                    "mode": mode,
                    "summary": {
                        "variables_promoted": report.variables_promoted,
                        "constants_folded": report.constants_folded,
                        "dead_instructions_removed": report.dead_instructions_removed,
                        "functions_inlined": report.functions_inlined,
                        "allocations_eliminated": report.allocations_eliminated,
                        "evidence_downgrades": report.evidence_downgrades,
                    },
                    "decision_trace": report.decision_trace,
                });
                if let Ok(json) = serde_json::to_string_pretty(&ledger_data)
                    && fs::write(&ledger_path, json).is_ok()
                {
                    println!("[Forgen] Ledger:  {}", ledger_path.display());
                }
            }

            if args.iter().any(|a| a == "--graph")
                && let Some(graph) = &res.semantic_graph
            {
                let graph_path = exe_p.with_extension("graph.json");
                // Round-trip through serde_json::Value so the graph's
                // HashMap fields emit with sorted (deterministic) keys.
                if let Ok(json) = serde_json::to_string_pretty(&to_json_value(graph))
                    && fs::write(&graph_path, json).is_ok()
                {
                    println!("[Forgen] Graph:   {}", graph_path.display());
                }
            }
        }
    } else {
        eprintln!(
            "{}",
            res.error.unwrap_or_else(|| "Compilation failed".into())
        );
        std::process::exit(1);
    }
    true
}

/// `forgen domain` — whole-program specialization report.
pub(crate) fn cmd_domain(args: &[String]) -> bool {
    let is_llvm = args.iter().any(|a| a == "--llvm");
    let compiler = ForgenCompiler::new("domain").with_llvm(is_llvm);

    let mut pgo_profile = None;
    let mut filter_args: Vec<String> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--pgo" && i + 1 < args.len() {
            pgo_profile = Some(PathBuf::from(&args[i + 1]));
            i += 2;
        } else if args[i] == "--json" || args[i] == "--llvm" {
            i += 1;
        } else {
            filter_args.push(args[i].clone());
            i += 1;
        }
    }

    let target_opt = filter_args.first().map(|s| Path::new(s.as_str()));
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Domain discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let compiler = compiler.with_pgo(pgo_profile.clone());
    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], None)
    } else {
        compiler.compile_files(&layout.source_files, None)
    };
    if res.success {
        if is_llvm {
            println!(
                "[Forgen LLVM] Whole-program Domain compilation with LLVM pipeline completed."
            );
        }
        let rep = res.optimization_report.unwrap_or_default();
        let t = res.timings;

        if args.iter().any(|a| a == "--json") {
            let mut json_obj = serde_json::Map::new();
            json_obj.insert("optimizationReport".into(), to_json_value(&rep));
            json_obj.insert("timings".into(), to_json_value(&t));
            json_obj.insert(
                "outputBinary".into(),
                serde_json::Value::String(
                    res.exe_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ),
            );
            if let Some(ref p) = pgo_profile {
                json_obj.insert(
                    "pgoProfile".into(),
                    serde_json::Value::String(p.to_string_lossy().to_string()),
                );
            }
            println!("{}", to_pretty_json(&json_obj));
            return false;
        }

        println!("============================================================");
        println!("             FORGEN DOMAIN SPECIALIZATION REPORT            ");
        println!("============================================================");
        println!(" Project name:               {}", layout.name);
        println!(" Modules analyzed:           {}", rep.modules_analyzed);
        println!(" Symbols analyzed:           {}", rep.symbols_analyzed);
        println!(" Reachable symbols:          {}", rep.reachable_symbols);
        println!(" Removed dead symbols:       {}", rep.removed_symbols);
        println!(
            " Generic specializations:    {:?}",
            rep.generic_specializations
        );
        println!(" Functions inlined:          {}", rep.functions_inlined);
        println!(
            " Allocations eliminated:     {}",
            rep.allocations_eliminated
        );
        println!(" Constants folded:           {}", rep.constants_folded);
        println!(
            " Dead instructions removed:  {}",
            rep.dead_instructions_removed
        );
        println!(
            " Linked runtime modules:     {:?}",
            rep.runtime_modules_linked
        );
        println!(
            " Stripped runtime modules:   {:?}",
            rep.runtime_modules_stripped
        );
        if let Some(ref p) = pgo_profile {
            println!(" PGO Profile applied:        {}", p.display());
        }
        println!("------------------------------------------------------------");
        println!(" Pipeline Timings Breakdown:");
        println!("   Discovery:   {:>4}ms", t.discovery_ms);
        println!("   Parse:       {:>4}ms", t.parse_ms);
        println!("   Resolve:     {:>4}ms", t.resolve_ms);
        println!("   TypeCheck:   {:>4}ms", t.typecheck_ms);
        println!("   Effects:     {:>4}ms", t.effects_ms);
        println!("   Ownership:   {:>4}ms", t.ownership_ms);
        println!("   Graph:       {:>4}ms", t.graph_ms);
        println!("   Optimizer:   {:>4}ms", t.optimizer_ms);
        println!("   Codegen:     {:>4}ms", t.codegen_ms);
        println!("   Link:        {:>4}ms", t.link_ms);
        println!("   Total:       {:>4}ms", t.total_ms);
        println!(
            " Output binary:              {}",
            res.exe_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        println!("============================================================");
    } else {
        eprintln!(
            "{}",
            res.error.unwrap_or_else(|| "Compilation failed".into())
        );
        std::process::exit(1);
    }
    true
}

/// `forgen profile` — run and write a static call-graph profile.
pub(crate) fn cmd_profile(args: &[String]) -> bool {
    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("-") && *a != "--")
        .map(Path::new);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Profile discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let compiler = ForgenCompiler::new("release");
    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], None)
    } else {
        compiler.compile_files(&layout.source_files, None)
    };

    if !res.success {
        eprintln!("Profile build failed: {}", res.error.unwrap_or_default());
        std::process::exit(1);
    }

    // Actually execute the program. Nothing below may be described as
    // "measured" unless it comes from this run.
    let exe = match res.exe_path.clone() {
        Some(p) => p,
        None => {
            eprintln!("Profile build produced no executable");
            std::process::exit(1);
        }
    };
    let run = compiler.codegen.run_executable(&exe, &[]);

    let mut prof = ProfileData::new(&layout.name);
    // Instrumented profiling (per-function execution counts, branch
    // taken ratios, trip counts) is NOT implemented. What we can report
    // truthfully is the compiler's own static call graph, so the profile
    // is labelled "static" and consumers must not treat the counts as
    // runtime behaviour.
    prof.source = "static".to_string();

    if let Some(module) = &res.dmir_module {
        let mut call_sites: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for f in module.functions.values() {
            for b in &f.blocks {
                for inst in &b.instructions {
                    match inst {
                        crate::dmir::Inst::Call { func, .. } => {
                            *call_sites.entry(func.clone()).or_insert(0) += 1;
                        }
                        crate::dmir::Inst::MethodCall { method, .. } => {
                            *call_sites.entry(method.clone()).or_insert(0) += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        for (name, sites) in call_sites {
            for _ in 0..sites {
                prof.record_function_call(&name);
            }
        }
    }

    let prof_dir = layout.root.join(".forgen_profile");
    let _ = fs::create_dir_all(&prof_dir);
    let prof_file = prof_dir.join(format!("{}.json", prof.project_name));
    let _ = prof.save_to_file(&prof_file);

    match run {
        Ok((stdout, stderr, code, elapsed_ns)) => {
            println!(
                "[Forgen Profile] Ran {} (exit {}, {:.2} ms)",
                exe.display(),
                code,
                elapsed_ns as f64 / 1_000_000.0
            );
            if !stdout.is_empty() {
                println!("[Forgen Profile] stdout: {}", stdout.trim_end());
            }
            if !stderr.is_empty() {
                println!("[Forgen Profile] stderr: {}", stderr.trim_end());
            }
        }
        Err(e) => {
            eprintln!("[Forgen Profile] Program failed to run: {}", e);
        }
    }

    println!(
        "[Forgen Profile] Wrote STATIC call-graph profile: {}",
        prof_file.display()
    );
    println!(
        "[Forgen Profile] NOTE: counts are numbers of call SITES, not executions. \
                 Runtime instrumentation is not implemented, so this profile carries no \
                 hot-path or branch data and PGO cannot use it for real decisions."
    );
    true
}
