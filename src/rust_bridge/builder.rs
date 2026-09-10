use crate::rust_bridge::codegen::{generate_wrapper_cargo_toml, generate_wrapper_lib_rs};
use crate::rust_bridge::config::{ResolvedRustFunction, RustCrateConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the Cargo executable on the host system.
pub fn find_cargo() -> Option<PathBuf> {
    if let Ok(c) = std::env::var("CARGO") {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(userprofile)
            .join(".cargo")
            .join("bin")
            .join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".cargo").join("bin").join("cargo");
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Check if the compiled static library is fresh relative to inputs.
fn is_artifact_fresh(staticlib_path: &Path, wrapper_dir: &Path, crate_path: Option<&Path>) -> bool {
    let lib_meta = match fs::metadata(staticlib_path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let lib_mtime = match lib_meta.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };

    // Check wrapper sources
    for p in &[
        wrapper_dir.join("Cargo.toml"),
        wrapper_dir.join("src").join("lib.rs"),
    ] {
        if let Ok(m) = fs::metadata(p) {
            if let Ok(t) = m.modified() {
                if t > lib_mtime {
                    return false;
                }
            }
        }
    }

    // Check crate sources if local path
    if let Some(cp) = crate_path {
        if cp.exists() {
            if let Ok(entries) = fs::read_dir(cp.join("src")) {
                for entry in entries.flatten() {
                    if let Ok(m) = entry.metadata() {
                        if let Ok(t) = m.modified() {
                            if t > lib_mtime {
                                return false;
                            }
                        }
                    }
                }
            }
        }
    }

    true
}

/// Build the staticlib wrapper for a Rust crate dependency.
/// Returns the absolute path to the produced `.lib` / `.a` static library.
pub fn build_rust_crate_wrapper(
    crate_name: &str,
    config: &RustCrateConfig,
    functions: &[ResolvedRustFunction],
    base_dir: &Path,
    lto: bool,
) -> Result<PathBuf, String> {
    let cargo = find_cargo().ok_or_else(|| {
        "Cargo executable not found. Ensure Rust and Cargo are installed\n\
         (checked $USERPROFILE/.cargo/bin/cargo.exe, $CARGO, and PATH)."
            .to_string()
    })?;

    let sanitized_name = crate_name.replace('-', "_");
    let wrapper_dir = base_dir
        .join(".forgen_cache")
        .join("rust_bridge")
        .join(format!("{}_wrapper", sanitized_name));

    fs::create_dir_all(wrapper_dir.join("src")).map_err(|e| {
        format!(
            "Failed to create directory '{}': {}",
            wrapper_dir.display(),
            e
        )
    })?;

    // Generate Cargo.toml
    let cargo_toml_content = generate_wrapper_cargo_toml(crate_name, config, base_dir)?;
    let cargo_toml_path = wrapper_dir.join("Cargo.toml");

    let needs_rewrite_cargo = fs::read_to_string(&cargo_toml_path)
        .map(|cur| cur != cargo_toml_content)
        .unwrap_or(true);
    if needs_rewrite_cargo {
        fs::write(&cargo_toml_path, &cargo_toml_content)
            .map_err(|e| format!("Failed to write '{}': {}", cargo_toml_path.display(), e))?;
    }

    // Generate src/lib.rs
    let lib_rs_content = generate_wrapper_lib_rs(crate_name, functions);
    let lib_rs_path = wrapper_dir.join("src").join("lib.rs");

    let needs_rewrite_lib = fs::read_to_string(&lib_rs_path)
        .map(|cur| cur != lib_rs_content)
        .unwrap_or(true);
    if needs_rewrite_lib {
        fs::write(&lib_rs_path, &lib_rs_content)
            .map_err(|e| format!("Failed to write '{}': {}", lib_rs_path.display(), e))?;
    }

    let staticlib_name = if cfg!(windows) {
        format!("{}_datara_wrapper.lib", sanitized_name)
    } else {
        format!("lib{}_datara_wrapper.a", sanitized_name)
    };

    let staticlib_path = wrapper_dir
        .join("target")
        .join("release")
        .join(&staticlib_name);

    let dep_crate_path = config.path.as_ref().map(|p| {
        if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            base_dir.join(p)
        }
    });

    if !needs_rewrite_cargo
        && !needs_rewrite_lib
        && is_artifact_fresh(&staticlib_path, &wrapper_dir, dep_crate_path.as_deref())
    {
        return Ok(staticlib_path);
    }

    // Invoke Cargo build --release
    let mut cmd = Command::new(&cargo);
    cmd.arg("build").arg("--release").current_dir(&wrapper_dir);

    // Strip ASan/sanitizer flags from the child cargo invocation.
    // The bridge crate contains proc-macro dependencies (e.g. zerocopy_derive,
    // paste) that must be compiled for the HOST. When RUSTFLAGS=-Zsanitizer=address
    // is inherited from `cargo test --target x86_64-unknown-linux-gnu`, those
    // proc-macro crates fail with E0463 ("can't find crate"). We reset RUSTFLAGS
    // here and re-apply only what is explicitly needed (e.g. LTO bitcode).
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("RUSTDOCFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("CARGO_BUILD_RUSTFLAGS");

    // If LTO requested, pass compiler flags to embed bitcode
    if lto {
        cmd.env("RUSTFLAGS", "-C embed-bitcode=yes");
    }

    // Forward PATH with Cargo bin
    if let Some(cargo_bin) = cargo.parent() {
        let mut paths = vec![cargo_bin.to_path_buf()];
        if let Ok(p) = std::env::var("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(new_path) = std::env::join_paths(paths) {
            cmd.env("PATH", new_path);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute cargo at '{}': {}", cargo.display(), e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to compile Rust bridge wrapper for crate '{}':\n--- Cargo stdout ---\n{}\n--- Cargo stderr ---\n{}",
            crate_name,
            stdout.trim(),
            stderr.trim()
        ));
    }

    if !staticlib_path.exists() {
        return Err(format!(
            "Cargo build succeeded but expected static library '{}' was not found in '{}'",
            staticlib_name,
            wrapper_dir.join("target").join("release").display()
        ));
    }

    Ok(staticlib_path)
}
