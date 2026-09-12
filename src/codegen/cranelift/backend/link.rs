use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::ast::Program;
use crate::codegen::CodegenBackend;
use crate::codegen::linker::linker_lock;
use crate::codegen::target::TargetInfo;
use crate::dmir::Module;
use crate::types::TypeChecker;

use super::RealCraneliftBackend;

impl RealCraneliftBackend {
    pub fn link_object_to_executable(
        &self,
        obj_bytes: &[u8],
        output_exe: &Path,
        exports: &[String],
        extra_libs: &[String],
    ) -> Result<PathBuf, String> {
        let abs_out = if output_exe.is_absolute() {
            output_exe.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(output_exe)
        };

        if let Some(parent) = abs_out.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create output directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
        }

        // The object file must be unique per compilation, not per process:
        // concurrent compilations inside one process (e.g. parallel tests)
        // share a pid, so a bare `{stem}_{pid}.obj` name let one compile's
        // link/delete race overwrite or remove another's object, producing
        // LNK1181 or — worse — an exe linked from the WRONG program's object
        // bytes. A per-call sequence number plus an absolute cache path makes
        // the temp object collision-free and cwd-independent.
        static OBJ_SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = OBJ_SEQ.fetch_add(1, Ordering::Relaxed);
        let cache_build_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to resolve current directory: {}", e))?
            .join(".forgen_cache")
            .join("build");
        fs::create_dir_all(&cache_build_dir).map_err(|e| {
            format!(
                "Failed to create build cache directory '{}': {}",
                cache_build_dir.display(),
                e
            )
        })?;
        let stem = abs_out
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out");
        let obj_filename = format!("{}_{}_{}.obj", stem, std::process::id(), seq);
        let obj_path = cache_build_dir.join(obj_filename);
        fs::write(&obj_path, obj_bytes)
            .map_err(|e| format!("Failed to write object file: {}", e))?;
        let _ = fs::copy(&obj_path, "scratch/bench_ray.obj");

        // Locate the toolchain and the Datara runtime at run time. Nothing here
        // may depend on this machine: `linker::discover` resolves MSVC through
        // `vswhere` (falling back to `PATH`), and the runtime archive path is
        // baked in by `build.rs` from `OUT_DIR`, so it is correct in any
        // checkout and can never be a stale artifact.
        let spec = crate::codegen::linker::ensure_linker()?;
        let runtime_lib = crate::runtime::runtime_lib_path();
        if !runtime_lib.exists() {
            return Err(format!(
                "Datara runtime library is missing at '{}'. Rebuild the compiler \
                 (`cargo build`) so build.rs can regenerate it.",
                runtime_lib.display()
            ));
        }

        crate::runtime::verify_runtime_abi().map_err(|e| format!("Linker failed: {}", e))?;

        let args = crate::codegen::linker::link_args(
            &spec,
            &obj_path,
            &runtime_lib,
            &abs_out,
            exports,
            extra_libs,
        );

        let output = {
            let _guard = linker_lock().lock().unwrap_or_else(|e| e.into_inner());
            Command::new(&spec.program)
                .args(&args)
                .output()
                .map_err(|e| {
                    format!(
                        "Failed to invoke linker '{}': {}\n{}",
                        spec.program.display(),
                        e,
                        crate::codegen::linker::describe(&spec)
                    )
                })?
        };

        if output.status.success() && abs_out.exists() {
            let _ = fs::remove_file(&obj_path);
            let _ = fs::remove_file(abs_out.with_extension("ilk"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&abs_out) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&abs_out, perms);
                }
            }
            return Ok(abs_out);
        }

        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "Linking failed: status={:?}\n{}\nargv: {} {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            crate::codegen::linker::describe(&spec),
            spec.program.display(),
            args.join(" "),
            out,
            err
        ))
    }
}

impl CodegenBackend for RealCraneliftBackend {
    fn target_info(&self) -> TargetInfo {
        self.target.clone()
    }

    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        let clif_emitter = crate::codegen::cranelift::ClifEmitter::new(&self.target);
        clif_emitter
            .emit_module(module, program, types)
            .unwrap_or_else(|e| format!("; clif emission error: {}\n", e))
    }

    fn compile_to_executable(&self, source: &str, output_path: &Path) -> Result<PathBuf, String> {
        // The former implementation only wrote a `.clif` file and returned
        // `Ok(output_path)` even though no executable was produced — callers
        // believed a binary existed. Fail honestly instead.
        let _ = source;
        Err(format!(
            "compile_to_executable is deprecated; build via compile_native with a DMIR Module (requested output: {})",
            output_path.display()
        ))
    }

    fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        let abs_exe = if exe_path.is_absolute() {
            exe_path.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(exe_path)
        } else {
            exe_path.to_path_buf()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&abs_exe) {
                let mut perms = metadata.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(perms.mode() | 0o755);
                    let _ = fs::set_permissions(&abs_exe, perms);
                }
            }
        }
        let start = Instant::now();
        let mut cmd = Command::new(&abs_exe);
        cmd.args(args);
        cmd.env_remove("LD_PRELOAD");
        // When running under ASan (ASAN_OPTIONS is set by cargo test harness),
        // set detect_leaks=0 on the child process to prevent LeakSanitizer from
        // triggering on compiled binaries that have benign runtime leaks.
        // We must NOT remove ASAN_OPTIONS entirely since the child binary itself
        // is compiled with -fsanitize=address (injected by link_args) and needs
        // a valid ASAN_OPTIONS env.
        if std::env::var("ASAN_OPTIONS").is_ok() {
            cmd.env("ASAN_OPTIONS", "detect_leaks=0");
        } else {
            cmd.env_remove("ASAN_OPTIONS");
        }
        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to run native executable '{}': {}",
                abs_exe.display(),
                e
            )
        })?;
        let duration = start.elapsed().as_millis();

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, code, duration))
    }
}
