//! Developer-tool commands: lsp, ui, fmt, lint/audit, doc and export.

use super::extract_target_arg;
use crate::driver::ForgenCompiler;
use crate::project::ProjectDiscovery;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// `forgen lsp` — start the language server over stdio.
pub(crate) fn cmd_lsp() -> bool {
    let server = crate::lsp::LspServer::new();
    if let Err(e) = server.run_stdio() {
        eprintln!("[Forgen LSP] Error: {}", e);
        std::process::exit(1);
    }
    true
}

/// `forgen ui` — build and launch the Datara frontend.
pub(crate) fn cmd_ui(args: &[String]) -> bool {
    let target_opt = extract_target_arg(args, 2);
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
                    {
                        // `cmd /C start` re-parses its arguments, so a
                        // crafted path could smuggle shell metachars.
                        // explorer.exe takes the path as a single,
                        // uninterpreted argument.
                        let _ = std::process::Command::new("explorer").arg(html).spawn();
                    }
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
    true
}

/// `forgen fmt` / `format` — format source files.
pub(crate) fn cmd_fmt(args: &[String]) -> bool {
    let is_check = args.iter().any(|a| a == "--check");
    let is_indent = args.iter().any(|a| a == "--indent");
    let is_operators = args.iter().any(|a| a == "--operators");
    let is_loops = args.iter().any(|a| a == "--loops");
    let is_style = args.iter().any(|a| a == "--style");
    let is_mut = args.iter().any(|a| a == "--mut");
    let is_all = args.iter().any(|a| a == "--all");

    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("-"))
        .map(Path::new);

    let has_granular_flag = is_indent || is_operators || is_loops || is_style || is_mut || is_all;

    let opts = if is_all {
        crate::fmt::FormatOptions::all()
    } else if has_granular_flag {
        crate::fmt::FormatOptions {
            check: is_check,
            indent: is_indent,
            operators: is_operators,
            loops: is_loops,
            blank_lines: is_indent,
            style: is_style,
            mut_fix: is_mut,
        }
    } else {
        crate::fmt::FormatOptions {
            check: is_check,
            ..Default::default()
        }
    };

    // Collect target files
    let source_files: Vec<std::path::PathBuf> = if let Some(target) = target_opt {
        if target.is_file() {
            vec![target.to_path_buf()]
        } else if target.is_dir() {
            crate::fmt::collect_datara_files(target)
        } else {
            eprintln!("Format error: path '{}' does not exist", target.display());
            std::process::exit(1);
        }
    } else if let Ok(layout) = ProjectDiscovery::discover(None) {
        layout.source_files
    } else {
        crate::fmt::collect_datara_files(Path::new("."))
    };

    if source_files.is_empty() {
        println!("[Forgen format] No Datara source files (.dtr, .forge) found to format.");
        return false;
    }

    let start = Instant::now();
    let mut formatted_count = 0;
    let mut total_diffs = 0;
    let mut unformatted_files = Vec::new();

    for file_path in &source_files {
        match crate::fmt::format_file(file_path, &opts) {
            Ok(diffs) => {
                if !diffs.is_empty() {
                    total_diffs += diffs.len();
                    unformatted_files.push(file_path.clone());
                    if !opts.check {
                        formatted_count += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("Format error on '{}': {}", file_path.display(), e);
            }
        }

        // If --style, --mut, or --all was requested, apply AST linter auto-fixes for style/mut
        if (opts.style || opts.mut_fix)
            && !opts.check
            && let Ok(diags) = crate::lint::lint_file(file_path)
        {
            let filtered_diags: Vec<_> = diags
                .into_iter()
                .filter(|d| {
                    (opts.style && d.code.starts_with("style::"))
                        || (opts.mut_fix && d.code == "perf::unnecessary_mut")
                })
                .collect();

            if !filtered_diags.is_empty()
                && let Ok(source) = fs::read_to_string(file_path)
            {
                let fixed = crate::lint::apply_fixes(&source, &filtered_diags);
                if fixed != source {
                    let _ = fs::write(file_path, fixed);
                }
            }
        }
    }

    let elapsed = start.elapsed().as_millis();

    if opts.check {
        if unformatted_files.is_empty() {
            println!(
                "[Forgen format] Clean! All {} file(s) follow official style guidelines (checked in {}ms)",
                source_files.len(),
                elapsed
            );
        } else {
            eprintln!(
                "[Forgen format] Check failed: {} file(s) require formatting ({} diffs detected):\n",
                unformatted_files.len(),
                total_diffs
            );
            for f in &unformatted_files {
                eprintln!("  --> {}", f.display());
            }
            eprintln!(
                "\nRun 'forgen format' or 'forgen format --all' to apply fixes automatically."
            );
            std::process::exit(1);
        }
    } else {
        if formatted_count == 0 {
            println!(
                "[Forgen format] {} file(s) already perfectly formatted ({}ms).",
                source_files.len(),
                elapsed
            );
        } else {
            println!(
                "[Forgen format] Successfully formatted {} of {} file(s) ({} fixes applied in {}ms).",
                formatted_count,
                source_files.len(),
                total_diffs,
                elapsed
            );
        }
    }
    true
}

/// `forgen lint` / `audit` — linter and security capability audit. The audit
/// variant reports the real warning count and exits non-zero on failures.
pub(crate) fn cmd_lint(command: &str, args: &[String]) -> bool {
    let is_audit = command == "audit";
    let is_fix = args.iter().any(|a| a == "--fix");
    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("-"))
        .map(Path::new);

    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to discover project: {}", e);
            std::process::exit(1);
        }
    };

    let start = Instant::now();
    let mut total_warnings = 0;
    let mut total_fixes = 0;

    for file_path in &layout.source_files {
        match crate::lint::lint_file(file_path) {
            Ok(diags) => {
                if !diags.is_empty() {
                    for diag in &diags {
                        print!("{}", diag.render(None));
                    }
                    total_warnings += diags.len();

                    if is_fix && let Ok(source) = fs::read_to_string(file_path) {
                        let fixed = crate::lint::apply_fixes(&source, &diags);
                        if fixed != source {
                            let _ = fs::write(file_path, fixed);
                            total_fixes += diags.iter().filter(|d| d.fix.is_some()).count();
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error checking {}: {}", file_path.display(), e);
            }
        }
    }

    let elapsed = start.elapsed().as_millis();
    if is_audit {
        if total_warnings == 0 {
            println!(
                "[Forgen audit] Security capability audit: 0 purity leaks detected. All external effects strictly isolated in Effect Lattice."
            );
        } else {
            println!(
                "[Forgen audit] Security capability audit FAILED: {} purity leak(s) detected across {} files. External effects are NOT strictly isolated.",
                total_warnings,
                layout.source_files.len()
            );
            std::process::exit(1);
        }
    }
    if total_warnings == 0 {
        println!(
            "[Forgen {}] Clean! 0 warnings across {} files (verified in {}ms)",
            command,
            layout.source_files.len(),
            elapsed
        );
    } else if is_fix {
        println!(
            "[Forgen {}] Completed in {}ms: {} warnings, {} automatic fixes applied",
            command, elapsed, total_warnings, total_fixes
        );
    } else {
        println!(
            "[Forgen {}] Found {} warnings across {} files in {}ms (run `forgen lint --fix` to auto-repair)",
            command,
            total_warnings,
            layout.source_files.len(),
            elapsed
        );
    }
    true
}

/// `forgen doc` — generate single-file SPA documentation.
pub(crate) fn cmd_doc(args: &[String]) -> bool {
    let is_open = args.iter().any(|a| a == "--open");
    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with('-'))
        .map(Path::new);

    let search_path = target_opt.unwrap_or(Path::new("."));
    let out_file = Path::new("target").join("doc").join("index.html");

    println!(
        "[Forgen doc] Scanning '{}' and building Single-File SPA documentation...",
        search_path.display()
    );
    match crate::doc::generate_docs(search_path, &out_file) {
        Ok(count) => {
            println!(
                "[Forgen doc] Generated documentation for {} item(s) at: {}",
                count,
                out_file.display()
            );
            if is_open {
                #[cfg(target_os = "windows")]
                {
                    // Same pattern as the run command: `cmd /C start`
                    // re-parses its arguments, so a crafted path could
                    // smuggle shell metachars. explorer.exe takes the
                    // path as a single, uninterpreted argument.
                    let _ = std::process::Command::new("explorer")
                        .arg(&out_file)
                        .spawn();
                }
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&out_file).spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open")
                    .arg(&out_file)
                    .spawn();
            }
        }
        Err(e) => {
            eprintln!("Doc generation error: {}", e);
            std::process::exit(1);
        }
    }
    true
}

