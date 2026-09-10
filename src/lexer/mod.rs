pub mod classify;
pub mod tokens;

use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
pub use classify::{TopLevelKind, classify_top_level};
pub use tokens::{Token, TokenType};

/// Keywords spelled with an internal `-`. These are the only identifiers the
/// lexer may continue across a `-`; anywhere else `-` is the minus operator.
const HYPHENATED_KEYWORDS: &[&str] = &["mut-view"];

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    file: String,
}

impl Lexer {
    pub fn new(source: &str, file: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            file: file.to_string(),
        }
    }

    pub fn tokenize(&mut self, diag: &mut DiagnosticEngine) -> Vec<Token> {
        let mut tokens = Vec::new();

        // A UTF-8 byte-order mark is an encoding marker, not source text.
        // Editors on Windows add one routinely, so strip a leading BOM instead
        // of reporting it as an unexpected character.
        if self.chars.first() == Some(&'\u{FEFF}') {
            self.pos = 1;
            self.col = 1;
        }

        while !self.is_at_end() {
            self.skip_whitespace_and_comments(diag);
            if self.is_at_end() {
                break;
            }

            let start_line = self.line;
            let start_col = self.col;
            let ch = self.advance();

            match ch {
                '(' => tokens.push(Token::new(
                    TokenType::LParen,
                    "(".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                ')' => tokens.push(Token::new(
                    TokenType::RParen,
                    ")".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '{' => tokens.push(Token::new(
                    TokenType::LBrace,
                    "{".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '}' => tokens.push(Token::new(
                    TokenType::RBrace,
                    "}".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '[' => tokens.push(Token::new(
                    TokenType::LBracket,
                    "[".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                ']' => tokens.push(Token::new(
                    TokenType::RBracket,
                    "]".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                ',' => tokens.push(Token::new(
                    TokenType::Comma,
                    ",".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                ';' => tokens.push(Token::new(
                    TokenType::Semicolon,
                    ";".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '.' => {
                    if self.peek() == '.' {
                        self.advance();
                        if self.peek() == '=' {
                            self.advance();
                            tokens.push(Token::new(
                                TokenType::DotDotEq,
                                "..=".into(),
                                SourceSpan::new(
                                    start_line,
                                    start_col,
                                    self.line,
                                    self.col,
                                    self.file.clone(),
                                ),
                            ));
                        } else if self.peek() == '<' {
                            self.advance();
                            tokens.push(Token::new(
                                TokenType::DotDotLt,
                                "..<".into(),
                                SourceSpan::new(
                                    start_line,
                                    start_col,
                                    self.line,
                                    self.col,
                                    self.file.clone(),
                                ),
                            ));
                        } else {
                            tokens.push(Token::new(
                                TokenType::DotDot,
                                "..".into(),
                                SourceSpan::new(
                                    start_line,
                                    start_col,
                                    self.line,
                                    self.col,
                                    self.file.clone(),
                                ),
                            ));
                        }
                    } else {
                        tokens.push(Token::new(
                            TokenType::Dot,
                            ".".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '+' => tokens.push(Token::new(
                    TokenType::Plus,
                    "+".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '-' => {
                    if self.peek() == '>' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::Arrow,
                            "->".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Minus,
                            "-".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '*' => tokens.push(Token::new(
                    TokenType::Star,
                    "*".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '/' => tokens.push(Token::new(
                    TokenType::Slash,
                    "/".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '%' => tokens.push(Token::new(
                    TokenType::Percent,
                    "%".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                ':' => {
                    if self.peek() == '=' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::ColonEqual,
                            ":=".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Colon,
                            ":".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '=' => {
                    if self.peek() == '=' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::EqualEqual,
                            "==".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else if self.peek() == '>' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::FatArrow,
                            "=>".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Equal,
                            "=".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '!' => {
                    if self.peek() == '=' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::NotEqual,
                            "!=".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Bang,
                            "!".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '<' => {
                    if self.peek() == '=' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::LessEqual,
                            "<=".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Less,
                            "<".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '>' => {
                    if self.peek() == '=' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::GreaterEqual,
                            ">=".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Greater,
                            ">".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '&' => {
                    if self.peek() == '&' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::And,
                            "&&".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        tokens.push(Token::new(
                            TokenType::Ampersand,
                            "&".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    }
                }
                '|' => {
                    if self.peek() == '|' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::Or,
                            "||".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else if self.peek() == '>' {
                        self.advance();
                        tokens.push(Token::new(
                            TokenType::Pipe,
                            "|>".into(),
                            SourceSpan::new(
                                start_line,
                                start_col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            ),
                        ));
                    } else {
                        // A lone '|' used to produce no token at all, so `a | b`
                        // was silently rewritten to `a b`. Report it instead.
                        diag.error(
                            ErrorCode::SyntaxUnexpectedToken,
                            "Unexpected character '|'. Datara has no bitwise-or operator; did you mean '||' (logical or) or '|>' (pipe)?".into(),
                            Some(SourceSpan::new(start_line, start_col, self.line, self.col, self.file.clone())),
                        );
                    }
                }
                '?' => tokens.push(Token::new(
                    TokenType::Question,
                    "?".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),
                '@' => tokens.push(Token::new(
                    TokenType::At,
                    "@".into(),
                    SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    ),
                )),

                '\'' => {
                    let mut val = '\0';
                    let mut closed = false;
                    if !self.is_at_end() {
                        let mut c = self.advance();
                        if c == '\\' && !self.is_at_end() {
                            let esc = self.advance();
                            c = match esc {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                '0' => '\0',
                                '\\' => '\\',
                                '\'' => '\'',
                                other => other,
                            };
                        }
                        val = c;
                        if self.peek() == '\'' {
                            self.advance();
                            closed = true;
                        }
                    }

                    let span = SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    );
                    if !closed {
                        diag.error(
                            ErrorCode::SyntaxUnterminatedString,
                            "Unterminated character literal".into(),
                            Some(span),
                        );
                    } else {
                        tokens.push(Token::new(
                            TokenType::CharLiteral(val),
                            format!("'{}'", val),
                            span,
                        ));
                    }
                }

                // Stream Template with `fmt` prefix: fmt"..." or FMT"..."
                'f' | 'F'
                    if (self.peek() == 'm' || self.peek() == 'M')
                        && (self.peek_next() == 't' || self.peek_next() == 'T')
                        && self.chars.get(self.pos + 2) == Some(&'"') =>
                {
                    self.advance(); // consume 'm' / 'M'
                    self.advance(); // consume 't' / 'T'
                    self.advance(); // consume opening quote '"'
                    self.scan_string_literal(start_line, start_col, true, diag, &mut tokens);
                }

                // Stream Template with `$` operator: $"..."
                '$' if self.peek() == '"' => {
                    self.advance(); // consume opening quote '"'
                    self.scan_string_literal(start_line, start_col, true, diag, &mut tokens);
                }

                // Compatibility format prefix: f"..." or F"..."
                'f' | 'F' if self.peek() == '"' => {
                    self.advance(); // consume opening quote '"'
                    self.scan_string_literal(start_line, start_col, true, diag, &mut tokens);
                }

                // Standard string literal: "..." (100% pure literal by default, {} not interpolated!)
                '"' => {
                    self.scan_string_literal(start_line, start_col, false, diag, &mut tokens);
                }

                _ if ch.is_ascii_digit() => {
                    let mut raw_str = ch.to_string();
                    let mut num_str = ch.to_string();
                    let mut is_float = false;

                    let is_hex = ch == '0' && (self.peek() == 'x' || self.peek() == 'X');
                    let is_bin = ch == '0' && (self.peek() == 'b' || self.peek() == 'B');
                    let is_oct = ch == '0' && (self.peek() == 'o' || self.peek() == 'O');

                    if is_hex || is_bin || is_oct {
                        let p = self.advance();
                        raw_str.push(p);
                        num_str.push(p);
                        let radix = if is_hex {
                            16
                        } else if is_bin {
                            2
                        } else {
                            8
                        };
                        while !self.is_at_end()
                            && ((radix == 16 && self.peek().is_ascii_hexdigit())
                                || (radix == 2 && (self.peek() == '0' || self.peek() == '1'))
                                || (radix == 8 && ('0'..='7').contains(&self.peek()))
                                || self.peek() == '_')
                        {
                            let c = self.advance();
                            raw_str.push(c);
                            if c != '_' {
                                num_str.push(c);
                            }
                        }
                        let suffix = self.scan_numeric_suffix();
                        if let Some(ref s) = suffix {
                            raw_str.push_str(s);
                        }
                        let span = SourceSpan::new(
                            start_line,
                            start_col,
                            self.line,
                            self.col,
                            self.file.clone(),
                        );
                        let raw_digits = if num_str.len() >= 2 {
                            &num_str[2..]
                        } else {
                            ""
                        };
                        match u128::from_str_radix(raw_digits, radix) {
                            Ok(uval) => {
                                if let Some(ref s) = suffix {
                                    if !Self::validate_int_suffix_range(uval as i128, s) {
                                        diag.error(
                                            ErrorCode::RangeViolation,
                                            format!(
                                                "Literal '{}' exceeds range for suffix '{}'",
                                                raw_str, s
                                            ),
                                            Some(span.clone()),
                                        );
                                    }
                                }
                                tokens.push(Token::new(
                                    TokenType::IntLiteral(uval as i64),
                                    raw_str,
                                    span,
                                ));
                            }
                            Err(_) => {
                                diag.error(
                                    ErrorCode::SyntaxInvalidNumber,
                                    format!("Invalid numeric literal '{}'", raw_str),
                                    Some(span),
                                );
                            }
                        }
                    } else {
                        while !self.is_at_end()
                            && (self.peek().is_ascii_digit()
                                || self.peek() == '_'
                                || (self.peek() == '.' && self.peek_next().is_ascii_digit()))
                        {
                            if self.peek() == '.' {
                                is_float = true;
                            }
                            let next_c = self.advance();
                            raw_str.push(next_c);
                            if next_c != '_' {
                                num_str.push(next_c);
                            }
                        }
                        let suffix = self.scan_numeric_suffix();
                        if let Some(ref s) = suffix {
                            raw_str.push_str(s);
                            if s == "f32" || s == "f64" {
                                is_float = true;
                            }
                        }

                        let span = SourceSpan::new(
                            start_line,
                            start_col,
                            self.line,
                            self.col,
                            self.file.clone(),
                        );
                        if is_float {
                            match num_str.parse::<f64>() {
                                Ok(val) => {
                                    tokens.push(Token::new(
                                        TokenType::FloatLiteral(val),
                                        raw_str,
                                        span,
                                    ));
                                }
                                Err(e) => {
                                    diag.error(
                                        ErrorCode::SyntaxInvalidNumber,
                                        format!("Invalid float literal '{}': {}", raw_str, e),
                                        Some(span),
                                    );
                                }
                            }
                        } else {
                            match num_str.parse::<i128>() {
                                Ok(val) => {
                                    if let Some(ref s) = suffix {
                                        if !Self::validate_int_suffix_range(val, s) {
                                            diag.error(
                                                ErrorCode::RangeViolation,
                                                format!(
                                                    "Literal '{}' exceeds range for suffix '{}'",
                                                    raw_str, s
                                                ),
                                                Some(span.clone()),
                                            );
                                        }
                                    } else if val < (i64::MIN as i128) || val > (i64::MAX as i128) {
                                        diag.error(
                                            ErrorCode::SyntaxInvalidNumber,
                                            format!(
                                                "Invalid integer literal '{}': value exceeds 64-bit range",
                                                raw_str
                                            ),
                                            Some(span.clone()),
                                        );
                                    }
                                    tokens.push(Token::new(
                                        TokenType::IntLiteral(val as i64),
                                        raw_str,
                                        span,
                                    ));
                                }
                                Err(e) => {
                                    diag.error(
                                        ErrorCode::SyntaxInvalidNumber,
                                        format!("Invalid integer literal '{}': {}", raw_str, e),
                                        Some(span),
                                    );
                                }
                            }
                        }
                    }
                }

                _ if ch.is_alphabetic() || ch == '_' => {
                    let mut ident_str = ch.to_string();
                    while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_')
                    {
                        ident_str.push(self.advance());
                    }

                    // A `-` may only continue the identifier when the whole
                    // word matches a known hyphenated keyword (e.g. `mut-view`);
                    // otherwise it is the subtraction operator, so `x-y` must
                    // lex as `x` `-` `y`.
                    if self.peek() == '-' {
                        let mut rest = String::new();
                        let mut i = self.pos + 1;
                        while i < self.chars.len() && self.chars[i].is_alphanumeric() {
                            rest.push(self.chars[i]);
                            i += 1;
                        }
                        let candidate = format!("{}-{}", ident_str, rest);
                        if HYPHENATED_KEYWORDS.contains(&candidate.as_str()) {
                            ident_str.push(self.advance()); // consume '-'
                            while !self.is_at_end() && self.peek().is_alphanumeric() {
                                ident_str.push(self.advance());
                            }
                        }
                    }

                    let span = SourceSpan::new(
                        start_line,
                        start_col,
                        self.line,
                        self.col,
                        self.file.clone(),
                    );
                    let tt = match ident_str.as_str() {
                        "let" => TokenType::Let,
                        "mut" => TokenType::Mut,
                        "const" => TokenType::Const,
                        "fn" => TokenType::Fn,
                        "function" => TokenType::Function,
                        "class" => TokenType::Class,
                        "struct" => TokenType::Struct,
                        "record" => TokenType::Record,
                        "enum" => TokenType::Enum,
                        "component" => TokenType::Component,
                        "role" => TokenType::Role,
                        "behavior" => TokenType::Behavior,
                        "from" => TokenType::From,
                        "extends" => TokenType::Extends,
                        "with" => TokenType::With,
                        "replaces" => TokenType::Replaces,
                        "export" => TokenType::Export,
                        "import" => TokenType::Import,
                        "as" => TokenType::As,
                        "if" => TokenType::If,
                        "else" => TokenType::Else,
                        "for" => TokenType::For,
                        "in" => TokenType::In,
                        "while" => TokenType::While,
                        "loop" => TokenType::Loop,
                        "match" => TokenType::Match,
                        "when" => TokenType::When,
                        "decide" => TokenType::Decide,
                        "select" => TokenType::Select,
                        "return" => TokenType::Return,
                        "break" => TokenType::Break,
                        "continue" => TokenType::Continue,
                        "parallel" => TokenType::Parallel,
                        "async" => TokenType::Async,
                        "await" => TokenType::Await,
                        "task" => TokenType::Task,
                        "flow" => TokenType::Flow,
                        "entity" => TokenType::Entity,
                        "process" => TokenType::Process,
                        "then" => TokenType::Then,
                        "unsafe" => TokenType::Unsafe,
                        "extern" => TokenType::Extern,
                        "true" => TokenType::True,
                        "false" => TokenType::False,
                        "None" => TokenType::None,
                        "own" => TokenType::Own,
                        "view" => TokenType::View,
                        "mut-view" => TokenType::MutView,
                        "shared" => TokenType::Shared,
                        "out" => TokenType::Out,
                        "err" => TokenType::Err,
                        "use" => TokenType::Use,
                        "try" => TokenType::Try,
                        "catch" => TokenType::Catch,
                        "cli" => TokenType::Cli,
                        "app" => TokenType::App,
                        "command" => TokenType::Command,
                        "val" => TokenType::Val,
                        "packet" => TokenType::Packet,
                        "using" => TokenType::Using,
                        "or" => TokenType::OrKeyword,
                        "type" => TokenType::Type,
                        "where" => TokenType::Where,
                        "require" => TokenType::Require,
                        "ensure" => TokenType::Ensure,
                        "register" => TokenType::Register,
                        "bit" => TokenType::Bit,
                        "bits" => TokenType::Bits,
                        "comptime" => TokenType::Comptime,
                        "wrapping" => TokenType::Wrapping,
                        "saturating" => TokenType::Saturating,
                        "trait" => TokenType::Trait,
                        "impl" => TokenType::Impl,
                        "pub" => TokenType::Pub,
                        "asm" if self.peek() == '!' => {
                            self.advance();
                            TokenType::Asm
                        }
                        _ => TokenType::Identifier(ident_str.clone()),
                    };

                    let lexeme = if tt == TokenType::Asm {
                        "asm!".to_string()
                    } else {
                        ident_str
                    };
                    tokens.push(Token::new(tt, lexeme, span));
                }

                // Catch-all: this used to be `_ => {}`, which silently DISCARDED
                // the character (the advance happens above, before the match).
                // That turned typos and unsupported operators into silently wrong
                // programs: `out 6 ^ 3` compiled cleanly and printed `6`.
                // An unsupported character must be a hard error, never a drop.
                ch => {
                    diag.error(
                        ErrorCode::SyntaxUnexpectedToken,
                        format!(
                            "Unexpected character '{}' (U+{:04X}). Datara has no operator spelled with this character.",
                            if ch.is_control() { '�' } else { ch },
                            ch as u32
                        ),
                        Some(SourceSpan::new(start_line, start_col, self.line, self.col, self.file.clone())),
                    );
                }
            }
        }

        tokens.push(Token::new(
            TokenType::Eof,
            "".into(),
            SourceSpan::new(self.line, self.col, self.line, self.col, self.file.clone()),
        ));
        tokens
    }

    fn skip_whitespace_and_comments(&mut self, diag: &mut DiagnosticEngine) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\r' | '\t' => {
                    self.advance();
                }
                '\n' => {
                    self.line += 1;
                    self.col = 1;
                    self.pos += 1;
                }
                '/' if self.peek_next() == '/' => {
                    while !self.is_at_end() && self.peek() != '\n' {
                        self.advance();
                    }
                }
                '/' if self.peek_next() == '*' => {
                    self.advance();
                    self.advance();
                    while !self.is_at_end() && !(self.peek() == '*' && self.peek_next() == '/') {
                        self.advance();
                    }
                    if !self.is_at_end() {
                        self.advance();
                        self.advance();
                    } else {
                        // EOF inside a block comment: report it instead of
                        // ending the comment silently.
                        diag.error(
                            ErrorCode::SyntaxUnterminatedComment,
                            "Unterminated block comment".into(),
                            Some(SourceSpan::new(
                                self.line,
                                self.col,
                                self.line,
                                self.col,
                                self.file.clone(),
                            )),
                        );
                    }
                }
                _ => break,
            }
        }
    }

    fn scan_string_literal(
        &mut self,
        start_line: usize,
        start_col: usize,
        allow_interpolation: bool,
        diag: &mut DiagnosticEngine,
        tokens: &mut Vec<Token>,
    ) {
        let mut s = String::new();
        let mut is_interpolated = false;
        let mut closed = false;

        while !self.is_at_end() {
            let c = self.advance();
            if c == '"' {
                closed = true;
                break;
            }
            if c == '\\' {
                if self.is_at_end() {
                    break;
                }
                let next_c = self.advance();
                match next_c {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    '\\' => s.push('\\'),
                    '"' => s.push('"'),
                    '0' => s.push('\0'),
                    '{' => {
                        if allow_interpolation {
                            s.push('\\');
                            s.push('{');
                        } else {
                            s.push('{');
                        }
                    }
                    other => {
                        s.push('\\');
                        s.push(other);
                    }
                }
                continue;
            }
            if allow_interpolation && c == '{' {
                is_interpolated = true;
            }
            s.push(c);
        }

        let span = SourceSpan::new(
            start_line,
            start_col,
            self.line,
            self.col,
            self.file.clone(),
        );
        if !closed {
            diag.error(
                ErrorCode::SyntaxUnterminatedString,
                "Unterminated string literal".into(),
                Some(span.clone()),
            );
        } else if allow_interpolation && is_interpolated && s.contains('}') {
            tokens.push(Token::new(
                TokenType::InterpolatedString(s.clone()),
                format!("\"{}\"", s),
                span,
            ));
        } else {
            tokens.push(Token::new(
                TokenType::StringLiteral(s.clone()),
                format!("\"{}\"", s),
                span,
            ));
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.chars[self.pos]
        }
    }

    fn peek_next(&self) -> char {
        if self.pos + 1 >= self.chars.len() {
            '\0'
        } else {
            self.chars[self.pos + 1]
        }
    }

    fn advance(&mut self) -> char {
        if self.is_at_end() {
            return '\0';
        }
        let ch = self.chars[self.pos];
        self.pos += 1;
        if ch == '\n' {
            // Match the newline handling in skip_whitespace_and_comments so
            // spans stay correct even when '\n' is consumed indirectly (e.g.
            // inside a string literal).
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        ch
    }

    fn scan_numeric_suffix(&mut self) -> Option<String> {
        if self.pos >= self.chars.len() {
            return None;
        }
        let has_leading_underscore = self.chars.get(self.pos) == Some(&'_');
        let offset = if has_leading_underscore { 1 } else { 0 };
        if self.pos + offset >= self.chars.len() {
            return None;
        }
        let remaining: String = self.chars[self.pos + offset..].iter().take(6).collect();
        for suffix in &[
            "usize", "isize", "u128", "i128", "f64", "f32", "i64", "i32", "i16", "i8", "u64",
            "u32", "u16", "u8", "byte",
        ] {
            if remaining.starts_with(suffix) {
                let next_char = self.chars.get(self.pos + offset + suffix.len());
                if next_char.is_none()
                    || (!next_char.unwrap().is_ascii_alphanumeric() && *next_char.unwrap() != '_')
                {
                    if has_leading_underscore {
                        self.advance();
                    }
                    for _ in 0..suffix.len() {
                        self.advance();
                    }
                    return Some((*suffix).to_string());
                }
            }
        }
        None
    }

    fn validate_int_suffix_range(val: i128, suffix: &str) -> bool {
        match suffix {
            "i8" => (-128..=127).contains(&val),
            "u8" | "byte" => (0..=255).contains(&val),
            "i16" => (-32768..=32767).contains(&val),
            "u16" => (0..=65535).contains(&val),
            "i32" => (-2147483648..=2147483647).contains(&val),
            "u32" => (0..=4294967295).contains(&val),
            "i64" | "isize" => val >= (i64::MIN as i128) && val <= (i64::MAX as i128),
            "u64" | "usize" => (0..=u64::MAX as i128).contains(&val),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_empty_and_comments() {
        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new("", "test.dtr");
        let tokens = lexer.tokenize(&mut diag);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Eof);
        assert!(!diag.has_errors());

        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new(
            "// single line comment\n/* multi\nline\ncomment */",
            "test.dtr",
        );
        let tokens = lexer.tokenize(&mut diag);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Eof);
        assert!(!diag.has_errors());
    }

    #[test]
    fn test_lexer_10k_parens() {
        let mut diag = DiagnosticEngine::new("en");
        let s = "(".repeat(10000);
        let mut lexer = Lexer::new(&s, "test.dtr");
        let tokens = lexer.tokenize(&mut diag);
        assert_eq!(tokens.len(), 10001);
        assert!(!diag.has_errors());
    }

    #[test]
    fn test_lexer_unicode_and_unterminated() {
        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new("let 变量 = 42;", "test.dtr");
        let tokens = lexer.tokenize(&mut diag);
        assert!(!diag.has_errors());
        assert_eq!(tokens[1].token_type, TokenType::Identifier("变量".into()));

        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new("\"unterminated string", "test.dtr");
        let _ = lexer.tokenize(&mut diag);
        assert!(diag.has_errors());

        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new("/* unterminated comment", "test.dtr");
        let _ = lexer.tokenize(&mut diag);
        assert!(diag.has_errors());
    }
}
