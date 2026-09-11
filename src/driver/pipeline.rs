//! The single compilation pipeline shared by every driver entry point.
//!
//! Before this module existed the pipeline (lex → parse → resolve →
//! typecheck → effects → ownership → security) was re-implemented in each of
//! `check_source`, `check_files`, `compile_source`/`compile_ast_internal` and
//! `compile_files`, and `CompilationResult` was constructed literally dozens
//! of times with near-identical field lists. Everything funnel now goes
//! through the helpers here:
//!
//! * [`parse_single_source`] / [`parse_multi_sources`] — the lex+parse front
//!   half (shared by single-source and multi-file entry points).
//! * [`run_check_pipeline`] — the analysis tail used by the `check_*` family
//!   (no early return after ownership/security; success is decided from the
//!   final diagnostic state).
//! * [`run_analysis_and_lower`] — the analyze + DMIR-lower + optimize tail
//!   used by the compile family (`compile_ast_internal`) and
//!   `lower_ast_to_dmir`. Unlike the check tail it early-returns after
//!   ownership and security, matching the original per-entry-point behavior.
//!
//! Failure-path `CompilationResult`s are built exclusively via
//! [`CompilationResult::failure`].

#![allow(clippy::result_large_err, clippy::too_many_arguments)]

use super::ForgenCompiler;
use crate::ast::{Decl, Program};
use crate::diagnostics::DiagnosticEngine;
use crate::dmir::{Lowering, Module};
use crate::effects::EffectAnalyzer;
use crate::lexer::Lexer;
use crate::optimizer::{OptimizationReport, Optimizer};
use crate::ownership::{DmirOwnershipAnalyzer, OwnershipTracker};
use crate::parser::Parser;
use crate::resolver::Resolver;
use crate::semantic_graph::SemanticGraph;
use crate::types::TypeChecker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Clone)]
pub struct CompilationResult {
    pub success: bool,
    pub exe_path: Option<PathBuf>,
    pub error: Option<String>,
    pub program: Option<Program>,
    pub semantic_graph: Option<SemanticGraph>,
    pub dmir_module: Option<Module>,
    pub optimization_report: Option<OptimizationReport>,
    pub schedule_proof: Option<std::sync::Arc<crate::schedule::ScheduleProof>>,
    pub diagnostics: String,
    pub diagnostic_records: Vec<crate::diagnostics::Diagnostic>,
    pub clif_source: Option<String>,
    pub llvm_source: Option<String>,
    pub timings: CompilationTimings,
}

impl CompilationResult {
    /// Renders diagnostics to a human-readable formatted string for backwards compatibility.
    pub fn render_to_string(&self) -> String {
        self.diagnostics.clone()
    }

    /// Structured list of diagnostics produced during compilation.
    pub fn diagnostic_items(&self) -> &[crate::diagnostics::Diagnostic] {
        &self.diagnostic_records
    }

    /// Canonical constructor for every failure-path `CompilationResult`.
    ///
    /// `error` and `diagnostics` are separate parameters because a few IO
    /// paths (e.g. `check_file`) report a richer `error` than their
    /// `diagnostics` line; every other failure path passes the same string
    /// for both.
    pub(super) fn failure(
        error: String,
        diagnostics: String,
        program: Option<Program>,
        timings: CompilationTimings,
    ) -> Self {
        Self::failure_with_diagnostics(error, diagnostics, Vec::new(), program, timings)
    }

    pub(super) fn failure_with_diagnostics(
        error: String,
        diagnostics: String,
        diagnostic_records: Vec<crate::diagnostics::Diagnostic>,
        program: Option<Program>,
        timings: CompilationTimings,
    ) -> Self {
        Self {
            success: false,
            exe_path: None,
            error: Some(error),
            program,
            semantic_graph: None,
            dmir_module: None,
            optimization_report: None,
            schedule_proof: None,
            diagnostics,
            diagnostic_records,
            clif_source: None,
            llvm_source: None,
            timings,
        }
    }

