use crate::driver::{CompilationResult, ForgenCompiler};
use crate::pgo::ProfileData;
use crate::project::{DataraManifest, ProjectDiscovery, ProjectInitializer, ProjectRunner};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn run_cli() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2
        || args[1] == "--help"
        || args[1] == "-h"
        || args[1] == "help"
        || (args.len() >= 2 && args.iter().skip(1).any(|a| a == "--help" || a == "-h") && args[1] != "run")
    {
        print_help();
        return;
    }

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-v" || args[1] == "version") {
        println!("Datara Toolchain & Forgen AOT Native Compiler v0.1.0");
        println!("Target Architecture: x86_64-pc-windows-msvc (Cranelift Backend)");
        println!("Datara Language Specification 2026 Edition");
        return;
    }

    let command = &args[1];

    if args.iter().any(|a| a == "--auto-install" || a == "-y") {
        unsafe {
            std::env::set_var("FORGEN_AUTO_INSTALL", "1");
        }
    }

    match command.as_str() {
        "init" | "new" => {
            let is_lib = args.iter().any(|a| a == "--lib");
            let project_name = args
                .iter()
                .skip(2)
                .find(|a| !a.starts_with("-"))
                .map(|s| s.as_str());
            let target_dir = Path::new(".");
            let res = if is_lib {
                ProjectInitializer::init_lib(project_name, target_dir)
            } else {
                ProjectInitializer::init(project_name, target_dir)
            };
            match res {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("Error initializing project: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "check" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Check error: {}", e);
                    std::process::exit(1);
                }
            };

            let compiler = ForgenCompiler::new("check");
            let start = Instant::now();
            // Check EVERY source file, not just the entry point: type errors
            // in library modules must surface under `check` too.
            let mut combined = String::new();
            for f in &layout.source_files {
                let r = compiler.check_file(f);
                if !r.success {
                    combined.push_str(&r.diagnostics);
                    combined.push('\n');
                }
            }
            let res = if combined.is_empty() {
                compiler.check_file(&layout.entry_point)
            } else {
                CompilationResult {
                    success: false,
                    diagnostics: combined,
                    ..compiler.check_file(&layout.entry_point)
                }
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
        }

        "lsp" | "language-server" => {
            let server = crate::lsp::LspServer::new();
            if let Err(e) = server.run_stdio() {
                eprintln!("[Forgen LSP] Error: {}", e);
                std::process::exit(1);
            }
        }

        "ui" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("UI error: {}", e);
                    std::process::exit(1);
                }
            };

            let compiler = ForgenCompiler::new("release");
            let bin_name = layout.binary_name();
            let exe_target = if layout.source_files.len() == 1 && layout.manifest.is_none() {
                layout.entry_point.with_extension("exe")
            } else {
                layout.root.join(format!("{}.exe", bin_name))
            };

            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], Some(&exe_target))
            } else {
                compiler.compile_files(&layout.source_files, Some(&exe_target))
            };

            if !res.success {
                eprintln!("[Forgen UI] Compilation failed:\n{}", res.diagnostics);
                std::process::exit(1);
            }

            println!(
                "[Forgen UI] Launching Datara Frontend: {}",
                exe_target.display()
            );
            let status = std::process::Command::new(&exe_target).status();

            match status {
                Ok(s) => {
                    let candidates = [
                        layout.entry_point.with_extension("html"),
                        std::path::PathBuf::from("index.html"),
                        std::path::PathBuf::from("dashboard.html"),
                        std::path::PathBuf::from("datara_frontend_app.html"),
                    ];
                    for html in &candidates {
                        if html.exists() {
                            println!("[Forgen UI] Detected Zero-JS HTML Page: {}", html.display());
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("cmd")
                                .args(["/C", "start", "", &html.to_string_lossy()])
                                .spawn();
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(html).spawn();
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(html).spawn();
                            break;
                        }
                    }
                    if !s.success() {
                        std::process::exit(s.code().unwrap_or(1));
                    }
                }
                Err(e) => {
                    eprintln!("Failed to execute UI binary: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "quick" | "run" | "start" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
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
            let compiler = ForgenCompiler::new(mode);

            let run_args_start = if target_opt.is_some() && !args[2].starts_with("-") {
                3
            } else {
                2
            };
            let run_args: Vec<String> = if args.len() > run_args_start {
                args[run_args_start..].to_vec()
            } else {
                Vec::new()
            };

            // Incremental caching check: if target binary is newer than all source files, run directly
            let bin_name = layout.binary_name();
            let exe_target = if layout.source_files.len() == 1 && layout.manifest.is_none() {
                layout.entry_point.with_extension("exe")
            } else {
                layout.root.join(format!("{}.exe", bin_name))
            };

            let mut newest_source_mod = None;
            for sf in &layout.source_files {
                if let Ok(meta) = fs::metadata(sf) {
                    if let Ok(mod_time) = meta.modified() {
                        newest_source_mod = Some(
                            newest_source_mod
                                .map_or(mod_time, |curr: std::time::SystemTime| curr.max(mod_time)),
                        );
                    }
                }
            }
            if let Ok(meta) = fs::metadata(layout.root.join("datara.toml")) {
                if let Ok(mod_time) = meta.modified() {
                    newest_source_mod = Some(
                        newest_source_mod
                            .map_or(mod_time, |curr: std::time::SystemTime| curr.max(mod_time)),
                    );
                }
            }

            let exe_mod = fs::metadata(&exe_target)
                .map(|m| m.modified().ok())
                .ok()
                .flatten();
            let need_recompile = match (newest_source_mod, exe_mod) {
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
                    return;
                }
            }

            match compiler.run_project(&layout, &run_args) {
                Ok((stdout, stderr, code, _)) => {
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
        }

        "test" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Test discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let compiler = ForgenCompiler::new("release");
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
        }

        "bench" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Benchmark discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let compiler = ForgenCompiler::new("release");
            if let Err(e) = ProjectRunner::run_benches(&layout, &compiler) {
                eprintln!("Benchmark failed: {}", e);
                std::process::exit(1);
            }
        }

        "build" | "release" | "debug" | "verify" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Build discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let mode = if command == "build" {
                "release"
            } else {
                command
            };
            let compiler = ForgenCompiler::new(mode);

            let start = Instant::now();
            let bin_name = layout.binary_name();
            let is_python_target = args.iter().any(|a| a == "--python");
            let output_exe = if let Some(pos) = args.iter().position(|a| a == "-o" || a == "--out")
            {
                if let Some(val) = args.get(pos + 1) {
                    PathBuf::from(val)
                } else if is_python_target {
                    layout.root.join(format!("{}.dll", bin_name))
                } else {
                    layout.root.join(format!("{}.exe", bin_name))
                }
            } else if is_python_target {
                layout.root.join(format!("{}.dll", bin_name))
            } else {
                layout.root.join(format!("{}.exe", bin_name))
            };

            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], Some(&output_exe))
            } else {
                compiler.compile_files(&layout.source_files, Some(&output_exe))
            };
            let elapsed = start.elapsed().as_millis();

            if res.success {
                println!("[Forgen] Build succeeded in {}ms ({} mode)", elapsed, mode);
                println!(
                    "[Forgen] Project: {} ({} source files)",
                    layout.name,
                    layout.source_files.len()
                );
                let exe_p = res.exe_path.as_ref().unwrap();
                println!("[Forgen] Output:  {}", exe_p.display());

                if is_python_target {
                    let py_path = exe_p.with_extension("py");
                    let mut py_code = format!(
                        "# Auto-generated Datara Python Bridge for {}\nimport ctypes\nimport os\n\n_dll_path = os.path.abspath(r\"{}\")\n_lib = ctypes.CDLL(_dll_path)\n\n",
                        bin_name,
                        exe_p.display()
                    );

                    if let Some(ref dmir) = res.dmir_module {
                        for fn_name in dmir.functions.keys() {
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

                if args.iter().any(|a| a == "--ledger") {
                    if let Some(report) = &res.optimization_report {
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
                        if let Ok(json) = serde_json::to_string_pretty(&ledger_data) {
                            if fs::write(&ledger_path, json).is_ok() {
                                println!("[Forgen] Ledger:  {}", ledger_path.display());
                            }
                        }
                    }
                }

                if args.iter().any(|a| a == "--graph") {
                    if let Some(graph) = &res.semantic_graph {
                        let graph_path = exe_p.with_extension("graph.json");
                        if let Ok(json) = serde_json::to_string_pretty(graph) {
                            if fs::write(&graph_path, json).is_ok() {
                                println!("[Forgen] Graph:   {}", graph_path.display());
                            }
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
        }

        "sae" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("SAE discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let compiler = ForgenCompiler::new("domain");
            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], None)
            } else {
                compiler.compile_files(&layout.source_files, None)
            };

            if res.success {
                let rep = res.optimization_report.unwrap_or_default();
                if args.iter().any(|a| a == "--json") {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&rep.adaptation_records).unwrap()
                    );
                    return;
                }

                println!(
                    "=================================================================================================="
                );
                println!(
                    "                   FORGEN SEMANTIC ADAPTATION ENGINE (SAE) DECISION REPORT                        "
                );
                println!(
                    "=================================================================================================="
                );
                println!(
                    "{:<16} | {:<24} | {:<28} | {:<8} | {:<8}",
                    "Category", "Candidate", "Decision", "Benefit", "Cost"
                );
                println!(
                    "--------------------------------------------------------------------------------------------------"
                );
                for r in &rep.adaptation_records {
                    println!(
                        "{:?} | {:<24} | {:<28} | {:>6.1}x | {:>6.1}x",
                        r.category, r.candidate, r.decision, r.benefit, r.cost
                    );
                    println!("  Reason:   {}", r.reason);
                    println!("  Evidence: {}", r.evidence);
                    println!(
                        "- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - -"
                    );
                }
                println!(
                    "=================================================================================================="
                );
            } else {
                eprintln!("{}", res.error.unwrap_or_default());
                std::process::exit(1);
            }
        }

        "domain" => {
            let compiler = ForgenCompiler::new("domain");
            let start = Instant::now();

            let mut pgo_profile = None;
            let mut filter_args: Vec<String> = Vec::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--pgo" && i + 1 < args.len() {
                    pgo_profile = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else if args[i] == "--json" {
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

            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], None)
            } else {
                compiler.compile_files(&layout.source_files, None)
            };
            let _elapsed = start.elapsed().as_millis();

            if res.success {
                let rep = res.optimization_report.unwrap_or_default();
                let t = res.timings;

                if args.iter().any(|a| a == "--json") {
                    let mut json_obj = serde_json::Map::new();
                    json_obj.insert(
                        "optimizationReport".into(),
                        serde_json::to_value(&rep).unwrap(),
                    );
                    json_obj.insert("timings".into(), serde_json::to_value(&t).unwrap());
                    json_obj.insert(
                        "outputBinary".into(),
                        serde_json::Value::String(
                            res.exe_path.unwrap().to_string_lossy().to_string(),
                        ),
                    );
                    if let Some(ref p) = pgo_profile {
                        json_obj.insert(
                            "pgoProfile".into(),
                            serde_json::Value::String(p.to_string_lossy().to_string()),
                        );
                    }
                    println!("{}", serde_json::to_string_pretty(&json_obj).unwrap());
                    return;
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
                    res.exe_path.unwrap().display()
                );
                println!("============================================================");
            } else {
                eprintln!(
                    "{}",
                    res.error.unwrap_or_else(|| "Compilation failed".into())
                );
                std::process::exit(1);
            }
        }

        "why" => {
            if args.len() < 3 {
                eprintln!("Usage: forgen why <symbol> [file.dtr] [--json]");
                std::process::exit(1);
            }
            let target_symbol = &args[2];
            let file_target = args.get(3).filter(|s| !s.starts_with("--")).map(Path::new);

            let compiler = ForgenCompiler::new("domain");
            let layout = match ProjectDiscovery::discover(file_target) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Why discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], None)
            } else {
                compiler.compile_files(&layout.source_files, None)
            };

            if let Some(graph) = res.semantic_graph {
                let rep = res.optimization_report.unwrap_or_default();
                let matching_decisions: Vec<_> = rep
                    .decision_trace
                    .iter()
                    .filter(|d| d.candidate.contains(target_symbol))
                    .collect();

                if args.iter().any(|a| a == "--json") {
                    let mut json_obj = serde_json::Map::new();
                    json_obj.insert(
                        "symbol".into(),
                        serde_json::Value::String(target_symbol.clone()),
                    );
                    if let Some(node) = graph.inspect_symbol(target_symbol) {
                        json_obj.insert("node".into(), serde_json::to_value(node).unwrap());
                    }
                    json_obj.insert(
                        "decisions".into(),
                        serde_json::to_value(&matching_decisions).unwrap(),
                    );
                    println!("{}", serde_json::to_string_pretty(&json_obj).unwrap());
                    return;
                }

                println!("============================================================");
                println!(
                    "             FORGEN EXPLAINABILITY REPORT: {}               ",
                    target_symbol
                );
                println!("============================================================");
                if let Some(node) = graph.inspect_symbol(target_symbol) {
                    println!(" Symbol Kind:      {:?}", node.kind);
                    println!(" Effects Lattice:  {:?}", node.effects);
                    println!(" Ownership Model:  {:?}", node.ownership);
                    println!(
                        " Dependencies:     {:?}",
                        graph.inspect_dependencies(target_symbol)
                    );
                } else {
                    println!(" Symbol:           {}", target_symbol);
                }
                println!("------------------------------------------------------------");
                println!(" Optimization Decisions & Cost Model Breakdown:");
                if matching_decisions.is_empty() {
                    println!("   (No specific function-level optimization overrides recorded)");
                } else {
                    for d in matching_decisions {
                        println!("   [{}] Decision: {}", d.pass, d.decision);
                        println!("     Benefit: {}", d.estimated_benefit);
                        println!("     Cost:    {}", d.estimated_cost);
                        println!("     Reason:  {}", d.reason);
                    }
                }
                println!("============================================================");
            } else {
                eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
                std::process::exit(1);
            }
        }

        "context" => {
            if args.len() < 3 {
                eprintln!("Usage: forgen context <symbol> [file.dtr]");
                std::process::exit(1);
            }
            let target_symbol = &args[2];
            let file_target = args.get(3).filter(|s| !s.starts_with("--")).map(Path::new);

            let compiler = ForgenCompiler::new("domain");
            let layout = match ProjectDiscovery::discover(file_target) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Context discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            let res = if layout.source_files.len() == 1 {
                compiler.compile_file(&layout.source_files[0], None)
            } else {
                compiler.compile_files(&layout.source_files, None)
            };

            if let Some(graph) = res.semantic_graph {
                let rep = res.optimization_report.unwrap_or_default();
                let matching_decisions: Vec<_> = rep
                    .decision_trace
                    .iter()
                    .filter(|d| d.candidate.contains(target_symbol))
                    .collect();

                let mut context_map = serde_json::Map::new();
                context_map.insert(
                    "symbol".into(),
                    serde_json::Value::String(target_symbol.clone()),
                );

                if let Some(node) = graph.inspect_symbol(target_symbol) {
                    context_map.insert("kind".into(), serde_json::to_value(&node.kind).unwrap());
                    context_map.insert(
                        "effects".into(),
                        serde_json::to_value(&node.effects).unwrap(),
                    );
                    context_map.insert(
                        "ownership".into(),
                        serde_json::to_value(&node.ownership).unwrap(),
                    );
                }

                context_map.insert(
                    "dependencies".into(),
                    serde_json::to_value(graph.inspect_dependencies(target_symbol)).unwrap(),
                );
                context_map.insert(
                    "callers".into(),
                    serde_json::to_value(graph.find_callers(target_symbol)).unwrap(),
                );
                context_map.insert(
                    "callees".into(),
                    serde_json::to_value(graph.find_callees(target_symbol)).unwrap(),
                );
                context_map.insert(
                    "optimizationDecisions".into(),
                    serde_json::to_value(matching_decisions).unwrap(),
                );

                println!("{}", serde_json::to_string_pretty(&context_map).unwrap());
            } else {
                eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
                std::process::exit(1);
            }
        }

        "profile" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
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
                for (_, f) in &module.functions {
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
        }

        "inspect" => {
            if args.len() < 4 {
                println!(
                    "Usage: forgen inspect <symbol|effects|optimize|dependencies|ast|dmir|codegen|asm|clif> <file.dtr> [symbolName]"
                );
                return;
            }
            let query = &args[2];
            let file_path = Path::new(&args[3]);
            let target_symbol = args.get(4).map(|s| s.as_str());

            let compiler = ForgenCompiler::new("debug");
            let res = compiler.compile_file(file_path, None);

            if let Some(graph) = res.semantic_graph {
                match query.as_str() {
                    "symbol" => {
                        let sym_name = target_symbol.unwrap_or("main");
                        if let Some(node) = graph.inspect_symbol(sym_name) {
                            println!("{}", serde_json::to_string_pretty(node).unwrap());
                        } else {
                            println!("Symbol '{}' not found in semantic graph", sym_name);
                        }
                    }
                    "effects" => {
                        if let Some(sym_name) = target_symbol {
                            if let Some(eff) = graph.inspect_effects(sym_name) {
                                println!("{}", serde_json::to_string_pretty(&eff).unwrap());
                            } else {
                                println!("Symbol '{}' not found", sym_name);
                            }
                        } else {
                            println!("{}", serde_json::to_string_pretty(&graph).unwrap());
                        }
                    }
                    "optimize" => {
                        let sym_name = target_symbol.unwrap_or("main");
                        if let Some(opt) = graph.inspect_optimization(sym_name) {
                            println!("{}", serde_json::to_string_pretty(&opt).unwrap());
                        } else {
                            println!("Symbol '{}' not found in semantic graph", sym_name);
                        }
                    }
                    "dependencies" => {
                        let sym_name = target_symbol.unwrap_or("main");
                        let deps = graph.inspect_dependencies(sym_name);
                        println!("{}", serde_json::to_string_pretty(&deps).unwrap());
                    }
                    "ast" => {
                        if let Some(prog) = res.program {
                            println!("{}", serde_json::to_string_pretty(&prog).unwrap());
                        }
                    }
                    "dmir" => {
                        if let Some(dmir) = res.dmir_module {
                            println!("{}", serde_json::to_string_pretty(&dmir).unwrap());
                        }
                    }
                    "codegen" => {
                        if let Some(dmir) = &res.dmir_module {
                            let cranelift_backend =
                                crate::codegen::cranelift::CraneliftBackend::for_host();
                            let inspection = cranelift_backend.inspect_module(dmir);
                            if args.iter().any(|a| a == "--json") {
                                println!("{}", serde_json::to_string_pretty(&inspection).unwrap());
                            } else {
                                println!(
                                    "============================================================"
                                );
                                println!(
                                    "             FORGEN CODEGEN & MACHINE CODE INSPECTION       "
                                );
                                println!(
                                    "============================================================"
                                );
                                println!(" Target:               {}", inspection.target);
                                println!(
                                    " Calling Convention:   {}",
                                    inspection.calling_convention
                                );
                                println!(" Total Functions:      {}", inspection.total_functions);
                                println!(
                                    " Total Instructions:   {}",
                                    inspection.total_instructions
                                );
                                println!(
                                    " Heap Allocations:     {} (Zero-Cost Stack/Scalarized)",
                                    inspection.total_heap_allocations
                                );
                                println!(
                                    "------------------------------------------------------------"
                                );
                                println!(
                                    " {:<20} | {:<5} | {:<7} | {:<5} | {:<5} | {:<5}",
                                    "Function", "Insts", "Stack", "Slots", "Calls", "Branch"
                                );
                                println!(
                                    "------------------------------------------------------------"
                                );
                                for f in &inspection.functions {
                                    println!(
                                        " {:<20} | {:>5} | {:>5}B | {:>5} | {:>5} | {:>5}",
                                        f.name,
                                        f.instruction_count,
                                        f.stack_frame_bytes,
                                        f.explicit_stack_slots,
                                        f.direct_calls,
                                        f.branches
                                    );
                                }
                                println!(
                                    "============================================================"
                                );
                            }
                        }
                    }
                    "asm" | "clif" => {
                        if let Some(clif) = res.clif_source {
                            println!("{}", clif);
                        } else if let Some(dmir) = &res.dmir_module {
                            let cranelift_backend =
                                crate::codegen::cranelift::CraneliftBackend::for_host();
                            let resolver = crate::resolver::Resolver::new();
                            let types = crate::types::TypeChecker::new(&resolver);
                            let prog = res.program.as_ref().unwrap();
                            let clif = cranelift_backend.emit_clif(dmir, prog, &types);
                            println!("{}", clif);
                        }
                    }
                    _ => {
                        println!("{}", serde_json::to_string_pretty(&graph).unwrap());
                    }
                }
            } else {
                eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
            }
        }

        "add" => {
            let target_arg = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!("Usage: forgen add <package_name> [--git <url>]");
                    std::process::exit(1);
                }
            };

            let (pkg_name, git_url) = if target_arg.starts_with("http://")
                || target_arg.starts_with("https://")
                || target_arg.starts_with("git@")
            {
                let name = target_arg
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .split('/')
                    .last()
                    .unwrap_or("pkg")
                    .to_string();
                (name, Some(target_arg.to_string()))
            } else if let Some(git_pos) = args.iter().position(|a| a == "--git") {
                let url = args.get(git_pos + 1).cloned();
                (target_arg.to_string(), url)
            } else {
                (target_arg.to_string(), None)
            };

            println!(":: [HyperGrid] Resolving package '{}'...", pkg_name);
            let registry = crate::project::HyperGridRegistry::new();

            if let Some(pkg) = registry.lookup(&pkg_name) {
                println!(
                    "[.....] Fetching {}@{} into Content-Addressed Store...",
                    pkg.name, pkg.version
                );
                println!("[====.] Verifying SHA-256 Merkle integrity...");
                match registry.install(pkg, Path::new(".")) {
                    Ok(_) => {
                        println!(
                            "[DONE] Installed {} (v{}) to packages/{}",
                            pkg.name, pkg.version, pkg.name
                        );
                        println!(
                            "[OK] Added '{} = \"{}\"' to datara.toml",
                            pkg.name, pkg.version
                        );
                    }
                    Err(e) => {
                        eprintln!("[FAIL] Installation failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(ref url) = git_url {
                let packages_dir = Path::new("packages");
                let _ = fs::create_dir_all(packages_dir);
                let target_clone = packages_dir.join(&pkg_name);
                if !target_clone.exists() {
                    println!("[.....] Cloning remote package from '{}'...", url);
                    let status = std::process::Command::new("git")
                        .arg("clone")
                        .arg("--depth")
                        .arg("1")
                        .arg(url)
                        .arg(target_clone.to_str().unwrap_or("."))
                        .status();
                    match status {
                        Ok(s) if s.success() => {
                            println!("[DONE] Downloaded '{}' to packages/{}", pkg_name, pkg_name);
                        }
                        _ => {
                            eprintln!(
                                "[WARN] Git clone failed. Recorded dependency in datara.toml."
                            );
                        }
                    }
                } else {
                    println!(
                        "[INFO] Package '{}' is already present in packages/",
                        pkg_name
                    );
                }

                let manifest_path = Path::new("datara.toml");
                let mut content = if manifest_path.exists() {
                    fs::read_to_string(manifest_path).unwrap_or_default()
                } else {
                    format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n")
                };
                if !content.contains("[dependencies]") {
                    content.push_str("\n[dependencies]\n");
                }
                if !content.contains(&format!("{} =", pkg_name))
                    && !content.contains(&format!("\"{}\" =", pkg_name))
                {
                    content.push_str(&format!("{} = {{ git = \"{}\" }}\n", pkg_name, url));
                    let _ = fs::write(manifest_path, content);
                    println!("[OK] Added dependency '{}' to datara.toml", pkg_name);
                }
            } else {
                eprintln!(
                    "[ERR] Package '{}' not found in HyperGrid registry.\n      Run 'forgen search <query>' to find packages, or use '--git <url>'",
                    pkg_name
                );
                std::process::exit(1);
            }
        }

        "remove" | "rm" => {
            let pkg_name = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!("Usage: forgen remove <package_name>");
                    std::process::exit(1);
                }
            };
            println!(":: [HyperGrid] Removing package '{}'...", pkg_name);
            let pkg_dir = Path::new("packages").join(pkg_name);
            if pkg_dir.exists() {
                let _ = fs::remove_dir_all(&pkg_dir);
                println!("[DONE] Removed packages/{}", pkg_name);
            }

            let manifest_path = Path::new("datara.toml");
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(manifest_path) {
                    let filtered: Vec<&str> = content
                        .lines()
                        .filter(|l| {
                            !l.trim().starts_with(&format!("{} =", pkg_name))
                                && !l.trim().starts_with(&format!("\"{}\" =", pkg_name))
                        })
                        .collect();
                    let _ = fs::write(manifest_path, filtered.join("\n") + "\n");
                    println!("[OK] Removed dependency from datara.toml");
                }
            }
        }

        "install" | "restore" => {
            println!(":: [HyperGrid] Restoring project dependencies from datara.toml...");
            let manifest_path = Path::new("datara.toml");
            if !manifest_path.exists() {
                println!("[INFO] No datara.toml found. Nothing to install.");
                return;
            }

            let manifest = match DataraManifest::from_file(manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[ERR] {}", e);
                    std::process::exit(1);
                }
            };

            let registry = crate::project::HyperGridRegistry::new();
            let mut installed_count = 0;
            for (dep_name, _) in &manifest.dependencies {
                let pkg_dir = Path::new("packages").join(dep_name);
                if !pkg_dir.exists() {
                    if let Some(pkg) = registry.lookup(dep_name) {
                        println!("[.....] Installing {} (v{})...", pkg.name, pkg.version);
                        if registry.install(pkg, Path::new(".")).is_ok() {
                            println!("[DONE] Installed packages/{}", pkg.name);
                            installed_count += 1;
                        }
                    } else {
                        eprintln!("[WARN] Dependency '{}' not found in HyperGrid", dep_name);
                    }
                }
            }
            println!(
                "[DONE] Synchronized dependencies ({} installed, {} up-to-date)",
                installed_count,
                manifest.dependencies.len().saturating_sub(installed_count)
            );
        }

        "publish" => {
            println!(":: [HyperGrid] Publishing package to registry...");
            println!("[.....] Indexing source files...");
            let mut registry = crate::project::HyperGridRegistry::new();
            match registry.publish(Path::new(".")) {
                Ok(pkg) => {
                    println!("[====.] Generating Merkle digest ({})", pkg.digest);
                    println!(
                        "[DONE] Package '{}' (v{}) published successfully to HyperGrid Registry",
                        pkg.name, pkg.version
                    );
                }
                Err(e) => {
                    eprintln!("[FAIL] Publish error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "search" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            let registry = crate::project::HyperGridRegistry::new();
            let results = registry.search(query);
            println!(":: [HyperGrid] Search results for '{}':", query);
            if results.is_empty() {
                println!("   (no packages found matching '{}')", query);
            } else {
                for p in results {
                    println!("• {} (v{}) - {}", p.name, p.version, p.description);
                }
            }
        }

        "info" => {
            let pkg_name = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!("Usage: forgen info <package_name>");
                    std::process::exit(1);
                }
            };
            let registry = crate::project::HyperGridRegistry::new();
            if let Some(pkg) = registry.lookup(pkg_name) {
                println!(":: [HyperGrid] Package '{}'", pkg.name);
                println!("   version:      {}", pkg.version);
                println!("   description:  {}", pkg.description);
                println!("   author:       {}", pkg.author);
                println!("   license:      {}", pkg.license);
                println!("   digest:       {}", pkg.digest);
                println!("   capabilities: [{}]", pkg.capabilities.join(", "));
                let f_list: Vec<&String> = pkg.files.keys().collect();
                println!(
                    "   files:        {}",
                    f_list
                        .into_iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                eprintln!(
                    "[ERR] Package '{}' not found in HyperGrid registry",
                    pkg_name
                );
                std::process::exit(1);
            }
        }

        "package" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Package discovery error: {}", e);
                    std::process::exit(1);
                }
            };

            println!("[Forgen Package] Verifying library '{}'...", layout.name);
            let compiler = ForgenCompiler::new("release");
            let rep = ProjectRunner::run_tests(&layout, &compiler);
            if rep.failed > 0 {
                eprintln!(
                    "[Forgen Package] Cannot package: {} test(s) failed. Fix tests before publishing.",
                    rep.failed
                );
                std::process::exit(1);
            }

            let pkg_out_dir = layout.root.join("target").join("package");
            let _ = fs::create_dir_all(&pkg_out_dir);
            let version = layout
                .manifest
                .as_ref()
                .map(|m| m.package.version.clone())
                .unwrap_or_else(|| "0.1.0".to_string());
            let archive_name = format!("{}-{}.zip", layout.name, version);
            let archive_path = pkg_out_dir.join(&archive_name);

            // Real archive: manifest + all sources + tests + examples.
            let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
            let manifest_path = layout.root.join("datara.toml");
            if manifest_path.exists() {
                entries.push(("datara.toml".into(), fs::read(&manifest_path).unwrap_or_default()));
            }
            for f in layout
                .source_files
                .iter()
                .chain(&layout.test_files)
                .chain(&layout.example_files)
            {
                if let Ok(rel) = f.strip_prefix(&layout.root) {
                    if let Ok(data) = fs::read(f) {
                        entries.push((rel.to_string_lossy().replace('\\', "/"), data));
                    }
                }
            }
            if let Err(e) = write_zip(&archive_path, &entries) {
                    eprintln!("[Forgen Package] Error: {}", e);
                    std::process::exit(1);
                }

            println!(
                "[Forgen Package] Package verified 100% PASS ({} tests).",
                rep.passed
            );
            println!(
                "[Forgen Package] Packaged {} file(s) ({} bytes) for Git publishing or distribution at '{}'!",
                entries.len(),
                entries.iter().map(|(_, d)| d.len()).sum::<usize>(),
                archive_path.display()
            );
            println!(
                "\nTo publish to the world via Git:\n  1. git init && git add .\n  2. git commit -m 'Release v0.1.0'\n  3. git remote add origin https://github.com/your-username/{}\n  4. git push -u origin main --tags",
                layout.name
            );
        }

        "fmt" => {
            let target_opt = args.get(2).filter(|s| !s.starts_with("-")).map(Path::new);
            let layout = match ProjectDiscovery::discover(target_opt) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Fmt error: {}", e);
                    std::process::exit(1);
                }
            };

            let mut formatted_count = 0;
            for file_path in &layout.source_files {
                if let Ok(raw) = fs::read_to_string(file_path) {
                    let mut formatted = String::new();
                    let mut indent_level: usize = 0;
                    // Tracks whether we are inside a multi-line string
                    // literal, so `fmt` never re-indents (and thereby
                    // corrupts) string contents.
                    let mut in_string = false;
                    for line in raw.lines() {
                        let trimmed = line.trim();
                        if in_string {
                            // Verbatim inside a string literal.
                            formatted.push_str(line);
                            formatted.push('\n');
                            let mut esc = false;
                            for c in line.chars() {
                                if esc {
                                    esc = false;
                                } else if c == '\\' {
                                    esc = true;
                                } else if c == '"' {
                                    in_string = false;
                                }
                            }
                            continue;
                        }
                        if trimmed.is_empty() {
                            formatted.push('\n');
                            continue;
                        }
                        if trimmed.starts_with('}')
                            || trimmed.starts_with(']')
                            || trimmed.starts_with(')')
                        {
                            indent_level = indent_level.saturating_sub(1);
                        }
                        let pad = "    ".repeat(indent_level);
                        formatted.push_str(&pad);
                        formatted.push_str(trimmed);
                        formatted.push('\n');
                        let mut esc = false;
                        for c in trimmed.chars() {
                            if esc {
                                esc = false;
                            } else if c == '\\' {
                                esc = true;
                            } else if c == '"' {
                                in_string = !in_string;
                            }
                        }
                        if trimmed.ends_with('{')
                            || trimmed.ends_with('[')
                            || trimmed.ends_with('(')
                        {
                            indent_level += 1;
                        }
                    }
                    if formatted != raw {
                        let _ = fs::write(file_path, formatted);
                        formatted_count += 1;
                    }
                }
            }
            println!(
                "[Forgen fmt] Formatted {} of {} file(s) in '{}'.",
                formatted_count,
                layout.source_files.len(),
                layout.name
            );
        }

        _ => {
            eprintln!("Unknown command: {}", command);
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"
Forgen — Optimizing Native Compiler for Datara (Rust Core v0.1)

Project Commands:
  init [name] [--lib]     Initialize a new Level 3 Datara application or library with datara.toml
  new <name> [--lib]      Create a new Datara application or library in a subdirectory
  add <package|url>       Add package from HyperGrid registry or Git URL
  remove <package>        Remove package dependency and delete from packages/
  install, restore        Restore and download all dependencies from datara.toml
  publish [target]        Verify, calculate Merkle digest, and publish to HyperGrid
  search <query>          Search HyperGrid registry for packages and extensions
  info <package>          Inspect package metadata, capabilities, digest, and files
  package [target]        Verify, test, and package library for Git publishing
  lsp                     Start official Datara Language Server Protocol (LSP v3.17 stdio)
  ui [target]             Build and launch pure Datara Frontend (Zero-JS Web UI or Native Window)
  run [target]            Auto-discover and run project (Level 1 Single, Level 2 Folder, Level 3 Manifest)
  build [target]          Build standalone native executable with auto-discovery
  test [target]           Auto-discover and run project integration tests in tests/
  bench [target]          Auto-discover and run benchmarks in benches/
  check [target]          Fast static verification (types, ownership, effects), 0 binaries
  domain [target]         Maximum whole-program specialization & SAE adaptation report
  sae [target]            Inspect Semantic Adaptation Engine decisions (WHAT -> HOW)
  profile [target]        Run execution profile and generate PGO runtime data
  fmt [target]            Format Datara source files in project
  why <symbol> [target]   Explain why optimizations were applied or rejected for symbol
  context <symbol> [tgt]  AI Semantic API providing structured semantic metadata (JSON)
  inspect <query> <file>  Inspect semantic graph (symbol, effects, optimize, dmir, codegen, clif)

Progressive Project Levels:
  Level 1 (Single File):    forgen run hello.dtr (0 manifest needed)
  Level 2 (Folder Project): forgen run (auto-discovers main.dtr + modules)
  Level 3 (Full Project):   forgen init myapp (datara.toml + src/ + tests/)

Examples:
  forgen run
  forgen run hello.dtr
  forgen build
  forgen test
  forgen domain
"#
    );
}

