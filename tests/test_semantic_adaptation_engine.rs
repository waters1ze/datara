use forgen::driver::ForgenCompiler;
use forgen::optimizer::adaptive::{
    AdaptationCategory, AdaptationDecisionLog, ExecutionAdapter, StrategyAdapter,
};

#[test]
fn test_sae_representation_adaptation() {
    let source = "
class Point {
    x: Int
    y: Int
}
fn compute() -> Int {
    let p = Point { x: 10, y: 20 }
    return p.x + p.y
}
fn main() {
    let res = compute()
    out res
}
";

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "sae_point.dtr", None);
    assert!(res.success, "Compile failed: {:?}", res.error);

    let report = res.optimization_report.unwrap();
    let records = report.adaptation_records;

    // Verify SAE recorded physical representation decisions
    let scalar_decision = records.iter().find(|r| {
        r.category == AdaptationCategory::Representation
            && r.decision == "Candidate:PromoteToScalarSSA"
    });
    assert!(
        scalar_decision.is_some(),
        "Expected PromoteToScalarSSA decision in SAE report"
    );
    let dec = scalar_decision.unwrap();
    assert!(dec.benefit > 0.0);
    assert!(dec.reason.contains("Non-escaping aggregate candidate"));
    assert!(dec.evidence.contains("actual SROA must be proven in DMIR"));
}

#[test]
fn test_sae_execution_strategy_selection() {
    let mut log = AdaptationDecisionLog::new();

    // The adapter's contract is to report what the compiler can actually EMIT.
    // The Cranelift backend has no vector lowering, no thread pool wired into
    // codegen and no async runtime, so every input class must resolve to
    // `SequentialScalar`. An earlier revision returned SIMDVectorized /
    // ParallelThreadPool / AsyncTaskReactor here, which made the SAE report
    // claim optimizations that no pass ever performed. See
    // docs/AUDIT_OPTIMIZATION_FIXES.md.
    let strat_small =
        ExecutionAdapter::select_execution_strategy("loop_small", 500, true, false, 8, &mut log);
    assert_eq!(
        strat_small,
        forgen::optimizer::adaptive::execution::ExecutionStrategy::SequentialScalar
    );

    let strat_med =
        ExecutionAdapter::select_execution_strategy("loop_med", 50_000, true, false, 8, &mut log);
    assert_eq!(
        strat_med,
        forgen::optimizer::adaptive::execution::ExecutionStrategy::SequentialScalar
    );

    let strat_large = ExecutionAdapter::select_execution_strategy(
        "loop_large",
        2_000_000,
        true,
        false,
        8,
        &mut log,
    );
    assert_eq!(
        strat_large,
        forgen::optimizer::adaptive::execution::ExecutionStrategy::SequentialScalar
    );

    let strat_io =
        ExecutionAdapter::select_execution_strategy("loop_io", 100, false, true, 8, &mut log);
    assert_eq!(
        strat_io,
        forgen::optimizer::adaptive::execution::ExecutionStrategy::SequentialScalar
    );

    // Unknown trip count must also resolve to scalar rather than fabricate a count.
    let strat_unknown =
        ExecutionAdapter::select_execution_strategy("loop_unknown", 0, true, false, 8, &mut log);
    assert_eq!(
        strat_unknown,
        forgen::optimizer::adaptive::execution::ExecutionStrategy::SequentialScalar
    );

    assert_eq!(log.records.len(), 5);
    for r in &log.records {
        assert_eq!(r.category, AdaptationCategory::Execution);
        assert_eq!(r.decision, "SequentialScalar");
        assert!(!r.evidence.is_empty());
    }

    // The rationale must say WHY the fancier strategies were rejected, so the
    // report never implies capability that does not exist.
    let pure = &log.records[1].reason;
    assert!(
        pure.contains("SIMDVectorized NOT selected"),
        "reason was: {}",
        pure
    );
    assert!(
        pure.contains("ParallelThreadPool NOT selected"),
        "reason was: {}",
        pure
    );
    assert!(pure.contains("no vector lowering"), "reason was: {}", pure);

    let io = &log.records[3].reason;
    assert!(
        io.contains("AsyncTaskReactor NOT selected"),
        "reason was: {}",
        io
    );

    let unknown = &log.records[4].reason;
    assert!(
        unknown.contains("trip count not statically known"),
        "reason was: {}",
        unknown
    );
}

// The layout-padding test that lived here was removed along with
// `adaptive::LayoutAdapter`. That module computed a struct-layout plan that no
// consumer ever read: nothing in the optimizer or the Cranelift backend changed
// field order or padding, so the test only proved the planner produced a plan.

#[test]
fn test_sae_strategy_pipeline_and_dispatch() {
    let mut log = AdaptationDecisionLog::new();

    // 1. Single consumer pipeline -> SingleFusedLoop
    let pipe_fused =
        StrategyAdapter::select_pipeline_strategy("users_dataflow", 3, false, false, &mut log);
    assert_eq!(
        pipe_fused,
        forgen::optimizer::adaptive::strategy::PipelineStrategy::SingleFusedLoop
    );

    // 2. Multi consumer pipeline -> MaterializedBuffer
    let pipe_mat =
        StrategyAdapter::select_pipeline_strategy("shared_stream", 3, true, false, &mut log);
    assert_eq!(
        pipe_mat,
        forgen::optimizer::adaptive::strategy::PipelineStrategy::MaterializedBuffer(3)
    );

    // 3. Monomorphic pure small function -> DirectInlined
    let disp_inlined =
        StrategyAdapter::select_dispatch_strategy("main:call_calc", "calc", 1, true, 10, &mut log);
    assert_eq!(
        disp_inlined,
        forgen::optimizer::adaptive::strategy::CallDispatchStrategy::DirectInlined
    );

    // 4. Closed polymorphism -> PolymorphicInlineCache
    let disp_pic = StrategyAdapter::select_dispatch_strategy(
        "main:call_render",
        "render",
        3,
        false,
        50,
        &mut log,
    );
    assert_eq!(
        disp_pic,
        forgen::optimizer::adaptive::strategy::CallDispatchStrategy::PolymorphicInlineCache
    );
}

#[test]
fn test_compilation_ladder_check_quick_domain() {
    let source = "
fn compute_val(x: Int) -> Int {
    return x * 2 + 10
}
fn main() {
    let v = compute_val(42)
    out v
}
";

    // 1. Check mode: 0 binaries generated, static check only
    let check_compiler = ForgenCompiler::new("check");
    let check_res = check_compiler.check_source(source, "test_check.dtr");
    assert!(check_res.success);
    assert!(check_res.exe_path.is_none());

    // 2. Quick mode: minimal pipeline, fast executable
    let quick_compiler = ForgenCompiler::new("quick");
    let quick_res = quick_compiler.compile_source(source, "test_quick.dtr", None);
    assert!(quick_res.success);
    assert!(quick_res.exe_path.is_some());
    assert!(quick_res.semantic_graph.is_none()); // Graph skipped for speed

    // 3. Domain mode: full SAE + SemanticGraph
    let domain_compiler = ForgenCompiler::new("domain");
    let domain_res = domain_compiler.compile_source(source, "test_domain.dtr", None);
    assert!(domain_res.success);
    assert!(domain_res.exe_path.is_some());
    assert!(domain_res.semantic_graph.is_some());
    assert!(domain_res.optimization_report.is_some());
}
