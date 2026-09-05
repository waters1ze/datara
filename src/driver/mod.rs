//! Compiler driver: public entry points that orchestrate the compilation
//! pipeline.
//!
//! The pipeline itself lives in the private [`pipeline`] submodule (parse
//! helpers, the check tail, the analyze+lower tail and `CompilationResult`);
//! `use`-import resolution lives in [`modules`]. Every entry point funnels
//! through the shared pipeline so a change to the compilation flow touches
//! one place. The public API of `crate::driver` is unchanged.

#![allow(clippy::result_large_err)]

mod modules;
mod pipeline;

use self::pipeline::{
    AnalysisOutput, parse_multi_sources, parse_single_source, run_analysis_and_lower,
    run_check_pipeline,
};
pub use self::pipeline::{CompilationResult, CompilationTimings};

use crate::ast::Program;
use crate::codegen::cranelift::CraneliftBackend;
use crate::diagnostics::DiagnosticEngine;
use crate::dmir::Module;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct ForgenCompiler {
    pub mode: String,
    pub locale: String,
    pub codegen: CraneliftBackend,
    pub cranelift: CraneliftBackend,
    pub use_llvm: bool,
    pub pgo_profile: Option<PathBuf>,
    pub debug_info: bool,
    pub target_triple: Option<String>,
}

impl ForgenCompiler {
    pub fn new(mode: &str) -> Self {
        let backend = CraneliftBackend::for_host();
        let debug_info = mode == "debug" || mode == "quick";
        Self {
            mode: mode.to_string(),
            locale: "en".to_string(),
            codegen: backend.clone(),
            cranelift: backend,
            use_llvm: false,
            pgo_profile: None,
            debug_info,
            target_triple: None,
        }
    }

    pub fn with_llvm(mut self, use_llvm: bool) -> Self {
        self.use_llvm = use_llvm;
        self
    }

    pub fn with_pgo(mut self, profile: Option<PathBuf>) -> Self {
        self.pgo_profile = profile;
        self
    }

    pub fn with_debug(mut self, debug_info: bool) -> Self {
        self.debug_info = debug_info;
        self
    }

    pub fn with_target(mut self, target_triple: Option<String>) -> Self {
        self.target_triple = target_triple;
        self
    }

    pub fn compile_source(
        &self,
        source: &str,
        file: &str,
        output_path: Option<&Path>,
    ) -> CompilationResult {
        let total_start = Instant::now();
        let mut diag = DiagnosticEngine::new(&self.locale);
        diag.set_source(file, source);

        // 1. Lexer & Parser
        let (mut program, timings) = match parse_single_source(
            source,
            file,
            &mut diag,
            CompilationTimings::default(),
            total_start,
        ) {
            Ok(ok) => ok,
            Err(res) => return res,
        };

        // 1b. Resolve `use` imports (stdlib + local project modules)
        let base_dirs = self.module_base_dirs(Path::new(file));
        self.resolve_modules(&mut program, &mut diag, &[], base_dirs);

        self.compile_ast_internal(program, file, output_path, &mut diag, total_start, timings)
    }

    pub fn check_source(&self, source: &str, file: &str) -> CompilationResult {
        let total_start = Instant::now();
        let mut diag = DiagnosticEngine::new(&self.locale);
        diag.set_source(file, source);

        // 1. Lexer & Parser
        let (mut program, timings) = match parse_single_source(
            source,
            file,
            &mut diag,
            CompilationTimings::default(),
            total_start,
        ) {
            Ok(ok) => ok,
            Err(res) => return res,
        };

        // 1b. Resolve `use` imports (stdlib + local project modules).
        // `check` must see the same program as a real compile, otherwise it
        // rejects valid stdlib imports with "Unknown class".
        let base_dirs = self.module_base_dirs(Path::new(file));
        self.resolve_modules(&mut program, &mut diag, &[], base_dirs);

        run_check_pipeline(program, &mut diag, timings, total_start)
    }

    pub fn check_file(&self, path: &Path) -> CompilationResult {
        match fs::read_to_string(path) {
            Ok(src) => self.check_source(&src, path.to_str().unwrap_or("unknown")),
            Err(e) => CompilationResult::failure(
                format!("Failed to read file '{}': {}", path.display(), e),
                format!("IO error: {}", e),
                None,
                CompilationTimings::default(),
            ),
        }
    }

    pub fn check_files(&self, paths: &[PathBuf]) -> CompilationResult {
        let total_start = Instant::now();
        let timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);

