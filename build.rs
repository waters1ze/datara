//! Compiles the Datara runtime (plain C) and exposes its location to the crate.
//!
//! Before this existed, `datara_runtime.obj` was a binary checked into the
//! repository and the backend referenced it through an absolute path
//! (`d:\DATARA\datara + forgen\src\runtime\datara_runtime.obj`). Two consequences:
//! the project only built on one machine, and editing the C source changed
//! nothing until someone happened to know to rebuild the object by hand. That
//! is how a float-printing bug survived unnoticed in released builds.
//!
//! Now the runtime is compiled on every build, so:
//!   * the object can never go stale relative to its source,
//!   * the path is correct wherever the crate is checked out,
//!   * non-Windows targets pick up their own toolchain.
//!
//! The archive is deliberately NOT linked into the compiler binary
//! (`cargo_metadata(false)`): it belongs in the programs Forgen generates, so
//! the backend passes its path to the linker.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/runtime/datara_runtime.c");
    println!("cargo:rerun-if-changed=src/runtime/datara_runtime.h");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR was not set by cargo"));

    let mut build = cc::Build::new();
    build
        .file("src/runtime/datara_runtime.c")
        .opt_level(2)
        // Keep the runtime out of forgen.exe itself.
        .cargo_metadata(false);

    if cfg!(target_os = "windows") {
        let pf = env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
        let msvc_base = PathBuf::from(&pf).join("Microsoft Visual Studio");
        if let Ok(entries) = std::fs::read_dir(&msvc_base) {
            for e in entries.flatten() {
                let vctools = e.path().join("BuildTools\\VC\\Tools\\MSVC");
                if let Ok(sub) = std::fs::read_dir(&vctools) {
                    for s in sub.flatten() {
                        let inc = s.path().join("include");
                        if inc.exists() {
                            build.include(inc);
                        }
                    }
                }
            }
        }
        let wk_base = PathBuf::from(&pf).join("Windows Kits\\10\\Include");
        if let Ok(entries) = std::fs::read_dir(&wk_base) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let ucrt = p.join("ucrt");
                    let shared = p.join("shared");
                    let um = p.join("um");
                    if ucrt.exists() {
                        build.include(ucrt);
                    }
                    if shared.exists() {
                        build.include(shared);
                    }
                    if um.exists() {
                        build.include(um);
                    }
                }
            }
        }
    }

    if build.get_compiler().is_like_msvc() {
        build.flag_if_supported("/O2").flag_if_supported("/W3");
    } else {
        build.flag_if_supported("-O2").flag_if_supported("-Wall");
    }

    build
        .try_compile("datara_runtime")
        .expect("failed to compile the Datara runtime (src/runtime/datara_runtime.c)");

    // cc names the archive per platform.
    let archive = if cfg!(target_env = "msvc") {
        out_dir.join("datara_runtime.lib")
    } else {
        out_dir.join("libdatara_runtime.a")
    };

    if !archive.exists() {
        panic!(
            "Datara runtime archive missing at {}; the cc crate did not produce it",
            archive.display()
        );
    }

    // Mirror the latest archive to runtime/ directory for installer and release packages
    let dest_runtime_dir = PathBuf::from("runtime");
    let _ = std::fs::create_dir_all(&dest_runtime_dir);
    let _ = std::fs::copy(
        &archive,
        dest_runtime_dir.join(archive.file_name().unwrap()),
    );

    if let Ok(home) = env::var("DATARA_HOME") {
        let home_runtime_dir = PathBuf::from(home).join("runtime");
        let _ = std::fs::create_dir_all(&home_runtime_dir);
        let _ = std::fs::copy(
            &archive,
            home_runtime_dir.join(archive.file_name().unwrap()),
        );
    }

    println!("cargo:rustc-env=DATARA_RUNTIME_LIB={}", archive.display());
}
