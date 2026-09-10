use super::Lexer;
use super::tokens::TokenType;
use crate::diagnostics::DiagnosticEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopLevelKind {
    Fn,
    Class,
    Entity,
    Behavior,
    Role,
    Component,
    Packet,
    Enum,
    Use,
    Extern,
    Trait,
    Impl,
    Struct,
    Other,
}

/// Classifies a top-level construct from source text using the lexer tokens,
/// resilient against leading whitespace, tabs, single-line/multi-line comments,
/// and multiline signatures.
pub fn classify_top_level(source: &str) -> TopLevelKind {
    let mut diag = DiagnosticEngine::new("en");
    let mut lexer = Lexer::new(source, "classify");
    let tokens = lexer.tokenize(&mut diag);

    let mut iter = tokens.iter();
    if let Some(tok) = iter.next() {
        match &tok.token_type {
            TokenType::Fn | TokenType::Function => TopLevelKind::Fn,
            TokenType::Class => TopLevelKind::Class,
            TokenType::Entity => TopLevelKind::Entity,
            TokenType::Behavior => TopLevelKind::Behavior,
            TokenType::Role => TopLevelKind::Role,
            TokenType::Component => TopLevelKind::Component,
            TokenType::Packet => TopLevelKind::Packet,
            TokenType::Enum => TopLevelKind::Enum,
            TokenType::Use => TopLevelKind::Use,
            TokenType::Extern => TopLevelKind::Extern,
            TokenType::Trait => TopLevelKind::Trait,
            TokenType::Impl => TopLevelKind::Impl,
            TokenType::Struct => TopLevelKind::Struct,
            TokenType::Pub => {
                if let Some(next) = iter.next() {
                    match &next.token_type {
                        TokenType::Fn | TokenType::Function => TopLevelKind::Fn,
                        TokenType::Class => TopLevelKind::Class,
                        TokenType::Entity => TopLevelKind::Entity,
                        TokenType::Behavior => TopLevelKind::Behavior,
                        TokenType::Role => TopLevelKind::Role,
                        TokenType::Component => TopLevelKind::Component,
                        TokenType::Packet => TopLevelKind::Packet,
                        TokenType::Enum => TopLevelKind::Enum,
                        TokenType::Trait => TopLevelKind::Trait,
                        TokenType::Impl => TopLevelKind::Impl,
                        TokenType::Struct => TopLevelKind::Struct,
                        _ => TopLevelKind::Other,
                    }
                } else {
                    TopLevelKind::Other
                }
            }
            _ => TopLevelKind::Other,
        }
    } else {
        TopLevelKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_simple_declarations() {
        assert_eq!(classify_top_level("fn foo() {}"), TopLevelKind::Fn);
        assert_eq!(
            classify_top_level("class Point { x: Int }"),
            TopLevelKind::Class
        );
        assert_eq!(classify_top_level("entity User {}"), TopLevelKind::Entity);
        assert_eq!(
            classify_top_level("behavior Loggable {}"),
            TopLevelKind::Behavior
        );
        assert_eq!(classify_top_level("role Admin {}"), TopLevelKind::Role);
        assert_eq!(
            classify_top_level("component Button {}"),
            TopLevelKind::Component
        );
        assert_eq!(classify_top_level("packet Header {}"), TopLevelKind::Packet);
        assert_eq!(
            classify_top_level("enum Color { Red, Green }"),
            TopLevelKind::Enum
        );
        assert_eq!(classify_top_level("use std::math"), TopLevelKind::Use);
        assert_eq!(
            classify_top_level("extern c fn puts(s: RawPtr) -> Int"),
            TopLevelKind::Extern
        );
    }

    #[test]
    fn test_classify_with_leading_whitespace_and_tabs() {
        assert_eq!(
            classify_top_level("   \t  \t fn add(a: Int, b: Int) -> Int {}"),
            TopLevelKind::Fn
        );
        assert_eq!(classify_top_level("\t\tclass Node {}"), TopLevelKind::Class);
    }

    #[test]
    fn test_classify_with_comments() {
        assert_eq!(
            classify_top_level("// Line comment\nfn calculate() {}"),
            TopLevelKind::Fn
        );
        assert_eq!(
            classify_top_level("/* Block comment */\n\t class Vector {}"),
            TopLevelKind::Class
        );
        assert_eq!(
            classify_top_level("// Comment 1\n// Comment 2\nenum State { Active }"),
            TopLevelKind::Enum
        );
    }

    #[test]
    fn test_classify_with_pub_prefix() {
        assert_eq!(
            classify_top_level("pub fn public_api() {}"),
            TopLevelKind::Fn
        );
        assert_eq!(
            classify_top_level("   pub   class PublicData {}"),
            TopLevelKind::Class
        );
    }

    #[test]
    fn test_classify_multiline() {
        let multiline =
            "fn complex_sig(\n    x: Int,\n    y: Float\n) -> Bool {\n    return true\n}";
        assert_eq!(classify_top_level(multiline), TopLevelKind::Fn);
    }

    #[test]
    fn test_classify_expressions_as_other() {
        assert_eq!(classify_top_level("1 + 2"), TopLevelKind::Other);
        assert_eq!(classify_top_level("let x = 10"), TopLevelKind::Other);
        assert_eq!(classify_top_level("out \"hello\""), TopLevelKind::Other);
        assert_eq!(classify_top_level(""), TopLevelKind::Other);
    }
}
