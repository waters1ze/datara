//! Regression tests that prove the loop optimizer performs REAL transformations.
//!
//! The pre-existing tests in this area only asserted that a decision-trace
//! record exists, which stays green even when the pass is a no-op. These tests
//! instead assert:
//!   1. The loop-invariant computation is physically absent from the loop after
//!      optimization (structural proof, not a log line).
//!   2. The loop body is not duplicated (guards against a naive "unroll" that
//!      replicates instructions without restructuring control flow).
//!   3. The compiled program still produces the correct answer, including a
//!      loop that runs zero times.

use forgen::codegen::cranelift::CraneliftBackend;
use forgen::diagnostics::DiagnosticEngine;
use forgen::dmir::cfg::ControlFlowGraph;
use forgen::dmir::{Inst, Lowering};
use forgen::effects::EffectAnalyzer;
use forgen::lexer::Lexer;
use forgen::optimizer::Optimizer;
use forgen::parser::Parser;
use forgen::resolver::Resolver;
use forgen::types::TypeChecker;
use std::path::PathBuf;

fn lower(src: &str) -> forgen::dmir::Module {
    let mut diag = DiagnosticEngine::new("en");
    diag.set_source("licm.dtr", src);

    let mut lexer = Lexer::new(src, "licm.dtr");
    let tokens = lexer.tokenize(&mut diag);
    let mut parser = Parser::new(tokens, &mut diag, "licm.dtr");
    let program = parser.parse_program();
    assert!(!diag.has_errors(), "parse errors: {:?}", diag.diagnostics);

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program, &mut diag);
    assert!(!diag.has_errors(), "resolve errors: {:?}", diag.diagnostics);

    let mut tc = TypeChecker::new(&resolver);
    tc.check_program(&program, &mut diag);
    assert!(
        !diag.has_errors(),
        "typecheck errors: {:?}",
        diag.diagnostics
    );

    let mut effects = EffectAnalyzer::new();
    effects.analyze_program(&program);

    let mut lowering = Lowering::new(&resolver, &tc);
    lowering.lower_program(&program, "licm")
}

fn compile_and_run(module: &forgen::dmir::Module, tag: &str) -> String {
    let be = CraneliftBackend::for_host();
    let exe = PathBuf::from(format!("target/licm_proof_{}.exe", tag));
    let p = be
        .compile_native(module, &exe)
        .unwrap_or_else(|e| panic!("[{}] native compilation failed: {}", tag, e));
    let (out, err, code, _) = be
        .run_executable(&p, &[])
        .unwrap_or_else(|e| panic!("[{}] execution failed: {}", tag, e));
    assert_eq!(code, 0, "[{}] non-zero exit; stderr={}", tag, err);
    out.trim().to_string()
}

/// Instructions contained in the blocks forming natural loops of `count_iters`.
fn loop_instructions(module: &forgen::dmir::Module) -> Vec<Inst> {
    let f = module
        .functions
        .get("count_iters")
        .expect("count_iters must exist");
    let cfg = ControlFlowGraph::build(f);
    cfg.loops
        .iter()
        .flat_map(|lp| lp.blocks.iter())
        .filter_map(|b| f.get_block(*b))
        .flat_map(|b| b.instructions.iter())
        .cloned()
        .collect()
}

/// `step = k * 5` is loop-invariant but depends on a runtime parameter, so it
/// cannot be constant folded — only genuine LICM can remove it from the loop.
fn licm_source(call: &str) -> String {
    format!(
        r#"
fn count_iters(n: Int, k: Int) -> Int {{
    mut i = 0
    mut c = 0
    while i < n {{
        let step = k * 5
        c = c + step
        i = i + 1
    }}
    return c
}}
fn main() {{
    {}
}}
"#,
        call
    )
}

#[test]
fn licm_physically_removes_invariant_multiply_from_loop() {
    let src = licm_source("out count_iters(21, 3)");

    // Baseline: the multiply really is inside the loop before optimization.
    let unopt = lower(&src);
    let before = loop_instructions(&unopt);
    assert!(
        before
            .iter()
            .any(|i| matches!(i, Inst::BinOp { op, .. } if op == "*")),
        "precondition: the multiply must be inside the loop before optimization"
    );

    for mode in ["release", "domain"] {
        let mut module = lower(&src);
        let mut opt = Optimizer::new(mode);
        opt.optimize_module(&mut module);

        let after = loop_instructions(&module);
        assert!(
            !after
                .iter()
                .any(|i| matches!(i, Inst::BinOp { op, .. } if op == "*")),
            "[{}] LICM must hoist the invariant multiply out of the loop; \
             loop still contains: {:?}",
            mode,
            after
        );

        // The body must shrink, never grow. A naive unroller would inflate it.
        assert!(
            after.len() < before.len(),
            "[{}] loop should shrink after LICM (before={}, after={}); \
             growth indicates body duplication",
            mode,
            before.len(),
            after.len()
        );

        assert_eq!(
            compile_and_run(&module, mode),
            "315",
            "[{}] wrong result after optimization",
            mode
        );
    }
}

#[test]
fn licm_preserves_zero_and_single_trip_loops() {
    // A zero-trip loop is the dangerous case: hoisting work into a preheader
    // that the loop never reaches must not change the observable result.
    for (tag, call, expected) in [
        ("trip0", "out count_iters(0, 3)", "0"),
        ("trip1", "out count_iters(1, 3)", "15"),
        ("trip7", "out count_iters(7, 3)", "105"),
    ] {
        let src = licm_source(call);

        let unopt = lower(&src);
        assert_eq!(
            compile_and_run(&unopt, &format!("{}_ctrl", tag)),
            expected,
            "[{}] unoptimized baseline wrong",
            tag
        );

        for mode in ["release", "domain"] {
            let mut module = lower(&src);
            let mut opt = Optimizer::new(mode);
            opt.optimize_module(&mut module);
            assert_eq!(
                compile_and_run(&module, &format!("{}_{}", tag, mode)),
                expected,
                "[{}/{}] optimization changed program behaviour",
                tag,
                mode
            );
        }
    }
}

#[test]
fn loop_body_is_not_duplicated_by_any_pass() {
    // Guards against reintroducing the previous "unroll" implementation, which
    // replicated the body in place and left control flow untouched.
    let src = licm_source("out count_iters(21, 3)");
    let unopt = lower(&src);
    let baseline = loop_instructions(&unopt).len();

    for mode in ["release", "domain"] {
        let mut module = lower(&src);
        let mut opt = Optimizer::new(mode);
        opt.optimize_module(&mut module);
        let got = loop_instructions(&module).len();
        assert!(
            got <= baseline,
            "[{}] loop body grew from {} to {} instructions; \
             body duplication without control-flow restructuring is unsound",
            mode,
            baseline,
            got
        );
    }
}