    /// Shared artifact bundle for the two codegen outcomes of
    /// `compile_ast_internal` (native linking succeeded or failed). Every
    /// field except `success`/`exe_path`/`error` is identical across the two
    /// outcomes, so the caller builds this base once and overrides those
    /// three fields with struct-update syntax.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn codegen_base(
        program: Program,
        graph: Option<SemanticGraph>,
        dmir_module: Module,
        optimization_report: OptimizationReport,
        schedule_proof: Option<std::sync::Arc<crate::schedule::ScheduleProof>>,
        diagnostics: String,
        clif_source: String,
        llvm_source: Option<String>,
        timings: CompilationTimings,
    ) -> Self {
        Self {
            success: false,
            exe_path: None,
            error: None,
            program: Some(program),
            semantic_graph: graph,
            dmir_module: Some(dmir_module),
            optimization_report: Some(optimization_report),
            schedule_proof,
            diagnostics,
            diagnostic_records: Vec::new(),
            clif_source: Some(clif_source),
            llvm_source,
            timings,
        }
    }
}

/// Lex and parse a single source string, early-returning a failure result on
/// lexer or parser errors (shared verbatim by `compile_source`,
/// `check_source` and `compile_source_to_dmir`).
pub(super) fn parse_single_source(
    source: &str,
    file: &str,
    diag: &mut DiagnosticEngine,
    mut timings: CompilationTimings,
    total_start: Instant,
) -> Result<(Program, CompilationTimings), CompilationResult> {
    // 1. Lexer & Parser
    let parse_start = Instant::now();
    let mut lexer = Lexer::new(source, file);
    let tokens = lexer.tokenize(diag);
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            None,
            timings,
        ));
    }

    let mut parser = Parser::new(tokens, diag, file);
    let program = parser.parse_program();
    timings.parse_ms = parse_start.elapsed().as_millis();

    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }

    Ok((program, timings))
}

/// Lex and parse several source files into one combined `Program`,
/// early-returning a failure result on read, lexer or parser errors (shared
/// verbatim by `check_files` and `compile_files`; `compile_files_to_dmir`
/// maps the failure result back to its `error` string).
pub(super) fn parse_multi_sources(
    paths: &[PathBuf],
    diag: &mut DiagnosticEngine,
    mut timings: CompilationTimings,
    total_start: Instant,
) -> Result<(Program, CompilationTimings), CompilationResult> {
    let mut combined_declarations = Vec::new();
    let mut combined_attributes = Vec::new();

    let parse_start = Instant::now();
    for p in paths {
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to read '{}': {}", p.display(), e);
                return Err(CompilationResult::failure(msg.clone(), msg, None, timings));
            }
        };
        diag.set_source(p.to_str().unwrap_or("file"), &src);

        let mut lexer = Lexer::new(&src, p.to_str().unwrap_or("file"));
        let tokens = lexer.tokenize(diag);
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return Err(CompilationResult::failure(
                d_str.clone(),
                d_str,
                None,
                timings,
            ));
        }

        let mut parser = Parser::new(tokens, diag, p.to_str().unwrap_or("file"));
        let mut prog = parser.parse_program();
        if diag.has_errors() {
            timings.total_ms = total_start.elapsed().as_millis();
            let d_str = diag.format_all();
            return Err(CompilationResult::failure(
                d_str.clone(),
                d_str,
                Some(prog),
                timings,
            ));
        }

        let has_exports = prog.declarations.iter().any(|d| match d {
            Decl::Class(c) => c.is_export,
            Decl::Enum(e) => e.is_export,
            Decl::Function(f) => f.is_export,
            Decl::Component(c) => c.is_export,
            Decl::Role(r) => r.is_export,
            Decl::Trait(t) => t.is_export,
            Decl::Type(td) => td.is_export,
            _ => false,
        });
        if !has_exports {
            for d in &mut prog.declarations {
                match d {
                    Decl::Class(c) => c.is_export = true,
                    Decl::Enum(e) => e.is_export = true,
                    Decl::Function(f) => f.is_export = true,
                    Decl::Component(c) => c.is_export = true,
                    Decl::Role(r) => r.is_export = true,
                    Decl::Trait(t) => t.is_export = true,
                    Decl::Type(td) => td.is_export = true,
                    _ => {}
                }
            }
        }

        combined_declarations.extend(prog.declarations);
        combined_attributes.extend(prog.attributes);
    }
    timings.parse_ms = parse_start.elapsed().as_millis();

    let main_file = paths[0].to_str().unwrap_or("main.dtr");
    let combined_program = Program {
        declarations: combined_declarations,
        attributes: combined_attributes,
        file: main_file.to_string(),
        link_libraries: Vec::new(),
    };
    Ok((combined_program, timings))
}

