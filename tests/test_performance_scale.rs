use forgen::driver::ForgenCompiler;

#[test]
fn test_scale_method_inlining_and_sroa() {
    let source = r#"
class Point {
    x: Int
    y: Int
}

behavior Point {
    dist_sq() -> Int {
        return this.x * this.x + this.y * this.y
    }
}

fn main() {
    mut total = 0
    mut i = 0
    while i < 1000 {
        mut p = Point { x: i, y: 2 }
        total = total + p.dist_sq()
        i = i + 1
    }
    out total
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "scale_sroa_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let inlined = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "Inlining" && r.decision == "Applied");
    assert!(
        inlined,
        "Inlining must report Applied for Point_dist_sq method"
    );
    assert!(
        report.allocations_eliminated >= 1,
        "SROA must eliminate the Point struct allocation (got {})",
        report.allocations_eliminated
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    // Sum_{i=0..999} (i^2 + 4) = 999*1000*1999/6 + 4000 = 332833500 + 4000 = 332837500
    assert_eq!(out.trim(), "332837500");
}

#[test]
fn test_scale_generalized_loop_folding_with_scale_and_inclusive_bound() {
    let source = r#"
fn compute_scaled(n: Int) -> Int {
    mut sum = 0
    mut i = 1
    while i <= n {
        sum = sum + i * 2
        i = i + 1
    }
    return sum
}

fn main() {
    out compute_scaled(1000)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "scale_loop_fold_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let loop_folded = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "LoopFold" && r.decision == "Applied");
    assert!(
        loop_folded,
        "LoopFold must report Applied for affine loop with <= operator"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    // 2 * (1000 * 1001 / 2) = 1001000
    assert_eq!(out.trim(), "1001000");
}

#[test]
fn test_scale_loop_fold_multiplied_invariant() {
    let source = r#"
fn compute_k(n: Int) -> Int {
    mut sum = 0
    mut i = 1
    while i <= n {
        sum = sum + 3 * i
        i = i + 1
    }
    return sum
}

fn main() {
    out compute_k(10)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "scale_loop_fold_k.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let loop_folded = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "LoopFold" && r.decision == "Applied");
    assert!(
        loop_folded,
        "LoopFold must report Applied for affine loop with 3 * i"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    // 3 * (10 * 11 / 2) = 165
    assert_eq!(out.trim(), "165");
}
