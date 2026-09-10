use forgen::diagnostics::DiagnosticEngine;
use forgen::driver::ForgenCompiler;
use forgen::lexer::Lexer;
use forgen::parser::Parser;

#[test]
fn audit_parser_depth_at_boundary_63_64_65() {
    // 1. Depth L-1 = 63 (expression boundary)
    let s63 = "(".repeat(63) + "42" + &")".repeat(63);
    let mut diag63 = DiagnosticEngine::new("en");
    let mut lexer63 = Lexer::new(&s63, "depth63.dtr");
    let tokens63 = lexer63.tokenize(&mut diag63);
    let mut parser63 = Parser::new(tokens63, &mut diag63, "depth63.dtr");
    let expr63 = parser63.parse_expression();
    assert!(
        expr63.is_some(),
        "Depth 63 expression MUST parse successfully"
    );
    assert!(
        !diag63.has_errors(),
        "Depth 63 (L-1) MUST parse without errors!"
    );

    // 2. Depth L = 64 (current MAX_PARSE_DEPTH limit)
    let s64 = "(".repeat(64) + "42" + &")".repeat(64);
    let mut diag64 = DiagnosticEngine::new("en");
    let mut lexer64 = Lexer::new(&s64, "depth64.dtr");
    let tokens64 = lexer64.tokenize(&mut diag64);
    let mut parser64 = Parser::new(tokens64, &mut diag64, "depth64.dtr");
    let expr64 = parser64.parse_expression();
    assert!(
        expr64.is_some(),
        "Depth 64 expression MUST parse successfully"
    );
    assert!(
        !diag64.has_errors(),
        "Depth 64 (L) MUST parse without errors! Errors: {}",
        diag64.format_all()
    );

    // 3. Depth L+1 = 65 -> Diagnostic error without crashing/panicking
    let s65 = "(".repeat(65) + "42" + &")".repeat(65);
    let mut diag65 = DiagnosticEngine::new("en");
    let mut lexer65 = Lexer::new(&s65, "depth65.dtr");
    let tokens65 = lexer65.tokenize(&mut diag65);
    let mut parser65 = Parser::new(tokens65, &mut diag65, "depth65.dtr");
    let _ = parser65.parse_expression();
    assert!(
        diag65.has_errors(),
        "Depth 65 (L+1) MUST produce diagnostic error"
    );
    let err_str = diag65.format_all();
    assert!(
        err_str.contains("Expression nesting")
            || err_str.contains("E-SYNTAX-001")
            || err_str.contains("depth"),
        "Diagnostic must state depth limit exceeded: {}",
        err_str
    );
}

#[test]
fn audit_unicode_cyrillic_emoji_and_iteration() {
    let source = r#"
fn main() {
    let cyr = "Привет"
    out str_len(cyr)
    out str_chars(cyr)

    let emo = "🦀🚀🎉"
    out str_len(emo)
    out str_chars(emo)

    mut count = 0
    for ch in cyr {
        count = count + 1
    }
    out count
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "unicode_audit.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (stdout, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);

    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines[0], "12", "Cyrillic byte length must be 12");
    assert_eq!(lines[1], "6", "Cyrillic scalar count must be 6");
    assert_eq!(lines[2], "12", "3 emoji bytes (4 each) must be 12");
    assert_eq!(lines[3], "3", "3 emoji scalars must be 3");
    assert_eq!(
        lines[4], "6",
        "Iteration over Cyrillic must iterate 6 times"
    );
}