/// The analysis tail shared by `check_source` and `check_files`.
///
/// Deliberately different from [`run_analysis_and_lower`]: the check family
/// does NOT early-return after the ownership or security phases — it runs the
/// full pipeline and decides `success` from the final diagnostic state.
pub(super) fn run_check_pipeline(
    program: Program,
    diag: &mut DiagnosticEngine,
    mut timings: CompilationTimings,
    total_start: Instant,
) -> CompilationResult {
    let mut program = program;
    let base_dir = Path::new(&program.file).parent().map(|p| p.to_path_buf());
    crate::cimport::expand_c_imports(&mut program, base_dir.as_deref(), diag);
    crate::rust_bridge::expand_rust_dependencies(&mut program, base_dir.as_deref(), diag);
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return CompilationResult::failure(d_str.clone(), d_str, Some(program), timings);
    }
    crate::derive::expand_derives_and_comptime(&mut program);

    // 2. Resolver
    let res_start = Instant::now();
    let mut resolver = Resolver::new();
    resolver.resolve_program(&program, diag);
    timings.resolve_ms = res_start.elapsed().as_millis();
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return CompilationResult::failure(d_str.clone(), d_str, Some(program), timings);
    }

    // 3. Type Checker
    let tc_start = Instant::now();
    let mut type_checker = TypeChecker::new(&resolver);
    type_checker.check_program(&program, diag);
    timings.typecheck_ms = tc_start.elapsed().as_millis();
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return CompilationResult::failure(d_str.clone(), d_str, Some(program), timings);
    }

    // 4. Effects
    let eff_start = Instant::now();
    let mut effects = EffectAnalyzer::new();
    effects.analyze_program(&program);
    timings.effects_ms = eff_start.elapsed().as_millis();

    // 5. Ownership
    let own_start = Instant::now();
    let mut ownership = OwnershipTracker::new(&resolver);
    ownership.check_program(&program, diag);
    timings.ownership_ms = own_start.elapsed().as_millis();

    // 6. Security & Zero-Trust Verifier (Proof-Carrying Code)
    let mut security = crate::security::SecurityVerifier::new(&resolver, &type_checker);
    security.verify_program(&program, diag);

    // 7. DMIR Ownership Fixpoint Verification (if no prior errors)
    if !diag.has_errors() {
        let mut lowering = Lowering::new(&resolver, &type_checker);
        let mut dmir_module = lowering.lower_program(&program, "main");
        let mut dmir_analyzer = DmirOwnershipAnalyzer::new(&resolver);
        let mut signatures = HashMap::new();
        for decl in &program.declarations {
            match decl {
                crate::ast::Decl::Function(f)
                | crate::ast::Decl::Flow(f)
                | crate::ast::Decl::Task(f) => {
                    signatures.insert(f.name.clone(), f.params.clone());
                }
                crate::ast::Decl::Class(c) => {
                    for item in &c.body_items {
                        if let crate::ast::ClassItem::Method(m) = item {
                            signatures.insert(format!("{}_{}", c.name, m.name), m.params.clone());
                        }
                    }
                }
                crate::ast::Decl::Behavior(b) => {
                    for item in &b.body_items {
                        if let crate::ast::ClassItem::Method(m) = item {
                            signatures
                                .insert(format!("{}_{}", b.target_type, m.name), m.params.clone());
                        }
                    }
                }
                crate::ast::Decl::Impl(i) => {
                    for m in &i.methods {
                        signatures
                            .insert(format!("{}_{}", i.target_type, m.name), m.params.clone());
                    }
                }
                _ => {}
            }
        }
        dmir_analyzer.set_signatures(signatures);
        let _ = dmir_analyzer.analyze_and_lower_module(&mut dmir_module, diag);
    }

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
        schedule_proof: None,
        diagnostics: diag_str,
        diagnostic_records: diag.diagnostics.clone(),
        clif_source: None,
        llvm_source: None,
        timings,
    }
}

