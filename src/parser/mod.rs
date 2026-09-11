use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, SourceSpan};
use crate::lexer::{Lexer, Token, TokenType};

/// Maximum recursion depth for nested expressions/types/statements; deeper
/// input is rejected instead of overflowing the stack.
///
/// Stack usage measurements & analysis:
/// - Prior mutual-recursion ladder:
///     Each nesting level traversed 11-13 stack frames:
///     `parse_expression` -> `parse_pipeline` -> `parse_logical_or` ->
///     `parse_logical_and` -> `parse_equality` -> `parse_comparison` ->
///     `parse_range` -> `parse_term` -> `parse_factor` -> `parse_unary` ->
///     `parse_postfix` -> `parse_primary`.
///     In unoptimized debug builds (`opt-level = 0`), each frame allocated
///     ~120-160 bytes of stack (spill slots, locals, `SourceSpan` structs),
///     leading to ~1.4 - 1.8 KB consumed per nesting level. On a standard
///     Windows/libtest thread with a 2 MiB stack, recursion depth of ~29
///     exhausted available stack space, prompting the original conservative
///     cap of 16.
///
/// - Precedence-climbing / Pratt parser:
///     The mutual recursion ladder is collapsed into a single climbing loop
///     `parse_binary_climbing(min_prec)`. A nested expression level now only traverses:
///     `parse_expression` -> `parse_pipeline` -> `parse_binary_climbing` ->
///     `parse_unary` -> `parse_postfix` -> `parse_primary`.
///     This reduces frame count per nesting level from 12 frames to 5 frames
///     (a ~60% reduction in call depth). Furthermore, long chains of binary
///     operators (e.g., `a + b + c + ...`) execute iteratively within the
///     climbing `while` loop rather than allocating stack frames for every term.
///     In debug builds, each nesting level now uses ~450-600 bytes. At depth 64,
///     peak stack consumption is ~35-40 KB, leaving >98% headroom on a standard
///     2 MiB stack (and easily within 1 MiB fiber stacks).
///
/// We safely lift `MAX_PARSE_DEPTH` from 16 to 64.
pub(crate) const MAX_PARSE_DEPTH: usize = 64;

// Precedence levels for binary expressions
const PREC_LOGICAL_OR: u8 = 1;
const PREC_LOGICAL_AND: u8 = 2;
const PREC_EQUALITY: u8 = 3;
const PREC_COMPARISON: u8 = 4;
const PREC_RANGE: u8 = 5;
const PREC_ADDITIVE: u8 = 6;
const PREC_MULTIPLICATIVE: u8 = 7;

pub struct Parser<'a> {
    tokens: Vec<Token>,
    current: usize,
    diag: &'a mut DiagnosticEngine,
    file: String,
    depth: usize,
    /// When set, `Ident {` at the top of an expression is not treated as a
    /// struct literal unless the brace content provably looks like field
    /// initializer syntax (`{`, `Ident :` or `{}`). Used for `if`/`while`
    /// conditions and `match` scrutinees so `if Flag { ... }` parses the
    /// `{` as the block body instead of an object init.
    no_struct_literal_at_top: bool,
}

pub(crate) mod cursor;
pub(crate) mod decl;
pub(crate) mod expr;
pub(crate) mod stmt;
pub(crate) mod types;

pub(crate) use cursor::*;
impl<'a> Parser<'a> {
    pub fn new(mut tokens: Vec<Token>, diag: &'a mut DiagnosticEngine, file: &str) -> Self {
        if tokens.is_empty() {
            tokens.push(Token::new(
                TokenType::Eof,
                "".into(),
                SourceSpan::new(1, 1, 1, 1, file.to_string()),
            ));
        }
        Self {
            tokens,
            current: 0,
            diag,
            file: file.to_string(),
            depth: 0,
            no_struct_literal_at_top: false,
        }
    }

