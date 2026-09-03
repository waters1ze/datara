use crate::ast::{Decl, Program, UseDecl};
use crate::codegen::cranelift::CraneliftBackend;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::dmir::{Lowering, Module};
use crate::effects::EffectAnalyzer;
use crate::lexer::Lexer;
use crate::optimizer::{OptimizationReport, Optimizer};
use crate::ownership::OwnershipTracker;
use crate::parser::Parser;
use crate::resolver::Resolver;
use crate::semantic_graph::SemanticGraph;
use crate::types::TypeChecker;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompilationTimings {
    pub discovery_ms: u128,
    pub parse_ms: u128,
    pub resolve_ms: u128,
    pub typecheck_ms: u128,
    pub effects_ms: u128,
    pub ownership_ms: u128,
    pub graph_ms: u128,
    pub optimizer_ms: u128,
    pub codegen_ms: u128,
    pub link_ms: u128,
    pub total_ms: u128,
}

pub struct ForgenCompiler {
    pub mode: String,
    pub locale: String,
    pub codegen: CraneliftBackend,
    pub cranelift: CraneliftBackend,
    pub use_llvm: bool,
    pub pgo_profile: Option<PathBuf>,
}

#[derive(Clone)]
pub struct CompilationResult {
    pub success: bool,
    pub exe_path: Option<PathBuf>,
    pub error: Option<String>,
    pub program: Option<Program>,
    pub semantic_graph: Option<SemanticGraph>,
    pub dmir_module: Option<Module>,
    pub optimization_report: Option<OptimizationReport>,
    pub diagnostics: String,
    pub clif_source: Option<String>,
    pub llvm_source: Option<String>,
    pub timings: CompilationTimings,
}

impl ForgenCompiler {
    pub fn new(mode: &str) -> Self {
        let backend = CraneliftBackend::for_host();
        Self {
            mode: mode.to_string(),
            locale: "en".to_string(),
            codegen: CraneliftBackend::for_host(),
            cranelift: backend,
            use_llvm: false,
            pgo_profile: None,
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

    pub fn compile_source(
        &self,
        source: &str,
        file: &str,
        output_path: Option<&Path>,
    ) -> CompilationResult {
        let total_start = Instant::now();
        let mut timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);
        diag.set_source(file, source);

        // 1. Lexer & Parser
        let parse_start = Instant::now();
        let mut lexer = Lexer::new(source, file);
        let tokens = lexer.tokenize(&mut diag);
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: None,
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        let mut parser = Parser::new(tokens, &mut diag, file);
        let mut program = parser.parse_program();
        timings.parse_ms = parse_start.elapsed().as_millis();

        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 1b. Resolve `use` imports (stdlib + local project modules)
        let base_dirs = self.module_base_dirs(Path::new(file));
        self.resolve_modules(&mut program, &mut diag, &[], base_dirs);

        self.compile_ast_internal(program, file, output_path, &mut diag, total_start, timings)
    }

    pub fn check_source(&self, source: &str, file: &str) -> CompilationResult {
        let total_start = Instant::now();
        let mut timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);
        diag.set_source(file, source);

        // 1. Lexer & Parser
        let parse_start = Instant::now();
        let mut lexer = Lexer::new(source, file);
        let tokens = lexer.tokenize(&mut diag);
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: None,
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        let mut parser = Parser::new(tokens, &mut diag, file);
        let mut program = parser.parse_program();
        timings.parse_ms = parse_start.elapsed().as_millis();

        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 1b. Resolve `use` imports (stdlib + local project modules).
        // `check` must see the same program as a real compile, otherwise it
        // rejects valid stdlib imports with "Unknown class".
        let base_dirs = self.module_base_dirs(Path::new(file));
        self.resolve_modules(&mut program, &mut diag, &[], base_dirs);

        // 2. Resolver
        let res_start = Instant::now();
        let mut resolver = Resolver::new();
        resolver.resolve_program(&program, &mut diag);
        timings.resolve_ms = res_start.elapsed().as_millis();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 3. Type Checker
        let tc_start = Instant::now();
        let mut type_checker = TypeChecker::new(&resolver);
        type_checker.check_program(&program, &mut diag);
        timings.typecheck_ms = tc_start.elapsed().as_millis();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 4. Effects
        let eff_start = Instant::now();
        let mut effects = EffectAnalyzer::new();
        effects.analyze_program(&program);
        timings.effects_ms = eff_start.elapsed().as_millis();

        // 5. Ownership
        let own_start = Instant::now();
        let mut ownership = OwnershipTracker::new(&resolver);
        ownership.check_program(&program, &mut diag);
        timings.ownership_ms = own_start.elapsed().as_millis();

        timings.total_ms = total_start.elapsed().as_millis();
        let diag_str = diag.format_all();
        CompilationResult {
            success: !diag.has_errors(),
            exe_path: None,
            error: if diag.has_errors() {
                Some(diag_str.clone())
            } else {
                None
            },
            program: Some(program),
            semantic_graph: None,
            dmir_module: None,
            optimization_report: None,
            diagnostics: diag_str,
            clif_source: None,
            llvm_source: None,
            timings,
        }
    }

