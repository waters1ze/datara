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

fn compile_datara_expect_error(code: &str, tag: &str, expected_code: &str) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, tag, None);
    assert!(
        !res.success,
        "Expected compilation failure for {}, but it succeeded",
        tag
    );
    let err = res.error.unwrap_or_default();
    assert!(
        err.contains(expected_code),
        "Expected error containing '{}' for {}, but got:\n{}",
        expected_code,
        tag,
        err
    );
}

// =========================================================================
// 1. MATCH EXHAUSTIVENESS & REACHABILITY (E0310, E0311)
// =========================================================================

#[test]
fn test_match_exhaustiveness_option_missing_none() {
    let code = r#"
fn check_opt(x: Option<Int>) -> Int {
    match x {
        Some(v) => v,
    }
}

fn main() {}
"#;
    compile_datara_expect_error(code, "test_match_opt_missing.dtr", "E0310");
}

#[test]
fn test_match_exhaustiveness_result_missing_err() {
    let code = r#"
fn check_res(r: Result<Int, String>) -> Int {
    match r {
        Ok(v) => v,
    }
}

fn main() {}
"#;
    compile_datara_expect_error(code, "test_match_res_missing.dtr", "E0310");
}

#[test]
fn test_match_exhaustiveness_enum_missing_variant() {
    let code = r#"
enum Status {
    Pending,
    Active,
    Suspended,
}

fn handle(s: Status) -> Int {
    match s {
        Status.Pending => 1,
        Status.Active => 2,
    }
}

fn main() {
    out handle(Status.Pending)
}
"#;
    compile_datara_expect_error(code, "test_match_enum_missing.dtr", "E0310");
}

#[test]
fn test_match_unreachable_pattern_after_catchall() {
    let code = r#"
enum Color {
    Red,
    Green,
}

fn code(c: Color) -> Int {
    match c {
        _ => 0,
        Color.Red => 1,
    }
}

fn main() {
    out code(Color.Red)
}
"#;
    compile_datara_expect_error(code, "test_match_unreachable_catchall.dtr", "E0311");
}

#[test]
fn test_match_duplicate_variant_rejected() {
    let code = r#"
enum Color {
    Red,
    Green,
}

fn code(c: Color) -> Int {
    match c {
        Color.Red => 1,
        Color.Red => 2,
        Color.Green => 3,
    }
}

fn main() {
    out code(Color.Red)
}
"#;
    compile_datara_expect_error(code, "test_match_duplicate.dtr", "E0311");
}