/// Minimal ZIP (STORE, no compression) writer — no external dependencies.
/// Writes a classic PKZIP archive with a central directory and correct CRC32.
pub fn write_zip(path: &Path, entries: &[(String, Vec<u8>)]) -> Result<(), String> {
    fn crc32(data: &[u8]) -> u32 {
        // Standard CRC-32/ISO-HDLC, table-free bit loop (cold path: packaging).
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }
    fn put_u16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    const DOS_TIME: u16 = 0; // midnight, no TZ dependency
    const DOS_DATE: u16 = (2026 - 1980) << 9 | 1 << 5 | 1; // 2026-01-01

    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();

    for (name, data) in entries {
        let name_bytes = name.as_bytes();
        let crc = crc32(data);
        let offset = out.len() as u32;

        // Local file header
        put_u32(&mut out, 0x04034b50);
        put_u16(&mut out, 20); // version needed
        put_u16(&mut out, 0x0800); // UTF-8 names
        put_u16(&mut out, 0); // STORE
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, crc);
        put_u32(&mut out, data.len() as u32);
        put_u32(&mut out, data.len() as u32);
        put_u16(&mut out, name_bytes.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        // Central directory entry
        put_u32(&mut central, 0x02014b50);
        put_u16(&mut central, 20); // version made by
        put_u16(&mut central, 20); // version needed
        put_u16(&mut central, 0x0800);
        put_u16(&mut central, 0);
        put_u16(&mut central, DOS_TIME);
        put_u16(&mut central, DOS_DATE);
        put_u32(&mut central, crc);
        put_u32(&mut central, data.len() as u32);
        put_u32(&mut central, data.len() as u32);
        put_u16(&mut central, name_bytes.len() as u16);
        put_u16(&mut central, 0); // extra
        put_u16(&mut central, 0); // comment
        put_u16(&mut central, 0); // disk
        put_u16(&mut central, 0); // internal attrs
        put_u32(&mut central, 0); // external attrs
        put_u32(&mut central, offset);
        central.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);

    // End of central directory
    put_u32(&mut out, 0x06054b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_size);
    put_u32(&mut out, cd_offset);
    put_u16(&mut out, 0);

    fs::write(path, out).map_err(|e| format!("Failed to write archive: {}", e))
}
