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

#[test]
fn test_loop_fold_quadratic_sum() {
    let source = r#"
fn compute(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + (i * i)
        i = i + 1
    }
    return sum
}

fn main() {
    out compute(5)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "loop_fold_quad_test.dtr", None);
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
        "LoopFold pass must report Applied for quadratic sum loop"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "30", "Sum of squares 0..4 must equal 30");
}

#[test]
fn test_loop_fold_float_sum() {
    let source = r#"
fn compute_float(n: Float) -> Float {
    mut sum = 0.0
    mut i = 0.0
    while i < n {
        sum = sum + i * 1.5
        i = i + 1.0
    }
    return sum
}

fn main() {
    out compute_float(10.0)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "loop_fold_float_test.dtr", None);
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
        "LoopFold pass must report Applied for float sum loop"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    // sum of (0..9)*1.5 = 45 * 1.5 = 67.5
    assert!(
        out.trim().starts_with("67.5"),
        "Float induction 0..10 * 1.5 must equal 67.5, got: {}",
        out
    );
}
