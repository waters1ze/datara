use forgen::driver::ForgenCompiler;

#[test]
fn test_pure_function_inlining_boosted() {
    let source = r#"
fn pure_cube(x: Int) -> Int {
    return x * x * x
}

fn main() {
    out pure_cube(4)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "pure_inline_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let rep = res
        .optimization_report
        .expect("optimization report missing");
    let inlined = rep
        .decision_trace
        .iter()
        .any(|r| r.pass == "Inlining" && r.candidate == "pure_cube" && r.decision == "Applied");
    assert!(
        inlined,
        "Pure function should be inlined via effect-guided pass"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "64", "4^3 must be 64");
}

#[test]
fn test_side_effect_function_preserved() {
    let source = r#"
fn impure_worker(x: Int) -> Int {
    out x
    return x + 10
}

fn main() {
    out impure_worker(5)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "impure_preserve_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let rep = res
        .optimization_report
        .expect("optimization report missing");
    let inlined = rep
        .decision_trace
        .iter()
        .any(|r| r.pass == "Inlining" && r.candidate == "impure_worker" && r.decision == "Applied");
    assert!(
        !inlined,
        "Function with side-effects must NOT be inlined across effect boundary"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    let lines: Vec<&str> = out.trim().lines().map(|s| s.trim()).collect();
    assert_eq!(
        lines,
        vec!["5", "15"],
        "Must print 5 from worker and 15 from main"
    );
}