        if paths.is_empty() {
            return CompilationResult {
                success: true,
                exe_path: None,
                error: None,
                program: None,
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: String::new(),
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        let (mut combined_program, timings) =
            match parse_multi_sources(paths, &mut diag, timings, total_start) {
                Ok(ok) => ok,
                Err(res) => return res,
            };

        let base_dirs = self.module_base_dirs(paths[0].as_path());
        self.resolve_modules(&mut combined_program, &mut diag, paths, base_dirs);

        run_check_pipeline(combined_program, &mut diag, timings, total_start)
    }

    pub fn compile_source_native(
        &self,
        source: &str,
        file: &str,
        output_path: Option<&Path>,
    ) -> CompilationResult {
        self.compile_source(source, file, output_path)
    }

    pub fn compile_ast(
        &self,
        program: Program,
        file: &str,
        output_path: Option<&Path>,
        diag: &mut DiagnosticEngine,
    ) -> CompilationResult {
        let total_start = Instant::now();
        let timings = CompilationTimings::default();
        self.compile_ast_internal(program, file, output_path, diag, total_start, timings)
    }

    pub fn lower_ast_to_dmir(
        &self,
        program: Program,
        file: &str,
        diag: &mut DiagnosticEngine,
        total_start: Instant,
        timings: CompilationTimings,
    ) -> Result<Module, CompilationResult> {
        run_analysis_and_lower(
            self,
            program,
            file,
            diag,
            total_start,
            timings,
            false,
            |out| out.dmir_module,
        )
    }

    fn compile_ast_internal(
        &self,
        program: Program,
        file: &str,
        output_path: Option<&Path>,
        diag: &mut DiagnosticEngine,
        total_start: Instant,
        timings: CompilationTimings,
    ) -> CompilationResult {
        let out = run_analysis_and_lower(
            self,
            program,
            file,
            diag,
            total_start,
            timings,
            true,
            |out| {
                let AnalysisOutput {
                    program,
                    type_checker,
                    mut graph,
                    dmir_module,
                    optimizer,
                    mut timings,
                    diag,
                } = out;

                if let Some(ref mut g) = graph {
                    g.attach_optimization_report(&optimizer.report, &dmir_module);
                }

                // 10. Native Codegen & Linking (Cranelift or LLVM)
                let codegen_start = Instant::now();
                let target_exe = if let Some(p) = output_path {
                    p.to_path_buf()
                } else {
                    Path::new(file).with_extension("exe")
                };

                let clif_code = self
                    .cranelift
                    .emit_clif(&dmir_module, &program, &type_checker);

                let cache_build_dir = if let Ok(cwd) = std::env::current_dir() {
                    cwd.join(".forgen_cache").join("build")
                } else {
                    PathBuf::from(".forgen_cache").join("build")
                };
                let _ = std::fs::create_dir_all(&cache_build_dir);
                let stem = target_exe
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("out");
                // Temp IR path must be unique per call, not per process: parallel
                // compilations share a pid, so a bare `{stem}_{pid}.ll` collides.
                static LL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let ll_seq = LL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let ll_path = if cache_build_dir.exists() {
                    cache_build_dir.join(format!("{}_{}_{}.ll", stem, std::process::id(), ll_seq))
                } else {
                    target_exe.with_extension("ll")
                };

                // LLVM IR is only emitted when the LLVM pipeline is actually used;
                // the former code generated the full IR on every build and threw it
                // away for Cranelift-only builds.
                let llvm_code = if self.use_llvm {
                    let target_info = if let Some(ref triple) = self.target_triple {
                        crate::codegen::target::TargetInfo::from_triple(triple)
                            .unwrap_or_else(|_| self.cranelift.target.clone())
                    } else {
                        self.cranelift.target.clone()
                    };
                    let llvm_emitter = crate::codegen::llvm::LlvmEmitter::new(&target_info)
                        .with_debug(self.debug_info);
                    let code = llvm_emitter.emit_module(&dmir_module, &program, &type_checker);
                    let _ = std::fs::write(&ll_path, &code);
                    Some(code)
                } else {
                    None
                };

                let compile_res = if self.use_llvm {
                    if crate::codegen::linker::find_clang().is_some()
                        || crate::codegen::linker::find_llc().is_some()
                    {
                        // Locate the Datara runtime independent of the current
                        // working directory. The former hardcoded relative path
                        // ("src/runtime/datara_runtime.c") only worked when the
                        // compiler was launched from the repository root; installed
                        // toolchains silently fell back to "no runtime".
                        let rt_source = PathBuf::from(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/src/runtime/datara_runtime.c"
                        ));
                        let rt_archive = crate::runtime::runtime_lib_path();
                        let rt_opt = if rt_source.exists() {
                            Some(rt_source.as_path())
                        } else if rt_archive.exists() {
                            Some(rt_archive.as_path())
                        } else {
                            None
                        };
                        let abs_target = if target_exe.is_absolute() {
                            target_exe.clone()
                        } else {
                            std::env::current_dir()
                                .map(|c| c.join(&target_exe))
                                .unwrap_or_else(|_| target_exe.clone())
                        };
                        match crate::codegen::linker::compile_with_clang(
                            &ll_path,
                            rt_opt,
                            &abs_target,
                            "3",
                            self.target_triple.as_deref(),
                            self.debug_info,
                        ) {
                            Ok(()) => {
                                let _ = std::fs::remove_file(&ll_path);
                                Ok(abs_target)
                            }
                            Err(e) => {
                                let _ = std::fs::remove_file(&ll_path);
                                eprintln!(
                                    "[Forgen LLVM Warning] LLVM compilation failed: {}. Falling back to native Cranelift backend.",
                                    e
                                );
                                self.cranelift.compile_native(&dmir_module, &target_exe)
                            }
                        }
                    } else {
                        eprintln!(
                            "[Forgen LLVM Notice] Neither Clang nor LLC was found on PATH or in system toolchain."
                        );
                        eprintln!(
                            "  -> LLVM IR successfully generated and saved to: {}",
                            ll_path.display()
                        );
                        eprintln!(
                            "  -> To compile with LLVM, install Clang or run 'rustup component add llvm-tools'."
                        );
                        eprintln!(
                            "  -> Compiling executable via high-speed native Cranelift backend instead."
                        );
                        self.cranelift.compile_native(&dmir_module, &target_exe)
                    }
                } else {
                    self.cranelift.compile_native(&dmir_module, &target_exe)
                };

                timings.codegen_ms = codegen_start.elapsed().as_millis();
                timings.link_ms = 0;
                timings.total_ms = total_start.elapsed().as_millis();
                let base = CompilationResult::codegen_base(
                    program,
                    graph,
                    dmir_module,
                    optimizer.report,
                    diag.format_all(),
                    clif_code,
                    llvm_code,
                    timings,
                );
                match compile_res {
                    Ok(exe_path) => CompilationResult {
                        success: true,
                        exe_path: Some(exe_path),
                        error: None,
                        ..base
                    },
                    Err(e) => CompilationResult {
                        success: false,
                        exe_path: None,
                        error: Some(e),
                        ..base
                    },
                }
            },
        );
        match out {
            Ok(res) => res,
            Err(res) => res,
        }
    }

