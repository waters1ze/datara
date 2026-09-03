//! Official Datara & Forgen Code Formatter Rules
//!
//! Handles indentation, operator spacing, loop & branch formatting,
//! blank line normalization, and style conformance.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOptions {
    pub check: bool,
    pub indent: bool,
    pub operators: bool,
    pub loops: bool,
    pub blank_lines: bool,
    pub style: bool,
    pub mut_fix: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            check: false,
            indent: true,
            operators: true,
            loops: true,
            blank_lines: true,
            style: false,
            mut_fix: false,
        }
    }
}

impl FormatOptions {
    pub fn all() -> Self {
        Self {
            check: false,
            indent: true,
            operators: true,
            loops: true,
            blank_lines: true,
            style: true,
            mut_fix: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDiff {
    pub line_number: usize,
    pub rule: &'static str,
    pub original: String,
    pub formatted: String,
}

/// Helper to tokenize a line into code segments vs string/comment literals
#[derive(Debug)]
enum Segment<'a> {
    Code(String),
    StringLiteral(&'a str),
    LineComment(&'a str),
}

fn split_code_and_literals(line: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut chars = line.char_indices().peekable();
    let mut code_buf = String::new();

    while let Some((i, ch)) = chars.next() {
        if ch == '/' && chars.peek().map(|&(_, c)| c) == Some('/') {
            // Rest of line is comment
            if !code_buf.is_empty() {
                segments.push(Segment::Code(std::mem::take(&mut code_buf)));
            }
            segments.push(Segment::LineComment(&line[i..]));
            return segments;
        } else if ch == '"' {
            if !code_buf.is_empty() {
                segments.push(Segment::Code(std::mem::take(&mut code_buf)));
            }
            let start = i;
            let mut escaped = false;
            let mut end = line.len();
            for (j, c) in chars.by_ref() {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    end = j + 1;
                    break;
                }
            }
            segments.push(Segment::StringLiteral(&line[start..end]));
        } else {
            code_buf.push(ch);
        }
    }

    if !code_buf.is_empty() {
        segments.push(Segment::Code(code_buf));
    }

    segments
}

/// Formats operator spacing in a raw code segment
pub fn format_operators_in_code(code: &str) -> String {
    let mut res = String::with_capacity(code.len() + 16);
    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Multi-char operators: ==, !=, <=, >=, +=, -=, *=, /=, &&, ||, ->, ..
        if i + 1 < len {
            let next = chars[i + 1];
            if (ch == '=' && next == '=')
                || (ch == '!' && next == '=')
                || (ch == '<' && next == '=')
                || (ch == '>' && next == '=')
                || (ch == '+' && next == '=')
                || (ch == '-' && next == '=')
                || (ch == '*' && next == '=')
                || (ch == '/' && next == '=')
                || (ch == '&' && next == '&')
                || (ch == '|' && next == '|')
                || (ch == '|' && next == '>')
                || (ch == '-' && next == '>')
                || (ch == '=' && next == '>')
            {
                if !res.ends_with(' ') {
                    res.push(' ');
                }
                res.push(ch);
                res.push(next);
                if i + 2 < len && chars[i + 2] != ' ' {
                    res.push(' ');
                }
                i += 2;
                continue;
            } else if ch == '=' && i + 2 < len && chars[i + 1] == ' ' && chars[i + 2] == '>' {
                // Collapse mistakenly split `= >` into `=>`
                if !res.ends_with(' ') {
                    res.push(' ');
                }
                res.push('=');
                res.push('>');
                if i + 3 < len && chars[i + 3] != ' ' {
                    res.push(' ');
                }
                i += 3;
                continue;
            } else if ch == '|' && i + 2 < len && chars[i + 1] == ' ' && chars[i + 2] == '>' {
                // Collapse mistakenly split `| >` into `|>`
                if !res.ends_with(' ') {
                    res.push(' ');
                }
                res.push('|');
                res.push('>');
                if i + 3 < len && chars[i + 3] != ' ' {
                    res.push(' ');
                }
                i += 3;
                continue;
            } else if ch == '.' && next == '.' {
                // Range `..` should NOT have spaces around it (e.g. `0..n`)
                res.push('.');
                res.push('.');
                i += 2;
                continue;
            }
        }

        // Single-char binary operators: =, +, *, /, %, <, >
        if ch == '=' || ch == '+' || ch == '*' || ch == '/' || ch == '%' {
            let prev_is_space = res.ends_with(' ') || res.is_empty();
            let next_is_space = i + 1 < len && chars[i + 1] == ' ';
            let next_is_eq = i + 1 < len && chars[i + 1] == '=';

            if !next_is_eq {
                if !prev_is_space {
                    res.push(' ');
                }
                res.push(ch);
                if !next_is_space && i + 1 < len {
                    res.push(' ');
                }
                i += 1;
                continue;
            }
        }

        // Single minus '-': distinguish binary minus `a - b` from unary `-5` or `-x`
        if ch == '-' {
            let prev_trimmed = res.trim_end();
            let prev_non_space = prev_trimmed.chars().last();
            let is_unary = prev_non_space.is_none_or(|p| {
                matches!(
                    p,
                    '(' | '[' | '{' | ',' | '=' | ':' | '+' | '-' | '*' | '/' | '!' | '<' | '>'
                )
            }) || prev_trimmed.ends_with("return")
                || prev_trimmed.ends_with("out")
                || prev_trimmed.ends_with("in");

            if is_unary {
                res.push('-');
            } else {
                if !res.ends_with(' ') {
                    res.push(' ');
                }
                res.push('-');
                if i + 1 < len && chars[i + 1] != ' ' {
                    res.push(' ');
                }
            }
            i += 1;
            continue;
        }

        // Single comparison or generic bracket: < or >
        if ch == '<' || ch == '>' {
            let prev_non_space = res.trim_end().chars().last();
            let is_ident_prev = prev_non_space.is_some_and(|p| p.is_alphanumeric() || p == '_');
            let next_char = if i + 1 < len {
                Some(chars[i + 1])
            } else {
                None
            };

            let is_generic_open = ch == '<'
                && is_ident_prev
                && next_char.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let is_generic_close = ch == '>'
                && is_ident_prev
                && (next_char.is_none()
                    || next_char.is_some_and(|c| {
                        matches!(c, ' ' | '{' | '(' | ')' | ',' | '.' | ';' | '\n' | '\r')
                    }))
                && res.contains('<');

            if is_generic_open || is_generic_close {
                while res.ends_with(' ') {
                    res.pop();
                }
                res.push(ch);
                if is_generic_close && next_char.is_some_and(|c| c == '{') {
                    res.push(' ');
                }
            } else {
                if !res.ends_with(' ') {
                    res.push(' ');
                }
                res.push(ch);
                if i + 1 < len && chars[i + 1] != ' ' {
                    res.push(' ');
                }
            }
            i += 1;
            continue;
        }

        // Comma `,` must be followed by space
        if ch == ',' {
            res.push(',');
            if i + 1 < len
                && chars[i + 1] != ' '
                && chars[i + 1] != '\n'
                && chars[i + 1] != '\r'
                && chars[i + 1] != ')'
                && chars[i + 1] != ']'
                && chars[i + 1] != '}'
            {
                res.push(' ');
            }
            i += 1;
            continue;
        }

        // Colon `:` in type annotation `name: Type`
        if ch == ':' && i + 1 < len && chars[i + 1] != ':' {
            // Trim space before colon
            while res.ends_with(' ') {
                res.pop();
            }
            res.push(':');
            if chars[i + 1] != ' ' {
                res.push(' ');
            }
            i += 1;
            continue;
        }

        res.push(ch);
        i += 1;
    }

    res
}

/// Normalizes loop and branch statements:
/// `for (i in 0..10)` -> `for i in 0..10`
/// `if (cond)` -> `if cond`
/// `while (cond)` -> `while cond`
/// Ensures `{` has a preceding space
pub fn format_loops_in_code(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    let mut result = trimmed.to_string();

    // Fix `if (...)`
    if let Some(rest) = result.strip_prefix("if (") {
        if let Some(close_idx) = rest.find(") {") {
            let cond = &rest[..close_idx];
            let after = &rest[close_idx + 1..]; // includes " {"
            result = format!("if {}{}", cond, after);
        } else if let Some(close_idx) = rest.find("){") {
            let cond = &rest[..close_idx];
            let after = &rest[close_idx + 1..]; // includes "{"
            result = format!("if {} {}", cond, after);
        }
    } else if let Some(rest) = result.strip_prefix("if ")
        && rest.ends_with('{')
        && !rest.ends_with(" {")
    {
        result = format!("{} {{", result[..result.len() - 1].trim_end());
    }

    // Fix `while (...)`
    if let Some(rest) = result.strip_prefix("while (") {
        if let Some(close_idx) = rest.find(") {") {
            let cond = &rest[..close_idx];
            let after = &rest[close_idx + 1..];
            result = format!("while {}{}", cond, after);
        } else if let Some(close_idx) = rest.find("){") {
            let cond = &rest[..close_idx];
            let after = &rest[close_idx + 1..];
            result = format!("while {} {}", cond, after);
        }
    } else if let Some(rest) = result.strip_prefix("while ")
        && rest.ends_with('{')
        && !rest.ends_with(" {")
    {
        result = format!("{} {{", result[..result.len() - 1].trim_end());
    }

    // Fix `for (...)`
    if let Some(rest) = result.strip_prefix("for (") {
        if let Some(close_idx) = rest.find(") {") {
            let body = &rest[..close_idx];
            let after = &rest[close_idx + 1..];
            result = format!("for {}{}", body, after);
        } else if let Some(close_idx) = rest.find("){") {
            let body = &rest[..close_idx];
            let after = &rest[close_idx + 1..];
            result = format!("for {} {}", body, after);
        }
    } else if let Some(rest) = result.strip_prefix("for ")
        && rest.ends_with('{')
        && !rest.ends_with(" {")
    {
        result = format!("{} {{", result[..result.len() - 1].trim_end());
    }

    format!("{}{}", indent, result)
}

/// Applies formatting rules across source lines
pub fn format_source(source: &str, opts: &FormatOptions) -> (String, Vec<FormatDiff>) {
    let mut diffs = Vec::new();
    let raw_lines: Vec<&str> = source.lines().collect();
    let mut formatted_lines: Vec<String> = Vec::with_capacity(raw_lines.len());

    let mut indent_level: usize = 0;
    let mut consecutive_blanks = 0;

    for (line_idx, &raw_line) in raw_lines.iter().enumerate() {
        let trimmed = raw_line.trim();

        // 1. Blank line handling
        if trimmed.is_empty() {
            consecutive_blanks += 1;
            if !opts.blank_lines || consecutive_blanks <= 1 {
                formatted_lines.push(String::new());
            } else if opts.blank_lines {
                diffs.push(FormatDiff {
                    line_number: line_idx + 1,
                    rule: "blank_lines",
                    original: raw_line.to_string(),
                    formatted: String::new(),
                });
            }
            continue;
        }
        consecutive_blanks = 0;

        // Process line segments (separating code from strings and comments)
        let segments = split_code_and_literals(trimmed);

        // Count leading closing braces on this line ONLY in code segments
        let mut leading_closing_braces = 0;
        for seg in &segments {
            if let Segment::Code(c) = seg {
                let trimmed_c = c.trim_start();
                let count = trimmed_c
                    .chars()
                    .take_while(|&ch| ch == '}' || ch == ']')
                    .count();
                leading_closing_braces += count;
                if !trimmed_c.is_empty() {
                    break;
                }
            } else {
                break;
            }
        }
        if opts.indent && leading_closing_braces > 0 {
            indent_level = indent_level.saturating_sub(leading_closing_braces);
        }

        let mut processed_line = String::new();

        for seg in &segments {
            match seg {
                Segment::StringLiteral(s) => processed_line.push_str(s),
                Segment::LineComment(c) => {
                    if !processed_line.is_empty() && !processed_line.ends_with(' ') {
                        processed_line.push(' ');
                    }
                    processed_line.push_str(c);
                }
                Segment::Code(c) => {
                    let mut code_str = c.clone();
                    if opts.operators {
                        code_str = format_operators_in_code(&code_str);
                    }
                    processed_line.push_str(&code_str);
                }
            }
        }

        if opts.loops {
            processed_line = format_loops_in_code(&processed_line);
        }

        // Apply indentation
        let final_line = if opts.indent {
            let prefix = "    ".repeat(indent_level);
            format!("{}{}", prefix, processed_line.trim())
        } else {
            // Keep original indentation but use formatted contents
            let orig_indent = &raw_line[..raw_line.len() - raw_line.trim_start().len()];
            format!("{}{}", orig_indent, processed_line.trim())
        };

        // Update indent level for following lines based on opening/closing braces ONLY in code segments
        if opts.indent {
            let mut open_braces = 0;
            let mut close_braces = 0;
            for seg in &segments {
                if let Segment::Code(c) = seg {
                    open_braces += c.chars().filter(|&ch| ch == '{' || ch == '[').count();
                    close_braces += c.chars().filter(|&ch| ch == '}' || ch == ']').count();
                }
            }
            let net_opens = open_braces.saturating_sub(close_braces);
            let net_closes = close_braces.saturating_sub(open_braces);

            if net_opens > 0 {
                indent_level += net_opens;
            } else if net_closes > 0 && leading_closing_braces == 0 {
                indent_level = indent_level.saturating_sub(net_closes);
            }
        }

        if final_line != raw_line {
            diffs.push(FormatDiff {
                line_number: line_idx + 1,
                rule: "format",
                original: raw_line.to_string(),
                formatted: final_line.clone(),
            });
        }

        formatted_lines.push(final_line);
    }

    let mut result = formatted_lines.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    (result, diffs)
}
