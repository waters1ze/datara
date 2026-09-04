//! Regression tests for the lexer's handling of unexpected characters.
//!
//! The tokenizer used to end with a bare `_ => {}`: since `self.advance()`
//! runs before the match, an unrecognised character was consumed and silently
//! discarded. `out 6 ^ 3` therefore compiled cleanly and printed `6`. Single
//! `&` and `|` were dropped the same way because their match arms had no
//! `else`. These tests pin down that such characters are now reported, while a
//! leading UTF-8 BOM is still accepted.

use forgen::driver::{CompilationResult, ForgenCompiler};

fn compile(source: &str, name: &str) -> CompilationResult {
    ForgenCompiler::new("release").compile_source_native(source, name, None)
}

#[test]
fn test_unexpected_character_is_reported() {
    for (ch, label) in [
        ("^", "caret"),
        ("~", "tilde"),
        ("@", "at sign"),
        ("$", "dollar"),
        ("#", "hash"),
        ("`", "backtick"),
    ] {
        let source = format!("fn main() {{\n    out 6 {} 3\n}}\n", ch);
        let res = compile(
            &source,
            &format!("test_lexer_unknown_{}.dtr", label.replace(' ', "_")),
        );

        assert!(
            !res.success,
            "character '{}' ({}) must be rejected, but compilation succeeded",
            ch, label
        );
        let err = res.error.unwrap_or_default();
        let expected = if ch == "@" {
            err.contains("Unexpected token: At") || err.contains("Unexpected character")
        } else {
            err.contains("Unexpected character")
        };
        assert!(
            expected,
            "character '{}' should produce an error, got: {}",
            ch, err
        );
    }
}

#[test]
fn test_lone_ampersand_and_pipe_are_reported() {
    // `&&` and `||` are valid; a single `&` or `|` is not.
    let res = compile("fn main() {\n    out 6 & 3\n}\n", "test_lexer_lone_amp.dtr");
    assert!(!res.success, "a lone '&' must be rejected");
    let err = res.error.unwrap_or_default();
    assert!(
        err.contains("&"),
        "error should name the character, got: {}",
        err
    );

    let res = compile(
        "fn main() {\n    out 6 | 3\n}\n",
        "test_lexer_lone_pipe.dtr",
    );
    assert!(!res.success, "a lone '|' must be rejected");
    let err = res.error.unwrap_or_default();
    assert!(
        err.contains("|"),
        "error should name the character, got: {}",
        err
    );
}

#[test]
fn test_double_ampersand_and_pipe_still_parse() {
    let res = compile(
        "fn main() {\n    out 1 && 1\n}\n",
        "test_lexer_double_amp.dtr",
    );
    assert!(res.success, "'&&' must still parse: {:?}", res.error);

    let res = compile(
        "fn main() {\n    out 1 || 0\n}\n",
        "test_lexer_double_pipe.dtr",
    );
    assert!(res.success, "'||' must still parse: {:?}", res.error);
}

#[test]
fn test_leading_utf8_bom_is_accepted() {
    // A byte-order mark is an encoding marker, not source text. Editors on
    // Windows add one routinely, so it must not break the build.
    let source = "\u{FEFF}fn main() {\n    out 42\n}\n";
    let res = compile(source, "test_lexer_bom.dtr");
    assert!(
        res.success,
        "a leading UTF-8 BOM must be accepted: {:?}",
        res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let compiler = ForgenCompiler::new("release");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42");

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
}

#[test]
fn test_unexpected_character_inside_string_is_fine() {
    // Characters that are invalid as operators are perfectly legal in strings.
    let res = compile(
        "fn main() {\n    out \"a@b#c$d%e\"\n}\n",
        "test_lexer_string_chars.dtr",
    );
    assert!(
        res.success,
        "characters inside a string literal must be fine: {:?}",
        res.error
    );
}
