pub mod tokens;

use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
pub use tokens::{Token, TokenType};

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
            self.skip_whitespace_and_comments();
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
                        // A lone '&' used to produce no token at all, so `a & b`
                        // was silently rewritten to `a b`. Report it instead.
                        diag.error(
                            ErrorCode::SyntaxUnexpectedToken,
                            "Unexpected character '&'. Datara has no bitwise-and operator; did you mean '&&' (logical and)?".into(),
                            Some(SourceSpan::new(start_line, start_col, self.line, self.col, self.file.clone())),
                        );
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
                    let mut num_str = ch.to_string();
                    let mut is_float = false;

                    while !self.is_at_end()
                        && (self.peek().is_ascii_digit()
                            || (self.peek() == '.' && self.peek_next().is_ascii_digit()))
                    {
                        if self.peek() == '.' {
                            is_float = true;
                        }
                        num_str.push(self.advance());
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
                                    num_str,
                                    span,
                                ));
                            }
                            Err(e) => {
                                diag.error(
                                    ErrorCode::SyntaxInvalidNumber,
                                    format!("Invalid float literal '{}': {}", num_str, e),
                                    Some(span),
                                );
                            }
                        }
                    } else {
                        match num_str.parse::<i64>() {
                            Ok(val) => {
                                tokens.push(Token::new(TokenType::IntLiteral(val), num_str, span));
                            }
                            Err(e) => {
                                diag.error(
                                    ErrorCode::SyntaxInvalidNumber,
                                    format!("Invalid integer literal '{}': {}", num_str, e),
                                    Some(span),
                                );
                            }
                        }
                    }
                }

                _ if ch.is_alphabetic() || ch == '_' => {
                    let mut ident_str = ch.to_string();
                    while !self.is_at_end()
                        && (self.peek().is_alphanumeric()
                            || self.peek() == '_'
                            || self.peek() == '-')
                    {
                        if self.peek() == '-' && !self.peek_next().is_alphabetic() {
                            break;
                        }
                        ident_str.push(self.advance());
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
                        _ => TokenType::Identifier(ident_str.clone()),
                    };

                    tokens.push(Token::new(tt, ident_str, span));
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

    fn skip_whitespace_and_comments(&mut self) {
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
                        if self.peek() == '\n' {
                            self.line += 1;
                            self.col = 1;
                        }
                        self.advance();
                    }
                    if !self.is_at_end() {
                        self.advance();
                        self.advance();
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
        let ch = self.chars[self.pos];
        self.pos += 1;
        self.col += 1;
        ch
    }
}
