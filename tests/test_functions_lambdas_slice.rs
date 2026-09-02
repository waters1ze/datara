use forgen::driver::ForgenCompiler;

#[test]
fn test_functions_compact_bindings_and_lambdas() {
    let source = r#"
fn add(a: Int, b: Int) -> Int => a + b

fn compute(val: Int) -> Int {
    const MULTIPLIER: Int = 10
    let compact_val = val * MULTIPLIER
    let result = add(compact_val, 5)
    return result
}

fn main() {
    let x = 4
    let res = compute(x)
    out "Result: {res}"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_fn.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Result: 45"));
}

#[test]
fn test_colon_equal_rejected() {
    let source = r#"
fn main() {
    x := 10
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_ce_fail.dtr", None);
    assert!(!res.success);
    let err = res.error.unwrap_or_default();
    assert!(err.contains("Operator ':=' is deprecated"));
}
