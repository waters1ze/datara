use forgen::driver::ForgenCompiler;
use std::fs;

fn run_datara(code: &str, tag: &str) -> (String, i32) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(code, tag, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        tag, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    (stdout.trim().replace("\r\n", "\n"), code)
}

#[test]
fn test_unproven_divisor_fails() {
    let code = r#"
fn divide(a: Int, b: Int) -> Int {
    return a / b
}

fn main() {
    let _ = divide(10, 2)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unproven_div.dtr", None);
    assert!(!res.success, "Unproven divisor must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0941"),
        "Diagnostics must contain error code E0941: {}",
        diag
    );
    assert!(
        diag.contains("Proof-Carrying Code Violation: Unproven divisor 'b' may be zero"),
        "Diagnostics must explain divisor 'b' may be zero: {}",
        diag
    );
}

#[test]
fn test_literal_zero_divisor_fails() {
    let code = r#"
fn div_zero(a: Int) -> Int {
    return a / 0
}

fn main() {
    let _ = div_zero(10)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "literal_zero_div.dtr", None);
    assert!(!res.success, "Literal zero divisor must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0941"),
        "Diagnostics must contain error code E0941: {}",
        diag
    );
    assert!(
        diag.contains("Unproven divisor '0' may be zero"),
        "Diagnostics must mention '0' may be zero: {}",
        diag
    );
}

#[test]
fn test_proven_divisor_with_contract_succeeds_and_runs() {
    let code = r#"
fn safe_div(a: Int, b: Int) -> Int {
    require b != 0
    return a / b
}

fn main() {
    let result = safe_div(42, 2)
    out fmt"DIV_RESULT={result}"
}
"#;
    let (out, code_ret) = run_datara(code, "proven_contract_div.dtr");
    assert_eq!(code_ret, 0);
    assert!(
        out.contains("DIV_RESULT=21"),
        "Output must contain DIV_RESULT=21: {:?}",
        out
    );
}

#[test]
fn test_proven_divisor_with_guard_succeeds_and_runs() {
    let code = r#"
fn safe_div_guard(a: Int, b: Int) -> Int {
    if b != 0 {
        return a / b
    }
    return -1
}

fn main() {
    let result = safe_div_guard(100, 5)
    out fmt"GUARD_RESULT={result}"
}
"#;
    let (out, code_ret) = run_datara(code, "proven_guard_div.dtr");
    assert_eq!(code_ret, 0);
    assert!(
        out.contains("GUARD_RESULT=20"),
        "Output must contain GUARD_RESULT=20: {:?}",
        out
    );
}

#[test]
fn test_proven_divisor_with_refinement_succeeds() {
    let code = r#"
type NonZero = Int where val != 0

fn div_nonzero(a: Int, b: NonZero) -> Int {
    return a / b
}

fn main() {
    let d: NonZero = 4
    let result = div_nonzero(40, d)
    out fmt"NONZERO_RESULT={result}"
}
"#;
    let (out, code_ret) = run_datara(code, "proven_refinement_div.dtr");
    assert_eq!(code_ret, 0);
    assert!(
        out.contains("NONZERO_RESULT=10"),
        "Output must contain NONZERO_RESULT=10: {:?}",
        out
    );
}

#[test]
fn test_zero_cost_proof_raw_sdiv() {
    let code = r#"
fn fast_div(a: Int, b: Int) -> Int {
    require b != 0
    return a / b
}

fn main() {
    let a = args_count() + 10
    let b = args_count() + 2
    let r = fast_div(a, b)
    out r
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "zero_cost_sdiv.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let clif = res.clif_source.expect("must produce clif");
    assert!(
        clif.contains("sdiv"),
        "Cranelift IR must contain raw sdiv instruction: {}",
        clif
    );
}

#[test]
fn test_unchecked_extern_c_call_fails() {
    let code = r#"
extern fn puts(s: String) -> Int

fn main() {
    puts("Hello C")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unchecked_ffi.dtr", None);
    assert!(
        !res.success,
        "Unchecked extern C call must fail compilation"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0942"),
        "Diagnostics must contain error code E0942: {}",
        diag
    );
    assert!(
        diag.contains(
            "Foreign call to extern function 'puts' requires 'unsafe(justification: \"...\")' block"
        ),
        "Diagnostics must explain missing unsafe justification: {}",
        diag
    );
}

#[test]
fn test_checked_extern_c_call_succeeds() {
    let code = r#"
extern fn puts(s: String) -> Int

fn main() {
    unsafe(justification: "Calling libc puts with valid null-terminated string") {
        puts("Hello C")
    }
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "checked_ffi.dtr", None);
    assert!(
        res.success,
        "Checked extern C call with justification must compile: {:?}",
        res.error
    );
}

#[test]
fn test_parallel_data_race_fails() {
    let code = r#"
fn race() {
    mut total: Int = 0
    parallel for i in 1..10 {
        total = total + i
    }
}

fn main() {
    race()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "parallel_race.dtr", None);
    assert!(!res.success, "Parallel data race must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0943"),
        "Diagnostics must contain error code E0943: {}",
        diag
    );
    assert!(
        diag.contains("Concurrency Violation: Potential data race on mutable variable 'total' accessed concurrently across threads"),
        "Diagnostics must explain data race on 'total': {}",
        diag
    );
}

#[test]
fn test_parallel_local_safe_succeeds() {
    let code = r#"
fn safe_parallel() {
    parallel for i in 1..10 {
        mut local: Int = 0
        local = local + i
    }
}

fn main() {
    safe_parallel()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "parallel_safe.dtr", None);
    assert!(
        res.success,
        "Safe parallel loop with local variable must compile: {:?}",
        res.error
    );
}

#[test]
fn test_parallel_data_race_unsafe_bypass() {
    let code = r#"
fn intentional_race() {
    mut total: Int = 0
    unsafe(justification: "Intentional lock-free accumulation benchmark") {
        parallel for i in 1..10 {
            total = total + i
        }
    }
}

fn main() {
    intentional_race()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "parallel_bypass.dtr", None);
    assert!(
        res.success,
        "Data race with explicit unsafe justification must compile: {:?}",
        res.error
    );
}
