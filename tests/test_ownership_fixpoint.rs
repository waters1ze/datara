use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_loop_carried_move_use_proven_by_fixpoint() {
    let source = r#"
fn compute_loop_reset(n: Int) -> Int {
    mut i = 0
    mut total = 0
    while i < n {
        mut item = 100 + i
        destroy(item)
        item = (i + 1) * 10
        total = total + item
        i = i + 1
    }
    return total
}

fn main() {
    let res = compute_loop_reset(5)
    out fmt"Total: {res}"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "loop_fixpoint_proven.dtr", None);
    assert!(
        res.success,
        "Loop with loop-carried reinitialization proven by fixpoint must compile: {:?}",
        res.error
    );

    // Verify execution
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Total: 150"), "Output was: {}", stdout);

    // Verify ownership audit in semantic graph
    let graph = res.semantic_graph.expect("Semantic graph must be present");
    let opt_facts = graph
        .inspect_optimization("compute_loop_reset")
        .expect("Optimization facts must exist for compute_loop_reset");
    let opt_str = opt_facts.to_string();
    println!("compute_loop_reset optimization:\n{}", opt_str);
    assert!(
        opt_str.contains("proven"),
        "Must record proven ratio in inspect optimize: {}",
        opt_str
    );
    assert!(
        opt_str.contains("0% guarded") || opt_str.contains("0.0% guarded"),
        "Proven function must have 0% guarded: {}",
        opt_str
    );
}

#[test]
fn test_dynamic_conditional_move_guarded_at_runtime() {
    let source = r#"
fn dynamic_move_guard(flag: Int, v: Int) -> Int {
    if flag > 0 {
        destroy(v)
    }
    out v
    return v
}

fn main() {
    let r = dynamic_move_guard(0, 42)
    out fmt"Guarded Result: {r}"
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "dynamic_move_guard.dtr", None);
    assert!(
        res.success,
        "Conditional move with unproven join must graduate to runtime guards: {:?}",
        res.error
    );

    // Verify execution with runtime refcount guards
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(
        stdout.contains("Guarded Result: 42"),
        "Output was: {}",
        stdout
    );

    // Verify ownership audit records guarded percentage
    let rep = res
        .optimization_report
        .expect("Optimization report must be present");
    let trace_line = rep
        .ownership_reports
        .get("dynamic_move_guard")
        .expect("Ownership report must exist for dynamic_move_guard");
    println!("dynamic_move_guard trace:\n{}", trace_line);
    assert!(
        trace_line.contains("guarded"),
        "Trace must record guarded percentage: {}",
        trace_line
    );
    assert!(
        !trace_line.contains("0% guarded") && !trace_line.contains("0.0% guarded"),
        "Dynamic move function must have > 0% guarded: {}",
        trace_line
    );

    // Prove the guard path is not dead code: the emitted CLIF for the
    // guarded function must contain the runtime guard calls. Without this
    // check the "guarded" percentage could be pure analysis bookkeeping
    // while codegen silently drops the lowering.
    let clif = res.clif_source.expect("CLIF source must be present");
    println!("Guarded CLIF excerpt:\n{}", {
        let mut excerpt = String::new();
        let mut in_fn = false;
        for line in clif.lines() {
            if line.contains("function u0:dynamic_move_guard(") {
                in_fn = true;
            } else if in_fn && line.starts_with("function ") {
                break;
            }
            if in_fn {
                excerpt.push_str(line);
                excerpt.push('\n');
            }
        }
        excerpt
    });
    assert!(
        clif.contains("datara_rt_own_acquire") && clif.contains("datara_rt_own_release"),
        "Guarded function lowering must emit runtime ownership guard calls into CLIF"
    );
}

#[test]
fn test_ownership_audit_in_inspect_optimize() {
    let temp_dir = std::env::temp_dir().join("datara_ownership_audit_test");
    let _ = fs::create_dir_all(&temp_dir);
    let file_path = temp_dir.join("audit_sample.dtr");

    let source = r#"
fn calculate_sum(a: Int, b: Int) -> Int {
    let s = a + b
    return s
}

fn main() {
    let res = calculate_sum(10, 20)
    out fmt"Sum: {res}"
}
"#;
    fs::write(&file_path, source).unwrap();

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_file(&file_path, None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must exist");
    let opt_json = graph
        .inspect_optimization("calculate_sum")
        .expect("Optimization facts must exist for calculate_sum");
    let opt_str = opt_json.to_string();
    println!("inspect optimize calculate_sum:\n{}", opt_str);

    // Verify "X% proven, Y% guarded" appears in inspect optimize output
    assert!(
        opt_str.contains("proven") && opt_str.contains("guarded"),
        "inspect optimize output must contain 'X% proven, Y% guarded': {}",
        opt_str
    );
    assert!(
        opt_str.contains("100% proven") || opt_str.contains("100.0% proven"),
        "Pure scalar function must be 100% proven: {}",
        opt_str
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_definite_use_after_move_both_branches_rejected_by_fixpoint() {
    let source = r#"
fn both_branches_move(flag: Int, v: Int) -> Int {
    if flag > 0 {
        destroy(v)
    } else {
        destroy(v)
    }
    // Definite use-after-move: moved on all paths
    out v
    return v
}

fn main() {
    both_branches_move(1, 10)
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "both_branches_move.dtr", None);
    assert!(
        !res.success,
        "Definite use-after-move on all paths must be rejected as a compile error"
    );
    println!("Diagnostics:\n{}", res.diagnostics);
    assert!(
        res.diagnostics.contains("moved") || res.diagnostics.contains("BorrowUseAfterMove"),
        "Diagnostic must report use of moved value: {}",
        res.diagnostics
    );
}

#[test]
fn test_ownership_audit_guarded_demo_in_inspect_optimize() {
    let source = fs::read_to_string("examples/dynamic_guarded_demo.dtr")
        .expect("dynamic_guarded_demo.dtr must exist");

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(&source, "dynamic_guarded_demo.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must exist");
    let opt_json = graph
        .inspect_optimization("dynamic_move")
        .expect("Optimization facts must exist for dynamic_move");
    let opt_str = opt_json.to_string();
    println!("inspect optimize dynamic_move:\n{}", opt_str);

    // Verify guarded mode: dynamic_move has unproven conditional move
    assert!(
        opt_str.contains("guarded"),
        "Dynamic move function must contain guarded state: {}",
        opt_str
    );
    assert!(
        !opt_str.contains("0% guarded") && !opt_str.contains("0.0% guarded"),
        "Dynamic move function must have > 0% guarded: {}",
        opt_str
    );
    assert!(
        opt_str.contains("42.9% guarded"),
        "Dynamic move function must report 42.9% guarded: {}",
        opt_str
    );
}
