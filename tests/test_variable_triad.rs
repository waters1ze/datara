//! Stage 1: Variable Declaration Triad Tests
//!
//! Verifies the exact semantics of let, mut, and al:
//! - let: immutable static, rejected on re-assignment, direct register SSA.
//! - mut: mutable static, locked to declared/inferred static type.
//! - al: gradual dynamic container, mutable only when prefixed with mut.

use forgen::driver::ForgenCompiler;

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
fn test_let_immutable_compiles_and_runs() {
    let out = run_datara(
        r#"
fn main() {
    let x: Int = 42
    let y = 58
    let z = x + y
    out z
}
"#,
        "test_let_success",
    );
    assert_eq!(out, "100");
}

#[test]
fn test_let_reassign_fails_compile() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(
        r#"
fn main() {
    let x: Int = 10
    x = 20
    out x
}
"#,
        "test_let_reassign_fail.dtr",
        None,
    );
    assert!(
        !res.success,
        "Reassigning 'let' variable must fail compilation"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("immutable") || diag.contains("E-IMMUTABLE-ASSIGN"),
        "Diagnostics must mention immutable: {}",
        diag
    );
}

#[test]
fn test_colon_equal_deprecated_fails() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(
        r#"
fn main() {
    x := 10
}
"#,
        "test_colon_equal_fail.dtr",
        None,
    );
    assert!(!res.success, "':=' operator must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("SyntaxError: Operator ':=' is deprecated. Use 'let' for immutable or 'mut' for mutable variables."),
        "Diagnostics must contain exact deprecation error: {}",
        diag
    );
}

#[test]
fn test_mut_variable_mutation_succeeds() {
    let out = run_datara(
        r#"
fn main() {
    mut x: Int = 10
    x = 25
    x = x + 15
    out x
}
"#,
        "test_mut_success",
    );
    assert_eq!(out, "40");
}

#[test]
fn test_mut_type_locking_fails_on_type_change() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(
        r#"
fn main() {
    mut x: Int = 10
    x = "illegal string type change"
    out x
}
"#,
        "test_mut_type_locking.dtr",
        None,
    );
    assert!(
        !res.success,
        "Changing static type of 'mut' variable must fail compilation"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("Type mismatch") || diag.contains("mismatch"),
        "Diagnostics must mention Type mismatch: {}",
        diag
    );
}

#[test]
fn test_val_immutable_and_mut_val_dynamics() {
    let out = run_datara(
        r#"
fn main() {
    val a = 77
    mut val b = 10
    b = 88
    out a + b
}
"#,
        "test_val_success",
    );
    assert_eq!(out, "165");
}

#[test]
fn test_val_without_mut_reassign_fails() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(
        r#"
fn main() {
    val constant_val = 100
    constant_val = 200
    out constant_val
}
"#,
        "test_val_reassign_fail.dtr",
        None,
    );
    assert!(
        !res.success,
        "Reassigning immutable 'val' without 'mut' must fail"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("immutable") || diag.contains("E-IMMUTABLE-ASSIGN"),
        "Diagnostics must mention immutable: {}",
        diag
    );
}