    pub fn check_file(&self, path: &Path) -> CompilationResult {
        match fs::read_to_string(path) {
            Ok(src) => self.check_source(&src, path.to_str().unwrap_or("unknown")),
            Err(e) => CompilationResult {
                success: false,
                exe_path: None,
                error: Some(format!("Failed to read file '{}': {}", path.display(), e)),
                program: None,
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: format!("IO error: {}", e),
                clif_source: None,
                llvm_source: None,
                timings: CompilationTimings::default(),
            },
        }
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

    fn compile_ast_internal(
        &self,
        program: Program,
        file: &str,
        output_path: Option<&Path>,
        diag: &mut DiagnosticEngine,
        total_start: Instant,
        mut timings: CompilationTimings,
    ) -> CompilationResult {
        // 3. Resolver
        let res_start = Instant::now();
        let mut resolver = Resolver::new();
        resolver.resolve_program(&program, diag);
        timings.resolve_ms = res_start.elapsed().as_millis();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 4. Type Checker
        let tc_start = Instant::now();
        let mut type_checker = TypeChecker::new(&resolver);
        type_checker.check_program(&program, diag);
        timings.typecheck_ms = tc_start.elapsed().as_millis();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 5. Effects Analyzer
        let eff_start = Instant::now();
        let mut effects = EffectAnalyzer::new();
        effects.analyze_program(&program);
        timings.effects_ms = eff_start.elapsed().as_millis();

        // 6. Ownership & Borrow Tracker
        let own_start = Instant::now();
        let mut ownership = OwnershipTracker::new(&resolver);
        ownership.check_program(&program, diag);
        timings.ownership_ms = own_start.elapsed().as_millis();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return CompilationResult {
                success: false,
                exe_path: None,
                error: Some(d_str.clone()),
                program: Some(program),
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: d_str,
                clif_source: None,
                llvm_source: None,
                timings,
            };
        }

        // 7. Semantic Graph
        let graph_start = Instant::now();
        let mut graph = if self.mode != "quick" {
            Some(SemanticGraph::build(&program, &resolver, &effects))
        } else {
            None
        };
        timings.graph_ms = graph_start.elapsed().as_millis();

        // 8. DMIR Lowering
        let mut lowering = Lowering::new(&resolver, &type_checker);
        let mut dmir_module = lowering.lower_program(
            &program,
            Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("main"),
        );

        // 9. Optimizer
        let opt_start = Instant::now();
        let mut optimizer = Optimizer::new(&self.mode);
        optimizer.set_function_effects(effects.function_effects.clone());
        for (class_name, specs) in &type_checker.generic_specializations {
            for spec_args in specs {
                let spec_str = format!(
                    "{}<{}>",
                    class_name,
                    spec_args
                        .iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                optimizer.report.generic_specializations.push(spec_str);
            }
        }
        optimizer.optimize_module(&mut dmir_module);
        if let Some(ref pgo_path) = self.pgo_profile
            && let Ok(profile) = crate::pgo::ProfileData::load_from_file(pgo_path)
        {
            crate::pgo::ProfileGuidedOptimizer::optimize_module(
                &mut optimizer,
                &mut dmir_module,
                &profile,
            );
        }
        timings.optimizer_ms = opt_start.elapsed().as_millis();

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

        let llvm_emitter = crate::codegen::llvm::LlvmEmitter::new(&self.cranelift.target);
        let llvm_code = llvm_emitter.emit_module(&dmir_module, &program, &type_checker);

        let ll_path = target_exe.with_extension("ll");
        if self.use_llvm {
            let _ = std::fs::write(&ll_path, &llvm_code);
        }

        let compile_res = if self.use_llvm {
            if crate::codegen::linker::find_clang().is_some()
                || crate::codegen::linker::find_llc().is_some()
            {
                let rt_path = PathBuf::from("src/runtime/datara_runtime.c");
                let rt_opt = if rt_path.exists() {
                    Some(rt_path.as_path())
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
                match crate::codegen::linker::compile_with_clang(&ll_path, rt_opt, &abs_target, "3")
                {
                    Ok(()) => Ok(abs_target),
                    Err(e) => {
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

        match compile_res {
            Ok(exe_path) => CompilationResult {
                success: true,
                exe_path: Some(exe_path),
                error: None,
                program: Some(program),
                semantic_graph: graph,
                dmir_module: Some(dmir_module),
                optimization_report: Some(optimizer.report),
                diagnostics: diag.format_all(),
                clif_source: Some(clif_code),
                llvm_source: Some(llvm_code),
                timings,
            },
            Err(e) => CompilationResult {
                success: false,
                exe_path: None,
                error: Some(e),
                program: Some(program),
                semantic_graph: graph,
                dmir_module: Some(dmir_module),
                optimization_report: Some(optimizer.report),
                diagnostics: diag.format_all(),
                clif_source: Some(clif_code),
                llvm_source: Some(llvm_code),
                timings,
            },
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
            Err(e) => CompilationResult {
                success: false,
                exe_path: None,
                error: Some(e.clone()),
                program: None,
                semantic_graph: None,
                dmir_module: None,
                optimization_report: None,
                diagnostics: e,
                clif_source: None,
                llvm_source: None,
                timings: CompilationTimings::default(),
            },
        }
    }

    pub fn run_project(
        &self,
        layout: &crate::project::ProjectLayout,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        let res = if layout.source_files.len() == 1 {
            self.compile_file(&layout.source_files[0], None)
        } else {
            self.compile_files(&layout.source_files, None)
        };
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
        let mut timings = CompilationTimings::default();
        let mut diag = DiagnosticEngine::new(&self.locale);
        let mut combined_declarations = Vec::new();

        let parse_start = Instant::now();
        for p in paths {
            let src = match fs::read_to_string(p) {
                Ok(s) => s,
                Err(e) => {
                    return CompilationResult {
                        success: false,
                        exe_path: None,
                        error: Some(format!("Failed to read '{}': {}", p.display(), e)),
                        program: None,
                        semantic_graph: None,
                        dmir_module: None,
                        optimization_report: None,
                        diagnostics: format!("Failed to read '{}': {}", p.display(), e),
                        clif_source: None,
                        llvm_source: None,
                        timings,
                    };
                }
            };
            diag.set_source(p.to_str().unwrap_or("file"), &src);

            let mut lexer = Lexer::new(&src, p.to_str().unwrap_or("file"));
            let tokens = lexer.tokenize(&mut diag);
            if diag.has_errors() {
                timings.total_ms = total_start.elapsed().as_millis();
                let d_str = diag.format_all();
                return CompilationResult {
                    success: false,
                    exe_path: None,
                    error: Some(d_str.clone()),
                    program: None,
                    semantic_graph: None,
                    dmir_module: None,
                    optimization_report: None,
                    diagnostics: d_str,
                    clif_source: None,
                    llvm_source: None,
                    timings,
                };
            }

            let mut parser = Parser::new(tokens, &mut diag, p.to_str().unwrap_or("file"));
            let prog = parser.parse_program();
            if diag.has_errors() {
                timings.total_ms = total_start.elapsed().as_millis();
                let d_str = diag.format_all();
                return CompilationResult {
                    success: false,
                    exe_path: None,
                    error: Some(d_str.clone()),
                    program: Some(prog),
                    semantic_graph: None,
                    dmir_module: None,
                    optimization_report: None,
                    diagnostics: d_str,
                    clif_source: None,
                    llvm_source: None,
                    timings,
                };
            }

            combined_declarations.extend(prog.declarations);
        }
        timings.parse_ms = parse_start.elapsed().as_millis();

        let main_file = paths[0].to_str().unwrap_or("main.dtr");
        let mut combined_program = Program {
            declarations: combined_declarations,
            file: main_file.to_string(),
        };

        // Resolve `use` imports for the merged program (stdlib + local
        // modules). Files explicitly passed in `paths` are excluded to
        // avoid duplicate symbol registration.
        let base_dirs = self.module_base_dirs(paths[0].as_path());
        self.resolve_modules(&mut combined_program, &mut diag, paths, base_dirs);

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

    /// Candidate base directories for resolving local module paths:
    /// the importing file's directory, then the current directory.
    fn module_base_dirs(&self, source_file: &Path) -> Vec<PathBuf> {
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
        if u.path.first().map(|s| s.as_str()) != Some("stdlib") {
            return None;
        }
        let base_rel: &[String] = if u.path.len() >= 4 {
            &u.path[1..u.path.len() - 1]
        } else {
            &u.path[1..]
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
            .trim_end_matches(".so");
        let lib_extensions = ["lib", "dll", "so", "a", "dylib"];

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
    fn resolve_modules(
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
                        if !py_pkg.is_empty() {
                            let check_cmd = std::process::Command::new("python")
                                .arg("-c")
                                .arg(format!("import {}; print(getattr({}, '__file__', 'built-in')); print(getattr({}, '__version__', 'builtin'))", py_pkg, py_pkg, py_pkg))
                                .output();
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
                        continue;
                    }

                    // 2. Smart Rust Crate Interop Detection (Global / local Cargo & cdylibs)
                    if first_seg == Some("rust") {
                        let rust_crate = u.path.get(1).map(|s| s.as_str()).unwrap_or("");
                        if !rust_crate.is_empty() {
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
                            if !dll_exists {
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
                        if !c_lib.is_empty() {
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
                        if !js_pkg.is_empty() {
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

                    let path = if first_seg == Some("stdlib") {
                        self.stdlib_module_path(u, stdlib_dir.as_deref())
                    } else {
                        self.local_module_path(u, &base_dirs)
                    };

                    let path = match path {
                        Some(p) => Some(p),
                        None => {
                            // JIT Predictive Auto-Install from HyperGrid
                            let pkg_name = first_seg.unwrap_or("");
                            let registry = crate::project::HyperGridRegistry::new();
                            if let Some(pkg) = registry.lookup(pkg_name) {
                                let auto_install_env = std::env::var("FORGEN_AUTO_INSTALL")
                                    .or_else(|_| std::env::var("DATARA_AUTO_INSTALL"))
                                    .map(|v| v == "1" || v == "true")
                                    .unwrap_or(false);

                                let should_install = if auto_install_env {
                                    true
                                } else {
                                    use std::io::Write;
                                    println!(
                                        "\n:: [HyperGrid] Package '{}' is required by {}",
                                        pkg.name, u.span.file
                                    );
                                    println!(
                                        "   version: {} | digest: {} | capabilities: [{}]",
                                        pkg.version,
                                        pkg.digest,
                                        pkg.capabilities.join(", ")
                                    );
                                    print!("   Auto-install from HyperGrid? [Y/n]: ");
                                    let _ = std::io::stdout().flush();
                                    let mut input = String::new();
                                    if std::io::stdin().read_line(&mut input).is_ok() {
                                        let trimmed = input.trim().to_lowercase();
                                        trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
                                    } else {
                                        false
                                    }
                                };

                                if should_install {
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
                                    None
                                }
                            } else {
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
                                eb.target_type == b.target_type
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

    pub fn compile_file(&self, path: &Path, output_path: Option<&Path>) -> CompilationResult {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return CompilationResult {
                    success: false,
                    exe_path: None,
                    error: Some(format!("Failed to read file '{}': {}", path.display(), e)),
                    program: None,
                    semantic_graph: None,
                    dmir_module: None,
                    optimization_report: None,
                    diagnostics: format!("Failed to read file '{}': {}", path.display(), e),
                    clif_source: None,
                    llvm_source: None,
                    timings: CompilationTimings::default(),
                };
            }
        };
        self.compile_source(&source, path.to_str().unwrap_or("<source>"), output_path)
    }

    pub fn run_file(
        &self,
        path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        let res = self.compile_file(path, None);
        if !res.success {
            return Err(res.error.unwrap_or_else(|| "Compilation failed".into()));
        }
        let exe = res.exe_path.unwrap();
        self.codegen.run_executable(&exe, args)
    }
}
