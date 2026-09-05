//! Miscellaneous commands and shared helpers: explain, watch, completions,
//! setup-tools, the help text and the minimal ZIP writer.

use crate::driver::ForgenCompiler;
use crate::project::{ProjectDiscovery, ProjectRunner};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// `forgen explain <CODE|RULE>`.
pub(crate) fn cmd_explain(args: &[String]) -> bool {
    let code = args.get(2).map(|s| s.as_str()).unwrap_or("");
    if code.is_empty() {
        println!("Usage: forgen explain <CODE|RULE>");
        println!("Examples:");
        println!("  forgen explain style::non_snake_case");
        println!("  forgen explain perf::unnecessary_mut");
        println!("  forgen explain style::prefer_for_loop");
        println!("  forgen explain style::bool_comparison");
        println!("  forgen explain E-OWN-001");
        return false;
    }
    explain_code(code);
    true
}

/// `forgen watch` — filesystem watcher that re-runs a subcommand on change.
pub(crate) fn cmd_watch(args: &[String]) -> bool {
    let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("check");
    let target_opt = args
        .iter()
        .skip(3)
        .find(|a| !a.starts_with("-"))
        .map(Path::new);

    println!(
        "[Forgen watch] Monitoring filesystem changes for `forgen {}`...",
        subcmd
    );
    println!("[Forgen watch] Press Ctrl+C to stop.\n");

    let mut last_modified_map: HashMap<PathBuf, std::time::SystemTime> = HashMap::new();
    if let Ok(layout) = ProjectDiscovery::discover(target_opt) {
        for file_path in &layout.source_files {
            if let Ok(meta) = fs::metadata(file_path)
                && let Ok(mtime) = meta.modified()
            {
                last_modified_map.insert(file_path.clone(), mtime);
            }
        }
    }
    run_watch_iteration(subcmd, args);

    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let layout = match ProjectDiscovery::discover(target_opt) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let mut changed = layout.source_files.len() != last_modified_map.len();
        let mut current_map = HashMap::new();
        for file_path in &layout.source_files {
            if let Ok(meta) = fs::metadata(file_path)
                && let Ok(mtime) = meta.modified()
            {
                match last_modified_map.get(file_path) {
                    Some(&prev) if mtime > prev => {
                        changed = true;
                    }
                    None => {
                        changed = true;
                    }
                    _ => {}
                }
                current_map.insert(file_path.clone(), mtime);
            }
        }
        last_modified_map = current_map;

        if changed {
            println!("\n==================================================");
            println!(
                "[Forgen watch] Changes detected. Re-running `forgen {}`...",
                subcmd
            );
            println!("==================================================");
            run_watch_iteration(subcmd, args);
        }
    }
}

