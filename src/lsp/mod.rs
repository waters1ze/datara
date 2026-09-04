use crate::lexer::tokens::TokenType;
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

pub struct LspServer {
    documents: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

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
        Self {
            documents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
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
                let resp = serde_json::json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "hoverProvider": true,
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": [".", ":", " "]
                        },
                        "documentFormattingProvider": true,
                        "definitionProvider": true,
                        "inlayHintProvider": true,
                        "codeActionProvider": {
                            "codeActionKinds": ["quickfix"]
                        },
                        "semanticTokensProvider": {
                            "legend": {
                                "tokenTypes": [
                                    "keyword", "type", "function", "variable", "parameter",
                                    "string", "number", "operator", "comment", "struct"
                                ],
                                "tokenModifiers": ["declaration", "readonly", "static"]
                            },
                            "full": true
                        }
                    },
                    "serverInfo": {
                        "name": "forgen-lsp",
                        "version": env!("CARGO_PKG_VERSION")
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
                        if let Ok(mut docs) = self.documents.lock() {
                            docs.insert(uri.to_string(), source.to_string());
                        }
                        self.publish_diagnostics(uri, source, writer)?;
                    }
                }
            }

            "textDocument/formatting" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());

                    let source_opt = uri.and_then(|u| self.documents.lock().ok()?.get(u).cloned());

                    if let Some(source) = source_opt {
                        let (formatted, diffs) = crate::fmt::format_source(
                            &source,
                            &crate::fmt::FormatOptions::default(),
                        );
                        if diffs.is_empty() {
                            self.send_response(id.clone(), serde_json::json!([]), writer)?;
                        } else {
                            let line_count = source.lines().count().max(1);
                            let last_len = source.lines().last().map(|l| l.len()).unwrap_or(0);
                            let edits = serde_json::json!([{
                                "range": {
                                    "start": { "line": 0, "character": 0 },
                                    "end": { "line": line_count + 1, "character": last_len }
                                },
                                "newText": formatted
                            }]);
                            self.send_response(id.clone(), edits, writer)?;
                        }
                    } else {
                        self.send_response(id.clone(), serde_json::json!([]), writer)?;
                    }
                }
            }

            "textDocument/hover" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());
                    let pos = req.params.as_ref().and_then(|p| p.get("position"));
                    let line = pos
                        .and_then(|p| p.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as usize;
                    let col = pos
                        .and_then(|p| p.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as usize;

                    let word = uri.and_then(|u| {
                        let docs = self.documents.lock().ok()?;
                        let src = docs.get(u)?;
                        Self::word_at_position(src, line, col)
                    });

                    let hover_text = word.as_deref().and_then(Self::hover_info_for_word);

                    let card_content = hover_text.unwrap_or_else(|| {
                        "**Datara Semantic Inspector**\n\n- **Triad**: `let` (0-cost SSA register), `mut` (1-word mutable), `val` (gradual)\n- **Purity**: Verified zero-effect pure scope\n- **Ownership**: Linear single-owner, zero-cost move semantics".to_string()
                    });

                    let hover_card = serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": card_content
                        }
                    });
                    self.send_response(id.clone(), hover_card, writer)?;
                }
            }

            "textDocument/completion" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());

                    let mut items = vec![
                        serde_json::json!({ "label": "let", "kind": 14, "detail": "Immutable static value (0 cost register SSA)" }),
                        serde_json::json!({ "label": "mut", "kind": 14, "detail": "Mutable static variable (1-word type locked)" }),
                        serde_json::json!({ "label": "val", "kind": 14, "detail": "Dynamic gradual container" }),
                        serde_json::json!({ "label": "class", "kind": 7, "detail": "Data-Oriented Class Declaration" }),
                        serde_json::json!({ "label": "behavior", "kind": 8, "detail": "Decoupled Behavior Implementation" }),
                        serde_json::json!({ "label": "packet", "kind": 22, "detail": "Hardware Bitfield Memory Packet" }),
                        serde_json::json!({ "label": "fn", "kind": 3, "detail": "Native High-Performance Function" }),
                        serde_json::json!({ "label": "out", "kind": 14, "detail": "Native Standard Output Stream" }),
                        serde_json::json!({ "label": "uuid_v4", "kind": 3, "detail": "fn uuid_v4() -> Str (RFC 4122 v4 UUID)" }),
                        serde_json::json!({ "label": "sha256", "kind": 3, "detail": "fn sha256(data: Str) -> Str" }),
                        serde_json::json!({ "label": "int_to_str", "kind": 3, "detail": "fn int_to_str(v: Int) -> Str" }),
                        serde_json::json!({ "label": "str_len", "kind": 3, "detail": "fn str_len(s: Str) -> Int" }),
                        serde_json::json!({ "label": "str_trim", "kind": 3, "detail": "fn str_trim(s: Str) -> Str" }),
                        serde_json::json!({ "label": "Page", "kind": 7, "detail": "stdlib.ui.page (Zero-JS HTML5 Page)" }),
                        serde_json::json!({ "label": "Card", "kind": 7, "detail": "stdlib.ui.components (Elevated Card Widget)" }),
                        serde_json::json!({ "label": "Button", "kind": 7, "detail": "stdlib.ui.components (Interactive Button)" }),
                        serde_json::json!({ "label": "MetricCard", "kind": 7, "detail": "stdlib.ui.components (KPI Metric Card)" }),
                        serde_json::json!({ "label": "ReactiveComponent", "kind": 7, "detail": "stdlib.ui.reactive (AOT Zero-VDOM Component)" }),
                    ];

                    if let Some(u) = uri
                        && let Ok(docs) = self.documents.lock()
                        && let Some(src) = docs.get(u)
                    {
                        let dynamic_symbols = Self::extract_symbols_from_source(src);
                        for (sym, kind, detail) in dynamic_symbols {
                            items.push(serde_json::json!({
                                "label": sym,
                                "kind": kind,
                                "detail": detail
                            }));
                        }
                    }

                    self.send_response(
                        id.clone(),
                        serde_json::json!({ "isIncomplete": false, "items": items }),
                        writer,
                    )?;
                }
            }

            "textDocument/definition" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());
                    let pos = req.params.as_ref().and_then(|p| p.get("position"));
                    let line = pos
                        .and_then(|p| p.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as usize;
                    let col = pos
                        .and_then(|p| p.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as usize;

                    let def_loc = if let Some(u) = uri
                        && let Ok(docs) = self.documents.lock()
                        && let Some(src) = docs.get(u)
                        && let Some(w) = Self::word_at_position(src, line, col)
                    {
                        Self::find_definition_in_source(src, &w).map(
                            |(def_line, def_col, end_col)| {
                                serde_json::json!({
                                    "uri": u,
                                    "range": {
                                        "start": { "line": def_line, "character": def_col },
                                        "end": { "line": def_line, "character": end_col }
                                    }
                                })
                            },
                        )
                    } else {
                        None
                    };

                    self.send_response(id.clone(), def_loc.unwrap_or(Value::Null), writer)?;
                }
            }

            "textDocument/inlayHint" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());

                    let hints = if let Some(u) = uri
                        && let Ok(docs) = self.documents.lock()
                        && let Some(src) = docs.get(u)
                    {
                        Self::compute_inlay_hints(src)
                    } else {
                        Vec::new()
                    };

                    self.send_response(
                        id.clone(),
                        serde_json::to_value(hints).unwrap_or(serde_json::json!([])),
                        writer,
                    )?;
                }
            }

            "textDocument/codeAction" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("file.dtr");

                    let diags = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("context"))
                        .and_then(|c| c.get("diagnostics"))
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let actions = Self::compute_code_actions(uri, &diags);
                    self.send_response(
                        id.clone(),
                        serde_json::to_value(actions).unwrap_or(serde_json::json!([])),
                        writer,
                    )?;
                }
            }

            "textDocument/semanticTokens/full" => {
                if let Some(ref id) = req.id {
                    let uri = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("textDocument"))
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str());

                    let token_data = if let Some(u) = uri
                        && let Ok(docs) = self.documents.lock()
                        && let Some(src) = docs.get(u)
                    {
                        Self::compute_semantic_tokens(src)
                    } else {
                        Vec::new()
                    };

                    let resp = serde_json::json!({ "data": token_data });
                    self.send_response(id.clone(), resp, writer)?;
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

    fn word_at_position(source: &str, line: usize, col: usize) -> Option<String> {
        let line_str = source.lines().nth(line)?;
        let chars: Vec<(usize, char)> = line_str.char_indices().collect();
        if chars.is_empty() {
            return None;
        }
        let target_idx = col.min(chars.len().saturating_sub(1));
        let mut idx = target_idx;
        if !chars[idx].1.is_alphanumeric() && chars[idx].1 != '_' {
            if idx > 0 && (chars[idx - 1].1.is_alphanumeric() || chars[idx - 1].1 == '_') {
                idx -= 1;
            } else {
                return None;
            }
        }
        let mut start = idx;
        while start > 0 && (chars[start - 1].1.is_alphanumeric() || chars[start - 1].1 == '_') {
            start -= 1;
        }
        let mut end = idx;
        while end + 1 < chars.len()
            && (chars[end + 1].1.is_alphanumeric() || chars[end + 1].1 == '_')
        {
            end += 1;
        }
        let byte_start = chars[start].0;
        let byte_end = if end + 1 < chars.len() {
            chars[end + 1].0
        } else {
            line_str.len()
        };
        Some(line_str[byte_start..byte_end].to_string())
    }

    fn hover_info_for_word(w: &str) -> Option<String> {
        match w {
            "let" => Some("**let**: Immutable static register (0-cost SSA register, linear ownership)".into()),
            "mut" => Some("**mut**: Mutable variable (1-word type locked, fast stack allocate)".into()),
            "val" => Some("**val**: Gradual dynamic container (heterogeneous variant box)".into()),
            "fn" => Some("**fn**: High-performance native function declaration".into()),
            "class" => Some("**class**: Data-oriented class declaration (contiguous memory layout)".into()),
            "behavior" => Some("**behavior**: Decoupled behavior implementation for a class".into()),
            "packet" => Some("**packet**: Hardware-aligned bitfield memory packet".into()),
            "extern" => Some("**extern**: Foreign Function Interface (zero-overhead C ABI)".into()),
            "out" => Some("**out**: Datara streaming stdout pipe".into()),
            "uuid_v4" => Some("**fn uuid_v4() -> Str**\n\nGenerates a cryptographically secure RFC 4122 v4 UUID with OS entropy.".into()),
            "sha256" => Some("**fn sha256(data: Str) -> Str**\n\nComputes SHA-256 cryptographic hash of the input string.".into()),
            "base64_encode" => Some("**fn base64_encode(data: Str) -> Str**\n\nEncodes input string to standard Base64.".into()),
            "base64_decode" => Some("**fn base64_decode(data: Str) -> Str**\n\nDecodes standard Base64 string back into raw text.".into()),
            "int_to_str" => Some("**fn int_to_str(v: Int) -> Str**\n\nConverts a 64-bit integer to its decimal string representation.".into()),
            "str_len" => Some("**fn str_len(s: Str) -> Int**\n\nReturns the byte length of the string.".into()),
            "str_trim" => Some("**fn str_trim(s: Str) -> Str**\n\nTrims leading and trailing whitespace from string.".into()),
            "str_to_int" => Some("**fn str_to_int(s: Str) -> Int**\n\nParses string into 64-bit signed integer.".into()),
            "str_to_float" => Some("**fn str_to_float(s: Str) -> Float**\n\nParses string into 64-bit float.".into()),
            "sleep" => Some("**fn sleep(ms: Int)**\n\nSuspends current thread for the specified duration in milliseconds.".into()),
            "parallel_for" => Some("**fn parallel_for(start: Int, end: Int, fn)**\n\nDistributes loop iterations across hardware worker threads.".into()),
            "parallel_invoke" => Some("**fn parallel_invoke(fn1, fn2)**\n\nExecutes two functions concurrently using work-stealing.".into()),
            "num_workers" => Some("**fn num_workers() -> Int**\n\nReturns the number of active thread pool worker threads.".into()),
            _ => None,
        }
    }

    fn extract_symbols_from_source(source: &str) -> Vec<(String, usize, String)> {
        let mut symbols = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("fn ") {
                let name = rest.split('(').next().unwrap_or("").trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    symbols.push((name.to_string(), 3, format!("User Function: {}", trimmed)));
                }
            } else if let Some(rest) = trimmed.strip_prefix("class ") {
                let name = rest.split_whitespace().next().unwrap_or("").trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    symbols.push((name.to_string(), 7, "User Class".to_string()));
                }
            } else if let Some(rest) = trimmed.strip_prefix("let ") {
                let name = rest
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    symbols.push((name.to_string(), 6, "Local Constant".to_string()));
                }
            } else if let Some(rest) = trimmed.strip_prefix("mut ") {
                let name = rest
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    symbols.push((name.to_string(), 6, "Mutable Variable".to_string()));
                }
            }
        }
        symbols
    }

    fn find_definition_in_source(source: &str, word: &str) -> Option<(usize, usize, usize)> {
        for (line_idx, line) in source.lines().enumerate() {
            let prefixes = [
                "fn ",
                "class ",
                "behavior ",
                "packet ",
                "let ",
                "mut ",
                "val ",
            ];
            for pre in prefixes {
                if let Some(pos) = line.find(pre) {
                    let after = &line[pos + pre.len()..];
                    let sym = after
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    if sym == word {
                        let start_col = pos + pre.len();
                        let end_col = start_col + word.len();
                        return Some((line_idx, start_col, end_col));
                    }
                }
            }
        }
        None
    }

    pub fn compute_inlay_hints(source: &str) -> Vec<Value> {
        let mut diag = crate::diagnostics::DiagnosticEngine::new("en");
        let mut lexer = crate::lexer::Lexer::new(source, "file.dtr");
        let tokens = lexer.tokenize(&mut diag);
        let mut hints = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            if matches!(
                tokens[i].token_type,
                TokenType::Let | TokenType::Mut | TokenType::Val
            ) && i + 2 < tokens.len()
                && let TokenType::Identifier(ref _name) = tokens[i + 1].token_type
                && tokens[i + 2].token_type == TokenType::Equal
            {
                let inferred = if i + 3 < tokens.len() {
                    match &tokens[i + 3].token_type {
                        TokenType::IntLiteral(_) => "Int",
                        TokenType::FloatLiteral(_) => "Float",
                        TokenType::StringLiteral(_) | TokenType::InterpolatedString(_) => "Str",
                        TokenType::True | TokenType::False => "Bool",
                        TokenType::Identifier(cls)
                            if cls
                                .chars()
                                .next()
                                .map(|c| c.is_ascii_uppercase())
                                .unwrap_or(false) =>
                        {
                            cls.as_str()
                        }
                        TokenType::LParen => "Tuple",
                        TokenType::LBracket => "List",
                        _ => "Any",
                    }
                } else {
                    "Any"
                };

                let id_span = &tokens[i + 1].span;
                hints.push(serde_json::json!({
                    "position": {
                        "line": id_span.start_line.saturating_sub(1),
                        "character": id_span.end_col.saturating_sub(1)
                    },
                    "label": format!(": {}", inferred),
                    "kind": 1,
                    "paddingLeft": true,
                    "paddingRight": false
                }));
            }
            i += 1;
        }

        hints
    }

    pub fn compute_code_actions(uri: &str, diagnostics: &[Value]) -> Vec<Value> {
        let mut actions = Vec::new();

        for diag in diagnostics {
            let code = diag.get("code").and_then(|c| c.as_str()).unwrap_or("");
            let msg = diag.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let range = diag.get("range").cloned().unwrap_or(serde_json::json!({
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 }
            }));

            if code == "E0310" || msg.to_lowercase().contains("non-exhaustive") {
                let end_line = range
                    .get("end")
                    .and_then(|e| e.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0);
                actions.push(serde_json::json!({
                    "title": "Quick Fix: Add default match arm '_ => { ... }'",
                    "kind": "quickfix",
                    "diagnostics": [diag],
                    "edit": {
                        "changes": {
                            uri: [
                                {
                                    "range": {
                                        "start": { "line": end_line, "character": 0 },
                                        "end": { "line": end_line, "character": 0 }
                                    },
                                    "newText": "        _ => { return 0 }\n"
                                }
                            ]
                        }
                    }
                }));
            } else if code == "E0311" || msg.to_lowercase().contains("unreachable") {
                actions.push(serde_json::json!({
                    "title": "Quick Fix: Remove unreachable match arm",
                    "kind": "quickfix",
                    "diagnostics": [diag],
                    "edit": {
                        "changes": {
                            uri: [
                                {
                                    "range": range.clone(),
                                    "newText": ""
                                }
                            ]
                        }
                    }
                }));
            } else if msg.to_lowercase().contains("mutable") || msg.to_lowercase().contains("mut") {
                actions.push(serde_json::json!({
                    "title": "Quick Fix: Replace 'mut' with immutable 'let'",
                    "kind": "quickfix",
                    "diagnostics": [diag],
                    "edit": {
                        "changes": {
                            uri: [
                                {
                                    "range": range.clone(),
                                    "newText": "let"
                                }
                            ]
                        }
                    }
                }));
            }
        }

        actions
    }

    pub fn compute_semantic_tokens(source: &str) -> Vec<u32> {
        let mut diag = crate::diagnostics::DiagnosticEngine::new("en");
        let mut lexer = crate::lexer::Lexer::new(source, "file.dtr");
        let tokens = lexer.tokenize(&mut diag);

        let mut parsed_tokens: Vec<(usize, usize, usize, usize, usize)> = Vec::new();

        for tok in tokens {
            if tok.token_type == TokenType::Eof {
                continue;
            }
            let line = tok.span.start_line.saturating_sub(1);
            let col = tok.span.start_col.saturating_sub(1);
            let len = tok.lexeme.len().max(1);

            let token_type = match &tok.token_type {
                TokenType::Let
                | TokenType::Mut
                | TokenType::Val
                | TokenType::Const
                | TokenType::Fn
                | TokenType::Function
                | TokenType::Class
                | TokenType::Record
                | TokenType::Enum
                | TokenType::Component
                | TokenType::Role
                | TokenType::Behavior
                | TokenType::From
                | TokenType::Extends
                | TokenType::With
                | TokenType::Replaces
                | TokenType::Export
                | TokenType::Import
                | TokenType::As
                | TokenType::If
                | TokenType::Else
                | TokenType::For
                | TokenType::In
                | TokenType::While
                | TokenType::Loop
                | TokenType::Match
                | TokenType::When
                | TokenType::Decide
                | TokenType::Select
                | TokenType::Return
                | TokenType::Break
                | TokenType::Continue
                | TokenType::Parallel
                | TokenType::Async
                | TokenType::Await
                | TokenType::Task
                | TokenType::Flow
                | TokenType::Entity
                | TokenType::Process
                | TokenType::Then
                | TokenType::Unsafe
                | TokenType::Extern
                | TokenType::True
                | TokenType::False
                | TokenType::None
                | TokenType::Own
                | TokenType::View
                | TokenType::MutView
                | TokenType::Shared
                | TokenType::Out
                | TokenType::Err
                | TokenType::Use
                | TokenType::Try
                | TokenType::Catch
                | TokenType::Comptime => 0, // keyword

                TokenType::StringLiteral(_)
                | TokenType::InterpolatedString(_)
                | TokenType::CharLiteral(_) => 5, // string
                TokenType::IntLiteral(_) | TokenType::FloatLiteral(_) => 6, // number

                TokenType::ColonEqual
                | TokenType::FatArrow
                | TokenType::Arrow
                | TokenType::Pipe
                | TokenType::DotDot
                | TokenType::DotDotEq
                | TokenType::DotDotLt
                | TokenType::Plus
                | TokenType::Minus
                | TokenType::Star
                | TokenType::Slash
                | TokenType::Percent
                | TokenType::EqualEqual
                | TokenType::NotEqual
                | TokenType::Less
                | TokenType::LessEqual
                | TokenType::Greater
                | TokenType::GreaterEqual
                | TokenType::And
                | TokenType::Or
                | TokenType::Bang
                | TokenType::Question
                | TokenType::Equal => 7, // operator

                TokenType::Identifier(s) => {
                    if s.chars()
                        .next()
                        .map(|c| c.is_ascii_uppercase())
                        .unwrap_or(false)
                    {
                        1 // type / struct
                    } else {
                        3 // variable
                    }
                }
                _ => continue,
            };

            parsed_tokens.push((line, col, len, token_type, 0));
        }

        parsed_tokens.sort_by_key(|a| (a.0, a.1));

        let mut data = Vec::with_capacity(parsed_tokens.len() * 5);
        let mut prev_line = 0;
        let mut prev_char = 0;

        for (line, col, len, token_type, modifiers) in parsed_tokens {
            let delta_line = line.saturating_sub(prev_line);
            let delta_char = if delta_line == 0 {
                col.saturating_sub(prev_char)
            } else {
                col
            };

            data.push(delta_line as u32);
            data.push(delta_char as u32);
            data.push(len as u32);
            data.push(token_type as u32);
            data.push(modifiers as u32);

            prev_line = line;
            prev_char = col;
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_initialize_and_capabilities() {
        let server = LspServer::new();
        let mut buf = Vec::new();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "initialize".into(),
            params: None,
        };
        server.handle_request(&req, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"documentFormattingProvider\":true"));
        assert!(output.contains("\"definitionProvider\":true"));
        assert!(output.contains("\"hoverProvider\":true"));
    }

    #[test]
    fn test_lsp_hover_and_completion_and_definition() {
        let server = LspServer::new();
        let mut buf = Vec::new();

        let doc_uri = "file:///workspace/test.dtr";
        let doc_code = "fn calculate_total() -> Int {\n    let x = 42\n    return x\n}\n";

        // didOpen
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

        // hover
        buf.clear();
        let hover_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(2)),
            method: "textDocument/hover".into(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": doc_uri },
                "position": { "line": 0, "character": 1 } // on 'fn'
            })),
        };
        server.handle_request(&hover_req, &mut buf).unwrap();
        let hover_out = String::from_utf8(buf.clone()).unwrap();
        assert!(hover_out.contains("High-performance native function"));

        // completion
        buf.clear();
        let comp_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(3)),
            method: "textDocument/completion".into(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": doc_uri }
            })),
        };
        server.handle_request(&comp_req, &mut buf).unwrap();
        let comp_out = String::from_utf8(buf.clone()).unwrap();
        assert!(comp_out.contains("calculate_total"));
        assert!(comp_out.contains("uuid_v4"));

        // definition
        buf.clear();
        let def_req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(4)),
            method: "textDocument/definition".into(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": doc_uri },
                "position": { "line": 0, "character": 5 } // on 'calculate_total'
            })),
        };
        server.handle_request(&def_req, &mut buf).unwrap();
        let def_out = String::from_utf8(buf.clone()).unwrap();
        assert!(def_out.contains(doc_uri));
    }
}
