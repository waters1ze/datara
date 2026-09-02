use forgen::driver::ForgenCompiler;

#[test]
fn test_spec_fn_and_function_keyword_equivalence() {
    let src1 = "fn calc(x: Int) -> Int { return x + 10 }\nfn main() { out calc(5) }";
    let src2 = "function calc(x: Int) -> Int { return x + 10 }\nfunction main() { out calc(5) }";

    let compiler = ForgenCompiler::new("release");
    let res1 = compiler.compile_source(src1, "spec_fn.dtr", None);
    let res2 = compiler.compile_source(src2, "spec_function.dtr", None);

    assert!(res1.success, "fn keyword failed: {:?}", res1.error);
    assert!(res2.success, "function keyword failed: {:?}", res2.error);

    let (out1, _, _, _) = compiler
        .cranelift
        .run_executable(&res1.exe_path.unwrap(), &[])
        .unwrap();
    let (out2, _, _, _) = compiler
        .cranelift
        .run_executable(&res2.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(out1.trim(), "15");
    assert_eq!(out2.trim(), "15");
}

#[test]
fn test_spec_strict_bool_coercion_fails_on_int() {
    let src = r#"
fn main() {
    if 1 {
        out "invalid truthy"
    }
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.compile_source(src, "spec_strict_bool.dtr", None);
    assert!(
        !res.success,
        "Compiler must reject non-Bool condition in if statement"
    );
    let diag = res.diagnostics.to_lowercase();
    assert!(
        diag.contains("bool"),
        "Diagnostic must state condition must be Bool, got: {}",
        res.diagnostics
    );
}

#[test]
fn test_spec_integer_wrapping_arithmetic() {
    // 9223372036854775807 (i64::MAX) + 1 should wrap to -9223372036854775808 (i64::MIN)
    let src = r#"
fn main() {
    mut max = 9223372036854775807
    max = max + 1
    out max
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_wrap.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (out, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        out.trim(),
        "-9223372036854775808",
        "Integer addition must wrap by default"
    );
}

#[test]
fn test_spec_with_composition() {
    let src = r#"
role Printable {
    label() -> String
}

class Counter with Printable {
    count: Int
    label() -> String {
        return "Count"
    }
}

fn main() {
    mut c = Counter { count: 42 }
    out c.label()
    out c.count
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_with.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (out, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().map(|s| s.trim()).collect();
    assert_eq!(lines, vec!["Count", "42"]);
}