/// `forgen completions <shell>`.
pub(crate) fn cmd_completions(args: &[String]) -> bool {
    let shell = args.get(2).map(|s| s.as_str()).unwrap_or("");
    match shell {
        "powershell" | "pwsh" => {
            println!(
                r#"# PowerShell Completion for Forgen
Register-ArgumentCompleter -Native -CommandName forgen -ScriptBlock {{
    param($wordToComplete, $commandAst, $cursorPosition)
    $subcommands = @("init","new","clean","lint","audit","explain","watch","tree","add","remove","install","restore","publish","search","info","package","lsp","ui","run","build","test","bench","check","domain","sae","profile","format","fmt","repl","doc","export","update","upgrade","vendor","completions","why","context","inspect")
    $flags = @("--llvm","--release","--check","--fix","--effects","--all","--pgo","--indent","--operators","--loops","--style","--mut","--open")
    if ($wordToComplete -like "-*") {{
        $flags | Where-Object {{ $_ -like "$wordToComplete*" }} | ForEach-Object {{
            [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterName', $_)
        }}
    }} else {{
        $subcommands | Where-Object {{ $_ -like "$wordToComplete*" }} | ForEach-Object {{
            [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
        }}
    }}
}}"#
            );
        }
        "bash" => {
            println!(
                r#"# Bash completion for forgen
_forgen() {{
    local cur prev words cword
    _init_completion || return
    local commands="init new clean lint audit explain watch tree add remove install restore publish search info package lsp ui run build test bench check domain sae profile format fmt repl doc export update upgrade vendor completions why context inspect"
    local flags="--llvm --release --check --fix --effects --all --pgo --indent --operators --loops --style --mut --open"
    if [[ ${{cur}} == -* ]]; then
        COMPREPLY=( $(compgen -W "${{flags}}" -- ${{cur}}) )
    else
        COMPREPLY=( $(compgen -W "${{commands}}" -- ${{cur}}) )
    fi
}}
complete -F _forgen forgen"#
            );
        }
        "zsh" => {
            println!(
                r#"#compdef forgen
_forgen() {{
    local -a commands
    commands=(
        'init:Initialize a new Datara project'
        'run:Run project'
        'build:Compile native standalone binary'
        'test:Execute test suites'
        'format:Format source code'
        'lint:Static code analyzer'
        'audit:Security capability lattice audit'
        'repl:Interactive JIT console'
        'doc:Generate SPA documentation'
        'export:Export C header or shared library'
        'clean:Clean build artifacts'
        'tree:Show dependency tree'
        'vendor:Bundle dependencies offline'
        'update:Update dependencies'
    )
    _describe -t commands 'forgen command' commands
}}
_forgen"#
            );
        }
        "fish" => {
            println!(
                r#"# Fish completion for forgen
complete -c forgen -f
complete -c forgen -n "__fish_use_subcommand" -a "run build test format lint audit repl doc export clean tree vendor update explain"
complete -c forgen -l llvm -d "Enable LLVM pipeline"
complete -c forgen -l check -d "Check mode"
complete -c forgen -l fix -d "Auto-repair mode"
complete -c forgen -l all -d "Apply to all targets""#
            );
        }
        _ => {
            eprintln!("Usage: forgen completions <bash|zsh|fish|powershell>");
            std::process::exit(1);
        }
    }
    true
}

fn run_watch_iteration(subcmd: &str, args: &[String]) {
    let target_opt = args
        .iter()
        .skip(3)
        .find(|s| !s.starts_with("-") && *s != "--")
        .map(Path::new);

    match subcmd {
        "run" => {
            if let Ok(layout) = ProjectDiscovery::discover(target_opt) {
                let compiler = ForgenCompiler::new("quick");
                if let Ok((stdout, stderr, _, _)) = compiler.run_project(&layout, &[]) {
                    print!("{}", stdout);
                    if !stderr.is_empty() {
                        eprint!("{}", stderr);
                    }
                }
            }
        }
        "test" => {
            if let Ok(layout) = ProjectDiscovery::discover(target_opt) {
                let compiler = ForgenCompiler::new("quick");
                let report = ProjectRunner::run_tests(&layout, &compiler);
                println!(
                    "[Forgen test] Passed: {}, Failed: {} in {}ms",
                    report.passed, report.failed, report.total_duration_ms
                );
            }
        }
        "lint" => {
            if let Ok(layout) = ProjectDiscovery::discover(target_opt) {
                for file_path in &layout.source_files {
                    if let Ok(diags) = crate::lint::lint_file(file_path) {
                        for diag in diags {
                            print!("{}", diag.render(None));
                        }
                    }
                }
            }
        }
        _ => {
            if let Ok(layout) = ProjectDiscovery::discover(target_opt) {
                let compiler = ForgenCompiler::new("check");
                let res = if layout.source_files.len() == 1 {
                    compiler.check_file(&layout.source_files[0])
                } else {
                    compiler.check_files(&layout.source_files)
                };
                if res.success {
                    println!(
                        "[Forgen check] Verified 100% OK ({} modules, 0 errors, valid ownership & effects)",
                        layout.source_files.len()
                    );
                } else {
                    eprintln!("{}", res.diagnostics);
                }
            }
        }
    }
}

fn explain_code(code: &str) {
    match code {
        "style::non_snake_case" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: style::non_snake_case");
            println!(
                "================================================================================"
            );
            println!(
                "In Datara, all local variables, parameters, functions, and methods must follow"
            );
            println!(
                "the `snake_case` naming convention (lowercase letters separated by underscores)."
            );
            println!();
            println!("❌ Bad Code:");
            println!("   let itemCount = 42");
            println!("   fn computeTotal() {{ ... }}");
            println!();
            println!("✅ Good Code:");
            println!("   let item_count = 42");
            println!("   fn compute_total() {{ ... }}");
            println!();
            println!(
                "Rationale: Enforces visual consistency with native systems languages (Rust/C)"
            );
            println!("and avoids ambiguity with PascalCase type names.");
            println!(
                "================================================================================"
            );
        }
        "style::non_camel_case_types" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: style::non_camel_case_types");
            println!(
                "================================================================================"
            );
            println!(
                "In Datara, all classes, components, roles, packets, and custom types must follow"
            );
            println!("the `PascalCase` (UpperCamelCase) naming convention.");
            println!();
            println!("❌ Bad Code:");
            println!("   class user_session {{ ... }}");
            println!("   component http_handler {{ ... }}");
            println!();
            println!("✅ Good Code:");
            println!("   class UserSession {{ ... }}");
            println!("   component HttpHandler {{ ... }}");
            println!();
            println!(
                "Rationale: Clearly distinguishes types and architectural entities from variables."
            );
            println!(
                "================================================================================"
            );
        }
        "perf::unnecessary_mut" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: perf::unnecessary_mut");
            println!(
                "================================================================================"
            );
            println!(
                "A variable was declared using `mut`, but its value is never modified or reassigned"
            );
            println!("in its entire scope.");
            println!();
            println!("❌ Bad Code:");
            println!("   mut max_limit = 1000");
            println!("   // max_limit is only read, never reassigned");
            println!();
            println!("✅ Good Code:");
            println!("   let max_limit = 1000");
            println!();
            println!(
                "Rationale: Declaring variables as immutable (`let`) allows the Evidence Gate"
            );
            println!(
                "optimizer to promote them to CPU registers via Mem2Reg, perform constant folding,"
            );
            println!("and guarantee thread-safety without locks.");
            println!("Run `forgen lint --fix` to automatically repair this across your project.");
            println!(
                "================================================================================"
            );
        }
        "style::unused_variable" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: style::unused_variable");
            println!(
                "================================================================================"
            );
            println!("A variable or parameter was declared, but its value is never read.");
            println!();
            println!("❌ Bad Code:");
            println!("   let unused_result = compute()");
            println!();
            println!("✅ Good Code (if intentionally ignored):");
            println!("   let _unused_result = compute()");
            println!();
            println!("Rationale: Prevents dead code, accidental resource leaks, and logic errors");
            println!("where a computed result was forgotten by the developer.");
            println!(
                "================================================================================"
            );
        }
        "style::prefer_for_loop" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: style::prefer_for_loop");
            println!(
                "================================================================================"
            );
            println!(
                "A `while` loop was used with a manual index counter increment (`i = i + 1`)."
            );
            println!();
            println!("❌ Bad Code:");
            println!("   mut i = 0");
            println!("   while i < 100 {{");
            println!("       process(i)");
            println!("       i = i + 1");
            println!("   }}");
            println!();
            println!("✅ Good Code:");
            println!("   for i in 0..100 {{");
            println!("       process(i)");
            println!("   }}");
            println!();
            println!(
                "Rationale: Range `for` loops in Datara are zero-cost abstractions that the compiler"
            );
            println!(
                "can automatically unroll, fold in closed-form, and vectorize with AVX2/NEON."
            );
            println!(
                "================================================================================"
            );
        }
        "style::bool_comparison" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: style::bool_comparison");
            println!(
                "================================================================================"
            );
            println!(
                "Comparing a boolean expression directly against `true` or `false` is redundant."
            );
            println!();
            println!("❌ Bad Code:");
            println!("   if is_valid == true {{ ... }}");
            println!("   if is_valid == false {{ ... }}");
            println!();
            println!("✅ Good Code:");
            println!("   if is_valid {{ ... }}");
            println!("   if !is_valid {{ ... }}");
            println!(
                "================================================================================"
            );
        }
        "E-OWN-001" | "ownership" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-OWN-001 (Ownership & Move Semantics)");
            println!(
                "================================================================================"
            );
            println!(
                "Datara uses affine ownership with zero-copy views. Once an owned object is moved"
            );
            println!("into another function or variable, the original binding becomes invalid.");
            println!();
            println!("To borrow data without taking ownership, use a `view` or pass by reference.");
            println!(
                "================================================================================"
            );
        }
        "E-TYPE-001" | "type_mismatch" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-TYPE-001 (Type Mismatch)");
            println!(
                "================================================================================"
            );
            println!(
                "An expression was evaluated with a type that does not match the expected type."
            );
            println!(
                "Datara is strictly typed and does not perform silent or lossy implicit conversions."
            );
            println!();
            println!("❌ Bad Code (Float assigned to Int):");
            println!("   let x: Int = 3.14");
            println!();
            println!("✅ Good Code:");
            println!("   let x: Int = 3.14 as Int");
            println!("   // or use explicit mathematical floor:");
            println!("   let x: Int = datara_rt_math_floor(3.14)");
            println!();
            println!("❌ Bad Code (Integer in Boolean condition):");
            println!("   if counter {{ ... }}");
            println!();
            println!("✅ Good Code:");
            println!("   if counter != 0 {{ ... }}");
            println!(
                "================================================================================"
            );
        }
        "E-RESOLVE-001" | "undefined_symbol" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-RESOLVE-001 (Undefined Symbol)");
            println!(
                "================================================================================"
            );
            println!(
                "A variable, function, class, or method was referenced that does not exist in"
            );
            println!("the current scope or imported modules.");
            println!();
            println!("Common causes:");
            println!(
                "1. Typo in identifier name (the compiler provides 'Did you mean?' suggestions)."
            );
            println!("2. Missing module import ('use stdlib.math').");
            println!(
                "3. Variable declared inside an inner block or loop scope and accessed outside."
            );
            println!(
                "================================================================================"
            );
        }
        "E-BORROW-001" | "immutable_assignment" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-BORROW-001 (Cannot Reassign Immutable Variable)");
            println!(
                "================================================================================"
            );
            println!("In Datara, variables declared with `let` or `val` are immutable by default.");
            println!(
                "Reassigning them without an explicit mutable binding triggers a compile-time error."
            );
            println!();
            println!("❌ Bad Code:");
            println!("   let total = 0");
            println!("   total = total + 1");
            println!();
            println!("✅ Good Code:");
            println!("   mut total = 0");
            println!("   total = total + 1");
            println!(
                "================================================================================"
            );
        }
        "E-BORROW-002" | "use_after_move" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-BORROW-002 (Use of Value After Move)");
            println!(
                "================================================================================"
            );
            println!(
                "An owned value was moved into another variable or function, and subsequently"
            );
            println!(
                "referenced again. Move transfers ownership and invalidates the previous binding."
            );
            println!();
            println!("❌ Bad Code:");
            println!("   let b = a");
            println!("   out a  // Error: 'a' was moved into 'b'");
            println!();
            println!("✅ Good Code (Zero-Copy View):");
            println!("   let b = view a");
            println!("   out a  // Valid: 'a' is borrowed immutably, not consumed");
            println!(
                "================================================================================"
            );
        }
        "E-EFFECT-001" | "effect_leak" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-EFFECT-001 (Effect Lattice Purity Violation)");
            println!(
                "================================================================================"
            );
            println!("A function declared or inferred as [pure] attempts to perform side effects");
            println!("such as filesystem I/O, network socket calls, or non-local state mutation.");
            println!();
            println!("Rationale: Datara's Evidence Gate optimizer relies on Effect Lattice purity");
            println!("to safely reorder, vectorize, eliminate dead code, and parallelize loops.");
            println!(
                "================================================================================"
            );
        }
        "E-SYNTAX-001" | "syntax_error" => {
            println!(
                "================================================================================"
            );
            println!(" EXPLANATION: E-SYNTAX-001 (Syntax Error)");
            println!(
                "================================================================================"
            );
            println!(
                "The source code violates Datara's grammar rules (missing braces, unclosed quotes,"
            );
            println!(
                "or misplaced keywords). Run 'forgen format' to auto-align syntax structures."
            );
            println!(
                "================================================================================"
            );
        }
        _ => {
            println!("Code `{}`: No extended documentation entry found.", code);
            println!(
                "Try: `forgen explain E-TYPE-001`, `forgen explain E-BORROW-001`, or `forgen explain style::non_snake_case`."
            );
        }
    }
}

