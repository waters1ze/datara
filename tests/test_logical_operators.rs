//! Regression tests for short-circuit logical operators.
//!
//! Before this fix, `a && b` and `a || b` were lowered to `Inst::BinOp` with
//! op "&&" / "||". The Cranelift backend had no arm for those strings and its
//! catch-all fell through to `iadd`, so `5 && 3` compiled to `5 + 3` and
//! printed `8`. These tests pin down the truth table, short-circuit side
//! effects, and use inside a loop condition.

use forgen::driver::ForgenCompiler;

/// Compile and run a Datara program, returning trimmed stdout.
fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, None);
    assert!(
        res.success,
        "compilation failed for {}: {:?}",
        name, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", name, code);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    // Normalise Windows CRLF so the assertions below can use "\n".
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_logical_operators_truth_table() {
    let out = run_datara(
        r#"
fn main() {
    out 5 && 3
    out 5 && 0
    out 0 && 3
    out 0 && 0
    out 5 || 3
    out 5 || 0
    out 0 || 3
    out 0 || 0
}
"#,
        "test_logic_truth_table.dtr",
    );

    // `&&` must yield a Bool (printed `true`/`false`), never the sum of its
    // operands.
    assert_eq!(
        out, "true\nfalse\nfalse\nfalse\ntrue\ntrue\ntrue\nfalse",
        "logical truth table is wrong"
    );
}

#[test]
fn test_logical_operators_short_circuit() {
    let out = run_datara(
        r#"
fn boom() -> Int {
    out 999
    return 1
}

fn main() {
    let a = 0 && boom()
    out a
    let b = 1 || boom()
    out b
    let c = 1 && boom()
    out c
    let d = 0 || boom()
    out d
}
"#,
        "test_logic_short_circuit.dtr",
    );

    // `999` is printed only when the right operand is actually evaluated:
    // not for `0 && boom()` and not for `1 || boom()`.
    assert_eq!(
        out, "false\ntrue\n999\ntrue\n999\ntrue",
        "short-circuit evaluation is wrong: the right operand must be skipped \
         when the left operand already decides the result"
    );
}

#[test]
fn test_logical_operators_guard_against_division_by_zero() {
    let out = run_datara(
        r#"
fn main() {
    mut x = 0
    if x != 0 && 10 / x > 1 {
        out 111
    } else {
        out 222
    }
}
"#,
        "test_logic_division_guard.dtr",
    );

    // Without short-circuiting this divides by zero.
    assert_eq!(out, "222", "the division guard must short-circuit");
}

#[test]
fn test_logical_operators_in_loop_condition() {
    let out = run_datara(
        r#"
fn main() {
    mut i = 0
    mut count = 0
    while i < 10 && count < 3 {
        count = count + 1
        i = i + 1
    }
    out i
    out count
}
"#,
        "test_logic_loop_condition.dtr",
    );

    // The loop must stop on `count < 3`. It stops at i == 3 only if the back
    // edge re-enters at the FIRST condition block and re-tests `i < 10`.
    assert_eq!(out, "3\n3", "loop with a short-circuit condition is wrong");
}

#[test]
fn test_logical_operators_chain() {
    let out = run_datara(
        r#"
fn main() {
    out 1 && 1 && 0
    out 1 && 1 && 1
    out 0 || 0 || 1
    out 0 || 0 || 0
}
"#,
        "test_logic_chain.dtr",
    );

    assert_eq!(
        out, "false\ntrue\ntrue\nfalse",
        "chained logical operators are wrong"
    );
}
