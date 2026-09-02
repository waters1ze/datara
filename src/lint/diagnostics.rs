use crate::diagnostics::SourceSpan;
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct LintFix {
    pub replacement: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub code: &'static str,
    pub severity: LintSeverity,
    pub message: String,
    pub span: SourceSpan,
    pub help: Option<String>,
    pub note: Option<String>,
    pub fix: Option<LintFix>,
}

impl LintDiagnostic {
    pub fn new(code: &'static str, message: String, span: SourceSpan) -> Self {
        Self {
            code,
            severity: LintSeverity::Warning,
            message,
            span,
            help: None,
            note: None,
            fix: None,
        }
    }

    pub fn with_help(mut self, help: String) -> Self {
        self.help = Some(help);
        self
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.note = Some(note);
        self
    }

    pub fn with_fix(mut self, replacement: String) -> Self {
        self.fix = Some(LintFix {
            replacement,
            span: self.span.clone(),
        });
        self
    }

    /// Render diagnostic in Rust/Cargo style with colored ANSI output and source context.
    pub fn render(&self, source_code: Option<&str>) -> String {
        let (color_warn, color_blue, color_cyan, color_reset, color_bold) = if is_terminal() {
            ("\x1b[1;33m", "\x1b[1;34m", "\x1b[1;36m", "\x1b[0m", "\x1b[1m")
        } else {
            ("", "", "", "", "")
        };

        let sev_str = match self.severity {
            LintSeverity::Warning => "warning",
            LintSeverity::Info => "info",
        };

        let mut out = format!(
            "{}{}[{}]{}: {}{}{}\n",
            color_warn, sev_str, self.code, color_reset, color_bold, self.message, color_reset
        );

        out.push_str(&format!(
            " {}-->{} {}:{}:{}\n",
            color_blue, color_reset, self.span.file, self.span.start_line, self.span.start_col
        ));

        // Read source line if available
        let file_content = match source_code {
            Some(s) => Some(s.to_string()),
            None => fs::read_to_string(&self.span.file).ok(),
        };

        if let Some(src) = file_content {
            let lines: Vec<&str> = src.lines().collect();
            let line_idx = self.span.start_line.saturating_sub(1);
            if line_idx < lines.len() {
                let line_str = lines[line_idx];
                let line_num = self.span.start_line;
                out.push_str(&format!("  {}|{}\n", color_blue, color_reset));
                out.push_str(&format!(
                    "{}{:>3} |{} {}\n",
                    color_blue, line_num, color_reset, line_str
                ));

                // Caret pointer
                let indent = " ".repeat(self.span.start_col.saturating_sub(1));
                let width = if self.span.end_line == self.span.start_line
                    && self.span.end_col >= self.span.start_col
                {
                    (self.span.end_col - self.span.start_col + 1).max(1)
                } else {
                    1
                };
                let carets = "^".repeat(width);

                if let Some(help) = &self.help {
                    out.push_str(&format!(
                        "  {}|{} {}{}{} {}help: {}{}\n",
                        color_blue, color_reset, indent, color_warn, carets, color_cyan, help, color_reset
                    ));
                } else {
                    out.push_str(&format!(
                        "  {}|{} {}{}{}{}\n",
                        color_blue, color_reset, indent, color_warn, carets, color_reset
                    ));
                }
            }
        }

        if let Some(note) = &self.note {
            out.push_str(&format!(
                "  {}={} note: {}{}\n",
                color_blue, color_reset, note, color_reset
            ));
        }

        out
    }
}

fn is_terminal() -> bool {
    // In CI or test environments, avoid colors if NO_COLOR is set
    std::env::var("NO_COLOR").is_err()
}