pub(crate) fn run_setup_tools() {
    println!("================================================================================");
    println!(" Datara Toolchain — Native C/C++ Build Tools & Linker Setup");
    println!("================================================================================");
    let spec = crate::codegen::linker::discover();
    if spec.is_available {
        println!("\n[OK] Linker is already configured and ready:");
        println!("     {}", spec.program.display());
        println!("\nDatara can build native .exe executables immediately.");
        return;
    }

    if cfg!(windows) {
        println!("\n[!] No C/C++ linker detected on this system.");
        println!("    Datara requires a C/C++ linker to produce native executables (.exe).");
        println!("    Launching official Microsoft C++ Build Tools installer (Node.js style)...\n");

        if crate::codegen::linker::run_windows_build_tools_installer() {
            crate::codegen::linker::invalidate_cache();
            println!("\n[SUCCESS] Setup process completed. Verifying toolchain...");
            let fresh = crate::codegen::linker::discover();
            if fresh.is_available {
                println!(
                    "[OK] Successfully verified linker at: {}",
                    fresh.program.display()
                );
            } else {
                println!(
                    "[Notice] Installation was initiated. If environment variables were updated,"
                );
                println!("         please restart your terminal to activate the new toolchain.");
            }
        } else {
            eprintln!("\n[ERROR] Failed to run automated setup.");
            eprintln!("You can install Microsoft C++ Build Tools manually:");
            eprintln!("  winget install Microsoft.VisualStudio.2022.BuildTools");
            std::process::exit(1);
        }
    } else {
        println!("\nOn Unix/Linux/macOS, please install your system's C compiler:");
        println!("  Debian/Ubuntu: sudo apt-get install build-essential");
        println!("  Fedora/RHEL:   sudo dnf groupinstall \"Development Tools\"");
        println!("  macOS:         xcode-select --install");
    }
}

