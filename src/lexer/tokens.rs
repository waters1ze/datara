use crate::diagnostics::SourceSpan;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Let,
    Mut,
    Const,
    Fn,
    Function,
    Class,
    Record,
    Enum,
    Component,
    Role,
    Behavior,
    From,
    Extends,
    With,
    Replaces,
    Export,
    Import,
    As,
    If,
    Else,
    For,
    In,
    While,
    Loop,
    Match,
    When,
    Decide,
    Select,
    Return,
    Break,
    Continue,
    Parallel,
    Async,
    Await,
    Task,
    Flow,
    Entity,
    Process,
    Then,
    Unsafe,
    Extern,
    True,
    False,
    None,
    Own,
    View,
    MutView,
    Shared,
    Out,
    Err,
    Use,
    Try,
    Catch,
    Cli,
    App,
    Command,
    Val,
    Packet,
    Using,
    OrKeyword,
    Type,
    Where,
    Require,
    Ensure,

    // Literals & Identifiers
    Identifier(String),
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    InterpolatedString(String),
    CharLiteral(char),

    // Operators & Delimiters
    ColonEqual,   // :=
    FatArrow,     // =>
    Arrow,        // ->
    Pipe,         // |>
    DotDot,       // ..
    DotDotEq,     // ..=
    DotDotLt,     // ..<
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Percent,      // %
    EqualEqual,   // ==
    NotEqual,     // !=
    Less,         // <
    LessEqual,    // <=
    Greater,      // >
    GreaterEqual, // >=
    And,          // &&
    Or,           // ||
    Bang,         // !
    Question,     // ?
    Equal,        // =
    Colon,        // :
    Comma,        // ,
    Dot,          // .
    LParen,       // (
    RParen,       // )
    LBrace,       // {
    RBrace,       // }
    LBracket,     // [
    RBracket,     // ]
    Semicolon,    // ;

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub lexeme: String,
    pub span: SourceSpan,
}

impl Token {
    pub fn new(token_type: TokenType, lexeme: String, span: SourceSpan) -> Self {
        Self {
            token_type,
            lexeme,
            span,
        }
    }
}
