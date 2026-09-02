use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;

#[test]
fn test_negative_generic_type_mismatch() {
    let source = r#"
fn same<T>(a: T, b: T) -> T {
    return a
}

fn main() {
    let res = same(10, "hello")
    out res
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_generic.dtr", None);
    assert!(
        !res.success,
        "Mismatched generic arguments must fail type checking"
    );

    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::TypeMismatch.as_str()) || diag.contains("Generic type parameter"),
        "Must report generic type mismatch error"
    );
}

#[test]
fn test_positive_generic_unification() {
    let source = r#"
fn same<T>(a: T, b: T) -> T {
    return a
}

fn main() {
    let res1 = same(100, 200)
    let res2 = same("foo", "bar")
    out res1
    out res2
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "good_generic.dtr", None);
    assert!(
        res.success,
        "Valid generic calls should succeed: {:?}",
        res.error
    );
}
