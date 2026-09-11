//! Phase 6 Test Suite: Tooling & Diagnostics Polish
//!
//! Validates:
//! 1. REPL piped session (multiline declaration, tabs/spaces, comments between declarations, evaluation)
//! 2. `forgen fmt --check` contract and idempotency (double pass is byte-for-byte identical with 0 diffs)
//! 3. `forgen doc` error on broken example in doc comments
//! 4. LSP structured diagnostics emitting standardized E-codes

use forgen::doc::generate_docs;
use forgen::fmt::{FormatOptions, format_source};
use forgen::lsp::{JsonRpcRequest, LspServer};
use forgen::repl::ReplSession;
use std::fs;

#[test]
fn test_repl_piped_multiline_session_with_comments() {
    let mut session = ReplSession::new();

    // 1. Standalone comment before any declaration
    let res = session.feed_line("// Setup math routines");
    assert!(res.is_none());

    // 2. Feed multiline function with tabs, spaces, and inner comments
    let lines = [
        "fn calculate_rect(w: Int, h: Int) -> Int {",
        "    // Area computation",
        "    let area = w * h",
        "    return area",
        "}",
    ];

    let mut func_res = None;
    for line in &lines {
        func_res = session.feed_line(line);
    }
    assert!(
        func_res.is_some(),
        "Closing brace of function must register declaration"
    );
    let decl_msg = func_res.unwrap();
    assert!(
        decl_msg.contains("calculate_rect"),
        "Should confirm calculate_rect registration: {decl_msg}"
    );

    // 3. Comments between declarations
    assert!(
        session
            .feed_line("// Intermediate section comment")
            .is_none()
    );
    assert!(
        session
            .feed_line("/* Multi-line style inline comment */")
            .is_none()
    );

    // 4. Feed record declaration
    let record_lines = [
        "record Dimensions {",
        "    width: Int,",
        "    height: Int",
        "}",
    ];
    let mut rec_res = None;
    for line in &record_lines {
        rec_res = session.feed_line(line);
    }
    assert!(rec_res.is_some(), "Record must register declaration");

    // 5. Feed variable instantiation
    let var_res = session.feed_line("let dims = Dimensions { width: 12, height: 5 }");
    assert!(var_res.is_some());
    assert!(var_res.unwrap().contains("dims"));

    // 6. Invoke function and output result
    let eval_res = session.feed_line("out calculate_rect(dims.width, dims.height)");
    assert!(eval_res.is_some(), "Expression should evaluate immediately");
    let out_str = eval_res.unwrap();
    assert!(
        out_str.contains("60"),
        "Expected output => 60, got: {out_str}"
    );
}

#[test]
fn test_fmt_check_and_idempotency() {
    let unformatted =
        "fn compute( a : Int , b : Int ) -> Int {\nlet res = a + b * 2\nreturn res\n}\n";
    let opts = FormatOptions::default();

    // First formatting pass
    let (formatted1, diffs1) = format_source(unformatted, &opts);
    assert!(
        !diffs1.is_empty(),
        "First pass must detect style differences"
    );
    assert!(formatted1.contains("fn compute("));

    // Second formatting pass on already formatted output (Idempotency)
    let (formatted2, diffs2) = format_source(&formatted1, &opts);
    assert!(
        diffs2.is_empty(),
        "Second pass must report 0 diffs on already formatted source"
    );
    assert_eq!(
        formatted1, formatted2,
        "Formatter must be byte-for-byte idempotent"
    );

    // Third pass to verify stability
    let (formatted3, diffs3) = format_source(&formatted2, &opts);
    assert!(diffs3.is_empty());
    assert_eq!(formatted2, formatted3);
}

#[test]
fn test_doc_broken_example_causes_generation_error() {
    let temp_dir = std::env::temp_dir().join("forgen_phase6_doc_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let good_file = temp_dir.join("good.dtr");
    fs::write(
        &good_file,
        "/// Computes square of a number.\n/// ```datara\n/// let val = 7\n/// out val * val\n/// ```\npub fn square(x: Int) -> Int {\n    return x * x\n}\n",
    )
    .unwrap();

    let out_html = temp_dir.join("target").join("doc").join("index.html");
    let gen_res = generate_docs(&temp_dir, &out_html);
    assert!(
        gen_res.is_ok(),
        "Doc generation with valid code example must succeed: {:?}",
        gen_res.err()
    );
    assert!(out_html.exists(), "Output HTML file must be generated");

    // Now write a file with a broken code example
    let bad_file = temp_dir.join("bad.dtr");
    fs::write(
        &bad_file,
        "/// This function contains a syntax error in doc example.\n/// ```datara\n/// let @@@broken_syntax_invalid@@@\n/// ```\npub fn bad_func() {\n}\n",
    )
    .unwrap();

    let bad_out_html = temp_dir.join("target").join("doc").join("bad_index.html");
    let bad_gen_res = generate_docs(&temp_dir, &bad_out_html);
    assert!(
        bad_gen_res.is_err(),
        "Doc generation with broken code example must fail with an error"
    );
    let err_msg = bad_gen_res.unwrap_err();
    assert!(
        err_msg.contains("Doc test failed"),
        "Error message must indicate doc test failure, got: {err_msg}"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_lsp_diagnostics_structured_e_codes() {
    let server = LspServer::new();
    let mut buf = Vec::new();

    let doc_uri = "file:///workspace/syntax_error.dtr";
    // Deliberate syntax error: invalid let syntax
    let doc_code = "fn bad() {\n    let = 42\n}\n";

    let open_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: "textDocument/didOpen".into(),
        params: Some(serde_json::json!({
            "textDocument": {
                "uri": doc_uri,
                "text": doc_code
            }
        })),
    };

    server.handle_request(&open_req, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("textDocument/publishDiagnostics"),
        "LSP must emit publishDiagnostics notification"
    );

    let json_start = output.find('{').expect("Must find JSON body start");
    let json_body = &output[json_start..];
    let parsed: serde_json::Value =
        serde_json::from_str(json_body).expect("Must parse valid JSON payload");

    let diags = parsed
        .pointer("/params/diagnostics")
        .and_then(|d| d.as_array())
        .expect("Diagnostics array must be present");

    assert!(
        !diags.is_empty(),
        "At least one diagnostic must be reported for syntax error"
    );

    for d in diags {
        let code = d
            .get("code")
            .and_then(|c| c.as_str())
            .expect("Code must be a string");
        assert!(
            code.starts_with('E'),
            "Diagnostic error code must be formatted as an E-code (e.g. E0001), got: {code}"
        );
        let source = d.get("source").and_then(|s| s.as_str()).unwrap_or("");
        assert_eq!(source, "forgen", "Diagnostic source must be 'forgen'");

        let range = d.get("range").expect("Diagnostic range must be present");
        assert!(range.get("start").is_some());
        assert!(range.get("end").is_some());
    }
}
