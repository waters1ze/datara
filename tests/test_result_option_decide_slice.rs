use forgen::driver::ForgenCompiler;

#[test]
fn test_decide_match_and_result_recovery() {
    let source = r#"
fn classify(score: Int) -> Str {
    let status = decide {
        score >= 90 => "A",
        score >= 80 => "B",
        else => "C"
    }
    return status
}

fn handle_input(score: Int) -> Str {
    let res = classify(score)
    return "Grade: " + res
}

fn main() {
    let g1 = handle_input(95)
    let g2 = handle_input(82)
    let g3 = handle_input(70)
    out g1
    out g2
    out g3
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_decision.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Grade: A"));
    assert!(stdout.contains("Grade: B"));
    assert!(stdout.contains("Grade: C"));
}

#[test]
fn test_try_catch_rejected() {
    let source = r#"
fn main() {
    try {
        let x = 1
    } catch err {
        let y = 2
    }
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_try_fail.dtr", None);
    assert!(!res.success);
    let err = res.error.unwrap_or_default();
    assert!(err.contains("'try/catch' has been removed"));
}
