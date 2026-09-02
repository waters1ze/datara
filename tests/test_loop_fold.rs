use forgen::driver::ForgenCompiler;

#[test]
fn test_loop_fold_induction_sum() {
    let source = r#"
fn compute(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}

fn main() {
    out compute(10)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "loop_fold_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let applied = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "LoopFold" && r.decision == "Applied");
    assert!(
        applied,
        "LoopFold pass must report Applied for induction sum loop"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "45", "Gaussian sum 0..9 must equal 45");
}

#[test]
fn test_loop_fold_zero_trips() {
    let source = r#"
fn compute(n: Int) -> Int {
    mut sum = 100
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}

fn main() {
    out compute(0)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "loop_fold_zero_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(
        out.trim(),
        "100",
        "Zero trips must preserve initial accumulator"
    );
}
