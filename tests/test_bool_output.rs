//! Bool values must render as `true` / `false`, never as raw 0/1.
//!
//! Bools share the I64 machine representation with Int, so the backend tracks
//! which values are Bool (comparisons, logical operators, `!`, `ConstBool`,
//! Bool-typed variables, params and returns) and routes them to a dedicated
//! runtime printer. These tests pin the textual form down in release mode,
//! where the optimizer runs before the backend sees the IR.

use forgen::driver::ForgenCompiler;

/// Compile and run a Datara program, returning trimmed stdout with CRLF
/// normalised away.
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
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_bool_literals_and_comparisons() {
    let out = run_datara(
        r#"
fn main() {
    out true
    out false
    out 1 < 2
    out 2 == 3
    out 5 >= 5
}
"#,
        "test_bool_literals.dtr",
    );

    assert_eq!(out, "true\nfalse\ntrue\nfalse\ntrue");
}

#[test]
fn test_bool_via_variables_and_logic() {
    let out = run_datara(
        r#"
fn main() {
    mut a = false
    a = 5 > 3
    out a
    mut b = false
    b = a && false
    out b
    mut c = false
    c = b || 10 == 10
    out c
    mut d = false
    d = !c
    out d
}
"#,
        "test_bool_vars.dtr",
    );

    assert_eq!(out, "true\nfalse\ntrue\nfalse");
}

#[test]
fn test_bool_from_function_return() {
    let out = run_datara(
        r#"
fn is_adult(age: Int) -> Bool => age >= 18

fn main() {
    out is_adult(20)
    out is_adult(10)
    mut verdict = false
    verdict = is_adult(30)
    out verdict
}
"#,
        "test_bool_fn_return.dtr",
    );

    assert_eq!(out, "true\nfalse\ntrue");
}

#[test]
fn test_bool_string_interpolation() {
    let out = run_datara(
        r#"
fn main() {
    mut flag = false
    flag = 3 > 1
    out fmt"flag: {flag}"
}
"#,
        "test_bool_interp.dtr",
    );

    assert_eq!(out, "flag: true");
}

#[test]
fn test_ints_still_print_as_ints() {
    let out = run_datara(
        r#"
fn main() {
    out 1
    out 0
    out 1 + 1
    out 5 - 3
}
"#,
        "test_bool_int_guard.dtr",
    );

    // The Bool printer must only fire for Bool-typed values; arithmetic on
    // 0/1 integers must never be reinterpreted as `true`/`false`.
    assert_eq!(out, "1\n0\n2\n2");
}

#[test]
fn test_bool_in_if_condition_still_works() {
    let out = run_datara(
        r#"
fn main() {
    mut ready = false
    ready = 2 * 2 == 4
    if ready {
        out "yes"
    } else {
        out "no"
    }
}
"#,
        "test_bool_if.dtr",
    );

    assert_eq!(out, "yes");
}
