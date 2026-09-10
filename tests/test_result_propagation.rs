//! Phase 4: `Result`/`?` propagation and Bool printing.
//!
//! Every test here is a structural or behavioral contract:
//! - `?` must unwrap on success and early-return the failed object on error;
//! - `?` is rejected where the error cannot propagate (no silent no-op);
//! - the `T!E` error channel must be `String` (Outcome representation);
//! - `return` values must match a Result/Option signature;
//! - Bools print `true`/`false`, including through SROA field forwarding.

use forgen::driver::ForgenCompiler;

fn compile_and_run(source: &str, name: &str) -> (String, bool, Option<String>) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, name, None);
    if !res.success {
        return (String::new(), false, res.error);
    }
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0, "executable exited non-zero for {}", name);
    (stdout, true, None)
}

fn expect_compile_error(source: &str, name: &str, needle: &str) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, name, None);
    assert!(
        !res.success,
        "{} was expected to fail compilation but succeeded",
        name
    );
    let diagnostics = res.diagnostics;
    assert!(
        diagnostics.contains(needle),
        "{} diagnostics missing '{}':\n{}",
        name,
        needle,
        diagnostics
    );
}

#[test]
fn test_question_success_and_error_paths() {
    let src = r#"
use stdlib.result.result.Outcome
fn find_user(id: Int) -> Outcome<String> {
    if id == 0 {
        return Outcome<String> { is_success: false, value: "", error_msg: "user not found" }
    }
    return Outcome<String> { is_success: true, value: "Alice", error_msg: "" }
}
fn greet(id: Int) -> Outcome<String> {
    let name = find_user(id)?
    return Outcome<String> { is_success: true, value: "hi " + name, error_msg: "" }
}
fn main() {
    let r1 = greet(1)
    out r1.value
    let r2 = greet(0)
    out r2.error_msg
}
"#;
    let (stdout, ok, err) = compile_and_run(src, "q_paths.dtr");
    assert!(ok, "compile failed: {:?}", err);
    assert!(stdout.contains("hi Alice"), "stdout: {}", stdout);
    assert!(stdout.contains("user not found"), "stdout: {}", stdout);
}

#[test]
fn test_question_option_maybe() {
    let src = r#"
use stdlib.result.option.Maybe
fn first(s: String) -> Maybe<Int> {
    if s == "" {
        return Maybe<Int> { is_some: false, value: 0 }
    }
    return Maybe<Int> { is_some: true, value: 42 }
}
fn answer(s: String) -> Maybe<Int> {
    let n = first(s)?
    return Maybe<Int> { is_some: true, value: n + 1 }
}
fn main() {
    let a = answer("x")
    out a.value
    let b = answer("")
    out b.is_some
}
"#;
    let (stdout, ok, err) = compile_and_run(src, "q_maybe.dtr");
    assert!(ok, "compile failed: {:?}", err);
    assert!(stdout.contains("43"), "stdout: {}", stdout);
    assert!(stdout.contains("false"), "stdout: {}", stdout);
}

#[test]
fn test_bang_result_syntax_end_to_end() {
    let src = r#"
use stdlib.result.result.Outcome
fn parse(s: String) -> Int!String {
    if s == "deep" {
        return Outcome<Int> { is_success: true, value: 42, error_msg: "" }
    }
    return Outcome<Int> { is_success: false, value: 0, error_msg: "wrong" }
}
fn chain() -> Int!String {
    let n = parse("deep")?
    return Outcome<Int> { is_success: true, value: n * 2, error_msg: "" }
}
fn main() {
    let ok = chain()
    out ok.is_success
    out ok.value
}
"#;
    let (stdout, ok, err) = compile_and_run(src, "q_bang.dtr");
    assert!(ok, "compile failed: {:?}", err);
    assert!(stdout.contains("true"), "stdout: {}", stdout);
    assert!(stdout.contains("84"), "stdout: {}", stdout);
}

#[test]
fn test_question_rejected_on_non_result_operand() {
    expect_compile_error(
        r#"
fn triple(x: Int) -> Int {
    return x * 3
}
fn main() {
    let n = triple(4)?
    out n
}
"#,
        "q_non_result_operand.dtr",
        "'?' requires a Result",
    );
}

#[test]
fn test_question_rejected_in_non_propagating_function() {
    // The enclosing function returns a bare Int: there is nothing to
    // propagate the error into, so this must be a hard error, not a silent
    // no-op unwrap.
    expect_compile_error(
        r#"
use stdlib.result.result.Outcome
fn parse(s: String) -> Int!String {
    return Outcome<Int> { is_success: false, value: 0, error_msg: "bad" }
}
fn main() -> Int {
    let n = parse("x")?
    return n
}
"#,
        "q_non_propagating_fn.dtr",
        "must return the same Result/Option type to propagate",
    );
}

#[test]
fn test_question_kind_must_match_signature() {
    // Propagating a Result error through a function that returns Maybe is a
    // type error: the failed Outcome object cannot pose as a None.
    expect_compile_error(
        r#"
use stdlib.result.result.Outcome
use stdlib.result.option.Maybe
fn parse(s: String) -> Int!String {
    return Outcome<Int> { is_success: false, value: 0, error_msg: "bad" }
}
fn try_it(s: String) -> Maybe<Int> {
    let n = parse(s)?
    return Maybe<Int> { is_some: true, value: n }
}
"#,
        "q_kind_mismatch.dtr",
        "'?' propagates a Result error",
    );
}

#[test]
fn test_error_channel_must_be_string() {
    // `T!E` sugar is represented by Outcome<T>, whose error channel is the
    // fixed String field `error_msg`; any other channel has no
    // representation and must be rejected.
    expect_compile_error(
        r#"
fn load(id: Int) -> Int!Int {
    return 1
}
"#,
        "q_err_channel.dtr",
        "must be String",
    );
}

#[test]
fn test_return_must_match_result_signature() {
    // Returning the bare payload from a Result-returning function silently
    // drops the error channel, so it must be rejected.
    expect_compile_error(
        r#"
use stdlib.result.result.Outcome
fn load(id: Int) -> Outcome<Int> {
    return id
}
"#,
        "q_return_mismatch.dtr",
        "function signature returns",
    );
}