    pub fn discover_project_files(&self, project_path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut files = Vec::new();
        let src_dir = project_path.join("src");
        let search_dir = if src_dir.exists() && src_dir.is_dir() {
            src_dir
        } else {
            project_path.to_path_buf()
        };

        self.collect_dtr_files(&search_dir, &mut files)?;
        if files.is_empty() {
            return Err(format!(
                "No .dtr source files found in '{}'",
                search_dir.display()
            ));
        }

        if let Some(main_idx) = files
            .iter()
            .position(|p| p.file_name().and_then(|n| n.to_str()) == Some("main.dtr"))
        {
            let main_file = files.remove(main_idx);
            files.insert(0, main_file);
        }

        Ok(files)
    }

    fn collect_dtr_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        if dir.is_dir() {
            let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.collect_dtr_files(&path, files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("dtr") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    pub fn compile_project(
        &self,
        project_path: &Path,
        output_path: Option<&Path>,
    ) -> CompilationResult {
        let discovery_start = Instant::now();
        match crate::project::ProjectDiscovery::discover(Some(project_path)) {
            Ok(layout) => {
                let discovery_ms = discovery_start.elapsed().as_millis();
                let mut res = if layout.source_files.len() == 1 {
                    self.compile_file(&layout.source_files[0], output_path)
                } else {
                    self.compile_files(&layout.source_files, output_path)
                };
                res.timings.discovery_ms = discovery_ms;
                res
            }
            Err(e) => CompilationResult::failure(e.clone(), e, None, CompilationTimings::default()),
        }
    }

    pub fn compile_files_to_dmir(&self, paths: &[PathBuf]) -> Result<Module, String> {
        let total_start = Instant::now();
        let timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);

        if paths.is_empty() {
            return Err("No source files provided for compilation".to_string());
        }

        let (mut combined_program, timings) =
            match parse_multi_sources(paths, &mut diag, timings, total_start) {
                Ok(ok) => ok,
                Err(res) => return Err(res.error.unwrap_or_else(|| "Compilation failed".into())),
            };

        let main_file = paths[0].to_str().unwrap_or("main.dtr");

        let base_dirs = self.module_base_dirs(paths[0].as_path());
        self.resolve_modules(&mut combined_program, &mut diag, paths, base_dirs);
        if diag.has_errors() {
            return Err(diag.format_all());
        }

        let dmir_mod = self
            .lower_ast_to_dmir(combined_program, main_file, &mut diag, total_start, timings)
            .map_err(|e| e.error.unwrap_or_else(|| "Compilation failed".into()))?;

        Ok(dmir_mod)
    }

    pub fn compile_file_to_dmir(&self, path: &Path) -> Result<Module, String> {
        self.compile_files_to_dmir(&[path.to_path_buf()])
    }

    pub fn compile_source_to_dmir(&self, source: &str, file: &str) -> Result<Module, String> {
        let total_start = Instant::now();
        let timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);
        diag.set_source(file, source);

        let (mut program, timings) =
            match parse_single_source(source, file, &mut diag, timings, total_start) {
                Ok(ok) => ok,
                Err(res) => return Err(res.error.unwrap_or_else(|| "Compilation failed".into())),
            };

        let base_dirs = self.module_base_dirs(Path::new(file));
        self.resolve_modules(&mut program, &mut diag, &[], base_dirs);
        if diag.has_errors() {
            return Err(diag.format_all());
        }

        let dmir_mod = self
            .lower_ast_to_dmir(program, file, &mut diag, total_start, timings)
            .map_err(|e| e.error.unwrap_or_else(|| "Compilation failed".into()))?;

        Ok(dmir_mod)
    }