pub(crate) fn print_help() {
    println!(
        r#"
Forgen — Optimizing Native Compiler for Datara (Rust Core v0.1)

Project Commands:
  init [name] [--lib]     Initialize a new Level 3 Datara application or library with datara.toml
  new <name> [--lib]      Create a new Datara application or library in a subdirectory
  repl                    Interactive zero-latency JIT console with live evaluation
  setup-tools             Check and automatically install C/C++ Build Tools / Linker (Node.js style)
  doc [target] [--open]   Generate autonomous Single-File SPA HTML API documentation
  export <c-header|shared> Export C99/C++ header (.h) or dynamic shared library (.dll/.so/.dylib)
  vendor [target]         Bundle dependencies into vendor/ for 100% offline air-gapped builds
  update, upgrade         Check and update dependency versions with Merkle verification
  check-update            Check for newer releases of the forgen compiler toolchain
  completions <shell>     Generate terminal auto-completions (bash, zsh, fish, powershell)
  clean [--all|--pgo|--llvm] Remove target/ build outputs, temporary files, and .pgo caches
  lint, audit [target]    Rust-grade linter & security effect capability lattice audit
  explain <code|rule>     Display interactive documentation with examples for error and lint codes
  watch [cmd] [target]    Auto-watch filesystem changes and instantly re-run command (~50ms loop)
  tree [--effects]        Visualize dependency tree with security capability lattice audit
  add <package|url>       Add package from HyperGrid registry or Git URL
  remove <package>        Remove package dependency and delete from packages/
  install, restore        Restore and download all dependencies from datara.toml
  publish [target]        Verify, calculate Merkle digest, and publish to HyperGrid
  search <query>          Search HyperGrid registry for packages and extensions
  info <package>          Inspect package metadata, capabilities, digest, and files
  package [target]        Verify, test, and package library for Git publishing
  lsp                     Start official Datara Language Server Protocol (LSP v3.17 stdio)
  ui [target]             Build and launch pure Datara Frontend (Zero-JS Web UI or Native Window)
  run [target] [--llvm]   Auto-discover and run project (Level 1 Single, Level 2 Folder, Level 3 Manifest)
  build [target] [--llvm] Build standalone native executable (--llvm enables ultra-optimized AOT LLVM pipeline)
  test [target]           Auto-discover and run project integration tests in tests/
  bench [target]          Auto-discover and run benchmarks in benches/
  check [target]          Fast static verification (types, ownership, effects), 0 binaries
  domain [target] [--llvm] Maximum whole-program specialization & SAE adaptation report (--llvm enables LLVM)
  sae [target]            Inspect Semantic Adaptation Engine decisions (WHAT -> HOW)
  profile [target]        Run execution profile and generate PGO runtime data
  format, fmt [path]      Format code (flags: --check, --indent, --operators, --loops, --style, --mut, --all)
  why <symbol> [target]   Explain why optimizations were applied or rejected for symbol
  context <symbol> [tgt]  AI Semantic API providing structured semantic metadata (JSON)
  inspect <query> <file>  Inspect semantic graph (symbol, effects, optimize, dmir, codegen, clif)

Progressive Project Levels:
  Level 1 (Single File):    forgen run hello.dtr (0 manifest needed)
  Level 2 (Folder Project): forgen run (auto-discovers main.dtr + modules)
  Level 3 (Full Project):   forgen init myapp (datara.toml + src/ + tests/)

Examples:
  forgen run
  forgen lint --fix
  forgen clean
  forgen explain perf::unnecessary_mut
  forgen tree --effects
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
