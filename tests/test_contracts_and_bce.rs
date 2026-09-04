use forgen::driver::ForgenCompiler;

#[test]
fn test_contracts_and_evidence_gate_bce() {
    let source = r#"
fn safe_array_access(arr: List<Int>, idx: Int in 0..<arr.len()) -> Int
    require arr.len() > 0, "Список не должен быть пустым"
    ensure result >= 0
{
    return arr[idx]
}

fn main() {
    val items = [10, 20, 30, 40]
    val res = safe_array_access(items, 2)
    out res
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "safe_access_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");

    let bce_applied = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "EvidenceGate:BCE" && r.decision == "Applied");
    assert!(
        bce_applied,
        "EvidenceGate:BCE pass must report Applied for safe_array_access parameter refinement. Trace: {:?}",
        report.decision_trace
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "30", "arr[2] of [10, 20, 30, 40] must be 30");
}

#[test]
fn test_unproven_access_keeps_bounds_check() {
    let source = r#"
fn unchecked_candidate(arr: List<Int>, idx: Int) -> Int {
    return arr[idx]
}

fn main() {
    val items = [10, 20, 30]
    out unchecked_candidate(items, 1)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "unproven_access_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let report = res
        .optimization_report
        .expect("optimization report missing");

    let bce_applied = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "EvidenceGate:BCE" && r.candidate.contains("unchecked_candidate"));
    assert!(
        !bce_applied,
        "EvidenceGate:BCE must NOT report Applied when index lacks refinement or proof"
    );
}

#[test]
fn test_contract_precondition_failure() {
    let source = r#"
fn safe_array_access(arr: List<Int>, idx: Int in 0..<arr.len()) -> Int
    require arr.len() > 0, "Список не должен быть пустым"
    ensure result >= 0
{
    return arr[idx]
}

fn main() {
    val empty_items: List<Int> = []
    out safe_array_access(empty_items, 0)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "contract_fail_test.dtr", None);
    assert!(
        res.success,
        "Compilation should succeed, failure is runtime assertion: {:?}",
        res.error
    );

    let exe_path = res.exe_path.expect("executable path missing");
    let (_, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution should run");
    assert_ne!(
        code, 0,
        "Runtime precondition failure must yield non-zero exit code"
    );
    assert!(
        err.contains("Список не должен быть пустым"),
        "Error output must contain custom contract failure message, got: {}",
        err
    );
}
