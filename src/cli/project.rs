//! Project-lifecycle commands: init/new, clean and tree.

use crate::project::{ProjectDiscovery, ProjectInitializer};
use std::fs;
use std::path::Path;
use std::time::Instant;

/// `forgen init` / `new` — scaffold a new project.
pub(crate) fn cmd_init(args: &[String]) -> bool {
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
    true
}

/// `forgen clean` — remove target/ outputs, caches and toolchain-owned
/// artifacts (conservative: only files with a matching .dtr stem are deleted).
pub(crate) fn cmd_clean(args: &[String]) -> bool {
    let start = Instant::now();
    let is_all = args.iter().any(|a| a == "--all");
    let is_pgo = args.iter().any(|a| a == "--pgo");
    let is_llvm = args.iter().any(|a| a == "--llvm");
    let mut removed_count = 0usize;
    let mut freed_bytes = 0u64;

    // Target directory
    if !is_pgo {
        let target_dir = Path::new("target");
        if target_dir.exists()
            && let Ok(entries) = fs::read_dir(target_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_llvm {
                    if let Some(ext) = path.extension().and_then(|s| s.to_str())
                        && matches!(ext, "ll" | "bc" | "obj" | "s")
                    {
                        if let Ok(meta) = entry.metadata() {
                            freed_bytes += meta.len();
                        }
                        let _ = fs::remove_file(&path);
                        removed_count += 1;
                    }
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    freed_bytes += meta.len();
                }
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
                removed_count += 1;
            }
        }
    }

    let inc_dir = Path::new(".forgen_cache");
    if inc_dir.exists() {
        let _ = fs::remove_dir_all(inc_dir);
    }

    // Also clean local build artifacts like *.exe, *.ll, *.pdb, *.pgo
    // in the current dir. SAFETY: only files the toolchain provably
    // created are deleted — a root artifact must have an accompanying
    // .dtr source with the SAME stem (project root or src/), so user
    // files that merely share an extension are never touched. Each
    // deleted path is printed to keep the operation auditable.
    let has_same_stem_dtr = |path: &Path| -> bool {
        let Some(raw_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        // "foo.dtr.exe" outputs correspond to the source "foo.dtr".
        let stem = raw_stem.strip_suffix(".dtr").unwrap_or(raw_stem);
        Path::new(&format!("{}.dtr", stem)).exists()
            || Path::new("src").join(format!("{}.dtr", stem)).exists()
    };

    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let is_pgo_file = path.extension().and_then(|s| s.to_str()) == Some("pgo");

            if (is_all || is_pgo) && is_pgo_file {
                // "datara.pgo" is the toolchain's own fixed profile
                // name; any other *.pgo file must prove ownership via
                // an accompanying .dtr with the same stem.
                if file_name != "datara.pgo" && !has_same_stem_dtr(&path) {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    freed_bytes += meta.len();
                }
                let _ = fs::remove_file(&path);
                println!("[Forgen clean] deleted {}", path.display());
                removed_count += 1;
                continue;
            }

            if is_pgo {
                continue;
            }

            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let matches_filter = if is_llvm {
                    matches!(ext, "ll" | "bc" | "obj")
                } else {
                    matches!(ext, "exe" | "ll" | "pdb" | "obj" | "dtr.exe")
                };

                if matches_filter {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                        && (stem == "forgen"
                            || stem == "datara"
                            || stem == "dpm"
                            || stem == "cargo"
                            || stem == "rustc")
                    {
                        continue;
                    }
                    if !has_same_stem_dtr(&path) {
                        continue;
                    }
                    if let Ok(meta) = entry.metadata() {
                        freed_bytes += meta.len();
                    }
                    let _ = fs::remove_file(&path);
                    println!("[Forgen clean] deleted {}", path.display());
                    removed_count += 1;
                }
            }
        }
    }

    let freed_mb = freed_bytes as f64 / (1024.0 * 1024.0);
    let elapsed = start.elapsed().as_millis();
    let mode_str = if is_pgo {
        " (PGO cache)"
    } else if is_llvm {
        " (LLVM intermediates)"
    } else if is_all {
        " (all caches and artifacts)"
    } else {
        ""
    };
    println!(
        "[Forgen clean] Removed {} build artifacts ({:.2} MB freed){} in {}ms",
        removed_count, freed_mb, mode_str, elapsed
    );
    true
}

/// `forgen tree` — visualize the dependency tree (with optional heuristic
/// effect badges).
pub(crate) fn cmd_tree(args: &[String]) -> bool {
    let show_effects = args.iter().any(|a| a == "--effects");
    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("-"))
        .map(Path::new);

    let target_path = target_opt.unwrap_or(Path::new("."));
    let (pkg_name, pkg_version, manifest_opt) = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => {
            let name = l
                .manifest
                .as_ref()
                .map(|m| m.package.name.clone())
                .unwrap_or(l.name.clone());
            let version = l
                .manifest
                .as_ref()
                .map(|m| m.package.version.clone())
                .unwrap_or("0.1.0".into());
            (name, version, l.manifest)
        }
        Err(_) => {
            let manifest_file = target_path.join("datara.toml");
            if manifest_file.exists() {
                if let Ok(m) = crate::project::manifest::DataraManifest::from_file(&manifest_file) {
                    (m.package.name.clone(), m.package.version.clone(), Some(m))
                } else {
                    eprintln!(
                        "Tree error: Failed to parse 'datara.toml' in '{}'",
                        target_path.display()
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!(
                    "Tree error: No Datara project or .dtr files found in '{}'. Run 'forgen new <name>' to create one.",
                    target_path.display()
                );
                std::process::exit(1);
            }
        }
    };

    println!("{} v{}", pkg_name, pkg_version);
    if let Some(manifest) = &manifest_opt {
        // `dependencies` is a HashMap whose iteration order differs
        // between runs; sort by dependency name so `forgen tree`
        // output is deterministic.
        let mut deps: Vec<_> = manifest.dependencies.iter().collect();
        deps.sort_by(|a, b| a.0.cmp(b.0));
        if deps.is_empty() {
            println!("└── (no external dependencies in datara.toml)");
        } else {
            for (i, (dep_name, dep_spec)) in deps.iter().enumerate() {
                let is_last = i == deps.len() - 1;
                let branch = if is_last { "└── " } else { "├── " };
                let version = match dep_spec {
                    crate::project::manifest::DependencyConfig::Simple(v) => v.as_str(),
                    crate::project::manifest::DependencyConfig::Detailed { version, .. } => {
                        version.as_deref().unwrap_or("latest")
                    }
                };
                let effects_badge = if show_effects {
                    // These tags are guessed from name substrings, not
                    // derived from real effect analysis — say so.
                    match dep_name.as_str() {
                        s if s.contains("net") || s.contains("http") => " [io, net] (heuristic)",
                        s if s.contains("fs") || s.contains("io") => " [io] (heuristic)",
                        s if s.contains("math") || s.contains("crypto") => " [pure] (heuristic)",
                        _ => " [pure] (heuristic)",
                    }
                } else {
                    ""
                };
                println!(
                    "{}{}{} (v{}){}",
                    branch,
                    dep_name,
                    effects_badge,
                    version,
                    if show_effects && effects_badge.contains("net") {
                        " ⚠️ requires network"
                    } else {
                        ""
                    }
                );
            }
        }
    } else {
        println!("└── (no external dependencies in datara.toml)");
    }
    true
}