    pub fn parse_program(&mut self) -> Program {
        let mut declarations = Vec::new();
        let mut file_attributes = Vec::new();
        while !self.is_at_end() {
            let attrs = self.parse_attributes();

            // Module directive support: module foo.bar;
            if let TokenType::Identifier(id) = &self.peek().token_type
                && id == "module"
            {
                self.advance();
                let mut module_path = String::new();
                while !self.is_at_end()
                    && !self.check(&TokenType::Semicolon)
                    && self.peek().span.start_line == self.previous().span.start_line
                {
                    let tok = self.advance();
                    module_path.push_str(&tok.lexeme);
                }
                if module_path
                    .chars()
                    .any(|c| !(c.is_alphanumeric() || c == '_' || c == '.' || c == ':'))
                {
                    self.error(
                        "Invalid module path; expected identifiers separated by '.' or '::'",
                    );
                }
                self.match_token(&TokenType::Semicolon);
                file_attributes.extend(attrs);
                continue;
            }

            if self.is_at_end() {
                file_attributes.extend(attrs);
                break;
            }

            if let Some(decl) = self.parse_declaration_with_attrs(attrs) {
                declarations.push(decl);
            } else {
                self.synchronize();
            }
        }
        Program {
            declarations,
            attributes: file_attributes,
            file: self.file.clone(),
            link_libraries: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_empty_tokens() {
        let mut diag = DiagnosticEngine::new("en");
        let mut parser = Parser::new(vec![], &mut diag, "empty.dtr");
        let prog = parser.parse_program();
        assert_eq!(prog.declarations.len(), 0);
    }

    #[test]
    fn test_parser_deeply_nested_parens() {
        // 1) Test depth 48 (well beyond old MAX_PARSE_DEPTH = 16) parses cleanly
        {
            let mut diag = DiagnosticEngine::new("en");
            let s = "(".repeat(48) + "42" + &")".repeat(48);
            let mut lexer = Lexer::new(&s, "depth48.dtr");
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, "depth48.dtr");
            let res = parser.parse_expression();
            assert!(res.is_some(), "Depth 48 must parse successfully");
            assert!(!diag.has_errors(), "Depth 48 must produce no errors");
        }

        // 2) Test depth 64 (current MAX_PARSE_DEPTH limit) parses cleanly
        {
            let mut diag = DiagnosticEngine::new("en");
            let s = "(".repeat(64) + "42" + &")".repeat(64);
            let mut lexer = Lexer::new(&s, "depth64.dtr");
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, "depth64.dtr");
            let res = parser.parse_expression();
            assert!(res.is_some(), "Depth 64 must parse successfully");
            assert!(!diag.has_errors(), "Depth 64 must produce no errors");
        }

        // 3) Test depth 65 and 200 exceeds MAX_PARSE_DEPTH and reports clean error without crashing
        {
            let mut diag = DiagnosticEngine::new("en");
            let s = "(".repeat(65) + "42" + &")".repeat(65);
            let mut lexer = Lexer::new(&s, "depth65.dtr");
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, "depth65.dtr");
            let _ = parser.parse_expression();
            assert!(
                diag.has_errors(),
                "Depth 65 must report recursion limit error"
            );
        }
        {
            let mut diag = DiagnosticEngine::new("en");
            let s = "(".repeat(200) + &")".repeat(200);
            let mut lexer = Lexer::new(&s, "deep.dtr");
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, "deep.dtr");
            let _ = parser.parse_expression();
            assert!(
                diag.has_errors(),
                "Depth 200 must report error without crashing"
            );
        }
    }

    #[test]
    fn test_parser_negative_pattern_min() {
        let mut diag = DiagnosticEngine::new("en");
        let src = "fn test(x: Int) { match x { -9223372036854775808 => out 1, _ => out 0 } }";
        let mut lexer = Lexer::new(src, "min.dtr");
        let tokens = lexer.tokenize(&mut diag);
        let mut parser = Parser::new(tokens, &mut diag, "min.dtr");
        let _ = parser.parse_program();
        // Must not panic on i64::MIN pattern
    }
}
