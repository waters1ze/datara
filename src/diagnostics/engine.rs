use super::codes::ErrorCode;
use super::span::SourceSpan;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub help: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiagnosticEngine {
    pub locale: String,
    pub diagnostics: Vec<Diagnostic>,
    pub source_map: HashMap<String, String>,
}

impl DiagnosticEngine {
    pub fn new(locale: &str) -> Self {
        Self {
            locale: locale.to_string(),
            diagnostics: Vec::new(),
            source_map: HashMap::new(),
        }
    }

    pub fn set_source(&mut self, file: &str, source: &str) {
        self.source_map.insert(file.to_string(), source.to_string());
    }

    pub fn error(&mut self, code: ErrorCode, message: String, span: Option<SourceSpan>) {
        self.error_with_help(code, message, span, None);
    }

    pub fn error_with_help(
        &mut self,
        code: ErrorCode,
        message: String,
        span: Option<SourceSpan>,
        help: Option<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            code: code.as_str().to_string(),
            severity: "ERROR".to_string(),
            message,
            span,
            help,
        });
    }

    pub fn warning(&mut self, code: ErrorCode, message: String, span: Option<SourceSpan>) {
        self.diagnostics.push(Diagnostic {
            code: code.as_str().to_string(),
            severity: "WARNING".to_string(),
            message,
            span,
            help: None,
        });
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == "ERROR")
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    pub fn format_all(&self) -> String {
        self.format_with_options(true)
    }

    pub fn format_plain(&self) -> String {
        self.format_with_options(false)
    }

    pub fn format_with_options(&self, use_color: bool) -> String {
        let (red_bold, yellow_bold, cyan_bold, blue_bold, dim, bold, reset) = if use_color {
            (
                "\x1b[1;31m",
                "\x1b[1;33m",
                "\x1b[1;36m",
                "\x1b[1;34m",
                "\x1b[2m",
                "\x1b[1m",
                "\x1b[0m",
            )
        } else {
            ("", "", "", "", "", "", "")
        };

        let mut out = String::new();
        for diag in &self.diagnostics {
            let (sev_color, sev_text) = if diag.severity == "ERROR" {
                (red_bold, "error")
            } else {
                (yellow_bold, "warning")
            };

            out.push_str(&format!(
                "{}{}[{}]{}: {}{}{}\n",
                sev_color, sev_text, diag.code, reset, bold, diag.message, reset
            ));

            if let Some(span) = &diag.span {
                let file_display = if span.file.is_empty() {
                    "<source>"
                } else {
                    &span.file
                };
                out.push_str(&format!(
                    "  {}-->{reset} {}:{}:{}\n",
                    blue_bold, file_display, span.start_line, span.start_col
                ));

                if let Some(src) = self.source_map.get(&span.file) {
                    let lines: Vec<&str> = src.lines().collect();
                    if span.start_line > 0 && span.start_line <= lines.len() {
                        let line_str = lines[span.start_line - 1];
                        let gutter = format!("{:>4} {}|{reset} ", span.start_line, blue_bold);
                        let empty_gutter = format!("     {}|{reset} ", blue_bold);

                        out.push_str(&empty_gutter);
                        out.push('\n');

                        out.push_str(&gutter);
                        out.push_str(line_str);
                        out.push('\n');

                        let indent = " ".repeat(span.start_col.saturating_sub(1));
                        let len = span.end_col.saturating_sub(span.start_col).max(1);
                        let carets = "^".repeat(len);
                        out.push_str(&empty_gutter);
                        out.push_str(&indent);
                        out.push_str(&format!("{}{}{reset}\n", red_bold, carets));
                    }
                }
                out.push_str(&format!("     {}|{reset}\n", blue_bold));

                if let Some(help) = &diag.help {
                    out.push_str(&format!(
                        "     {}={reset} {}help:{reset} {}\n",
                        blue_bold, cyan_bold, help
                    ));
                }
                out.push_str(&format!(
                    "     {}={reset} {}note: for more details, run 'forgen explain {}'{reset}\n\n",
                    blue_bold, dim, diag.code
                ));
            } else {
                if let Some(help) = &diag.help {
                    out.push_str(&format!("  {}help:{reset} {}\n", cyan_bold, help));
                }
                out.push_str(&format!(
                    "  {}note: for more details, run 'forgen explain {}'{reset}\n\n",
                    dim, diag.code
                ));
            }
        }
        out
    }
}
