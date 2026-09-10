use forgen::ast::*;
use forgen::diagnostics::{DiagnosticEngine, ErrorCode};
use forgen::lexer::Lexer;
use forgen::lexer::tokens::TokenType;
use forgen::parser::Parser;
use forgen::resolver::Resolver;
use forgen::types::{DataraType, TypeChecker};
use std::collections::HashMap;

#[test]
fn test_numeric_literal_suffixes_valid() {
    let mut diag = DiagnosticEngine::new("en");
    let code = "let a = 42i32; let b = 2.75f32; let c = 0xFFu8; let d = 100_000usize; let e = 0b1010u8; let f = 0o755u16;";
    let mut lexer = Lexer::new(code, "valid_suffixes.dtr");
    let tokens = lexer.tokenize(&mut diag);

    assert!(
        !diag.has_errors(),
        "Expected no lexer errors for valid suffixes: {:?}",
        diag.diagnostics
    );

    // Filter literals
    let int_tokens: Vec<_> = tokens
        .iter()
        .filter_map(|t| match t.token_type {
            TokenType::IntLiteral(v) => Some((v, t.lexeme.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(int_tokens.len(), 5);
    assert_eq!(int_tokens[0], (42, "42i32".to_string()));
    assert_eq!(int_tokens[1], (255, "0xFFu8".to_string()));
    assert_eq!(int_tokens[2], (100000, "100_000usize".to_string()));
    assert_eq!(int_tokens[3], (10, "0b1010u8".to_string()));
    assert_eq!(int_tokens[4], (493, "0o755u16".to_string()));

    let float_tokens: Vec<_> = tokens
        .iter()
        .filter_map(|t| match t.token_type {
            TokenType::FloatLiteral(v) => Some((v, t.lexeme.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(float_tokens.len(), 1);
    assert_eq!(float_tokens[0].1, "2.75f32");
    assert!((float_tokens[0].0 - 2.75).abs() < 1e-5);
}

#[test]
fn test_numeric_literal_suffix_range_violation() {
    let mut diag = DiagnosticEngine::new("en");
    let code = "let x = 300u8;";
    let mut lexer = Lexer::new(code, "range_err.dtr");
    let _ = lexer.tokenize(&mut diag);

    assert!(diag.has_errors(), "Expected RangeViolation error for 300u8");
    let has_range_violation = diag
        .diagnostics
        .iter()
        .any(|d| d.code == ErrorCode::RangeViolation.as_str());
    assert!(
        has_range_violation,
        "Expected ErrorCode::RangeViolation (E0947), got: {:?}",
        diag.diagnostics
    );

    let mut diag2 = DiagnosticEngine::new("en");
    let code2 = "let y = 0xFFFFu8;";
    let mut lexer2 = Lexer::new(code2, "hex_range_err.dtr");
    let _ = lexer2.tokenize(&mut diag2);

    assert!(
        diag2.has_errors(),
        "Expected RangeViolation error for 0xFFFFu8"
    );
    assert!(
        diag2
            .diagnostics
            .iter()
            .any(|d| d.code == ErrorCode::RangeViolation.as_str())
    );
}

#[test]
fn test_outcome_type_resolution_and_size() {
    let resolver = Resolver::new();

    // Outcome<Int, String>
    let mut tn = TypeNode::new("Outcome", Default::default());
    tn.generic_args = vec![
        TypeNode::new("Int", Default::default()),
        TypeNode::new("String", Default::default()),
    ];

    let resolved = TypeChecker::resolve_tn(&resolver, &tn);
    assert_eq!(
        resolved,
        DataraType::Result(Box::new(DataraType::Int), Box::new(DataraType::String))
    );
    assert!(resolved.is_outcome());

    // Outcome sum-type layout: 8-byte discriminant + max(Int: 8, String: 24) = 32 bytes
    assert_eq!(resolved.size_in_bytes(), 32);

    // Shorthand Outcome<Int> defaults error type to String
    let mut tn_single = TypeNode::new("Outcome", Default::default());
    tn_single.generic_args = vec![TypeNode::new("Int", Default::default())];

    let resolved_single = TypeChecker::resolve_tn(&resolver, &tn_single);
    assert_eq!(
        resolved_single,
        DataraType::Result(Box::new(DataraType::Int), Box::new(DataraType::String))
    );
}

#[test]
fn test_dyn_trait_fat_pointer_layout() {
    let mut resolver = Resolver::new();

    // Declare a trait in the resolver
    let trait_sym = forgen::resolver::Symbol {
        name: "Drawable".to_string(),
        kind: forgen::resolver::SymbolKind::Trait,
        span: Default::default(),
        is_mut: false,
        is_export: true,
        generic_params: vec![],
        fields: HashMap::new(),
        methods: HashMap::new(),
        base_type: None,
        compositions: vec![],
        type_node: None,
        return_type: None,
    };
    resolver.traits.insert("Drawable".to_string(), trait_sym);

    // Resolve `dyn Drawable`
    let tn_dyn = TypeNode::new("dyn Drawable", Default::default());
    let resolved_dyn = TypeChecker::resolve_tn(&resolver, &tn_dyn);
    assert_eq!(resolved_dyn, DataraType::Trait("Drawable".to_string()));
    assert!(resolved_dyn.is_dyn_trait());

    // Dynamic trait object fat pointer must be 16 bytes: instance_ptr (8) + vtable_ptr (8)
    assert_eq!(
        resolved_dyn.size_in_bytes(),
        16,
        "dyn Trait fat pointer must be exactly 16 bytes (data_ptr + vtable_ptr)"
    );

    // Resolve bare `Drawable` known trait
    let tn_bare = TypeNode::new("Drawable", Default::default());
    let resolved_bare = TypeChecker::resolve_tn(&resolver, &tn_bare);
    assert_eq!(resolved_bare, DataraType::Trait("Drawable".to_string()));
    assert_eq!(resolved_bare.size_in_bytes(), 16);
}

#[test]
fn test_question_operator_error_propagate_parsing() {
    let mut diag = DiagnosticEngine::new("en");
    let code = "fn compute() -> Int {\n    let val = fetch_data()?;\n    return val;\n}";
    let mut lexer = Lexer::new(code, "test_propagate.dtr");
    let tokens = lexer.tokenize(&mut diag);
    assert!(!diag.has_errors());

    let mut parser = Parser::new(tokens, &mut diag, "test_propagate.dtr");
    let prog = parser.parse_program();
    assert!(!diag.has_errors(), "Parse errors: {:?}", diag.diagnostics);

    let mut found_propagate = false;
    for decl in &prog.declarations {
        if let Decl::Function(f) = decl
            && let Stmt::Block(stmts, _) = f.body.as_ref()
        {
            for stmt in stmts {
                if let Stmt::Let { init, .. } = stmt
                    && matches!(init, Expr::ErrorPropagate(..))
                {
                    found_propagate = true;
                }
            }
        }
    }
    assert!(
        found_propagate,
        "Expected Expr::ErrorPropagate generated by '?' operator"
    );
}
