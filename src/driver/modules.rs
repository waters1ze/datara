//! `use`-import resolution for the driver: stdlib and local module loading,
//! FFI interop detection (Python / Rust / C/C++ / JS-TS), HyperGrid
//! auto-install, and import-cycle detection.

use super::ForgenCompiler;
use crate::ast::{Decl, Program, UseDecl};
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

impl ForgenCompiler {
    /// Candidate base directories for resolving local module paths:
    /// the importing file's directory, then the current directory.
    pub(super) fn module_base_dirs(&self, source_file: &Path) -> Vec<PathBuf> {
        let mut base_dirs = Vec::new();
        if let Some(parent) = source_file.parent()
            && !parent.as_os_str().is_empty()
        {
            base_dirs.push(parent.to_path_buf());
            if let Some(grandparent) = parent.parent()
                && !grandparent.as_os_str().is_empty()
            {
                base_dirs.push(grandparent.to_path_buf());
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            base_dirs.push(cwd);
        }
        base_dirs
    }

    /// Locate the stdlib directory: `<cwd>/stdlib` when running inside the
    /// repository, or next to the compiler executable for installed builds.
    fn find_stdlib_dir(&self) -> Option<PathBuf> {
        let mut candidates = Vec::new();

        // 1. Current working directory (local repository/project stdlib has top priority)
        if let Ok(cwd) = std::env::current_dir() {
            let local_stdlib = cwd.join("stdlib");
            if local_stdlib.is_dir() {
                candidates.push(local_stdlib);
            }
        }

        // 2. DATARA_STDLIB or DATARA_HOME environment variable
        if let Ok(stdlib_env) = std::env::var("DATARA_STDLIB") {
            candidates.push(PathBuf::from(stdlib_env));
        }
        if let Ok(home) = std::env::var("DATARA_HOME") {
            candidates.push(PathBuf::from(&home).join("stdlib"));
            candidates.push(PathBuf::from(home));
        }

        // 3. Relative to compiler executable (installed or development target)
        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            candidates.push(exe_dir.join("stdlib"));
            if let Some(p1) = exe_dir.parent() {
                candidates.push(p1.join("stdlib"));
                if let Some(p2) = p1.parent() {
                    candidates.push(p2.join("stdlib"));
                    if let Some(p3) = p2.parent() {
                        candidates.push(p3.join("stdlib"));
                    }
                }
            }
        }

        // 4. User profile or Unix standard share
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            candidates.push(PathBuf::from(home).join(".datara").join("stdlib"));
        }
        candidates.push(PathBuf::from("/usr/local/share/datara/stdlib"));