    pub fn run_project(
        &self,
        layout: &crate::project::ProjectLayout,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        self.run_project_captured(layout, args, true)
    }

    pub fn run_project_captured(
        &self,
        layout: &crate::project::ProjectLayout,
        args: &[String],
        capture: bool,
    ) -> Result<(String, String, i32, u128), String> {
        if !self.use_llvm {
            let dmir_mod = if layout.source_files.len() == 1 {
                self.compile_file_to_dmir(&layout.source_files[0])?
            } else {
                self.compile_files_to_dmir(&layout.source_files)?
            };
            return self.cranelift.run_jit(&dmir_mod, args, capture);
        }
        let res = if layout.source_files.len() == 1 {
            self.compile_file(&layout.source_files[0], None)
        } else {
            self.compile_files(&layout.source_files, None)
        };
        if !res.success {
            return Err(res.error.unwrap_or_else(|| "Compilation failed".into()));
        }
        let exe = res
            .exe_path
            .ok_or_else(|| "Compilation succeeded but produced no executable".to_string())?;
        self.codegen.run_executable(&exe, args)
    }

    pub fn run_source(
        &self,
        source: &str,
        file: &str,
        args: &[String],
        capture: bool,
    ) -> Result<(String, String, i32, u128), String> {
        if !self.use_llvm {
            let dmir_mod = self.compile_source_to_dmir(source, file)?;
            return self.cranelift.run_jit(&dmir_mod, args, capture);
        }
        let res = self.compile_source(source, file, None);
        if !res.success {
            return Err(res.error.unwrap_or_else(|| "Compilation failed".into()));
        }
        let exe = res.exe_path.unwrap();
        self.codegen.run_executable(&exe, args)
    }

    pub fn compile_files(
        &self,
        paths: &[PathBuf],
        output_path: Option<&Path>,
    ) -> CompilationResult {
        let total_start = Instant::now();
        let timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);

        if paths.is_empty() {
            let msg = "No source files provided for compilation".to_string();
            return CompilationResult::failure(
                msg.clone(),
                msg,
                None,
                CompilationTimings::default(),
            );
        }

        let (mut combined_program, timings) =
            match parse_multi_sources(paths, &mut diag, timings, total_start) {
                Ok(ok) => ok,
                Err(res) => return res,
            };

        // Resolve `use` imports for the merged program (stdlib + local
        // modules). Files explicitly passed in `paths` are excluded to
        // avoid duplicate symbol registration.
        let base_dirs = self.module_base_dirs(paths[0].as_path());
        self.resolve_modules(&mut combined_program, &mut diag, paths, base_dirs);

        let main_file = paths[0].to_str().unwrap_or("main.dtr");
        let mut res = self.compile_ast_internal(
            combined_program,
            main_file,
            output_path,
            &mut diag,
            total_start,
            timings,
        );
        if let Some(ref mut rep) = res.optimization_report {
            rep.modules_analyzed = paths.len();
        }
        res
    }

    pub fn compile_file(&self, path: &Path, output_path: Option<&Path>) -> CompilationResult {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to read file '{}': {}", path.display(), e);
                return CompilationResult::failure(
                    msg.clone(),
                    msg,
                    None,
                    CompilationTimings::default(),
                );
            }
        };
        self.compile_source(&source, path.to_str().unwrap_or("<source>"), output_path)
    }

    pub fn run_file(
        &self,
        path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        if !self.use_llvm {
            let dmir_mod = self.compile_file_to_dmir(path)?;
            return self.cranelift.run_jit(&dmir_mod, args, true);
        }
        let res = self.compile_file(path, None);
        if !res.success {
            return Err(res.error.unwrap_or_else(|| "Compilation failed".into()));
        }
        let exe = res.exe_path.unwrap();
        self.codegen.run_executable(&exe, args)
    }
}
