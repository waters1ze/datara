use super::*;
use crate::diagnostics::{ErrorCode, SourceSpan};
use crate::lexer::{Token, TokenType};

impl<'a> Parser<'a> {
    pub(crate) fn consume(&mut self, token_type: &TokenType, msg: &str) -> Option<Token> {
        if self.check(token_type) {
            Some(self.advance())
        } else {
            self.error(msg);
            None
        }
    }

    pub(crate) fn consume_ident(&mut self, msg: &str) -> Option<String> {
        self.consume_ident_or_keyword(msg)
    }

    pub(crate) fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(token_type)
        }
    }

    /// True when the next tokens are a `-` immediately followed by an integer
    /// literal, i.e. a negative literal that must not be accepted as a count,
    /// size, bit index or address (casting it to `usize` would wrap).
    pub(crate) fn negative_literal_follows(&self) -> bool {
        self.check(&TokenType::Minus)
            && matches!(
                self.tokens.get(self.current + 1).map(|t| &t.token_type),
                Some(TokenType::IntLiteral(_))
            )
    }

    pub(crate) fn check_ident_str(&self, expected: &str) -> bool {
        if self.is_at_end() {
            false
        } else if let TokenType::Identifier(s) = &self.peek().token_type {
            s == expected
        } else {
            false
        }
    }

    pub(crate) fn match_ident_str(&mut self, expected: &str) -> bool {
        if self.check_ident_str(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn advance(&mut self) -> Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    pub(crate) fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    pub(crate) fn peek(&self) -> &Token {
        if self.current < self.tokens.len() {
            &self.tokens[self.current]
        } else if let Some(last) = self.tokens.last() {
            last
        } else {
            static FALLBACK_EOF: std::sync::LazyLock<Token> = std::sync::LazyLock::new(|| {
                Token::new(
                    TokenType::Eof,
                    String::new(),
                    SourceSpan::new(1, 1, 1, 1, String::new()),
                )
            });
            &FALLBACK_EOF
        }
    }

    pub(crate) fn previous(&self) -> Token {
        if self.tokens.is_empty() {
            Token::new(
                TokenType::Eof,
                String::new(),
                SourceSpan::new(1, 1, 1, 1, self.file.clone()),
            )
        } else {
            let idx = self.current.saturating_sub(1).min(self.tokens.len() - 1);
            self.tokens[idx].clone()
        }
    }

    pub(crate) fn error(&mut self, msg: &str) {
        let span = self.peek().span.clone();
        self.diag.error(
            ErrorCode::SyntaxUnexpectedToken,
            msg.to_string(),
            Some(span),
        );
    }

    pub(crate) fn error_depth_limit(&mut self, what: &str) {
        let span = self.peek().span.clone();
        self.diag.error(
            ErrorCode::RecursionLimitExceeded,
            format!(
                "{} nesting too deep (limit {} levels exceeded)",
                what, MAX_PARSE_DEPTH
            ),
            Some(span),
        );
    }

    pub(crate) fn synchronize(&mut self) {
        self.advance();
        while !self.is_at_end() {
            match self.peek().token_type {
                TokenType::Class
                | TokenType::Fn
                | TokenType::Function
                | TokenType::Behavior
                | TokenType::Component
                | TokenType::Role
                | TokenType::Enum
                | TokenType::Extern
                | TokenType::Type
                | TokenType::Packet
                | TokenType::Let
                | TokenType::Mut
                | TokenType::If
                | TokenType::For
                | TokenType::While
                | TokenType::Return => return,
                // Stop at (but do not consume) block/statement terminators so
                // recovery does not eat the enclosing block and cascade errors.
                TokenType::RBrace | TokenType::Semicolon => return,
                _ => {
                    self.advance();
                }
            }
        }
    }
}

pub(crate) fn start_line_from(s: &SourceSpan) -> usize {
    s.start_line
}
pub(crate) fn start_col_from(s: &SourceSpan) -> usize {
    s.start_col
}
