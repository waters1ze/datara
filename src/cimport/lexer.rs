#[derive(Debug, Clone, PartialEq)]
pub enum CTokenKind {
    Ident(String),
    IntLit(i64),
    StringLit(String),

    // Keywords
    Void,
    Int,
    Short,
    Long,
    Signed,
    Unsigned,
    Float,
    Double,
    Char,
    Const,
    Volatile,
    Static,
    Extern,
    Inline,
    Struct,
    Union,
    Enum,
    Typedef,

    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Star,
    Equal,
    Pipe,
    Ampersand,
    Plus,
    Minus,
    Tilde,
    Exclamation,
    Ellipsis, // ...

    // Preprocessor
    Hash,
    Newline,

    Eof,
}

#[derive(Debug, Clone)]
pub struct CToken {
    pub kind: CTokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct CLexer<'a> {
    _src: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> CLexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            _src: src,
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    pub fn tokenize(&mut self) -> Vec<CToken> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek() {
            let line = self.line;
            let col = self.col;

            // Whitespace (excluding newline)
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
                continue;
            }

            // Newline
            if ch == '\n' {
                self.advance();
                tokens.push(CToken {
                    kind: CTokenKind::Newline,
                    line,
                    col,
                });
                continue;
            }

            // Line continuation backslash
            if ch == '\\' && self.peek_next() == Some('\n') {
                self.advance();
                self.advance();
                continue;
            }

            // Comments
            if ch == '/' {
                if self.peek_next() == Some('/') {
                    // Line comment
                    self.advance();
                    self.advance();
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                    continue;
                } else if self.peek_next() == Some('*') {
                    // Block comment
                    self.advance();
                    self.advance();
                    while let Some(c) = self.peek() {
                        if c == '*' && self.peek_next() == Some('/') {
                            self.advance();
                            self.advance();
                            break;
                        }
                        self.advance();
                    }
                    continue;
                }
            }

            // String literal
            if ch == '"' {
                self.advance();
                let mut s = String::new();
                while let Some(c) = self.peek() {
                    if c == '"' {
                        self.advance();
                        break;
                    } else if c == '\\' {
                        self.advance();
                        match self.peek() {
                            Some('n') => {
                                s.push('\n');
                                self.advance();
                            }
                            Some('t') => {
                                s.push('\t');
                                self.advance();
                            }
                            Some('r') => {
                                s.push('\r');
                                self.advance();
                            }
                            Some('0') => {
                                s.push('\0');
                                self.advance();
                            }
                            Some('"') => {
                                s.push('"');
                                self.advance();
                            }
                            Some('\\') => {
                                s.push('\\');
                                self.advance();
                            }
                            Some(other) => {
                                s.push(other);
                                self.advance();
                            }
                            None => break,
                        }
                    } else {
                        s.push(c);
                        self.advance();
                    }
                }
                tokens.push(CToken {
                    kind: CTokenKind::StringLit(s),
                    line,
                    col,
                });
                continue;
            }

            // Number literal
            if ch.is_ascii_digit() {
                let mut s = String::new();
                let is_hex =
                    ch == '0' && (self.peek_next() == Some('x') || self.peek_next() == Some('X'));
                if is_hex {
                    s.push(self.advance().unwrap());
                    s.push(self.advance().unwrap());
                    while let Some(c) = self.peek() {
                        if c.is_ascii_hexdigit() {
                            s.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                } else {
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            s.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                }
                // Skip integer suffixes (u, l, ul, ull, etc.)
                while let Some(c) = self.peek() {
                    if c == 'u' || c == 'U' || c == 'l' || c == 'L' {
                        self.advance();
                    } else {
                        break;
                    }
                }

                let val = if is_hex {
                    i64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                        .unwrap_or(0)
                } else if s.starts_with('0') && s.len() > 1 {
                    i64::from_str_radix(&s, 8).unwrap_or(0)
                } else {
                    s.parse::<i64>().unwrap_or(0)
                };

                tokens.push(CToken {
                    kind: CTokenKind::IntLit(val),
                    line,
                    col,
                });
                continue;
            }

            // Identifiers and Keywords
            if ch.is_alphabetic() || ch == '_' {
                let mut id = String::new();
                while let Some(c) = self.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        id.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }

                let kind = match id.as_str() {
                    "void" => CTokenKind::Void,
                    "int" => CTokenKind::Int,
                    "short" => CTokenKind::Short,
                    "long" => CTokenKind::Long,
                    "signed" => CTokenKind::Signed,
                    "unsigned" => CTokenKind::Unsigned,
                    "float" => CTokenKind::Float,
                    "double" => CTokenKind::Double,
                    "char" => CTokenKind::Char,
                    "const" => CTokenKind::Const,
                    "volatile" => CTokenKind::Volatile,
                    "static" => CTokenKind::Static,
                    "extern" => CTokenKind::Extern,
                    "inline" => CTokenKind::Inline,
                    "struct" => CTokenKind::Struct,
                    "union" => CTokenKind::Union,
                    "enum" => CTokenKind::Enum,
                    "typedef" => CTokenKind::Typedef,
                    _ => CTokenKind::Ident(id),
                };

                tokens.push(CToken { kind, line, col });
                continue;
            }

            // Punctuation and Operators
            self.advance();
            let kind = match ch {
                '(' => CTokenKind::LParen,
                ')' => CTokenKind::RParen,
                '{' => CTokenKind::LBrace,
                '}' => CTokenKind::RBrace,
                '[' => CTokenKind::LBracket,
                ']' => CTokenKind::RBracket,
                ';' => CTokenKind::Semicolon,
                ',' => CTokenKind::Comma,
                '*' => CTokenKind::Star,
                '=' => CTokenKind::Equal,
                '|' => CTokenKind::Pipe,
                '&' => CTokenKind::Ampersand,
                '+' => CTokenKind::Plus,
                '-' => CTokenKind::Minus,
                '~' => CTokenKind::Tilde,
                '!' => CTokenKind::Exclamation,
                '#' => CTokenKind::Hash,
                '.' => {
                    if self.peek() == Some('.') && self.peek_next() == Some('.') {
                        self.advance();
                        self.advance();
                        CTokenKind::Ellipsis
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            tokens.push(CToken { kind, line, col });
        }

        tokens.push(CToken {
            kind: CTokenKind::Eof,
            line: self.line,
            col: self.col,
        });

        tokens
    }
}
