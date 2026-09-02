use forgen::driver::ForgenCompiler;

#[test]
fn test_real_cranelift_native_execution() {
    let source = r#"
fn compute_sum(a: Int, b: Int) -> Int => a + b

fn main() {
    let sum = compute_sum(150, 250)
    out sum
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_cranelift_native_run.dtr", None);
    assert!(
        res.success,
        "Native Cranelift compilation failed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(
        exe.exists(),
        "Executable must exist on disk: {}",
        exe.display()
    );

    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native exe");
    assert_eq!(code, 0, "Native executable should exit with code 0");
    assert_eq!(stdout.trim(), "400", "Native execution output mismatch");

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
}

#[test]
fn test_real_cranelift_floats_and_branches() {
    let source = r#"
fn evaluate(score: Float) -> Float {
    if score > 50.0 {
        return score * 2.0
    } else {
        return score / 2.0
    }
}

fn main() {
    let res = evaluate(75.5)
    out res
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_cranelift_float_branch.dtr", None);
    assert!(
        res.success,
        "Native Cranelift compilation failed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native exe");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "151");

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
}
