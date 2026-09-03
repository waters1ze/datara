pub mod diagnostics;
pub mod rules;

use crate::diagnostics::DiagnosticEngine;
use crate::lexer::Lexer;
use crate::parser::Parser;
pub use diagnostics::{LintDiagnostic, LintFix, LintSeverity};
use std::fs;
use std::path::Path;

pub fn lint_source(source: &str, file_name: &str) -> Result<Vec<LintDiagnostic>, String> {
    let mut diag_engine = DiagnosticEngine::new("en");
    diag_engine.set_source(file_name, source);

    let mut lexer = Lexer::new(source, file_name);
    let tokens = lexer.tokenize(&mut diag_engine);
    if diag_engine.has_errors() {
        return Err(diag_engine.format_all());
    }

    let mut parser = Parser::new(tokens, &mut diag_engine, file_name);
    let program = parser.parse_program();
    if diag_engine.has_errors() {
        return Err(diag_engine.format_all());
    }

    let mut diags = rules::run_all_rules(&program);

    // Sort diagnostics by line and column
    diags.sort_by(|a, b| {
        a.span
            .start_line
            .cmp(&b.span.start_line)
            .then_with(|| a.span.start_col.cmp(&b.span.start_col))
    });

    Ok(diags)
}

pub fn lint_file(path: &Path) -> Result<Vec<LintDiagnostic>, String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Cannot read file {}: {}", path.display(), e))?;
    let file_str = path.to_string_lossy().to_string();
    lint_source(&source, &file_str)
}

/// Apply non-destructive automated fixes (--fix) to source code.
pub fn apply_fixes(source: &str, diags: &[LintDiagnostic]) -> String {
    let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();

    // Apply fixes in reverse order of line number so line indices remain stable
    let mut fixable: Vec<&LintDiagnostic> = diags.iter().filter(|d| d.fix.is_some()).collect();
    fixable.sort_by(|a, b| {
        b.span
            .start_line
            .cmp(&a.span.start_line)
            .then_with(|| b.span.start_col.cmp(&a.span.start_col))
    });

    for diag in fixable {
        if let Some(_fix) = &diag.fix {
            let line_idx = diag.span.start_line.saturating_sub(1);
            if line_idx < lines.len() {
                let line = &lines[line_idx];
                // If it's mut -> let replacement
                if diag.code == "perf::unnecessary_mut"
                    && let Some(mut_pos) = line.find("mut ")
                {
                    let mut new_line = line.clone();
                    new_line.replace_range(mut_pos..mut_pos + 4, "let ");
                    lines[line_idx] = new_line;
                }
            }
        }
    }

    let mut result = lines.join("\n");
    if source.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}