/// `forgen export` — emit C header or shared library.
pub(crate) fn cmd_export(args: &[String]) -> bool {
    let submode = args.get(2).map(|s| s.as_str()).unwrap_or("");
    let target_opt = args
        .iter()
        .skip(3)
        .find(|a| !a.starts_with('-'))
        .map(Path::new);

    match submode {
        "c-header" | "header" => {
            let default_entry = Path::new("src").join("main.dtr");
            let src_file = target_opt.unwrap_or(if default_entry.exists() {
                &default_entry
            } else {
                Path::new("main.dtr")
            });
            let stem = src_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("datara_export");
            let out_h = Path::new("target")
                .join("include")
                .join(format!("{}.h", stem));

            match crate::export::export_c_header(src_file, &out_h) {
                Ok(p) => {
                    println!("[Forgen export] Generated C99/C++ header: {}", p.display())
                }
                Err(e) => {
                    eprintln!("Export error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "shared" | "lib" => {
            let default_entry = Path::new("src").join("main.dtr");
            let src_file = target_opt.unwrap_or(if default_entry.exists() {
                &default_entry
            } else {
                Path::new("main.dtr")
            });
            let stem = src_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("datara_export");
            #[cfg(target_os = "windows")]
            let lib_name = format!("{}.dll", stem);
            #[cfg(target_os = "macos")]
            let lib_name = format!("lib{}.dylib", stem);
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            let lib_name = format!("lib{}.so", stem);

            let out_lib = Path::new("target").join("lib").join(&lib_name);
            match crate::export::export_shared_library(src_file, &out_lib) {
                Ok(p) => println!(
                    "[Forgen export] Compiled native shared library: {}",
                    p.display()
                ),
                Err(e) => {
                    eprintln!("Export error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("Usage: forgen export <c-header|shared> [target]");
            println!("Examples:");
            println!("  forgen export c-header src/main.dtr");
            println!("  forgen export shared src/main.dtr");
            std::process::exit(1);
        }
    }
    true
}
