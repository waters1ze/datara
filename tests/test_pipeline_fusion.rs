use forgen::driver::ForgenCompiler;

#[test]
fn test_string_polyhedral_fusion() {
    let source = r#"
fn format_quad(a: Str, b: Str, c: Str, d: Str) -> Str {
    return a + b + c + d
}

fn main() {
    out format_quad("Datara", "_", "Forgen", "_LLVM")
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "str_fusion_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let applied = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "StringConcatPolyhedralFusion" && r.decision == "Applied");
    assert!(
        applied,
        "StringConcatPolyhedralFusion pass must report Applied for multi-part string"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "Datara_Forgen_LLVM");
}

#[test]
fn test_arithmetic_pipeline_reassociation() {
    let source = r#"
fn compute_pipeline(x: Int) -> Int {
    let step1 = x + 10
    let step2 = step1 + 20
    let step3 = step2 + 30
    return step3
}

fn main() {
    out compute_pipeline(5)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "arith_fusion_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");
    let applied = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "ArithmeticPipelineReassociation" && r.decision == "Applied");
    assert!(
        applied,
        "ArithmeticPipelineReassociation pass must report Applied for chained additions"
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "65");
}
