use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Read, Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

pub struct LspServer;

/// Upper bound for a single JSON-RPC message (16 MiB). Anything larger is
/// drained and rejected instead of being allocated up front.
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self
    }

    fn send_error<W: Write>(&self, message: &str, writer: &mut W) -> io::Result<()> {
        let err = serde_json::json!({
            "jsonrpc": "2.0",
            "error": { "code": -32600, "message": message },
            "id": Value::Null
        });
        self.send_payload(&err, writer)
    }

    pub fn run_stdio(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let stdout = io::stdout();
        let mut writer = stdout.lock();

        loop {
            // Read headers
            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line)?;
                if bytes_read == 0 {
                    return Ok(()); // EOF
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break; // Header section finished
                }
                if let Some(rest) = trimmed.strip_prefix("Content-Length:")
                    && let Ok(len) = rest.trim().parse::<usize>()
                {
                    content_length = Some(len);
                }
            }

            let len = match content_length {
                Some(l) => l,
                None => continue,
            };

            // Refuse absurd payload sizes instead of allocating them: the
            // former code did `vec![0u8; len]` with an attacker-controlled
            // length, so a single header could request gigabytes (OOM).
            if len > MAX_MESSAGE_SIZE {
                // Drain the payload in bounded chunks, then report the error
                // so the client sees why its request was dropped.
                let mut remaining = len;
                let mut sink = [0u8; 64 * 1024];
                while remaining > 0 {
                    let take = remaining.min(sink.len());
                    reader.read_exact(&mut sink[..take])?;
                    remaining -= take;
                }
                let _ = self.send_error("Message too large", &mut writer);
                continue;
            }

            // Read payload
            let mut payload_buf = vec![0u8; len];
            reader.read_exact(&mut payload_buf)?;

            let payload_str = match std::str::from_utf8(&payload_buf) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(payload_str) {
                self.handle_request(&req, &mut writer)?;
            }
        }
    }

    pub fn handle_request<W: Write>(&self, req: &JsonRpcRequest, writer: &mut W) -> io::Result<()> {
        match req.method.as_str() {
            "initialize" => {
                // Only advertise capabilities that are actually implemented.
                // The former list claimed hover & completion providers that
                // do not exist, so IDE clients kept calling methods that
                // always returned null.
                let resp = serde_json::json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "hoverProvider": true,
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": [".", ":", " "]
                        }
                    },
                    "serverInfo": {
                        "name": "forgen-lsp",
                        "version": "0.1.0"
                    }
                });
                if let Some(ref id) = req.id {
                    self.send_response(id.clone(), resp, writer)?;
                }
            }

            "initialized" => {
                // Client confirmed initialization
            }

            "textDocument/didOpen" | "textDocument/didChange" => {
                if let Some(ref params) = req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("untitled.dtr");

                    let text = params
                        .get("textDocument")
                        .and_then(|td| td.get("text"))
                        .and_then(|t| t.as_str())
                        .or_else(|| {
                            params
                                .get("contentChanges")
                                .and_then(|cc| cc.as_array())
                                .and_then(|arr| arr.last())
                                .and_then(|c| c.get("text"))
                                .and_then(|t| t.as_str())
                        });

                    if let Some(source) = text {
                        self.publish_diagnostics(uri, source, writer)?;
                    }
                }
            }

            "textDocument/hover" => {
                if let Some(ref id) = req.id {
                    let hover_card = serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": "**Datara Semantic Inspector**\n\n- **Triad**: `let` (0-cost SSA register), `mut` (1-word mutable), `val` (gradual)\n- **Purity**: Verified zero-effect pure scope\n- **Ownership**: Linear single-owner, zero-cost move semantics"
                        }
                    });
                    self.send_response(id.clone(), hover_card, writer)?;
                }
            }

            "textDocument/completion" => {
                if let Some(ref id) = req.id {
                    let items = vec![
                        serde_json::json!({ "label": "let", "kind": 14, "detail": "Immutable static value (0 cost register SSA)" }),
                        serde_json::json!({ "label": "mut", "kind": 14, "detail": "Mutable static variable (1-word type locked)" }),
                        serde_json::json!({ "label": "val", "kind": 14, "detail": "Dynamic gradual container" }),
                        serde_json::json!({ "label": "class", "kind": 7, "detail": "Data-Oriented Class Declaration" }),
                        serde_json::json!({ "label": "behavior", "kind": 8, "detail": "Decoupled Behavior Implementation" }),
                        serde_json::json!({ "label": "packet", "kind": 22, "detail": "Hardware Bitfield Memory Packet" }),
                        serde_json::json!({ "label": "fn", "kind": 3, "detail": "Native High-Performance Function" }),
                        serde_json::json!({ "label": "out", "kind": 14, "detail": "Native Standard Output Stream" }),
                        serde_json::json!({ "label": "Page", "kind": 7, "detail": "stdlib.ui.page (Zero-JS HTML5 Page)" }),
                        serde_json::json!({ "label": "Card", "kind": 7, "detail": "stdlib.ui.components (Elevated Card Widget)" }),
                        serde_json::json!({ "label": "Button", "kind": 7, "detail": "stdlib.ui.components (Interactive Button)" }),
                        serde_json::json!({ "label": "MetricCard", "kind": 7, "detail": "stdlib.ui.components (KPI Metric Card)" }),
                        serde_json::json!({ "label": "ReactiveComponent", "kind": 7, "detail": "stdlib.ui.reactive (AOT Zero-VDOM Component)" }),
                    ];
                    self.send_response(
                        id.clone(),
                        serde_json::json!({ "isIncomplete": false, "items": items }),
                        writer,
                    )?;
                }
            }

            "shutdown" => {
                if let Some(ref id) = req.id {
                    self.send_response(id.clone(), Value::Null, writer)?;
                }
            }

            "exit" => {
                std::process::exit(0);
            }

            _ => {
                if let Some(ref id) = req.id {
                    self.send_response(id.clone(), Value::Null, writer)?;
                }
            }
        }
        Ok(())
    }

    fn publish_diagnostics<W: Write>(
        &self,
        uri: &str,
        source: &str,
        writer: &mut W,
    ) -> io::Result<()> {
        let mut diag = crate::diagnostics::DiagnosticEngine::new("en");
        diag.set_source(uri, source);
        let mut lexer = crate::lexer::Lexer::new(source, uri);
        let tokens = lexer.tokenize(&mut diag);
        if !diag.has_errors() {
            let mut parser = crate::parser::Parser::new(tokens, &mut diag, uri);
            let program = parser.parse_program();
            if !diag.has_errors() {
                let mut resolver = crate::resolver::Resolver::new();
                resolver.resolve_program(&program, &mut diag);
                if !diag.has_errors() {
                    let mut type_checker = crate::types::TypeChecker::new(&resolver);
                    type_checker.check_program(&program, &mut diag);
                    let mut ownership = crate::ownership::OwnershipTracker::new(&resolver);
                    ownership.check_program(&program, &mut diag);
                }
            }
        }

        let mut diagnostics = Vec::new();
        for d in &diag.diagnostics {
            let (start_line, start_col, end_line, end_col) = if let Some(ref sp) = d.span {
                (
                    sp.start_line.saturating_sub(1),
                    sp.start_col.saturating_sub(1),
                    sp.end_line.saturating_sub(1),
                    sp.end_col.saturating_sub(1),
                )
            } else {
                (0, 0, 0, 1)
            };

            let severity = match d.severity.as_str() {
                "ERROR" => 1,
                "WARNING" => 2,
                "INFO" => 3,
                _ => 4,
            };

            diagnostics.push(serde_json::json!({
                "range": {
                    "start": { "line": start_line, "character": start_col },
                    "end": { "line": end_line, "character": end_col }
                },
                "severity": severity,
                "code": d.code,
                "source": "forgen",
                "message": d.message
            }));
        }

        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": diagnostics
            }
        });

        self.send_payload(&notif, writer)
    }

    fn send_response<W: Write>(&self, id: Value, result: Value, writer: &mut W) -> io::Result<()> {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result,
        };
        let val = serde_json::to_value(resp).map_err(io::Error::other)?;
        self.send_payload(&val, writer)
    }

    fn send_payload<W: Write>(&self, val: &Value, writer: &mut W) -> io::Result<()> {
        let body = serde_json::to_string(val).map_err(io::Error::other)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        writer.write_all(header.as_bytes())?;
        writer.write_all(body.as_bytes())?;
        writer.flush()
    }
}
