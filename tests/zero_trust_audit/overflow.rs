use super::helpers::*;

#[test]
fn audit_overflow_i64_max_plus_one_traps_all_backends_debug_and_release() {
    let source = r#"
fn main() {
    mut max = 9223372036854775807
    mut ovf = max + 1
    out ovf
}
"#;

    // 1. Cranelift (debug & release)
    for mode in ["debug", "release"] {
        let res = compile_and_run_cranelift(source, mode, &format!("ovf_clif_{}", mode));
        assert!(
            res.is_ok(),
            "Cranelift must compile and execute: {:?}",
            res.err()
        );
        let (stdout, stderr, code) = res.unwrap();
        assert_ne!(
            code, 0,
            "Cranelift ({}) must trap on i64::MAX + 1 with non-zero exit code! Got stdout='{}', stderr='{}', code={}",
            mode, stdout, stderr, code
        );
        assert_ne!(
            stdout.trim(),
            "-9223372036854775808",
            "Cranelift ({}) MUST NOT silently wrap!",
            mode
        );
    }

    // 2. LLVM (release)
    let res_llvm = compile_and_run_llvm(source, "release", "ovf_llvm");
    assert!(
        res_llvm.is_ok(),
        "LLVM must compile and execute: {:?}",
        res_llvm.err()
    );
    let (stdout_llvm, stderr_llvm, code_llvm) = res_llvm.unwrap();
    assert_ne!(
        code_llvm, 0,
        "LLVM must trap on i64::MAX + 1 with non-zero exit code! Got stdout='{}', stderr='{}', code={}",
        stdout_llvm, stderr_llvm, code_llvm
    );
    assert_ne!(
        stdout_llvm.trim(),
        "-9223372036854775808",
        "LLVM MUST NOT silently wrap!"
    );
    assert!(
        stderr_llvm.contains("integer overflow"),
        "LLVM runtime must output 'integer overflow' diagnostic to stderr, got: '{}'",
        stderr_llvm
    );

    // 3. WASM
    let res_wasm = compile_and_run_wasm(source, "ovf_wasm");
    assert!(
        res_wasm.is_err(),
        "WASM must trap (unreachable) on i64::MAX + 1, but succeeded with output: {:?}",
        res_wasm.ok()
    );
    let wasm_err = res_wasm.err().unwrap();
    assert!(
        wasm_err.contains("unreachable") || wasm_err.contains("overflow"),
        "WASM trap must indicate unreachable/overflow, got: {}",
        wasm_err
    );
}

#[test]
fn audit_overflow_i64_min_div_neg_one_traps_all_backends() {
    let source = r#"
fn safe_div(a: Int, b: Int) -> Int
    require b != 0
{
    return a / b
}

fn main() {
    let min = wrapping(-9223372036854775807 - 1)
    let neg_one = -1
    let div_ovf = safe_div(min, neg_one)
    out div_ovf
}
"#;

    // 1. Cranelift
    let res_clif = compile_and_run_cranelift(source, "release", "ovf_div_clif");
    assert!(res_clif.is_ok(), "Cranelift must run: {:?}", res_clif.err());
    let (stdout_clif, stderr_clif, code_clif) = res_clif.unwrap();
    assert_ne!(
        code_clif, 0,
        "Cranelift must trap on i64::MIN / -1! Got stdout='{}', stderr='{}', code={}",
        stdout_clif, stderr_clif, code_clif
    );

    // 2. LLVM
    let res_llvm = compile_and_run_llvm(source, "release", "ovf_div_llvm");
    assert!(res_llvm.is_ok(), "LLVM must run: {:?}", res_llvm.err());
    let (stdout_llvm, stderr_llvm, code_llvm) = res_llvm.unwrap();
    assert_ne!(
        code_llvm, 0,
        "LLVM must trap on i64::MIN / -1! Got stdout='{}', stderr='{}', code={}",
        stdout_llvm, stderr_llvm, code_llvm
    );
    assert!(
        code_llvm != 0 || stderr_llvm.contains("integer overflow"),
        "LLVM must trap on division overflow, got code={} stderr='{}'",
        code_llvm,
        stderr_llvm
    );

    // 3. WASM
    let res_wasm = compile_and_run_wasm(source, "ovf_div_wasm");
    assert!(
        res_wasm.is_err(),
        "WASM must trap on i64::MIN / -1, but returned: {:?}",
        res_wasm.ok()
    );
    let wasm_err = res_wasm.err().unwrap();
    assert!(
        wasm_err.contains("unreachable")
            || wasm_err.contains("overflow")
            || wasm_err.contains("divide result unrepresentable")
            || wasm_err.contains("integer")
            || wasm_err.contains("divide"),
        "WASM error must report division trap, got: {}",
        wasm_err
    );
}