#[test]
fn test_match_exhaustive_success() {
    let code = r#"
enum TrafficLight {
    Red,
    Yellow,
    Green,
}

fn light_to_int(tl: TrafficLight) -> Int {
    match tl {
        TrafficLight.Red => 1,
        TrafficLight.Yellow => 2,
        TrafficLight.Green => 3,
    }
}

fn main() {
    let v = light_to_int(TrafficLight.Green)
    if v == 3 {
        out "MATCH_EXHAUSTIVE_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_match_exhaustive_success.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("MATCH_EXHAUSTIVE_OK"));
}

// =========================================================================
// 2. UNITS OF MEASURE & DIMENSIONAL ALGEBRA (E0420)
// =========================================================================

#[test]
fn test_units_of_measure_incompatible_addition() {
    let code = r#"
fn main() {
    let length: Float<m> = 10.0
    let time: Float<s> = 2.0
    let bad = length + time
    out bad
}
"#;
    compile_datara_expect_error(code, "test_measure_add.dtr", "E0420");
}

#[test]
fn test_units_of_measure_assignment_mismatch() {
    let code = r#"
fn main() {
    let mass: Float<kg> = 50.0
    let length: Float<m> = mass
    out length
}
"#;
    compile_datara_expect_error(code, "test_measure_assign.dtr", "E0420");
}

#[test]
fn test_units_of_measure_valid_dimensional_algebra() {
    let code = r#"
fn main() {
    let distance: Float<m> = 100.0
    let time: Float<s> = 5.0
    let speed: Float<m/s> = distance / time
    let ratio: Float = distance / distance
    if speed > 10.0 && ratio > 0.0 {
        out "DIMENSIONAL_ALGEBRA_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_units_valid.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("DIMENSIONAL_ALGEBRA_OK"));
}

// =========================================================================
// 3. RANGE/INTERVAL TYPES & BOUNDS CHECKING (E0947)
// =========================================================================

#[test]
fn test_range_type_out_of_bounds_assignment() {
    let code = r#"
fn main() {
    let byte_val: Int<0..255> = 300
    out byte_val
}
"#;
    compile_datara_expect_error(code, "test_range_oob.dtr", "E0947");
}

#[test]
fn test_range_type_negative_bounds_violation() {
    let code = r#"
fn main() {
    let positive: Int<1..100> = 0
    out positive
}
"#;
    compile_datara_expect_error(code, "test_range_neg.dtr", "E0947");
}

#[test]
fn test_static_array_index_out_of_bounds() {
    let code = r#"
fn main() {
    let arr = [10, 20, 30]
    let bad = arr[5]
    out bad
}
"#;
    compile_datara_expect_error(code, "test_array_oob.dtr", "E0947");
}

#[test]
fn test_static_array_negative_index_rejected() {
    let code = r#"
fn main() {
    let arr = [1, 2, 3]
    let bad = arr[-1]
    out bad
}
"#;
    compile_datara_expect_error(code, "test_array_neg_index.dtr", "E0947");
}

#[test]
fn test_range_types_valid_execution() {
    let code = r#"
fn main() {
    let val: Int<0..100> = 42
    let arr = [10, 20, 30, 40]
    let item = arr[2]
    if val == 42 && item == 30 {
        out "RANGE_SAFETY_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_range_valid.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("RANGE_SAFETY_OK"));
}

// =========================================================================
// 4. CLASS/STRUCT INVARIANTS (E0945)
// =========================================================================

#[test]
fn test_class_invariant_violation_on_mutation() {
    let code = r#"
class BankAccount {
    balance: Int
    invariant this.balance >= 0;

    fn withdraw_bad(amount: Int) {
        this.balance = -100
    }
}

fn main() {
    let acc = BankAccount { balance: 100 }
    acc.withdraw_bad(200)
}
"#;
    compile_datara_expect_error(code, "test_invariant_violation.dtr", "E0945");
}

#[test]
fn test_class_invariant_valid_execution() {
    let code = r#"
class BankAccount {
    balance: Int
    invariant this.balance >= 0;

    fn deposit(amount: Int) {
        this.balance = this.balance + amount
    }
}

fn main() {
    let acc = BankAccount { balance: 100 }
    acc.deposit(50)
    if acc.balance == 150 {
        out "CLASS_INVARIANT_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_invariant_valid.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("CLASS_INVARIANT_OK"));
}

// =========================================================================
// 5. TOTALITY & TERMINATION CHECKING (E0946)
// =========================================================================

#[test]
fn test_totality_infinite_loop_in_pure_rejected() {
    let code = r#"
@pure
fn infinite_loop_pure() -> Int {
    loop {
        let x = 1
    }
    out 0
}

fn main() {
    out infinite_loop_pure()
}
"#;
    compile_datara_expect_error(code, "test_pure_infinite_loop.dtr", "E0946");
}

#[test]
fn test_totality_infinite_while_true_in_pure_rejected() {
    let code = r#"
@pure
fn infinite_while_pure() -> Int {
    while true {
        let y = 2
    }
    out 0
}

fn main() {
    out infinite_while_pure()
}
"#;
    compile_datara_expect_error(code, "test_pure_while_true.dtr", "E0946");
}

#[test]
fn test_totality_termination_valid_execution() {
    let code = r#"
@pure
fn factorial(n: Int) -> Int decreases n {
    if n <= 1 {
        return 1
    } else {
        let prev = factorial(n - 1)
        return n * prev
    }
}

fn main() {
    let f = factorial(5)
    if f == 120 {
        out "TERMINATION_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_termination_valid.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("TERMINATION_OK"));
}