/// Everything [`run_analysis_and_lower`] hands to the caller's continuation.
///
/// `TypeChecker` borrows the `Resolver` built inside the pipeline, so the
/// artifacts cannot be returned by value — the compile/lower tails instead
/// run as a continuation that receives them while the resolver is still
/// alive. The diagnostics engine is included (reborrowed) because the
/// compile tail needs `diag.format_all()` for its final result.
pub(super) struct AnalysisOutput<'a, 'd> {
    pub program: Program,
    pub type_checker: TypeChecker<'a>,
    pub graph: Option<SemanticGraph>,
    pub dmir_module: Module,
    pub optimizer: Optimizer,
    pub schedule_proof: Option<std::sync::Arc<crate::schedule::ScheduleProof>>,
    pub timings: CompilationTimings,
    pub diag: &'d mut DiagnosticEngine,
}

/// The analyze + lower tail shared by `lower_ast_to_dmir` and
/// `compile_ast_internal` (phases 3–9: derive expansion, resolver, type
/// checker, effects, ownership, security, DMIR lowering, optimizer + PGO).
///
/// Unlike the check pipeline this one early-returns a failure result after
/// each fallible phase (resolver, typecheck, ownership, security) — matching
/// the original behavior of both entry points. `build_graph` is passed by the
/// compile tail only, which builds the semantic graph between security and
/// lowering exactly where it did before.
///
/// On success the [`AnalysisOutput`] is passed to `finish`, which performs
/// the tail that differs per entry point (JIT-ready module extraction for
/// `lower_ast_to_dmir`, native codegen + linking for `compile_ast_internal`).
pub(super) fn run_analysis_and_lower<R>(
    compiler: &ForgenCompiler,
    mut program: Program,
    file: &str,
    diag: &mut DiagnosticEngine,
    total_start: Instant,
    mut timings: CompilationTimings,
    build_graph: bool,
    finish: impl FnOnce(AnalysisOutput<'_, '_>) -> R,
) -> Result<R, CompilationResult> {
    let base_dir = Path::new(file).parent();
    crate::cimport::expand_c_imports(&mut program, base_dir, diag);
    crate::rust_bridge::expand_rust_dependencies(&mut program, base_dir, diag);
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }
    crate::derive::expand_derives_and_comptime(&mut program);

    // 3. Resolver
    let res_start = Instant::now();
    let mut resolver = Resolver::new();
    resolver.resolve_program(&program, diag);
    timings.resolve_ms = res_start.elapsed().as_millis();
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }

    // 4. Type Checker
    let tc_start = Instant::now();
    let mut type_checker = TypeChecker::new(&resolver);
    type_checker.check_program(&program, diag);
    timings.typecheck_ms = tc_start.elapsed().as_millis();
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
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
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }

    // 7. Security & Zero-Trust Verifier (Proof-Carrying Code)
    let mut security = crate::security::SecurityVerifier::new(&resolver, &type_checker);
    security.verify_program(&program, diag);
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }

    // 8. Semantic Graph (compile tail only; skipped entirely by
    //    `lower_ast_to_dmir` and by `quick` mode).
    let mut graph = None;
    if build_graph {
        let graph_start = Instant::now();
        if compiler.mode != "quick" {
            graph = Some(SemanticGraph::build(&program, &resolver, &effects));
        }
        timings.graph_ms = graph_start.elapsed().as_millis();
    }

    // 8. DMIR Lowering
    let mut lowering = Lowering::new(&resolver, &type_checker);
    let mut dmir_module = lowering.lower_program(
        &program,
        Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main"),
    );

    // 8b. DMIR Abstract Interpretation Ownership Analyzer & Dual-Mode Lowering
    let own_abs_start = Instant::now();
    let mut dmir_analyzer = DmirOwnershipAnalyzer::new(&resolver);
    let mut signatures = HashMap::new();
    for decl in &program.declarations {
        match decl {
            crate::ast::Decl::Function(f)
            | crate::ast::Decl::Flow(f)
            | crate::ast::Decl::Task(f) => {
                signatures.insert(f.name.clone(), f.params.clone());
            }
            crate::ast::Decl::Class(c) => {
                for item in &c.body_items {
                    if let crate::ast::ClassItem::Method(m) = item {
                        signatures.insert(format!("{}_{}", c.name, m.name), m.params.clone());
                    }
                }
            }
            crate::ast::Decl::Behavior(b) => {
                for item in &b.body_items {
                    if let crate::ast::ClassItem::Method(m) = item {
                        signatures
                            .insert(format!("{}_{}", b.target_type, m.name), m.params.clone());
                    }
                }
            }
            crate::ast::Decl::Impl(i) => {
                for m in &i.methods {
                    signatures.insert(format!("{}_{}", i.target_type, m.name), m.params.clone());
                }
            }
            _ => {}
        }
    }
    dmir_analyzer.set_signatures(signatures);
    let ownership_reports = dmir_analyzer.analyze_and_lower_module(&mut dmir_module, diag);
    timings.ownership_ms += own_abs_start.elapsed().as_millis();
    if diag.has_errors() {
        timings.total_ms = total_start.elapsed().as_millis();
        let d_str = diag.format_all();
        return Err(CompilationResult::failure(
            d_str.clone(),
            d_str,
            Some(program),
            timings,
        ));
    }

    // 9. Optimizer
    let opt_start = Instant::now();
    let mut optimizer = Optimizer::new(&compiler.mode);
    let mut sorted_ownership_reports: Vec<(&String, &crate::ownership::OwnershipFunctionReport)> =
        ownership_reports.iter().collect();
    sorted_ownership_reports.sort_by_key(|(name, _)| (*name).clone());
    for (fn_name, rep) in sorted_ownership_reports {
        optimizer.record_ownership_report(fn_name, &rep.trace_line);
    }
    optimizer.set_function_effects(effects.function_effects.clone());
    // Deterministic reporting order: sort the specialization strings (the
    // underlying maps/sets are unordered).
    let mut spec_strs: Vec<String> = Vec::new();
    for (class_name, specs) in &type_checker.generic_specializations {
        for spec_args in specs {
            spec_strs.push(format!(
                "{}<{}>",
                class_name,
                spec_args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    spec_strs.sort();
    for spec_str in spec_strs {
        optimizer.report.generic_specializations.push(spec_str);
    }
    if let Err(diag) = optimizer.optimize_module(&mut dmir_module) {
        let msg = diag.message.clone();
        return Err(CompilationResult::failure_with_diagnostics(
            msg.clone(),
            msg,
            vec![diag],
            Some(program),
            timings,
        ));
    }
    if !optimizer.diagnostics.is_empty() {
        let msg = optimizer.diagnostics[0].message.clone();
        return Err(CompilationResult::failure_with_diagnostics(
            msg.clone(),
            msg,
            optimizer.diagnostics,
            Some(program),
            timings,
        ));
    }
    if let Some(ref pgo_path) = compiler.pgo_profile
        && let Ok(profile) = crate::pgo::ProfileData::load_from_file(pgo_path)
    {
        crate::pgo::ProfileGuidedOptimizer::optimize_module(
            &mut optimizer,
            &mut dmir_module,
            &profile,
        );
        if !optimizer.diagnostics.is_empty() {
            let msg = optimizer.diagnostics[0].message.clone();
            return Err(CompilationResult::failure_with_diagnostics(
                msg.clone(),
                msg,
                optimizer.diagnostics,
                Some(program),
                timings,
            ));
        }
    }
    if let Some(ref pgo_gen) = compiler.profile_generate {
        crate::pgo::ProfileInstrumenter::instrument_module(
            &mut dmir_module,
            Some(&pgo_gen.to_string_lossy()),
        );
    }
    timings.optimizer_ms = opt_start.elapsed().as_millis();

    // 10. Proof-Carrying Scheduler: Precompute ScheduleProof DAG and Kahn waves
    let schedule_proof = Some(std::sync::Arc::new(crate::schedule::ScheduleProof::build(
        &program,
        &dmir_module,
        &effects,
        &optimizer,
    )));

    Ok(finish(AnalysisOutput {
        program,
        type_checker,
        graph,
        dmir_module,
        optimizer,
        schedule_proof,
        timings,
        diag,
    }))
}