#[test]
fn audit_overflow_wrapping_identity_all_backends() {
    let source = r#"
fn main() {
    mut max = 9223372036854775807
    mut wrapped = wrapping(max + 1)
    out wrapped
    mut restored = wrapping(wrapped - 1)
    out restored
}
"#;

    // 1. Cranelift
    let res_clif = compile_and_run_cranelift(source, "release", "wrap_id_clif").expect("clif run");
    assert_eq!(res_clif.2, 0, "Cranelift non-zero exit");
    let clif_lines: Vec<&str> = res_clif.0.lines().collect();
    assert_eq!(
        clif_lines,
        vec!["-9223372036854775808", "9223372036854775807"]
    );

    // 2. LLVM
    let res_llvm = compile_and_run_llvm(source, "release", "wrap_id_llvm").expect("llvm run");
    assert_eq!(res_llvm.2, 0, "LLVM non-zero exit");
    let llvm_lines: Vec<&str> = res_llvm.0.lines().collect();
    assert_eq!(
        llvm_lines,
        vec!["-9223372036854775808", "9223372036854775807"]
    );

    // 3. WASM
    let res_wasm = compile_and_run_wasm(source, "wrap_id_wasm").expect("wasm run");
    let wasm_lines: Vec<&str> = res_wasm.lines().collect();
    assert_eq!(
        wasm_lines,
        vec!["-9223372036854775808", "9223372036854775807"]
    );

    // Parity check across all 3
    assert_eq!(
        res_clif.0, res_llvm.0,
        "Cranelift and LLVM wrapping output differ"
    );
    assert_eq!(
        res_clif.0, res_wasm,
        "Cranelift and WASM wrapping output differ"
    );
}

#[test]
fn audit_overflow_saturating_all_backends() {
    let source = r#"
fn main() {
    mut max = 9223372036854775807
    mut sat_max = saturating(max + 10)
    out sat_max

    mut min = wrapping(-9223372036854775807 - 1)
    mut sat_min = saturating(min - 10)
    out sat_min
}
"#;

    // 1. Cranelift
    let res_clif = compile_and_run_cranelift(source, "release", "sat_clif").expect("clif run");
    assert_eq!(res_clif.2, 0, "Cranelift non-zero exit");
    let clif_lines: Vec<&str> = res_clif.0.lines().collect();
    assert_eq!(
        clif_lines,
        vec!["9223372036854775807", "-9223372036854775808"]
    );

    // 2. LLVM
    let res_llvm = compile_and_run_llvm(source, "release", "sat_llvm").expect("llvm run");
    assert_eq!(res_llvm.2, 0, "LLVM non-zero exit");
    let llvm_lines: Vec<&str> = res_llvm.0.lines().collect();
    assert_eq!(
        llvm_lines,
        vec!["9223372036854775807", "-9223372036854775808"]
    );

    // 3. WASM
    let res_wasm = compile_and_run_wasm(source, "sat_wasm").expect("wasm run");
    let wasm_lines: Vec<&str> = res_wasm.lines().collect();
    assert_eq!(
        wasm_lines,
        vec!["9223372036854775807", "-9223372036854775808"]
    );

    // Parity check across all 3
    assert_eq!(
        res_clif.0, res_llvm.0,
        "Cranelift and LLVM saturating output differ"
    );
    assert_eq!(
        res_clif.0, res_wasm,
        "Cranelift and WASM saturating output differ"
    );
}