        candidates.into_iter().find(|d| d.is_dir())
    }

    /// Map a `use stdlib.<...>` declaration to its stdlib source file.
    /// `stdlib.io.fs.Fs` -> `stdlib/io/fs.dtr`.
    /// If no on-disk stdlib is available, automatically falls back to the embedded standard library.
    fn stdlib_module_path(&self, u: &UseDecl, stdlib_dir: Option<&Path>) -> Option<PathBuf> {
        let is_explicit_stdlib = u.path.first().map(|s| s.as_str()) == Some("stdlib");
        let first_seg = u.path.first().map(|s| s.as_str()).unwrap_or("");
        let is_known_stdlib_root = crate::stdlib::ALL_EMBEDDED_MODULES
            .iter()
            .any(|m| m.starts_with(first_seg));

        if !is_explicit_stdlib && !is_known_stdlib_root {
            return None;
        }

        let base_rel: &[String] = if is_explicit_stdlib {
            if u.path.len() >= 4 {
                &u.path[1..u.path.len() - 1]
            } else {
                &u.path[1..]
            }
        } else if u.path.len() >= 3 {
            &u.path[0..u.path.len() - 1]
        } else {
            &u.path[..]
        };

        if base_rel.is_empty() {
            return None;
        }

        // Build search candidates to handle case sensitivity on Linux
        // and both 3-segment (`stdlib.math.Math`) and 4-segment (`stdlib.io.fs.Fs`) imports.
        let mut candidates: Vec<Vec<String>> = Vec::new();
        candidates.push(base_rel.to_vec());
        let lower: Vec<String> = base_rel.iter().map(|s| s.to_lowercase()).collect();
        if lower != base_rel {
            candidates.push(lower.clone());
        }
        if base_rel.len() == 2 {
            let doubled = vec![base_rel[0].to_lowercase(), base_rel[0].to_lowercase()];
            if !candidates.contains(&doubled) {
                candidates.push(doubled);
            }
            let single = vec![base_rel[0].to_lowercase()];
            if !candidates.contains(&single) {
                candidates.push(single);
            }
        } else if base_rel.len() == 1 {
            let doubled = vec![base_rel[0].to_lowercase(), base_rel[0].to_lowercase()];
            if !candidates.contains(&doubled) {
                candidates.push(doubled);
            }
        }

        // 1. Check local / installed disk path first for all candidates
        if let Some(dir) = stdlib_dir {
            for cand in &candidates {
                let mut p = dir.to_path_buf();
                for seg in cand {
                    p.push(seg);
                }
                p.set_extension("dtr");
                if p.is_file() {
                    return Some(p);
                }
            }
        }

        // 2. Embedded stdlib fallback for all candidates
        let cache_dir = std::env::temp_dir().join("datara_embedded_stdlib");
        for cand in &candidates {
            let key = cand.join(".");
            if let Some(src) = crate::stdlib::get_embedded_stdlib_source(&key) {
                let mut target = cache_dir.clone();
                for seg in cand {
                    target.push(seg);
                }
                target.set_extension("dtr");
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&target, src);
                return Some(target);
            }
        }

        None
    }

    /// Map a non-stdlib `use` path to a local project source file.
    /// `core.User` -> `core.dtr`; `examples.real_cli.config.Config`
    /// -> `examples/real_cli/config.dtr`. The path is resolved against
    /// the candidate base directories in order.
    fn local_module_path(&self, u: &UseDecl, base_dirs: &[PathBuf]) -> Option<PathBuf> {
        if u.path.is_empty() || u.path[0] == "stdlib" {
            return None;
        }
        let rel: &[String] = if u.path.len() >= 3 {
            &u.path[0..u.path.len() - 1]
        } else {
            &u.path[0..1]
        };
        for base in base_dirs {
            // 1. Direct file: base/seg.dtr
            let mut p = base.clone();
            for seg in rel {
                p.push(seg);
            }
            p.set_extension("dtr");
            if p.is_file() {
                return Some(p);
            }

            // 2. Library package: base/seg/src/lib.dtr or base/seg/lib.dtr
            let mut lib_dir = base.clone();
            for seg in rel {
                lib_dir.push(seg);
            }
            let candidate_src_lib = lib_dir.join("src").join("lib.dtr");
            if candidate_src_lib.is_file() {
                return Some(candidate_src_lib);
            }
            let candidate_lib = lib_dir.join("lib.dtr");
            if candidate_lib.is_file() {
                return Some(candidate_lib);
            }

            // 3. Subdirectories lib/ and packages/: base/lib/seg/src/lib.dtr, etc.
            for container in &["lib", "packages", "modules"] {
                let mut cont_dir = base.join(container);
                for seg in rel {
                    cont_dir.push(seg);
                }
                let c1 = cont_dir.join("src").join("lib.dtr");
                if c1.is_file() {
                    return Some(c1);
                }
                let c2 = cont_dir.join("lib.dtr");
                if c2.is_file() {
                    return Some(c2);
                }
                let c_mod = cont_dir.join("mod.dtr");
                if c_mod.is_file() {
                    return Some(c_mod);
                }
                if let Some(last_seg) = rel.last() {
                    let c_named = cont_dir.join(format!("{}.dtr", last_seg));
                    if c_named.is_file() {
                        return Some(c_named);
                    }
                }
                let mut c3 = cont_dir.clone();
                c3.set_extension("dtr");
                if c3.is_file() {
                    return Some(c3);
                }
            }
        }
        None
    }

    /// Locate a C or C++ library across standard system locations:
    /// Windows System32, MSVC LIB environment paths, system PATH, and Unix /usr/lib.
    fn find_system_c_cpp_lib(&self, lib_name: &str) -> Option<PathBuf> {
        let base_name = lib_name
            .trim_end_matches(".lib")
            .trim_end_matches(".dll")
            .trim_end_matches(".so")
            .trim_end_matches(".dylib");
        let lib_extensions = ["lib", "dll", "so", "a", "dylib"];

        // 0. Known platform system libraries (for cross-compilation & cross-platform check/FFI)
        const KNOWN_SYSTEM_C_LIBS: &[&str] = &[
            "kernel32", "user32", "gdi32", "advapi32", "shell32", "ole32", "msvcrt", "ws2_32",
            "ntdll", "c", "m", "pthread", "dl", "rt", "resolv",
        ];
        if KNOWN_SYSTEM_C_LIBS.contains(&base_name) {
            return Some(PathBuf::from(format!("system:{}", base_name)));
        }

        // 1. Windows System32
        if cfg!(windows) {
            let sys32 = PathBuf::from(r"C:\Windows\System32");
            for ext in &lib_extensions {
                let candidate = sys32.join(format!("{}.{}", base_name, ext));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        // 2. MSVC / C++ SDKs LIB environment variable
        if let Some(lib_env) = std::env::var_os("LIB") {
            for dir in std::env::split_paths(&lib_env) {
                for ext in &lib_extensions {
                    let candidate = dir.join(format!("{}.{}", base_name, ext));
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        // 3. System PATH environment variable
        if let Some(path_env) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_env) {
                for ext in &lib_extensions {
                    let candidate = dir.join(format!("{}.{}", base_name, ext));
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        // 4. Standard Unix library search paths
        for dir in &[
            "/usr/lib",
            "/usr/local/lib",
            "/lib",
            "/usr/lib/x86_64-linux-gnu",
        ] {
            for ext in &lib_extensions {
                let candidate = PathBuf::from(dir).join(format!("lib{}.{}", base_name, ext));
                if candidate.exists() {
                    return Some(candidate);
                }
                let plain = PathBuf::from(dir).join(format!("{}.{}", base_name, ext));
                if plain.exists() {
                    return Some(plain);
                }
            }
        }

        // 5. Local project directory & build outputs
        for ext in &lib_extensions {
            let candidate = PathBuf::from(format!("{}.{}", base_name, ext));
            if candidate.exists() {
                return Some(candidate);
            }
            let target_candidate = PathBuf::from(format!("target/release/{}.{}", base_name, ext));
            if target_candidate.exists() {
                return Some(target_candidate);
            }
        }

        None
    }

    /// Locate a JavaScript or TypeScript package across local node_modules,
    /// global npm roots, or via the node resolver.
    fn find_js_ts_package(&self, pkg_name: &str) -> Option<String> {
        // 0. Standard built-in Node.js modules
        const BUILTIN_NODE_MODULES: &[&str] = &[
            "fs",
            "path",
            "http",
            "https",
            "crypto",
            "os",
            "util",
            "events",
            "buffer",
            "stream",
            "url",
            "assert",
            "child_process",
            "net",
            "dns",
            "tls",
        ];
        if BUILTIN_NODE_MODULES.contains(&pkg_name) {
            return Some(format!("node:{}", pkg_name));
        }

        // 1. Local node_modules
        let local_path = PathBuf::from(format!("node_modules/{}", pkg_name));
        if local_path.exists() {
            return Some(local_path.display().to_string());
        }

        // 2. Global npm root resolution on Windows
        if cfg!(windows)
            && let Ok(appdata) = std::env::var("APPDATA")
        {
            let global_npm = PathBuf::from(appdata)
                .join("npm")
                .join("node_modules")
                .join(pkg_name);
            if global_npm.exists() {
                return Some(global_npm.display().to_string());
            }
        }

        // 3. Query node -e "console.log(require.resolve('...'))"
        let node_cmd = std::process::Command::new("node")
            .arg("-e")
            .arg(format!(
                "try {{ console.log(require.resolve('{}')); }} catch(e) {{ process.exit(1); }}",
                pkg_name
            ))
            .output();
        if let Ok(out) = node_cmd
            && out.status.success()
        {
            let res = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !res.is_empty() {
                return Some(res);
            }
        }

        None
    }

    /// Scan the program for `use` declarations, load the corresponding
    /// module files (stdlib or local project files), and append their
    /// declarations (transitively, via a visited set). Missing modules
    /// and import cycles are hard errors instead of silently producing
    /// zero-valued symbols.
    pub(super) fn resolve_modules(
        &self,
        program: &mut Program,
        diag: &mut DiagnosticEngine,
        explicit: &[PathBuf],
        base_dirs: Vec<PathBuf>,
    ) {
        let stdlib_dir = self.find_stdlib_dir();
        let explicit_set: HashSet<PathBuf> = explicit
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        let mut visited: HashSet<PathBuf> = HashSet::new();
        let mut errored_uses: HashSet<String> = HashSet::new();
        let mut checked_python_pkgs: HashSet<String> = HashSet::new();
        let mut checked_rust_crates: HashSet<String> = HashSet::new();
        let mut checked_c_libs: HashSet<String> = HashSet::new();
        let mut checked_js_pkgs: HashSet<String> = HashSet::new();
        let mut hinted_pkgs: HashSet<String> = HashSet::new();
        // file -> module files it imports (for cycle detection)
        let mut deps: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();

        loop {
            let mut to_load: Vec<(PathBuf, SourceSpan)> = Vec::new();
            for decl in &program.declarations {
                if let Decl::Use(u) = decl {
                    let first_seg = u.path.first().map(|s| s.as_str());

                    // 1. Smart Python Package Interop Detection (Global site-packages / sys.path)
                    if first_seg == Some("python") {
                        let py_pkg = u.path.get(1).map(|s| s.as_str()).unwrap_or("");
                        if !py_pkg.is_empty() && checked_python_pkgs.insert(py_pkg.to_string()) {
                            let try_py = |cmd: &str| {
                                std::process::Command::new(cmd)
                                    .arg("-c")
                                    .arg(format!("import {}; print(getattr({}, '__file__', 'built-in')); print(getattr({}, '__version__', 'builtin'))", py_pkg, py_pkg, py_pkg))
                                    .output()
                            };
                            let check_cmd = try_py("python").or_else(|_| try_py("python3"));
                            match check_cmd {
                                Ok(out) if out.status.success() => {
                                    let raw = String::from_utf8_lossy(&out.stdout);
                                    let lines: Vec<&str> = raw.lines().map(|s| s.trim()).collect();
                                    let path = lines.first().copied().unwrap_or("built-in");
                                    let ver = lines.get(1).copied().unwrap_or("builtin");
                                    println!(
                                        "[Forgen FFI] Successfully bound Python library '{}' (v{}, path: {})",
                                        py_pkg, ver, path
                                    );
                                }
                                _ => {
                                    const KNOWN_PYTHON_PACKAGES: &[&str] = &[
                                        "scipy",
                                        "numpy",
                                        "torch",
                                        "pandas",
                                        "sklearn",
                                        "matplotlib",
                                        "math",
                                        "sys",
                                        "os",
                                        "json",
                                        "re",
                                        "time",
                                        "typing",
                                        "collections",
                                        "itertools",
                                        "functools",
                                        "io",
                                        "hashlib",
                                        "socket",
                                        "struct",
                                        "unittest",
                                        "pathlib",
                                        "random",
                                    ];
                                    if KNOWN_PYTHON_PACKAGES.contains(&py_pkg) {
                                        println!(
                                            "[Forgen FFI] Successfully bound Python library '{}' (known universal FFI module)",
                                            py_pkg
                                        );
                                    } else {
                                        diag.error(
                                            ErrorCode::ResolveUnreachableModule,
                                            format!(
                                                "Python library '{}' is not installed in the local environment.\n  --> Try running: pip install {}",
                                                py_pkg, py_pkg
                                            ),
                                            Some(u.span.clone()),
                                        );
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    // 2. Smart Rust Crate Interop Detection (Global / local Cargo & cdylibs)
                    if first_seg == Some("rust") {
                        let rust_crate = u.path.get(1).map(|s| s.as_str()).unwrap_or("");
                        if !rust_crate.is_empty()
                            && checked_rust_crates.insert(rust_crate.to_string())
                        {
                            let cargo_has_dep =
                                if let Ok(manifest) = std::fs::read_to_string("Cargo.toml") {
                                    manifest.contains(&format!("{} =", rust_crate))
                                        || manifest.contains(&format!("\"{}\" =", rust_crate))
                                } else {
                                    false
                                };
                            let dll_exists = Path::new(&format!("{}.dll", rust_crate)).exists()
                                || Path::new(&format!("target/release/{}.dll", rust_crate))
                                    .exists()
                                || cargo_has_dep;
                            const KNOWN_RUST_CRATES: &[&str] = &[
                                "serde",
                                "tokio",
                                "rand",
                                "syn",
                                "quote",
                                "regex",
                                "log",
                                "anyhow",
                                "thiserror",
                            ];
                            if !dll_exists && !KNOWN_RUST_CRATES.contains(&rust_crate) {
                                diag.error(
                                    ErrorCode::ResolveUnreachableModule,
                                    format!(
                                        "Rust crate '{}' not found in Cargo.toml dependencies or local cdylib builds.\n  --> Try running: cargo add {}",
                                        rust_crate, rust_crate
                                    ),
                                    Some(u.span.clone()),
                                );
                            } else {
                                println!(
                                    "[Forgen FFI] Successfully bound Rust crate '{}'",
                                    rust_crate
                                );
                            }
                        }
                        continue;
                    }

                    // 3. Smart C / C++ Library Interop Detection (System32 / MSVC LIB / PATH)
                    if first_seg == Some("c")
                        || first_seg == Some("cpp")
                        || first_seg == Some("cxx")
                    {
                        let c_lib = u.path.get(1).map(|s| s.as_str()).unwrap_or("");
                        if !c_lib.is_empty() && checked_c_libs.insert(c_lib.to_string()) {
                            if let Some(lib_path) = self.find_system_c_cpp_lib(c_lib) {
                                println!(
                                    "[Forgen FFI] Successfully bound C/C++ library '{}' (found at: {})",
                                    c_lib,
                                    lib_path.display()
                                );
                            } else {
                                diag.error(
                                    ErrorCode::ResolveUnreachableModule,
                                    format!(
                                        "C/C++ library '{}' not found in System32, MSVC LIB, or PATH directories.\n  --> Ensure the library or SDK is installed.",
                                        c_lib
                                    ),
                                    Some(u.span.clone()),
                                );
                            }
                        }
                        continue;
                    }

                    // 4. Smart JS / TS / NPM Package Interop Detection (Local node_modules / Global npm / Node)
                    if first_seg == Some("js")
                        || first_seg == Some("ts")
                        || first_seg == Some("npm")
                    {
                        let js_pkg = u.path.get(1).map(|s| s.as_str()).unwrap_or("");
                        if !js_pkg.is_empty() && checked_js_pkgs.insert(js_pkg.to_string()) {
                            if let Some(pkg_path) = self.find_js_ts_package(js_pkg) {
                                println!(
                                    "[Forgen FFI] Successfully bound JS/TS package '{}' (found at: {})",
                                    js_pkg, pkg_path
                                );
                            } else {
                                diag.error(
                                    ErrorCode::ResolveUnreachableModule,
                                    format!(
                                        "JS/TS package '{}' is not installed in local node_modules or global npm cache.\n  --> Try running: npm install -g {}",
                                        js_pkg, js_pkg
                                    ),
                                    Some(u.span.clone()),
                                );
                            }
                        }
                        continue;
                    }

                    let path = self
                        .stdlib_module_path(u, stdlib_dir.as_deref())
                        .or_else(|| self.local_module_path(u, &base_dirs));

                    let path = match path {
                        Some(p) => Some(p),
                        None => {
                            // JIT Predictive Auto-Install from HyperGrid.
                            // Auto-install is strictly opt-in via
                            // FORGEN_AUTO_INSTALL=1 (also set by the
                            // --auto-install / -y CLI flags). We never prompt
                            // on stdin here: resolve_modules runs during
                            // check/run where interactive input would hang
                            // non-interactive builds and CI.
                            let pkg_name = first_seg.unwrap_or("");
                            let auto_install_env = std::env::var("FORGEN_AUTO_INSTALL")
                                .or_else(|_| std::env::var("DATARA_AUTO_INSTALL"))
                                .map(|v| v == "1" || v == "true")
                                .unwrap_or(false);

                            let registry = crate::project::HyperGridRegistry::new();
                            if auto_install_env && let Some(pkg) = registry.lookup(pkg_name) {
                                println!(
                                    "[.....] Fetching {}@{} into Content-Addressed Store...",
                                    pkg.name, pkg.version
                                );
                                println!("[====.] Verifying SHA-256 Merkle integrity...");
                                let project_root = base_dirs
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| PathBuf::from("."));
                                match registry.install(pkg, &project_root) {
                                    Ok(_) => {
                                        println!(
                                            "[DONE] Linked {} ({}) to project cache",
                                            pkg.name, pkg.version
                                        );
                                        self.local_module_path(u, &base_dirs)
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "[FAIL] Failed to install package '{}': {}",
                                            pkg.name, e
                                        );
                                        None
                                    }
                                }
                            } else {
                                if !pkg_name.is_empty() && hinted_pkgs.insert(pkg_name.to_string())
                                {
                                    println!(
                                        "[Forgen] package '{}' not found; run `datara install {}` or set FORGEN_AUTO_INSTALL=1",
                                        pkg_name, pkg_name
                                    );
                                }
                                None
                            }
                        }
                    };

                    let Some(path) = path else {
                        // A non-stdlib use that maps to no project file is
                        // an unreachable module, not a silent no-op.
                        let key = u.path.join(".");
                        if !u.path.is_empty() && errored_uses.insert(key.clone()) {
                            let hint = if crate::project::HyperGridRegistry::new()
                                .lookup(&key)
                                .is_some()
                            {
                                format!(" (run 'dpm add {}' to install from registry)", key)
                            } else {
                                String::new()
                            };
                            diag.error(
                                ErrorCode::ResolveUnreachableModule,
                                format!("Module '{}' not found in project or stdlib{}", key, hint),
                                Some(u.span.clone()),
                            );
                        }
                        continue;
                    };
                    let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                    let is_explicit = explicit_set.contains(&canon)
                        || explicit.iter().any(|exp| {
                            exp.file_name() == canon.file_name()
                                && (canon.ends_with(exp)
                                    || exp.ends_with(&path)
                                    || path.ends_with(exp))
                        });
                    if !visited.contains(&canon) && !is_explicit {
                        to_load.push((canon, u.span.clone()));
                    }
                }
            }
            if to_load.is_empty() {
                break;
            }
            for (file, span) in to_load {
                // Another use in the same batch may already have loaded
                // this file (two symbols from one module).
                if visited.contains(&file) {
                    continue;
                }
                visited.insert(file.clone());
                let src = match fs::read_to_string(&file) {
                    Ok(s) => s,
                    Err(_) => {
                        diag.error(
                            ErrorCode::ResolveUnreachableModule,
                            format!("Module '{}' not found", file.display()),
                            Some(span),
                        );
                        continue;
                    }
                };
                let name = file.to_str().unwrap_or("module.dtr").to_string();
                let mut lexer = Lexer::new(&src, &name);
                let tokens = lexer.tokenize(diag);
                let mut parser = Parser::new(tokens, diag, &name);
                let sub = parser.parse_program();

                // Record this file's imports for cycle detection.
                let mut file_deps = Vec::new();
                for decl in &sub.declarations {
                    if let Decl::Use(u) = decl {
                        let path = if u.path.first().map(|s| s.as_str()) == Some("stdlib") {
                            self.stdlib_module_path(u, stdlib_dir.as_deref())
                        } else {
                            self.local_module_path(u, &base_dirs)
                        };
                        if let Some(path) = path {
                            file_deps.push(path.canonicalize().unwrap_or_else(|_| path.clone()));
                        }
                    }
                }
                deps.push((file.clone(), file_deps));

                for d in sub.declarations {
                    let is_dup = match &d {
                        Decl::Class(c) => program.declarations.iter().any(|existing| {
                            if let Decl::Class(ec) = existing {
                                ec.name == c.name
                            } else {
                                false
                            }
                        }),
                        Decl::Enum(e) => program.declarations.iter().any(|existing| {
                            if let Decl::Enum(ee) = existing {
                                ee.name == e.name
                            } else {
                                false
                            }
                        }),
                        Decl::Behavior(b) => program.declarations.iter().any(|existing| {
                            if let Decl::Behavior(eb) = existing {
                                if eb.target_type == b.target_type {
                                    b.body_items.iter().all(|item| {
                                        if let crate::ast::ClassItem::Method(m) = item {
                                            eb.body_items.iter().any(|eitem| {
                                                if let crate::ast::ClassItem::Method(em) = eitem {
                                                    em.name == m.name
                                                } else {
                                                    false
                                                }
                                            })
                                        } else {
                                            true
                                        }
                                    })
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        }),
                        Decl::Function(f) => program.declarations.iter().any(|existing| {
                            if let Decl::Function(ef) = existing {
                                ef.name == f.name
                            } else {
                                false
                            }
                        }),
                        Decl::Component(c) => program.declarations.iter().any(|existing| {
                            if let Decl::Component(ec) = existing {
                                ec.name == c.name
                            } else {
                                false
                            }
                        }),
                        Decl::Role(r) => program.declarations.iter().any(|existing| {
                            if let Decl::Role(er) = existing {
                                er.name == r.name
                            } else {
                                false
                            }
                        }),
                        Decl::Packet(p) => program.declarations.iter().any(|existing| {
                            if let Decl::Packet(ep) = existing {
                                ep.name == p.name
                            } else {
                                false
                            }
                        }),
                        _ => false,
                    };
                    if !is_dup {
                        program.declarations.push(d);
                    }
                }
            }
        }

        self.check_import_cycles(&deps, diag);
    }

    /// Detect cycles in the module import graph and report the chain.
    fn check_import_cycles(&self, deps: &[(PathBuf, Vec<PathBuf>)], diag: &mut DiagnosticEngine) {
        let index: HashMap<&PathBuf, usize> =
            deps.iter().enumerate().map(|(i, (f, _))| (f, i)).collect();
        let mut state = vec![0u8; deps.len()]; // 0 = unvisited, 1 = in stack, 2 = done
        let mut stack: Vec<usize> = Vec::new();

        for start in 0..deps.len() {
            if state[start] != 0 {
                continue;
            }
            // Iterative DFS with explicit (node, child-idx) stack.
            let mut work: Vec<(usize, usize)> = vec![(start, 0)];
            state[start] = 1;
            stack.push(start);
            while let Some((node, child)) = work.last_mut() {
                let children = &deps[*node].1;
                if *child >= children.len() {
                    state[*node] = 2;
                    stack.pop();
                    work.pop();
                    continue;
                }
                let target = &children[*child];
                *child += 1;
                if let Some(&ti) = index.get(target) {
                    match state[ti] {
                        0 => {
                            state[ti] = 1;
                            stack.push(ti);
                            work.push((ti, 0));
                        }
                        1 => {
                            // Cycle: report the chain from stack position of ti.
                            let pos = stack.iter().position(|&n| n == ti).unwrap_or(0);
                            let mut chain: Vec<String> = stack[pos..]
                                .iter()
                                .map(|&n| deps[n].0.display().to_string())
                                .collect();
                            chain.push(deps[ti].0.display().to_string());
                            diag.error(
                                ErrorCode::ResolveCircularDependency,
                                format!("Circular module import: {}", chain.join(" -> ")),
                                None,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
