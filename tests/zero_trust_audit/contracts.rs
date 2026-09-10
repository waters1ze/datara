use forgen::driver::ForgenCompiler;

#[test]
fn audit_contract_proven_no_runtime_check_in_clif() {
    let source = r#"
type NonZero = Int in 1..100

fn safe_div(a: Int, b: NonZero) -> Int
    require b != 0
{
    return a / b
}

fn main() {
    let d: NonZero = 2
    out safe_div(42, d)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "audit_contract_proven.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let clif = res.clif_source.expect("CLIF IR generated");
    // A proven contract eliminates runtime assertion panic calls
    assert!(
        !clif.contains("CONTRACT_VIOLATION") && !clif.contains("contract_violation"),
        "CLIF IR for proven contract must NOT contain runtime contract violation traps! Found:\n{}",
        clif
    );

    let (stdout, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "21");
}

#[test]
fn audit_contract_unproven_violation_traps_at_runtime() {
    let source = r#"
fn get_zero() -> Int {
    return 0
}

fn divide_contract(a: Int, b: Int) -> Int
    require b != 0, "CONTRACT_VIOLATION: zero divisor"
{
    return a / b
}

fn main() {
    let z = get_zero()
    out divide_contract(100, z)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "audit_contract_violation.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed (contract is dynamic): {:?}",
        res.error
    );

    let (_, stderr, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_ne!(
        code, 0,
        "Contract violation must trap with non-zero exit code"
    );
    assert!(
        stderr.contains("CONTRACT_VIOLATION") || stderr.contains("zero divisor"),
        "Error output must contain contract violation text, got: {}",
        stderr
    );
}

#[test]
fn audit_contract_invalid_contract_rejected_at_compile_time() {
    let source = r#"
fn broken_contract(a: Int) -> Int
    require nonexistent_symbol > 0
{
    return a
}

fn main() {
    out broken_contract(10)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "audit_invalid_contract.dtr", None);
    assert!(
        !res.success,
        "Compiler MUST reject invalid contract referencing undefined symbols"
    );
    assert!(
        res.diagnostics.contains("nonexistent_symbol")
            || res.diagnostics.to_lowercase().contains("undefined")
            || res.diagnostics.to_lowercase().contains("not found"),
        "Diagnostic must report undefined symbol in contract: {}",
        res.diagnostics
    );
}
