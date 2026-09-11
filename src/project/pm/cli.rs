use super::manifest::*;
use super::registry::*;
use super::verify::*;
use crate::project::manifest::DataraManifest;
use std::fs;
use std::path::{Path, PathBuf};

/// DPM (Datara Package Manager) CLI Entry Point
pub fn run_dpm_cli() {
    let args: Vec<String> = std::env::args().collect();
    run_dpm_cli_args(&args);
}

/// Router for DPM commands
pub fn run_dpm_cli_args(args: &[String]) {
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let mut registry = HyperGridRegistry::new();
    let current_dir = Path::new(".");

    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_default();
    let is_sparks = exe_name.eq_ignore_ascii_case("sparks")
        || args.first().map(|s| s.contains("sparks")).unwrap_or(false);

    let is_offline = args.iter().any(|a| a == "--offline");
    if is_offline {
        registry.set_offline(true);
    }
    if let Some(pos) = args.iter().position(|a| a == "--registry") {
        if let Some(url) = args.get(pos + 1) {
            registry.set_registry_url(url);
        }
    }

    match cmd {
        "help" | "--help" | "-h" => {
            if is_sparks {
                print_sparks_help();
            } else {
                print_dpm_help();
            }
        }

        "version" | "--version" | "-V" | "-v" => {
            if is_sparks {
                println!(
                    "sparks {} (Datara Package & Sparks Manager)",
                    env!("CARGO_PKG_VERSION")
                );
                println!("Registry: Sparks Decentralized Capability Grid");
                println!(
                    "Endpoint: {}",
                    std::env::var("DATARA_SPARKS_REGISTRY")
                        .unwrap_or_else(|_| "https://waters1ze.github.io/sparks".to_string())
                );
            } else {
                println!("dpm {} (Datara Package Manager)", env!("CARGO_PKG_VERSION"));
                println!("Engine: HyperGrid CAS v2 / Forgen Toolchain");
            }
        }

        "self-update" | "check-update" => {
            println!(
                ":: [{}] Checking for Datara & Sparks toolchain updates...",
                if is_sparks { "SPARKS" } else { "DPM" }
            );
            crate::update::run_check_update_command();
        }

        "init" => {
            let name_arg = args
                .get(2)
                .filter(|s| !s.starts_with("-"))
                .map(|s| s.as_str());
            let is_lib = args.iter().any(|a| a == "--lib");
            let project_name = name_arg.unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "my_app".into())
                    .leak()
            });

            println!(
                ":: [DPM] Initializing Datara {} '{}'...",
                if is_lib { "library" } else { "project" },
                project_name
            );
            match HyperGridRegistry::init_project(current_dir, project_name, is_lib) {
                Ok(_) => {
                    println!("[DONE] Created datara.toml");
                    println!(
                        "[DONE] Created src/{}",
                        if is_lib { "lib.dtr" } else { "main.dtr" }
                    );
                    println!("[DONE] Created .gitignore");
                    println!("\n  Ready to build! Run: dpm run");
                }
                Err(e) => {
                    eprintln!("[ERR] Failed to init project: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "add" => {
            let target_arg = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!(
                        "Usage: dpm add <package_name> [--git <url>] [--tarball <url> --sha256 <hash>] [--offline]"
                    );
                    std::process::exit(1);
                }
            };

            let tarball_pos = args.iter().position(|a| a == "--tarball");
            let sha256_pos = args.iter().position(|a| a == "--sha256");

            let (pkg_name, git_url) = if target_arg.starts_with("http://")
                || target_arg.starts_with("https://")
                || target_arg.starts_with("git@")
            {
                let name = target_arg
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .split('/')
                    .next_back()
                    .unwrap_or("pkg")
                    .to_string();
                (name, Some(target_arg.to_string()))
            } else if let Some(git_pos) = args.iter().position(|a| a == "--git") {
                let url = args.get(git_pos + 1).cloned();
                (target_arg.to_string(), url)
            } else {
                (target_arg.to_string(), None)
            };

            let (clean_pkg_name, version_req) = if let Some((base, ver)) = pkg_name.split_once('@')
            {
                (base.to_string(), Some(ver.to_string()))
            } else if let Some(ver_pos) = args.iter().position(|a| a == "--version") {
                (pkg_name.clone(), args.get(ver_pos + 1).cloned())
            } else {
                (pkg_name.clone(), None)
            };

            println!(":: [DPM] Resolving package '{}'...", clean_pkg_name);

            // Package names become path components everywhere below
            // (packages/<name>, CAS store); reject anything path-like.
            if !HyperGridRegistry::is_valid_package_name(&clean_pkg_name) {
                eprintln!(
                    "[ERR] Invalid package name '{}': must be a plain identifier without path separators or '..'",
                    clean_pkg_name
                );
                std::process::exit(1);
            }

            if clean_pkg_name.starts_with("sparks/") {
                let has_explicit_reg = args.iter().any(|a| a == "--registry");
                let reg_url = if has_explicit_reg {
                    registry.registry_url.clone()
                } else {
                    std::env::var("DATARA_SPARKS_REGISTRY")
                        .unwrap_or_else(|_| "https://waters1ze.github.io/sparks".to_string())
                };
                let ver_display = version_req.as_deref().unwrap_or("latest");
                println!(
                    ":: [SPARKS] Installing capability-verified package '{}' (v{}) from '{}'...",
                    clean_pkg_name, ver_display, reg_url
                );
                match registry.fetch_and_install_sparks(
                    &clean_pkg_name,
                    version_req.as_deref(),
                    &reg_url,
                    current_dir,
                ) {
                    Ok(path) => {
                        println!("[DONE] Installed {} -> {}", clean_pkg_name, path.display());
                        println!("[OK] Cryptographic ed25519 signature & SHA-256 verified.");
                        record_sparks_telemetry(&clean_pkg_name, version_req.as_deref());
                        return;
                    }
                    Err(e) => {
                        eprintln!("[FAIL] Sparks installation failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            if let Some(pos) = tarball_pos {
                let url = args.get(pos + 1).cloned().unwrap_or_default();
                let expected_sha = sha256_pos
                    .and_then(|p| args.get(p + 1))
                    .cloned()
                    .unwrap_or_default();
                println!(":: [DPM] Fetching tarball from '{}'...", url);
                match registry.fetch_and_install_tarball(
                    &pkg_name,
                    "0.1.0",
                    &url,
                    &expected_sha,
                    current_dir,
                ) {
                    Ok(_) => {
                        println!("[DONE] Installed {} from tarball", pkg_name);
                        return;
                    }
                    Err(e) => {
                        eprintln!("[FAIL] {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(pkg) = registry.lookup(&pkg_name) {
                println!(
                    "[.....] Fetching {}@{} into Content-Addressed Store...",
                    pkg.name, pkg.version
                );
                println!(
                    "[====.] Verifying SHA-256 Merkle integrity ({})",
                    &pkg.digest[0..16.min(pkg.digest.len())]
                );
                match registry.install(pkg, current_dir) {
                    Ok(_) => {
                        println!(
                            "[DONE] Installed {} (v{}) -> packages/{}",
                            pkg.name, pkg.version, pkg.name
                        );
                        println!("[OK] Recorded dependency in datara.toml & datara.lock");
                    }
                    Err(e) => {
                        eprintln!("[FAIL] Installation failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(ref url) = git_url {
                let packages_dir = current_dir.join("packages");
                let _ = fs::create_dir_all(&packages_dir);
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
                            println!("[DONE] Downloaded '{}' -> packages/{}", pkg_name, pkg_name);
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

                let manifest_path = current_dir.join("datara.toml");
                let mut content = if manifest_path.exists() {
                    fs::read_to_string(&manifest_path).unwrap_or_default()
                } else {
                    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n".to_string()
                };
                if !content.contains("[dependencies]") {
                    content.push_str("\n[dependencies]\n");
                }
                if !content.contains(&format!("{} =", pkg_name))
                    && !content.contains(&format!("\"{}\" =", pkg_name))
                {
                    content.push_str(&format!("{} = {{ git = \"{}\" }}\n", pkg_name, url));
                    let _ = fs::write(&manifest_path, content);
                    println!("[OK] Recorded git dependency in datara.toml");
                }
            } else {
                let sparks_candidate = format!("sparks/{}", clean_pkg_name);
                let reg_url = std::env::var("DATARA_SPARKS_REGISTRY")
                    .unwrap_or_else(|_| "https://waters1ze.github.io/sparks".to_string());
                println!(
                    ":: [SPARKS] Looking up '{}' in Sparks registry ('{}')...",
                    sparks_candidate, reg_url
                );
                match registry.fetch_and_install_sparks(
                    &sparks_candidate,
                    version_req.as_deref(),
                    &reg_url,
                    current_dir,
                ) {
                    Ok(path) => {
                        println!(
                            "[DONE] Installed {} -> {}",
                            sparks_candidate,
                            path.display()
                        );
                        println!("[OK] Cryptographic ed25519 signature & SHA-256 verified.");
                        record_sparks_telemetry(&sparks_candidate, version_req.as_deref());
                    }
                    Err(_) => {
                        eprintln!(
                            "[ERR] Package '{}' not found in local registry or Sparks ('{}').\n      Run 'dpm search <query>' or use 'dpm add --git <url>'",
                            pkg_name, reg_url
                        );
                        std::process::exit(1);
                    }
                }
            }
        }

        "remove" | "rm" => {
            let pkg_name = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!("Usage: dpm remove <package_name>");
                    std::process::exit(1);
                }
            };
            println!(":: [DPM] Removing package '{}'...", pkg_name);
            match registry.remove(pkg_name, current_dir) {
                Ok(true) => {
                    println!("[DONE] Removed packages/{}", pkg_name);
                    println!("[OK] Synchronized datara.toml & datara.lock");
                }
                Ok(false) => {
                    println!("[INFO] Package was not in packages/, cleaned manifest & lockfile.");
                }
                Err(e) => {
                    eprintln!("[ERR] Failed to remove package: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "install" | "i" | "restore" => {
            if let Some(target) = args.get(2).filter(|s| !s.starts_with("-")) {
                let actual_target = if target.starts_with("sparks/") {
                    target.to_string()
                } else if is_sparks || registry.lookup(target).is_none() {
                    format!("sparks/{}", target)
                } else {
                    target.to_string()
                };

                if actual_target.starts_with("sparks/") {
                    let has_explicit_reg = args.iter().any(|a| a == "--registry");
                    let reg_url = if has_explicit_reg {
                        registry.registry_url.clone()
                    } else {
                        std::env::var("DATARA_SPARKS_REGISTRY")
                            .unwrap_or_else(|_| "https://waters1ze.github.io/sparks".to_string())
                    };
                    println!(
                        ":: [SPARKS] Installing capability-verified package '{}' from '{}'...",
                        actual_target, reg_url
                    );
                    match registry.fetch_and_install_sparks(
                        &actual_target,
                        None,
                        &reg_url,
                        current_dir,
                    ) {
                        Ok(path) => {
                            println!("[DONE] Installed {} -> {}", actual_target, path.display());
                            println!("[OK] Cryptographic ed25519 signature & SHA-256 verified.");
                            record_sparks_telemetry(&actual_target, None);
                            return;
                        }
                        Err(e) => {
                            eprintln!("[FAIL] Sparks installation failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else if let Some(pkg) = registry.lookup(target) {
                    println!("[.....] Installing {} (v{})...", pkg.name, pkg.version);
                    if let Err(e) = registry.install(pkg, current_dir) {
                        eprintln!("[FAIL] Installation failed: {}", e);
                        std::process::exit(1);
                    }
                    println!("[DONE] Installed packages/{}", pkg.name);
                    return;
                }
            }
            println!(":: [DPM] Synchronizing project dependencies...");
            let lock_path = current_dir.join("datara.lock");
            if lock_path.exists() {
                if let Some(lock) = DataraLock::load(current_dir) {
                    println!("[INFO] Restoring pinned dependencies from datara.lock...");
                    match lock.restore(&registry, current_dir) {
                        Ok(restored) => {
                            println!(
                                "[DONE] Synchronized dependencies from datara.lock ({} restored)",
                                restored
                            );
                            return;
                        }
                        Err(e) => {
                            eprintln!("[ERR] Failed to restore from datara.lock: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }

            let manifest_path = current_dir.join("datara.toml");
            if !manifest_path.exists() {
                println!("[INFO] No datara.toml found. Run 'dpm init' to create a project.");
                return;
            }

            let manifest = match DataraManifest::from_file(&manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[ERR] {}", e);
                    std::process::exit(1);
                }
            };

            let mut installed_count = 0;
            for dep_name in manifest.dependencies.keys() {
                let pkg_dir = current_dir.join("packages").join(dep_name);
                if !pkg_dir.exists() {
                    if let Some(pkg) = registry.lookup(dep_name) {
                        println!("[.....] Installing {} (v{})...", pkg.name, pkg.version);
                        if registry.install(pkg, current_dir).is_ok() {
                            println!("[DONE] Installed packages/{}", pkg.name);
                            installed_count += 1;
                        }
                    } else {
                        eprintln!("[WARN] Dependency '{}' not found in registry", dep_name);
                    }
                }
            }
            println!(
                "[DONE] Synchronized dependencies: {} installed, {} up-to-date",
                installed_count,
                manifest.dependencies.len().saturating_sub(installed_count)
            );
        }

        "list" | "ls" => {
            let manifest_path = current_dir.join("datara.toml");
            let proj_name = if manifest_path.exists() {
                DataraManifest::from_file(&manifest_path)
                    .map(|m| format!("{} v{}", m.package.name, m.package.version))
                    .unwrap_or_else(|_| "Project".into())
            } else {
                "Project (no datara.toml)".into()
            };

            println!(":: [DPM] Dependency tree for {}:", proj_name);
            let installed = registry.list_installed(current_dir);
            if installed.is_empty() {
                println!("   (no packages installed in packages/)");
            } else {
                for (i, pkg) in installed.iter().enumerate() {
                    let is_last = i == installed.len() - 1;
                    let branch = if is_last { "└── " } else { "├── " };
                    println!(
                        "{}{} (v{}) [{}]",
                        branch,
                        pkg.name,
                        pkg.version,
                        &pkg.digest[0..15.min(pkg.digest.len())]
                    );
                }
            }
        }

        "verify" | "verify-pkg" => {
            println!(":: [DPM] Verifying package integrity against datara.lock...");
            match registry.verify(current_dir) {
                Ok(results) => {
                    let mut all_valid = true;
                    for r in &results {
                        match r.status {
                            VerificationStatus::Valid => {
                                println!("  [OK] {} (v{}) - {}", r.name, r.version, r.message);
                            }
                            VerificationStatus::Mismatch => {
                                eprintln!("  [FAIL] {} (v{}) - {}", r.name, r.version, r.message);
                                all_valid = false;
                            }
                            VerificationStatus::Missing => {
                                eprintln!(
                                    "  [MISSING] {} (v{}) - {}",
                                    r.name, r.version, r.message
                                );
                                all_valid = false;
                            }
                            VerificationStatus::Untracked => {
                                eprintln!("  [UNTRACKED] {} - {}", r.name, r.message);
                                all_valid = false;
                            }
                        }
                    }
                    if all_valid {
                        println!(
                            "[DONE] All {} packages verified successfully!",
                            results.len()
                        );
                    } else {
                        eprintln!(
                            "[ERR] Package verification failed! Run 'dpm install' to restore pristine files."
                        );
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("[ERR] {}", e);
                    std::process::exit(1);
                }
            }
        }

        "update" | "upgrade" => {
            if args
                .iter()
                .any(|a| a == "--self" || a == "--toolchain" || a == "-s")
            {
                println!(
                    ":: [{}] Checking for Datara & Sparks toolchain updates...",
                    if is_sparks { "SPARKS" } else { "DPM" }
                );
                crate::update::run_check_update_command();
                return;
            }
            crate::update::notify_if_update_available();
            let manifest_path = current_dir.join("datara.toml");
            if !manifest_path.exists() {
                println!(
                    ":: [{}] Checking for Datara & Sparks toolchain updates...",
                    if is_sparks { "SPARKS" } else { "DPM" }
                );
                crate::update::run_check_update_command();
                println!("[INFO] No 'datara.toml' in current directory to update dependencies.");
                return;
            }
            let manifest = match DataraManifest::from_file(&manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[ERR] Failed to read manifest: {}", e);
                    std::process::exit(1);
                }
            };
            let mut lock = DataraLock::load(current_dir).unwrap_or_default();
            println!(
                ":: [DPM] Checking and updating dependencies from HyperGrid and Git sources..."
            );
            let mut updated_count = 0;
            for dep_name in manifest.dependencies.keys() {
                let pkg_dir = current_dir.join("packages").join(dep_name);
                if pkg_dir.join(".git").exists() {
                    println!("[.....] Updating git dependency '{}'...", dep_name);
                    let status = std::process::Command::new("git")
                        .arg("pull")
                        .current_dir(&pkg_dir)
                        .status();
                    if let Ok(s) = status
                        && s.success()
                    {
                        println!("[DONE] Pulled latest git changes for '{}'", dep_name);
                        updated_count += 1;
                    }
                } else if let Some(pkg) = registry.lookup(dep_name) {
                    let current_locked = lock.packages.get(dep_name);
                    let needs_update = current_locked
                        .map(|l| l.version != pkg.version)
                        .unwrap_or(true);
                    if needs_update {
                        println!("[.....] Updating '{}' to v{}...", dep_name, pkg.version);
                        if registry.install(pkg, current_dir).is_ok() {
                            lock.insert_or_update(
                                &pkg.name,
                                &pkg.version,
                                &pkg.digest,
                                "hypergrid",
                                pkg.dependencies.clone(),
                            );
                            println!("[DONE] Updated '{}' to v{}", dep_name, pkg.version);
                            updated_count += 1;
                        }
                    } else {
                        println!(
                            "  • '{}' (v{}) is already up-to-date",
                            dep_name, pkg.version
                        );
                    }
                }
            }
            let _ = lock.save(current_dir);
            println!(
                "[DONE] Dependencies checked ({} updated, datara.lock synchronized)",
                updated_count
            );
        }

        "search" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            let results = registry.search(query);
            println!(":: [DPM] Packages matching '{}':", query);
            if results.is_empty() {
                println!("   (no packages found matching query)");
            } else {
                for p in results {
                    println!("• {:<14} v{:<8} - {}", p.name, p.version, p.description);
                }
            }
        }

        "info" => {
            let pkg_name = match args.get(2) {
                Some(p) => p.as_str(),
                None => {
                    eprintln!("Usage: dpm info <package_name>");
                    std::process::exit(1);
                }
            };
            if let Some(pkg) = registry.lookup(pkg_name) {
                println!(":: [DPM] Package '{}'", pkg.name);
                println!("   Version:      {}", pkg.version);
                println!("   Description:  {}", pkg.description);
                println!("   Author:       {}", pkg.author);
                println!("   License:      {}", pkg.license);
                println!("   Digest:       {}", pkg.digest);
                println!("   Entry:        {}", pkg.entry);
                if !pkg.capabilities.is_empty() {
                    println!(
                        "   Capabilities (detected heuristically): [{}]",
                        pkg.capabilities.join(", ")
                    );
                }
                let f_list: Vec<&String> = pkg.files.keys().collect();
                println!(
                    "   Files:        {}",
                    f_list
                        .into_iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                eprintln!("[ERR] Package '{}' not found in registry", pkg_name);
                std::process::exit(1);
            }
        }

        "publish" => {
            println!(":: [DPM] Publishing package to local & CAS registry...");
            let mut reg = HyperGridRegistry::new();
            match reg.publish(current_dir) {
                Ok(pkg) => {
                    println!("[====.] Generated Merkle digest: {}", pkg.digest);
                    if !pkg.capabilities.is_empty() {
                        println!(
                            "   Detected Capabilities (heuristic): {}",
                            pkg.capabilities.join(", ")
                        );
                    }
                    println!(
                        "[DONE] Package '{}' (v{}) published successfully to DPM Registry",
                        pkg.name, pkg.version
                    );
                }
                Err(e) => {
                    eprintln!("[FAIL] Publish error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "run" => {
            // Forward directly to forgen run
            let mut cmd = std::process::Command::new(
                std::env::current_exe()
                    .ok()
                    .and_then(|p| {
                        p.parent().map(|d| {
                            d.join(if cfg!(windows) {
                                "forgen.exe"
                            } else {
                                "forgen"
                            })
                        })
                    })
                    .unwrap_or_else(|| PathBuf::from("forgen")),
            );
            cmd.arg("run");
            for a in &args[2..] {
                cmd.arg(a);
            }
            if let Ok(mut child) = cmd.spawn() {
                let _ = child.wait();
            } else {
                eprintln!("[ERR] Could not execute 'forgen run'");
            }
        }

        "rust-bridge" | "rust_bridge" => {
            if let Err(e) = crate::rust_bridge::run_rust_bridge_cli(args) {
                eprintln!("[ERR] {}", e);
                std::process::exit(1);
            }
        }

        other => {
            let tool_name = if is_sparks { "sparks" } else { "dpm" };
            eprintln!(
                "Unknown command '{} {}'. Run '{} --help' for available commands.",
                tool_name, other, tool_name
            );
            std::process::exit(1);
        }
    }
}

fn print_sparks_help() {
    println!(
        r#"
  ____  ____   _    ____  _  ______
 / ___||  _ \ / \  |  _ \| |/ / ___|   Sparks Package Manager (v{})
 \___ \| |_) / _ \ | |_) | ' /\___ \   Decentralized Capability Packages
  ___) |  __/ ___ \|  _ <| . \ ___) |  https://waters1ze.github.io/sparks
 |____/|_| /_/   \_\_| \_\_|\_\____/

USAGE:
    sparks <command> [arguments...]

COMMANDS:
    install (add) <name>     Install a capability-verified Sparks package
    remove (rm) <name>       Remove an installed package
    update [--self]          Update project packages or Sparks toolchain itself
    self-update              Check and install latest Datara & Sparks release
    list (ls)                Display installed packages and capability tree
    search <query>           Search available packages in the Sparks registry
    info <name>              Show detailed package capabilities & metadata
    verify                   Verify cryptographic ed25519 signatures & digests
    publish                  Package and publish local library to Sparks registry
    init [name] [--lib]      Initialize a new Datara project with Sparks support
    run [args...]            Compile and execute project entry point

FLAGS:
    -h, --help               Print help information
    -V, --version            Print version information

EXAMPLES:
    sparks install crypto_core
    sparks install math_simd
    sparks update
    sparks self-update
"#,
        env!("CARGO_PKG_VERSION")
    );
}

fn print_dpm_help() {
    println!(
        r#"
  ____  ____  __  __
 |  _ \|  _ \|  \/  |  Datara Package Manager (DPM)
 | | | | |_) | |\/| |  Content-Addressed Merkle Registry
 | |_| |  __/| |  | |  https://github.com/waters1ze/datara
 |____/|_|   |_|  |_|

USAGE:
    dpm <command> [arguments...]

COMMANDS:
    init [name] [--lib]      Initialize a new Datara project or library
    add <name> [--git <url>] Add a package dependency to project & datara.lock
    remove <name>            Remove a package and update datara.toml & datara.lock
    install (i)              Install and synchronize all dependencies in datara.toml
    list (ls)                Display installed packages and version tree
    search <query>           Search available packages in the registry
    info <name>              Show detailed package metadata and capabilities
    verify                   Verify package integrity and cryptographic digests
    publish                  Package and publish local library to the registry
    rust-bridge <crate>      Generate Rust shim cdylib & Datara bindings from manifest
    run [file] [args...]     Compile and execute project entry or file

FLAGS:
    -h, --help               Print help information
    -V, --version            Print version information

EXAMPLES:
    dpm init my_service
    dpm add redis
    dpm add uuid
    dpm list
    dpm verify
"#
    );
}

fn record_sparks_telemetry(pkg_name: &str, _version: Option<&str>) {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let datara_dir = Path::new(&home).join(".datara");
    let _ = fs::create_dir_all(&datara_dir);
    let telem_file = datara_dir.join("sparks_telemetry.json");

    let mut data: serde_json::Value = fs::read_to_string(&telem_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({ "downloads": {}, "last_updated": 0 }));

    if let Some(obj) = data.get_mut("downloads").and_then(|v| v.as_object_mut()) {
        let count = obj.get(pkg_name).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        obj.insert(pkg_name.to_string(), serde_json::json!(count));
    } else {
        let mut map = serde_json::Map::new();
        map.insert(pkg_name.to_string(), serde_json::json!(1));
        data["downloads"] = serde_json::Value::Object(map);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    data["last_updated"] = serde_json::json!(timestamp);

    let _ = fs::write(
        &telem_file,
        serde_json::to_string_pretty(&data).unwrap_or_default(),
    );
}
