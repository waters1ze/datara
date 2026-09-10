use forgen::lsp::{JsonRpcRequest, LspServer};
use serde_json::json;

#[test]
fn test_lsp_317_capabilities() {
    let server = LspServer::new();
    let mut buf = Vec::new();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "initialize".into(),
        params: None,
    };
    server.handle_request(&req, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(output.contains("\"inlayHintProvider\":true"));
    assert!(output.contains("\"codeActionProvider\":{\"codeActionKinds\":[\"quickfix\"]}"));
    assert!(output.contains("\"semanticTokensProvider\""));
}

#[test]
fn test_lsp_inlay_hints() {
    let server = LspServer::new();
    let mut buf = Vec::new();
    let doc_uri = "file:///workspace/hints.dtr";
    let doc_code = "fn compute() {\n    let count = 42\n    let title = \"Datara\"\n    let active = true\n}\n";

    // 1. Open document
    let open_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: "textDocument/didOpen".into(),
        params: Some(json!({
            "textDocument": {
                "uri": doc_uri,
                "text": doc_code
            }
        })),
    };
    server.handle_request(&open_req, &mut buf).unwrap();

    // 2. Request inlay hints
    buf.clear();
    let hint_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(2)),
        method: "textDocument/inlayHint".into(),
        params: Some(json!({
            "textDocument": { "uri": doc_uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 5, "character": 0 }
            }
        })),
    };
    server.handle_request(&hint_req, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains(": Int"),
        "Expected ': Int' hint, got: {}",
        output
    );
    assert!(
        output.contains(": Str"),
        "Expected ': Str' hint, got: {}",
        output
    );
    assert!(
        output.contains(": Bool"),
        "Expected ': Bool' hint, got: {}",
        output
    );
}

#[test]
fn test_lsp_code_actions_quickfix() {
    let server = LspServer::new();
    let mut buf = Vec::new();
    let doc_uri = "file:///workspace/match_test.dtr";

    let action_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(3)),
        method: "textDocument/codeAction".into(),
        params: Some(json!({
            "textDocument": { "uri": doc_uri },
            "range": {
                "start": { "line": 3, "character": 4 },
                "end": { "line": 6, "character": 5 }
            },
            "context": {
                "diagnostics": [
                    {
                        "code": "E0310",
                        "message": "Non-exhaustive match: pattern 'false' is not covered",
                        "range": {
                            "start": { "line": 3, "character": 4 },
                            "end": { "line": 6, "character": 5 }
                        }
                    }
                ]
            }
        })),
    };
    server.handle_request(&action_req, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(output.contains("Quick Fix: Add default match arm"));
    assert!(output.contains("_ => { return 0 }"));
}

#[test]
fn test_lsp_semantic_tokens_full() {
    let server = LspServer::new();
    let mut buf = Vec::new();
    let doc_uri = "file:///workspace/semantic.dtr";
    let doc_code = "class Point {\n    x: Int\n}\n\nfn main() -> Int {\n    let p = Point { x: 10 }\n    return p.x\n}\n";

    // 1. Open document
    let open_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: "textDocument/didOpen".into(),
        params: Some(json!({
            "textDocument": {
                "uri": doc_uri,
                "text": doc_code
            }
        })),
    };
    server.handle_request(&open_req, &mut buf).unwrap();

    // 2. Request semantic tokens
    buf.clear();
    let sem_req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(4)),
        method: "textDocument/semanticTokens/full".into(),
        params: Some(json!({
            "textDocument": { "uri": doc_uri }
        })),
    };
    server.handle_request(&sem_req, &mut buf).unwrap();
    let output = String::from_utf8(buf).unwrap();

    assert!(output.contains("\"data\":["));
    // Parse response and verify token count is divisible by 5
    let resp: serde_json::Value =
        serde_json::from_str(&output[output.find('{').unwrap()..]).unwrap();
    let data = resp
        .get("result")
        .unwrap()
        .get("data")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!data.is_empty(), "Semantic tokens data must not be empty");
    assert_eq!(
        data.len() % 5,
        0,
        "Semantic tokens data length must be a multiple of 5"
    );
}
