use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
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
const MAX_PARSE_DEPTH: usize = 64;

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

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        while self.match_token(&TokenType::At) {
            let start_span = self.previous().span.clone();
            if let Some(name) = self.consume_ident_or_keyword("Expected attribute name after '@'") {
                let mut args = Vec::new();
                if self.match_token(&TokenType::LParen) {
                    while !self.check(&TokenType::RParen) && !self.is_at_end() {
                        if let Some(arg_name) =
                            self.consume_ident_or_keyword("Expected argument name")
                        {
                            let mut val = String::new();
                            if self.match_token(&TokenType::Colon) {
                                val = self.consume_literal_or_ident();
                            }
                            args.push((arg_name, val));
                        }
                        if !self.match_token(&TokenType::Comma) {
                            break;
                        }
                    }
                    let _ =
                        self.consume(&TokenType::RParen, "Expected ')' after attribute arguments");
                }
                let end_span = self.previous().span.clone();
                attrs.push(Attribute {
                    name,
                    args,
                    span: SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        end_span.end_line,
                        end_span.end_col,
                        self.file.clone(),
                    ),
                });
            }
        }
        attrs
    }

    fn consume_literal_or_ident(&mut self) -> String {
        let token = self.advance();
        match &token.token_type {
            TokenType::StringLiteral(s) => s.clone(),
            TokenType::IntLiteral(n) => n.to_string(),
            TokenType::FloatLiteral(f) => f.to_string(),
            TokenType::Identifier(id) => id.clone(),
            _ => token.lexeme.clone(),
        }
    }

    fn check_ident_lexeme(&self, expected: &str) -> bool {
        if self.is_at_end() {
            return false;
        }
        match &self.peek().token_type {
            TokenType::Identifier(id) => id == expected,
            _ => false,
        }
    }

    fn parse_c_import_decl(&mut self) -> Option<CImportDecl> {
        let start_span = self.previous().span.clone();
        self.advance(); // consume 'c'
        let header_token = self.peek().clone();
        let header_path = match &header_token.token_type {
            TokenType::StringLiteral(s) => {
                self.advance();
                s.clone()
            }
            _ => {
                self.error("Expected header file string after 'import c'");
                return None;
            }
        };

        let mut link_libs = Vec::new();
        if self.match_token(&TokenType::With) {
            if self.check_ident_lexeme("link") {
                self.advance(); // consume 'link'
                self.consume(&TokenType::LParen, "Expected '(' after 'link'")?;
                while !self.check(&TokenType::RParen) && !self.is_at_end() {
                    let lib_token = self.peek().clone();
                    match &lib_token.token_type {
                        TokenType::StringLiteral(s) => {
                            self.advance();
                            link_libs.push(s.clone());
                        }
                        _ => {
                            self.error("Expected library name string in link(...)");
                            return None;
                        }
                    }
                    if !self.match_token(&TokenType::Comma) {
                        break;
                    }
                }
                self.consume(&TokenType::RParen, "Expected ')' after link libraries")?;
            }
        }
        let _ = self.match_token(&TokenType::Semicolon);
        let end_span = self.previous().span.clone();
        Some(CImportDecl {
            header_path,
            link_libs,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_span.end_line,
                end_span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_declaration_with_attrs(&mut self, attrs: Vec<Attribute>) -> Option<Decl> {
        let is_export = self.match_token(&TokenType::Export) || self.match_token(&TokenType::Pub);

        if self.match_token(&TokenType::Import) {
            if self.check_ident_lexeme("c") {
                return self.parse_c_import_decl().map(Decl::CImport);
            }
            return self.parse_use_decl().map(Decl::Use);
        }
        if self.match_token(&TokenType::Use) || self.match_token(&TokenType::Using) {
            return self.parse_use_decl().map(Decl::Use);
        }
        if self.match_token(&TokenType::Register) {
            return self.parse_register_decl(attrs).map(Decl::Register);
        }
        if self.match_token(&TokenType::Class)
            || self.match_token(&TokenType::Entity)
            || self.match_token(&TokenType::Struct)
            || self.match_token(&TokenType::Record)
        {
            return self.parse_class_decl(is_export, attrs).map(Decl::Class);
        }
        if self.match_token(&TokenType::Enum) {
            return self.parse_enum_decl(is_export).map(Decl::Enum);
        }
        if self.match_token(&TokenType::Trait) {
            return self.parse_trait_decl(is_export).map(Decl::Trait);
        }
        if self.match_token(&TokenType::Impl) {
            return self.parse_impl_block().map(Decl::Impl);
        }
        if self.match_token(&TokenType::Behavior) {
            return self.parse_behavior_decl().map(Decl::Behavior);
        }
        if self.match_token(&TokenType::Component) {
            return self.parse_component_decl(is_export).map(Decl::Component);
        }
        if self.match_token(&TokenType::Role) {
            return self.parse_role_decl(is_export).map(Decl::Role);
        }
        if self.match_token(&TokenType::Async) {
            let async_span = self.previous().span.clone();
            let mut async_attrs = attrs;
            async_attrs.push(Attribute {
                name: "async".to_string(),
                args: Vec::new(),
                span: async_span,
            });
            if self.match_token(&TokenType::Fn) || self.match_token(&TokenType::Function) {
                return self
                    .parse_function_decl(is_export, async_attrs)
                    .map(Decl::Task);
            }
            if self.match_token(&TokenType::Task) {
                return self
                    .parse_function_decl(is_export, async_attrs)
                    .map(Decl::Task);
            }
            self.error("Expected 'fn', 'function', or 'task' after 'async'");
            return None;
        }
        if self.match_token(&TokenType::Fn) || self.match_token(&TokenType::Function) {
            return self
                .parse_function_decl(is_export, attrs)
                .map(Decl::Function);
        }
        if self.check_ident_str("test") {
            let next_is_fn = self
                .tokens
                .get(self.current + 1)
                .map(|t| matches!(t.token_type, TokenType::Fn | TokenType::Function))
                .unwrap_or(false);
            if next_is_fn {
                self.advance(); // consume "test"
                self.advance(); // consume "fn" / "function"
                let mut test_attrs = attrs;
                test_attrs.push(Attribute {
                    name: "test".to_string(),
                    args: Vec::new(),
                    span: self.previous().span.clone(),
                });
                return self
                    .parse_function_decl(is_export, test_attrs)
                    .map(Decl::Function);
            }
        }
        if self.match_token(&TokenType::Flow) {
            self.error(
                "SyntaxError: 'flow' is not a top-level declaration. Use 'fn' to define functions, and 'flow' inside pipelines: '|> flow Stage'.",
            );
            return None;
        }
        if self.match_token(&TokenType::Process) {
            return self.parse_function_decl(is_export, attrs).map(Decl::Flow);
        }
        if self.match_token(&TokenType::Task) {
            return self.parse_function_decl(is_export, attrs).map(Decl::Task);
        }
        if self.match_token(&TokenType::Packet) {
            return self.parse_packet_decl().map(Decl::Packet);
        }
        if self.match_token(&TokenType::Extern) {
            return self.parse_extern_fn_decl().map(Decl::ExternFn);
        }
        if self.match_token(&TokenType::Type) {
            return self.parse_type_decl(is_export).map(Decl::Type);
        }

        self.error(
            "Expected top-level declaration (class, entity, behavior, fn, register, process, component, role, packet, extern, type, use)",
        );
        None
    }

    fn parse_register_decl(&mut self, attributes: Vec<Attribute>) -> Option<RegisterDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected register name after 'register'")?;
        let tok = self.consume_ident_or_keyword("Expected 'at' after register name")?;
        if tok != "at" {
            self.error("Expected 'at' after register name");
            return None;
        }

        let base_address = match &self.peek().token_type {
            TokenType::IntLiteral(n) => {
                if *n < 0 {
                    self.error("Register base address must be non-negative");
                    return None;
                }
                let val = *n as u64;
                self.advance();
                val
            }
            _ => {
                if self.negative_literal_follows() {
                    self.error("Register base address must be non-negative");
                    return None;
                }
                self.error("Expected base address integer/hex literal after 'at'");
                return None;
            }
        };

        self.consume(
            &TokenType::LBrace,
            "Expected '{' to begin register definition",
        )?;
        let mut fields = Vec::new();
        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let f_span = self.peek().span.clone();
            let f_name = self.consume_ident("Expected register field name")?;
            self.consume(&TokenType::Colon, "Expected ':' after register field name")?;
            let f_type = self.parse_type()?;
            let tok = self.consume_ident_or_keyword("Expected 'at' after register field type")?;
            if tok != "at" {
                self.error("Expected 'at' after register field type");
                return None;
            }
            let offset = match &self.peek().token_type {
                TokenType::IntLiteral(n) => {
                    if *n < 0 {
                        self.error("Register field offset must be non-negative");
                        return None;
                    }
                    let val = *n as u64;
                    self.advance();
                    val
                }
                _ => {
                    if self.negative_literal_follows() {
                        self.error("Register field offset must be non-negative");
                        return None;
                    }
                    self.error("Expected register field offset integer/hex literal after 'at'");
                    return None;
                }
            };
            self.match_token(&TokenType::Comma);
            fields.push(RegisterField {
                name: f_name,
                type_node: f_type,
                offset,
                span: SourceSpan::new(
                    f_span.start_line,
                    f_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }
        self.consume(&TokenType::RBrace, "Expected '}' after register fields")?;
        Some(RegisterDecl {
            name,
            base_address,
            fields,
            attributes,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_type_decl(&mut self, is_export: bool) -> Option<TypeDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected type alias name after 'type'")?;
        self.consume(&TokenType::Equal, "Expected '=' after type alias name")?;
        let base_type = self.parse_type()?;
        self.match_token(&TokenType::Semicolon);
        Some(TypeDecl {
            name,
            base_type,
            is_export,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn consume_ident_or_keyword(&mut self, message: &str) -> Option<String> {
        if !self.is_at_end() {
            let token = self.advance();
            match &token.token_type {
                TokenType::Identifier(s) => return Some(s.clone()),
                TokenType::Role => return Some("role".to_string()),
                TokenType::App => return Some("app".to_string()),
                TokenType::Cli => return Some("cli".to_string()),
                TokenType::Command => return Some("command".to_string()),
                TokenType::Task => return Some("task".to_string()),
                TokenType::Flow => return Some("flow".to_string()),
                TokenType::Out => return Some("out".to_string()),
                TokenType::Err => return Some("err".to_string()),
                TokenType::Try => return Some("try".to_string()),
                TokenType::Catch => return Some("catch".to_string()),
                TokenType::Component => return Some("component".to_string()),
                TokenType::Behavior => return Some("behavior".to_string()),
                TokenType::From => return Some("from".to_string()),
                TokenType::Use => return Some("use".to_string()),
                TokenType::Import => return Some("import".to_string()),
                TokenType::View => return Some("view".to_string()),
                TokenType::Mut => return Some("mut".to_string()),
                TokenType::With => return Some("with".to_string()),
                TokenType::Match => return Some("match".to_string()),
                TokenType::Decide => return Some("decide".to_string()),
                TokenType::Select => return Some("select".to_string()),
                TokenType::Replaces => return Some("replaces".to_string()),
                TokenType::Val => return Some("val".to_string()),
                TokenType::Packet => return Some("packet".to_string()),
                TokenType::Using => return Some("using".to_string()),
                TokenType::OrKeyword => return Some("or".to_string()),
                TokenType::Process => return Some("process".to_string()),
                TokenType::Async => return Some("async".to_string()),
                TokenType::Await => return Some("await".to_string()),
                TokenType::Extern => return Some("extern".to_string()),
                TokenType::Loop => return Some("loop".to_string()),
                _ => {}
            }
        }
        self.error(message);
        None
    }

    fn consume_import_name(&mut self, message: &str) -> Option<String> {
        self.consume_ident_or_keyword(message)
    }

    fn parse_use_decl(&mut self) -> Option<UseDecl> {
        let start_span = self.previous().span.clone();
        let mut path = Vec::new();
        let mut group = Vec::new();
        let mut alias = None;

        if let Some(first) = self.consume_import_name("Expected module name") {
            path.push(first);
        }

        // Wave 5.2: Support `use python <module> as <alias>` syntax (marker: use python)
        if path.len() == 1
            && path[0] == "python"
            && !self.check(&TokenType::Dot)
            && !self.check(&TokenType::As)
            && !self.check(&TokenType::Semicolon)
            && !self.is_at_end()
            && self.peek().span.start_line == start_span.start_line
            && matches!(self.peek().token_type, TokenType::Identifier(_))
        {
            if let Some(mod_name) = self.consume_import_name("Expected Python module name") {
                path.push(mod_name);
            }
        }

        while self.match_token(&TokenType::Dot) {
            if self.match_token(&TokenType::LBrace) {
                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    if let Some(item) = self.consume_import_name("Expected imported item name") {
                        group.push(item);
                    }
                    if !self.match_token(&TokenType::Comma) {
                        break;
                    }
                }
                self.consume(&TokenType::RBrace, "Expected '}' after imported items")?;
                break;
            } else if let Some(segment) = self.consume_import_name("Expected path segment") {
                path.push(segment);
            }
        }

        if self.match_token(&TokenType::As) {
            alias = self.consume_import_name("Expected alias name");
        }

        let end_span = self.previous().span.clone();
        Some(UseDecl {
            path,
            group,
            alias,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_span.end_line,
                end_span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_class_decl(
        &mut self,
        is_export: bool,
        attributes: Vec<Attribute>,
    ) -> Option<ClassDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected class name")?;

        let mut generic_params = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(param) = self.consume_ident("Expected generic parameter name") {
                    generic_params.push(param);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after generic parameters")?;
        }

        let base_type = None;
        let mut compositions = Vec::new();

        if self.match_token(&TokenType::From) || self.match_token(&TokenType::Extends) {
            self.error("SyntaxError: Class inheritance ('from'/'extends') has been removed. Use flat composition 'using Component' inside the class body.");
            return None;
        }

        while self.match_token(&TokenType::Plus) || self.match_token(&TokenType::With) {
            loop {
                if let Some(comp) = self.consume_ident("Expected component or role name") {
                    compositions.push(comp);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume(&TokenType::LBrace, "Expected '{' before class body")?;
        let mut body_items = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if let Some(item) = self.parse_class_item() {
                body_items.push(item);
            } else {
                self.synchronize();
            }
        }

        let mut invariants = Vec::new();
        for item in &body_items {
            if let ClassItem::Invariant(inv, _) = item {
                invariants.push(inv.clone());
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after class body")?;
        Some(ClassDecl {
            name,
            attributes,
            generic_params,
            base_type,
            compositions,
            body_items,
            invariants,
            is_export,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_enum_decl(&mut self, is_export: bool) -> Option<EnumDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected enum name")?;

        let mut generic_params = Vec::new();
        if self.match_token(&TokenType::Less) {
            loop {
                if let Some(gp) = self.consume_ident("Expected generic parameter name") {
                    generic_params.push(gp);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after generic parameters")?;
        }

        self.consume(&TokenType::LBrace, "Expected '{' before enum body")?;
        let mut variants = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let v_span_start = self.peek().span.clone();
            if let Some(v_name) = self.consume_ident("Expected variant name") {
                let mut fields = Vec::new();
                if self.match_token(&TokenType::LParen) {
                    if !self.check(&TokenType::RParen) {
                        loop {
                            if let Some(ty) = self.parse_type() {
                                fields.push(ty);
                            } else {
                                break;
                            }
                            if !self.match_token(&TokenType::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(&TokenType::RParen, "Expected ')' after variant fields")?;
                }
                let v_span = SourceSpan::new(
                    v_span_start.start_line,
                    v_span_start.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                );
                variants.push(EnumVariant {
                    name: v_name,
                    fields,
                    span: v_span,
                });
                self.match_token(&TokenType::Comma);
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after enum body")?;
        Some(EnumDecl {
            name,
            generic_params,
            variants,
            is_export,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_behavior_decl(&mut self) -> Option<BehaviorDecl> {
        let start_span = self.previous().span.clone();
        let target_type = self.consume_ident("Expected target type for behavior")?;

        self.consume(&TokenType::LBrace, "Expected '{' before behavior body")?;
        let mut body_items = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if let Some(item) = self.parse_class_item() {
                body_items.push(item);
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after behavior body")?;
        Some(BehaviorDecl {
            target_type,
            body_items,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_component_decl(&mut self, is_export: bool) -> Option<ComponentDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected component name")?;

        self.consume(&TokenType::LBrace, "Expected '{' before component body")?;
        let mut body_items = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if let Some(item) = self.parse_class_item() {
                body_items.push(item);
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after component body")?;
        Some(ComponentDecl {
            name,
            body_items,
            is_export,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_role_decl(&mut self, is_export: bool) -> Option<RoleDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected role name")?;

        self.consume(&TokenType::LBrace, "Expected '{' before role body")?;
        let mut methods = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if let Some(ClassItem::Method(m)) = self.parse_class_item() {
                methods.push(m);
            } else {
                // parse_class_item can fail without consuming anything
                // (e.g. a stray token); without resync this loop spins forever.
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after role body")?;
        Some(RoleDecl {
            name,
            methods,
            is_export,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_trait_decl(&mut self, is_export: bool) -> Option<TraitDef> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected trait name after 'trait'")?;

        let mut generic_params = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(param) = self.consume_ident("Expected generic parameter name") {
                    generic_params.push(param);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after generic parameters")?;
        }

        let mut super_traits = Vec::new();
        if self.match_token(&TokenType::Colon) {
            while !self.check(&TokenType::LBrace) && !self.is_at_end() {
                if let Some(st) = self.consume_ident("Expected super-trait name") {
                    super_traits.push(st);
                }
                if !self.match_token(&TokenType::Plus) {
                    break;
                }
            }
        }

        self.consume(&TokenType::LBrace, "Expected '{' before trait body")?;
        let mut methods = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Fn) || self.match_token(&TokenType::Function) {
                if let Some(sig) = self.parse_trait_method_sig() {
                    methods.push(sig);
                }
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after trait body")?;
        Some(TraitDef {
            name,
            generic_params,
            super_traits,
            methods,
            is_export,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_trait_method_sig(&mut self) -> Option<TraitMethodSignature> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident_or_keyword("Expected method name in trait")?;

        let mut generic_params = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(p) = self.consume_ident("Expected generic parameter name") {
                    generic_params.push(p);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after generic parameters")?;
        }

        self.consume(&TokenType::LParen, "Expected '(' after method name")?;
        let params = self.parse_param_list()?;
        self.consume(&TokenType::RParen, "Expected ')' after parameters")?;

        let mut return_type = None;
        if self.match_token(&TokenType::Arrow) {
            return_type = self.parse_type();
        }
        let default_body = if self.check(&TokenType::LBrace) {
            Some(Box::new(self.parse_block()?))
        } else if self.match_token(&TokenType::FatArrow) {
            let expr = self.parse_expression()?;
            let span = expr.span().clone();
            let _ = self.match_token(&TokenType::Semicolon);
            Some(Box::new(Stmt::Expr(expr, span)))
        } else {
            let _ = self.match_token(&TokenType::Semicolon);
            None
        };

        let end_span = self.previous().span.clone();
        Some(TraitMethodSignature {
            name,
            generic_params,
            params,
            return_type,
            default_body,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_span.end_line,
                end_span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_impl_block(&mut self) -> Option<ImplBlock> {
        let start_span = self.previous().span.clone();
        let first_name = self.consume_ident("Expected trait name or target type after 'impl'")?;

        let (trait_name, target_type) = if self.match_token(&TokenType::For) {
            let target = self.consume_ident("Expected target type after 'for'")?;
            (Some(first_name), target)
        } else {
            (None, first_name)
        };

        let mut target_type_args = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(arg) = self.consume_ident("Expected type argument") {
                    target_type_args.push(arg);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after type arguments")?;
        }

        self.consume(&TokenType::LBrace, "Expected '{' before impl body")?;
        let mut methods = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let mut attrs = self.parse_attributes();
            let is_export =
                self.match_token(&TokenType::Export) || self.match_token(&TokenType::Pub);
            let is_async = self.match_token(&TokenType::Async);
            if is_async {
                attrs.push(Attribute {
                    name: "async".to_string(),
                    args: Vec::new(),
                    span: self.previous().span.clone(),
                });
            }
            if self.match_token(&TokenType::Fn)
                || self.match_token(&TokenType::Function)
                || (is_async && self.match_token(&TokenType::Task))
            {
                if let Some(m) = self.parse_function_decl(is_export, attrs) {
                    methods.push(m);
                }
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after impl body")?;
        Some(ImplBlock {
            trait_name,
            target_type,
            target_type_args,
            methods,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_class_item(&mut self) -> Option<ClassItem> {
        let mut attrs = self.parse_attributes();
        if self.match_token(&TokenType::Using) {
            let name = self.consume_ident("Expected class name after 'using'")?;
            return Some(ClassItem::Using(name, self.previous().span.clone()));
        }

        if self.check_ident_str("invariant") {
            self.advance();
            let expr = self.parse_expression()?;
            let span = expr.span().clone();
            let _ = self.match_token(&TokenType::Semicolon);
            return Some(ClassItem::Invariant(expr, span));
        }

        let is_async = self.match_token(&TokenType::Async);
        if is_async {
            attrs.push(Attribute {
                name: "async".to_string(),
                args: Vec::new(),
                span: self.previous().span.clone(),
            });
        }

        let _ = self.match_token(&TokenType::Fn)
            || self.match_token(&TokenType::Function)
            || (is_async && self.match_token(&TokenType::Task));
        let is_replaces = self.match_token(&TokenType::Replaces);
        let is_mut = self.match_token(&TokenType::Mut);

        let mut name = self.consume_ident_or_keyword("Expected member name")?;
        let mut replaces_target = None;
        if self.match_token(&TokenType::Dot) {
            let sub_name = self.consume_ident_or_keyword("Expected member name after '.'")?;
            replaces_target = Some(format!("{}.{}", name, sub_name));
            name = sub_name;
        } else if is_replaces {
            replaces_target = Some(name.clone());
        }

        if self.check(&TokenType::LParen) {
            // Method declaration
            self.consume(&TokenType::LParen, "Expected '('")?;
            let params = self.parse_param_list()?;
            self.consume(&TokenType::RParen, "Expected ')' after parameters")?;

            let mut return_type = None;
            if self.match_token(&TokenType::Arrow) {
                return_type = self.parse_type();
            }

            let (requires, ensures) = self.parse_contracts();
            let mut decreases = None;
            if self.match_ident_str("decreases") {
                decreases = Some(self.parse_expression()?);
            }

            let mut body = None;
            let mut is_expression_body = false;

            if self.match_token(&TokenType::FatArrow) {
                let expr = self.parse_expression()?;
                let span = expr.span().clone();
                body = Some(Box::new(Stmt::Expr(expr, span)));
                is_expression_body = true;
            } else if self.check(&TokenType::LBrace) {
                body = Some(Box::new(self.parse_block()?));
            }

            Some(ClassItem::Method(MethodDecl {
                name,
                attributes: attrs,
                generic_params: Vec::new(),
                params,
                return_type,
                requires,
                ensures,
                decreases,
                body,
                is_expression_body,
                is_replaces,
                replaces_target,
                span: self.previous().span.clone(),
            }))
        } else {
            // Field declaration
            self.consume(&TokenType::Colon, "Expected ':' after field name")?;
            let type_node = self.parse_type();

            let mut bit_field = None;
            if self.match_token(&TokenType::In) {
                if self.match_token(&TokenType::Bit) {
                    if self.negative_literal_follows() {
                        self.error("Bit index must be non-negative");
                        self.advance();
                        self.advance();
                    } else if let TokenType::IntLiteral(bit_idx) = self.peek().token_type {
                        self.advance();
                        if bit_idx < 0 {
                            self.error("Bit index must be non-negative");
                        } else {
                            bit_field = Some(BitFieldRange::Single(bit_idx as usize));
                        }
                    } else {
                        self.error("Expected bit index integer after 'in bit'");
                    }
                } else if self.match_token(&TokenType::Bits) {
                    if self.negative_literal_follows() {
                        self.error("Bit index must be non-negative");
                        self.advance();
                        self.advance();
                    } else if let TokenType::IntLiteral(start) = self.peek().token_type {
                        self.advance();
                        self.consume(&TokenType::DotDotEq, "Expected '..=' in bit range")?;
                        if self.negative_literal_follows() {
                            self.error("Bit index must be non-negative");
                            self.advance();
                            self.advance();
                        } else if let TokenType::IntLiteral(end) = self.peek().token_type {
                            self.advance();
                            if start < 0 || end < 0 {
                                self.error("Bit index must be non-negative");
                            } else {
                                bit_field = Some(BitFieldRange::Range {
                                    start: start as usize,
                                    end: end as usize,
                                });
                            }
                        } else {
                            self.error("Expected end bit index integer after '..='");
                        }
                    } else {
                        self.error("Expected start bit index integer after 'in bits'");
                    }
                }
            }

            let mut default_value = None;
            if self.match_token(&TokenType::Equal) {
                default_value = self.parse_expression();
            }
            self.match_token(&TokenType::Comma);

            Some(ClassItem::Field(FieldDecl {
                name,
                type_node,
                bit_field,
                default_value,
                is_mut,
                span: self.previous().span.clone(),
            }))
        }
    }

    fn parse_function_decl(
        &mut self,
        is_export: bool,
        attributes: Vec<Attribute>,
    ) -> Option<FunctionDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected function name")?;

        let mut generic_params = Vec::new();
        let mut generic_constraints = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(param) = self.consume_ident("Expected generic parameter name") {
                    if self.match_token(&TokenType::Colon) {
                        loop {
                            if let Some(constraint) =
                                self.consume_ident("Expected trait constraint name")
                            {
                                generic_constraints.push((param.clone(), constraint));
                            }
                            if !self.match_token(&TokenType::Plus) {
                                break;
                            }
                        }
                    }
                    generic_params.push(param);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(&TokenType::Greater, "Expected '>' after generic parameters")?;
        }

        self.consume(&TokenType::LParen, "Expected '(' after function name")?;
        let params = self.parse_param_list()?;
        self.consume(&TokenType::RParen, "Expected ')' after parameters")?;

        let mut return_type = None;
        if self.match_token(&TokenType::Arrow) {
            return_type = self.parse_type();
        }

        let (requires, ensures) = self.parse_contracts();
        let mut decreases = None;
        if self.match_ident_str("decreases") {
            decreases = Some(self.parse_expression()?);
        }

        let mut is_expression_body = false;
        let body = if self.match_token(&TokenType::FatArrow) {
            let expr = self.parse_expression()?;
            let span = expr.span().clone();
            is_expression_body = true;
            Box::new(Stmt::Expr(expr, span))
        } else {
            Box::new(self.parse_block()?)
        };

        Some(FunctionDecl {
            name,
            attributes,
            generic_params,
            generic_constraints,
            params,
            return_type,
            requires,
            ensures,
            decreases,
            body,
            is_expression_body,
            is_export,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_packet_decl(&mut self) -> Option<PacketDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected packet name")?;
        self.consume(&TokenType::LBrace, "Expected '{' after packet name")?;
        let mut fields = Vec::new();
        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let field_span = self.peek().span.clone();
            let fname = self.consume_ident("Expected field name in packet")?;
            self.match_token(&TokenType::Colon);
            let bits = if self.negative_literal_follows() {
                self.error("Packet field bit count must be non-negative");
                self.advance();
                self.advance();
                1
            } else if let TokenType::IntLiteral(val) = self.peek().token_type {
                self.advance();
                if val < 0 {
                    self.error("Packet field bit count must be non-negative");
                    1
                } else {
                    val as usize
                }
            } else {
                self.error("Expected bit count for packet field");
                1
            };
            fields.push(PacketField {
                name: fname,
                bits,
                span: SourceSpan::new(
                    field_span.start_line,
                    field_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }
        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after packet fields")?;
        Some(PacketDecl {
            name,
            fields,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_extern_fn_decl(&mut self) -> Option<ExternFnDecl> {
        let start_span = self.previous().span.clone();
        let abi = if let TokenType::StringLiteral(ref s) = self.peek().token_type {
            let s = s.clone();
            self.advance();
            s
        } else {
            "C".to_string()
        };
        self.consume(&TokenType::Fn, "Expected 'fn' in extern declaration")?;
        let name = self.consume_ident("Expected function name in extern declaration")?;
        self.consume(&TokenType::LParen, "Expected '(' in extern declaration")?;
        let params = self.parse_param_list()?;
        self.consume(&TokenType::RParen, "Expected ')' after extern params")?;
        let mut return_type = None;
        if self.match_token(&TokenType::Arrow) {
            return_type = self.parse_type();
        }
        Some(ExternFnDecl {
            abi,
            name,
            params,
            return_type,
            span: SourceSpan::new(
                start_line_from(&start_span),
                start_col_from(&start_span),
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_param_list(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        if !self.check(&TokenType::RParen) {
            loop {
                let mut ownership_mode = "owned".to_string();
                if self.match_token(&TokenType::View) {
                    ownership_mode = "view".to_string();
                } else if self.match_token(&TokenType::MutView) {
                    ownership_mode = "mut-view".to_string();
                } else if self.match_token(&TokenType::Shared) {
                    ownership_mode = "shared".to_string();
                } else if self.match_token(&TokenType::Own) {
                    ownership_mode = "own".to_string();
                }

                let is_ampersand = self.match_token(&TokenType::Ampersand);
                let is_amp_mut = is_ampersand && self.match_token(&TokenType::Mut);
                if is_ampersand {
                    ownership_mode = if is_amp_mut {
                        "mut-view".to_string()
                    } else {
                        "view".to_string()
                    };
                }

                let name = self.consume_ident("Expected parameter name")?;
                let type_node = if (is_ampersand
                    || name == "self"
                    || name == "&self"
                    || name == "mut self"
                    || name == "this")
                    && !self.check(&TokenType::Colon)
                {
                    Some(TypeNode {
                        name: "Self".to_string(),
                        generic_args: Vec::new(),
                        is_option: false,
                        error_type: None,
                        refinement: None,
                        span: self.previous().span.clone(),
                    })
                } else {
                    self.consume(&TokenType::Colon, "Expected ':' after parameter name")?;
                    self.parse_type()
                };

                params.push(Param {
                    name,
                    type_node,
                    ownership_mode,
                    span: self.previous().span.clone(),
                });

                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }
        Some(params)
    }

    fn parse_type(&mut self) -> Option<TypeNode> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.error_depth_limit("Type");
            self.depth -= 1;
            return None;
        }
        let result = self.parse_type_inner();
        self.depth -= 1;
        result
    }

    fn parse_type_inner(&mut self) -> Option<TypeNode> {
        let start_span = self.peek().span.clone();
        let name = self.consume_ident("Expected type name")?;

        let mut generic_args = Vec::new();
        let mut refinement = None;
        if self.match_token(&TokenType::Less) {
            if matches!(self.peek().token_type, TokenType::IntLiteral(_))
                && self.current + 1 < self.tokens.len()
                && matches!(
                    self.tokens[self.current + 1].token_type,
                    TokenType::DotDot | TokenType::DotDotEq
                )
            {
                if let Some(range_expr) = self.parse_range()
                    && let Expr::Range {
                        start,
                        end,
                        inclusive,
                        ..
                    } = range_expr
                {
                    refinement = Some(Refinement::Range {
                        start,
                        end,
                        inclusive,
                    });
                }
            } else {
                while !self.check(&TokenType::Greater) && !self.is_at_end() {
                    if let TokenType::Identifier(ref id) = self.peek().token_type {
                        // A generic argument starting with an identifier is a
                        // nested type when followed by `<` (e.g. `Vec<Vec<Int>>`);
                        // otherwise it is a unit name (possibly composite, like
                        // `m/s` or `N*m`).
                        let next_is_less = matches!(
                            self.tokens.get(self.current + 1).map(|t| &t.token_type),
                            Some(TokenType::Less)
                        );
                        if next_is_less {
                            if let Some(arg) = self.parse_type() {
                                generic_args.push(arg);
                            }
                        } else {
                            let mut unit_name = id.clone();
                            let unit_span = self.peek().span.clone();
                            self.advance();
                            while (self.check(&TokenType::Slash) || self.check(&TokenType::Star))
                                && !self.is_at_end()
                            {
                                let op_tok = self.advance();
                                let op_str = if matches!(op_tok.token_type, TokenType::Slash) {
                                    "/"
                                } else {
                                    "*"
                                };
                                if let TokenType::Identifier(ref next_id) = self.peek().token_type {
                                    unit_name.push_str(op_str);
                                    unit_name.push_str(next_id);
                                    self.advance();
                                }
                            }
                            generic_args.push(TypeNode {
                                name: unit_name,
                                generic_args: Vec::new(),
                                is_option: false,
                                error_type: None,
                                refinement: None,
                                span: unit_span,
                            });
                        }
                    } else if let Some(arg) = self.parse_type() {
                        generic_args.push(arg);
                    }
                    if !self.match_token(&TokenType::Comma) {
                        break;
                    }
                }
            }
            self.consume(
                &TokenType::Greater,
                "Expected '>' after generic type arguments",
            )?;
        }

        let is_option = self.match_token(&TokenType::Question);

        let mut error_type = None;
        if self.match_token(&TokenType::Bang) {
            error_type = self.parse_type().map(Box::new);
        }

        if refinement.is_none() && self.check(&TokenType::In) {
            let is_bitfield = self.current + 1 < self.tokens.len()
                && matches!(
                    self.tokens[self.current + 1].token_type,
                    TokenType::Bit | TokenType::Bits
                );
            if !is_bitfield {
                self.advance();
                let range_expr = self.parse_range()?;
                if let Expr::Range {
                    start,
                    end,
                    inclusive,
                    ..
                } = range_expr
                {
                    refinement = Some(Refinement::Range {
                        start,
                        end,
                        inclusive,
                    });
                } else {
                    self.error("Expected range after 'in' in type refinement");
                }
            }
        } else if self.match_token(&TokenType::Where) {
            let predicate_expr = self.parse_expression()?;
            let var_name =
                Self::find_first_ident(&predicate_expr).unwrap_or_else(|| "val".to_string());
            refinement = Some(Refinement::Predicate {
                var_name,
                predicate: Box::new(predicate_expr),
            });
        }

        Some(TypeNode {
            name,
            generic_args,
            is_option,
            error_type,
            refinement,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
    }

    fn parse_contracts(&mut self) -> (Vec<ContractClause>, Vec<ContractClause>) {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        loop {
            if self.match_token(&TokenType::Require) {
                let start_span = self.previous().span.clone();
                if let Some(cond) = self.parse_expression() {
                    let message = if self.match_token(&TokenType::Comma) {
                        if let TokenType::StringLiteral(s) = &self.peek().token_type {
                            let msg = s.clone();
                            self.advance();
                            Some(msg)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    self.match_token(&TokenType::Semicolon);
                    let span = SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        self.previous().span.end_line,
                        self.previous().span.end_col,
                        self.file.clone(),
                    );
                    requires.push(ContractClause {
                        condition: cond,
                        message,
                        span,
                    });
                }
            } else if self.match_token(&TokenType::Ensure) {
                let start_span = self.previous().span.clone();
                if let Some(cond) = self.parse_expression() {
                    let message = if self.match_token(&TokenType::Comma) {
                        if let TokenType::StringLiteral(s) = &self.peek().token_type {
                            let msg = s.clone();
                            self.advance();
                            Some(msg)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    self.match_token(&TokenType::Semicolon);
                    let span = SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        self.previous().span.end_line,
                        self.previous().span.end_col,
                        self.file.clone(),
                    );
                    ensures.push(ContractClause {
                        condition: cond,
                        message,
                        span,
                    });
                }
            } else {
                break;
            }
        }
        (requires, ensures)
    }

    fn find_first_ident(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(name, _) => Some(name.clone()),
            Expr::Binary { left, right, .. } => {
                Self::find_first_ident(left).or_else(|| Self::find_first_ident(right))
            }
            Expr::Unary { expr, .. } => Self::find_first_ident(expr),
            Expr::MemberAccess { object, .. } => Self::find_first_ident(object),
            Expr::IndexAccess { object, index, .. } => {
                Self::find_first_ident(object).or_else(|| Self::find_first_ident(index))
            }
            _ => None,
        }
    }

    fn parse_block(&mut self) -> Option<Stmt> {
        let start_token = self.consume(&TokenType::LBrace, "Expected '{'")?;
        let mut statements = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Semicolon) {
                continue;
            }
            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
                while self.match_token(&TokenType::Semicolon) {}
            } else {
                self.synchronize();
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}'")?;
        Some(Stmt::Block(
            statements,
            SourceSpan::new(
                start_token.span.start_line,
                start_token.span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        ))
    }

    /// Decides whether a `{`-delimited arm body (match/decide/select) is a
    /// statement block (`{ let y = 2; y }`) rather than a map literal. Scans
    /// ahead without consuming: a map literal must contain a top-level `:`,
    /// while a block body either starts with a statement keyword, contains a
    /// top-level `;`, or simply has no top-level `:`. An empty `{}` is treated
    /// as an empty block (Unit); a map literal needs at least a key.
    fn arm_body_is_block(&self) -> bool {
        if !self.check(&TokenType::LBrace) {
            return false;
        }
        let body_start = self.current + 1;
        let mut depth = 0usize;
        let mut has_top_level_colon = false;
        let mut i = body_start;
        while i < self.tokens.len() {
            match &self.tokens[i].token_type {
                TokenType::LBrace => depth += 1,
                TokenType::RBrace => {
                    if depth == 0 {
                        if i == body_start {
                            // Empty braces: block (Unit).
                            return true;
                        }
                        break;
                    }
                    depth -= 1;
                }
                TokenType::Semicolon if depth == 0 => return true,
                TokenType::Colon if depth == 0 => has_top_level_colon = true,
                t if depth == 0
                    && i == body_start
                    && matches!(
                        t,
                        TokenType::Let
                            | TokenType::Mut
                            | TokenType::Val
                            | TokenType::Const
                            | TokenType::If
                            | TokenType::For
                            | TokenType::While
                            | TokenType::Loop
                            | TokenType::Return
                            | TokenType::Out
                            | TokenType::Err
                            | TokenType::Match
                            | TokenType::Decide
                            | TokenType::Select
                            | TokenType::Unsafe
                            | TokenType::Try
                            | TokenType::Parallel
                    ) =>
                {
                    return true;
                }
                _ => {}
            }
            i += 1;
        }
        // No top-level `:` anywhere: it cannot be a map literal, so treat it
        // as a block (e.g. `{ 1 }`, `{ x + 1 }`).
        !has_top_level_colon
    }

    /// Parses a `{ ... }` arm body as a statement block: statements run in
    /// order and the trailing expression (if any) becomes the block's value.
    fn parse_arm_block(&mut self) -> Option<Expr> {
        let start_span = self.peek().span.clone();
        self.consume(&TokenType::LBrace, "Expected '{'")?;
        let mut stmts: Vec<Stmt> = Vec::new();
        let mut value: Option<Box<Expr>> = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Semicolon) {
                continue;
            }
            match self.parse_statement() {
                Some(Stmt::Expr(e, sp)) => {
                    while self.match_token(&TokenType::Semicolon) {}
                    if self.check(&TokenType::RBrace) || self.is_at_end() {
                        value = Some(Box::new(e));
                    } else {
                        stmts.push(Stmt::Expr(e, sp));
                    }
                }
                Some(s) => {
                    stmts.push(s);
                    while self.match_token(&TokenType::Semicolon) {}
                }
                None => self.synchronize(),
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}'")?;
        Some(Expr::Block(
            stmts,
            value,
            SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        ))
    }

    /// Arm body for match/decide/select: a `{`-shaped body that is a statement
    /// block parses as `Expr::Block`; anything else stays a plain expression
    /// (including map literals).
    fn parse_arm_body(&mut self) -> Option<Expr> {
        if self.arm_body_is_block() {
            self.parse_arm_block()
        } else {
            self.parse_expression()
        }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.error_depth_limit("Statement");
            self.depth -= 1;
            return None;
        }
        let result = self.parse_statement_inner();
        self.depth -= 1;
        result
    }

    fn parse_statement_inner(&mut self) -> Option<Stmt> {
        let start_span = self.peek().span.clone();

        if self.match_token(&TokenType::Let) {
            let name = self.consume_ident("Expected variable name after 'let'")?;
            let mut type_node = None;
            if self.match_token(&TokenType::Colon) {
                type_node = self.parse_type();
            }
            self.consume(&TokenType::Equal, "Expected '=' in let binding")?;
            let init = self.parse_expression()?;
            return Some(Stmt::Let {
                name,
                type_node,
                init,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Mut) {
            let is_val = self.match_token(&TokenType::Val);
            let name = self.consume_ident("Expected variable name after 'mut'")?;
            let mut type_node = None;
            if self.match_token(&TokenType::Colon) {
                type_node = self.parse_type();
            }
            self.consume(&TokenType::Equal, "Expected '=' in mut binding")?;
            let init = self.parse_expression()?;
            let span = SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            );
            if is_val {
                return Some(Stmt::Val {
                    name,
                    type_node,
                    init,
                    is_mut: true,
                    span,
                });
            } else {
                return Some(Stmt::Mut {
                    name,
                    type_node,
                    init,
                    span,
                });
            }
        }

        if self.match_token(&TokenType::Val) {
            let name = self.consume_ident("Expected variable name after 'val'")?;
            let mut type_node = None;
            if self.match_token(&TokenType::Colon) {
                type_node = self.parse_type();
            }
            self.consume(&TokenType::Equal, "Expected '=' in val binding")?;
            let init = self.parse_expression()?;
            return Some(Stmt::Val {
                name,
                type_node,
                init,
                is_mut: false,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Out) {
            let expr = self.parse_expression()?;
            return Some(Stmt::Out(
                expr,
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            ));
        }

        if self.match_token(&TokenType::Err) {
            let expr = self.parse_expression()?;
            return Some(Stmt::Err(
                expr,
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            ));
        }

        if self.match_token(&TokenType::Require) {
            let cond = self.parse_expression()?;
            if self.match_token(&TokenType::Comma)
                && let TokenType::StringLiteral(_) = &self.peek().token_type
            {
                self.advance();
            }
            self.match_token(&TokenType::Semicolon);
            let span = SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            );
            return Some(Stmt::Expr(
                Expr::Call {
                    callee: Box::new(Expr::Identifier("require".into(), span.clone())),
                    args: vec![cond],
                    span: span.clone(),
                },
                span,
            ));
        }

        if self.match_token(&TokenType::Return) {
            // A bare `return` is terminated by the end of the statement
            // (`;`), the enclosing block (`}`) or EOF — not just `}`.
            let expr = if !self.check(&TokenType::RBrace)
                && !self.check(&TokenType::Semicolon)
                && !self.is_at_end()
            {
                self.parse_expression()
            } else {
                None
            };
            return Some(Stmt::Return(
                expr,
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            ));
        }

        if self.match_token(&TokenType::If) {
            let condition = self.parse_condition()?;
            let then_branch = Box::new(self.parse_block()?);
            let mut else_branch = None;
            if self.match_token(&TokenType::Else) {
                if self.check(&TokenType::If) {
                    else_branch = self.parse_statement().map(Box::new);
                } else {
                    else_branch = self.parse_block().map(Box::new);
                }
            }
            return Some(Stmt::If {
                condition,
                then_branch,
                else_branch,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Const) {
            let name = self.consume_ident("Expected constant name after 'const'")?;
            let mut type_node = None;
            if self.match_token(&TokenType::Colon) {
                type_node = self.parse_type();
            }
            self.consume(&TokenType::Equal, "Expected '=' in const binding")?;
            let init = self.parse_expression()?;
            return Some(Stmt::Const {
                name,
                type_node,
                init,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Try) {
            self.error("SyntaxError: 'try/catch' has been removed. Use Result propagation '?' and railway recovery 'or { ... }' instead.");
            return None;
        }

        if self.match_token(&TokenType::Parallel) {
            if self.match_token(&TokenType::For) {
                let var_name = self.consume_ident("Expected variable name after 'parallel for'")?;
                self.consume(&TokenType::In, "Expected 'in' after variable name")?;
                let iterable = self.parse_expression()?;
                let body = Box::new(self.parse_block()?);
                return Some(Stmt::ParallelFor {
                    var_name,
                    iterable,
                    body,
                    span: SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        self.previous().span.end_line,
                        self.previous().span.end_col,
                        self.file.clone(),
                    ),
                });
            } else {
                let body = Box::new(self.parse_block()?);
                return Some(Stmt::Parallel(
                    body,
                    SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        self.previous().span.end_line,
                        self.previous().span.end_col,
                        self.file.clone(),
                    ),
                ));
            }
        }

        if self.match_token(&TokenType::For) {
            let var_name = self.consume_ident("Expected variable name after 'for'")?;
            self.consume(&TokenType::In, "Expected 'in' after variable name")?;
            let iterable = self.parse_expression()?;
            let body = Box::new(self.parse_block()?);
            return Some(Stmt::For {
                var_name,
                iterable,
                body,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Loop) {
            let body = Box::new(self.parse_block()?);
            return Some(Stmt::Loop {
                body,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::While) {
            let condition = self.parse_condition()?;
            let body = Box::new(self.parse_block()?);
            return Some(Stmt::While {
                condition,
                body,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::With) {
            let resource_name =
                self.consume_ident_or_keyword("Expected resource variable name after 'with'")?;
            self.consume(
                &TokenType::Equal,
                "Expected '=' after resource name in 'with'",
            )?;
            let init = self.parse_expression()?;
            let body = Box::new(self.parse_block()?);
            return Some(Stmt::With {
                resource_name,
                init,
                body,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Unsafe) {
            let mut justification = None;
            if self.match_token(&TokenType::LParen) {
                if let TokenType::Identifier(id) = &self.peek().token_type
                    && id == "justification"
                {
                    self.advance();
                    self.consume(&TokenType::Colon, "Expected ':' after 'justification'")?;
                }
                if let TokenType::StringLiteral(s) = &self.peek().token_type {
                    justification = Some(s.clone());
                    self.advance();
                } else {
                    self.error("Expected string literal justification in 'unsafe(...)'");
                    return None;
                }
                self.consume(
                    &TokenType::RParen,
                    "Expected ')' after unsafe justification",
                )?;
            }
            let body = Box::new(self.parse_block()?);
            return Some(Stmt::Unsafe {
                justification,
                body,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        if self.match_token(&TokenType::Asm) {
            self.consume(&TokenType::LBrace, "Expected '{' after 'asm!'")?;
            let mut instructions = Vec::new();
            let mut options = Vec::new();
            while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                if let TokenType::StringLiteral(s) = &self.peek().token_type {
                    instructions.push(s.clone());
                    self.advance();
                    self.match_token(&TokenType::Comma);
                } else if let TokenType::Identifier(id) = &self.peek().token_type {
                    if id == "options" {
                        self.advance();
                        self.consume(&TokenType::Colon, "Expected ':' after 'options'")?;
                        self.consume(&TokenType::LBracket, "Expected '[' after 'options:'")?;
                        while !self.check(&TokenType::RBracket) && !self.is_at_end() {
                            if let Some(opt) = self.consume_ident_or_keyword("Expected option name")
                            {
                                options.push(opt);
                            }
                            if !self.match_token(&TokenType::Comma) {
                                break;
                            }
                        }
                        self.consume(&TokenType::RBracket, "Expected ']' after options list")?;
                        self.match_token(&TokenType::Comma);
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            self.consume(&TokenType::RBrace, "Expected '}' to close 'asm!' block")?;
            return Some(Stmt::Asm {
                instructions,
                options,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        // Compact binding `x := 10` or assignment `x = 10` or Expression statement
        let expr = self.parse_expression()?;

        if self.match_token(&TokenType::ColonEqual) {
            self.error("SyntaxError: Operator ':=' is deprecated. Use 'let' for immutable or 'mut' for mutable variables.");
            return None;
        } else if self.match_token(&TokenType::Equal) {
            let value = self.parse_expression()?;
            return Some(Stmt::Assign {
                target: expr,
                value,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            });
        }

        let span = expr.span().clone();
        Some(Stmt::Expr(expr, span))
    }

    pub fn parse_expression(&mut self) -> Option<Expr> {
        if self.depth > MAX_PARSE_DEPTH {
            self.error_depth_limit("Expression");
            return None;
        }
        self.depth += 1;
        let result = self.parse_pipeline();
        self.depth -= 1;
        result
    }

    /// Parse an expression in "condition / scrutinee" position (`if`, `while`,
    /// `match`). In this position a capitalized identifier followed by `{` is
    /// ambiguous between a struct literal (`if User { name: "a" }.is_admin { }`)
    /// and a plain variable followed by the statement's block body
    /// (`if Flag { out 1 }`). We only treat `Ident {` as a struct literal when
    /// the brace content provably looks like field-initializer syntax.
    fn parse_condition(&mut self) -> Option<Expr> {
        self.no_struct_literal_at_top = true;
        let expr = self.parse_expression();
        self.no_struct_literal_at_top = false;
        expr
    }

    /// Lookahead: assuming `self.current` points at `{`, does the brace content
    /// look like struct-literal field-initializer syntax (`{}` or `{ Ident :`)?
    fn struct_literal_ahead(&self) -> bool {
        match self.tokens.get(self.current + 1).map(|t| &t.token_type) {
            Some(TokenType::RBrace) => true,
            Some(TokenType::Identifier(_)) => matches!(
                self.tokens.get(self.current + 2).map(|t| &t.token_type),
                Some(TokenType::Colon)
            ),
            _ => false,
        }
    }

    fn parse_pipeline(&mut self) -> Option<Expr> {
        let mut expr = self.parse_logical_or()?;

        if self.check(&TokenType::Pipe) || self.check(&TokenType::Then) {
            expr = self.parse_pipeline_tail(expr)?;
        }

        if self.check(&TokenType::OrKeyword) {
            expr = self.parse_or_recovery_tail(expr)?;
        }

        Some(expr)
    }

    #[inline(never)]
    fn parse_pipeline_tail(&mut self, expr: Expr) -> Option<Expr> {
        let mut stages = vec![expr];
        loop {
            let is_pipe = self.match_token(&TokenType::Pipe);
            let is_then = !is_pipe && self.match_token(&TokenType::Then);
            if !is_pipe && !is_then {
                break;
            }
            // Optional `flow` keyword: `data |> flow Name` or `data |> Name`
            self.match_token(&TokenType::Flow);
            let stage = self.parse_logical_or()?;
            stages.push(stage);
        }
        let start = stages[0].span().clone();
        let end = stages.last().unwrap().span().clone();
        Some(Expr::Pipeline {
            stages,
            span: SourceSpan::new(
                start.start_line,
                start.start_col,
                end.end_line,
                end.end_col,
                self.file.clone(),
            ),
        })
    }

    #[inline(never)]
    fn parse_or_recovery_tail(&mut self, mut result_expr: Expr) -> Option<Expr> {
        self.advance(); // consume TokenType::OrKeyword
        let start_span = result_expr.span().clone();
        if self.match_token(&TokenType::LBrace) {
            let mut arms = Vec::new();
            while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                let pattern = self.parse_pattern()?;
                let mut guard = None;
                if self.match_token(&TokenType::If) || self.match_token(&TokenType::When) {
                    guard = self.parse_expression();
                }
                self.consume(&TokenType::FatArrow, "Expected '=>' in or arm")?;
                let body = self.parse_expression()?;
                let span = SourceSpan::new(
                    pattern.span().start_line,
                    pattern.span().start_col,
                    body.span().end_line,
                    body.span().end_col,
                    self.file.clone(),
                );
                arms.push(MatchArm {
                    pattern,
                    guard,
                    body,
                    span,
                });
                self.match_token(&TokenType::Comma);
            }
            let end_token = self.consume(&TokenType::RBrace, "Expected '}' after or block")?;
            result_expr = Expr::OrRecovery {
                expr: Box::new(result_expr),
                arms,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                ),
            };
        } else {
            let default_body = self.parse_expression()?;
            let wildcard_pat = Pattern::Wildcard(default_body.span().clone());
            let arm_span = default_body.span().clone();
            let arms = vec![MatchArm {
                pattern: wildcard_pat,
                guard: None,
                body: default_body,
                span: arm_span,
            }];
            result_expr = Expr::OrRecovery {
                expr: Box::new(result_expr),
                arms,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            };
        }
        Some(result_expr)
    }

    pub fn parse_logical_or(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_LOGICAL_OR)
    }

    pub fn parse_logical_and(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_LOGICAL_AND)
    }

    pub fn parse_equality(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_EQUALITY)
    }

    pub fn parse_comparison(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_COMPARISON)
    }

    pub fn parse_range(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_RANGE)
    }

    pub fn parse_term(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_ADDITIVE)
    }

    pub fn parse_factor(&mut self) -> Option<Expr> {
        self.parse_binary_climbing(PREC_MULTIPLICATIVE)
    }

    /// Parse binary expressions using precedence climbing with an explicit operator stack.
    /// Eliminates recursion on binary operators completely: all operator chaining
    /// happens in an iterative loop using an operator stack and value stack.
    /// Recursion is reserved strictly for parens, unary expressions, and primary expressions.
    fn parse_binary_climbing(&mut self, min_prec: u8) -> Option<Expr> {
        let first = self.parse_unary()?;
        if let Some((prec, _, _)) = self.peek_binary_op() {
            if prec < min_prec {
                return Some(first);
            }
        } else {
            return Some(first);
        }

        let mut val_stack: Vec<Expr> = vec![first];
        // Operator stack: (precedence, is_range, inclusive, op_str)
        let mut op_stack: Vec<(u8, bool, bool, &'static str)> = Vec::new();

        while let Some((prec, is_range, op_str)) = self.peek_binary_op() {
            if prec < min_prec {
                break;
            }

            let op_tok = self.advance();
            let inclusive = matches!(op_tok.token_type, TokenType::DotDotEq);

            // While operator at top of stack has >= precedence, reduce it.
            // All standard binary operators in Datara are left-associative, so `>=` applies.
            while let Some(&(top_prec, top_is_range, top_inclusive, top_op)) = op_stack.last() {
                if top_prec >= prec {
                    op_stack.pop();
                    let right = val_stack.pop()?;
                    let left = val_stack.pop()?;
                    let span = SourceSpan::new(
                        left.span().start_line,
                        left.span().start_col,
                        right.span().end_line,
                        right.span().end_col,
                        self.file.clone(),
                    );
                    let combined = if top_is_range {
                        Expr::Range {
                            start: Box::new(left),
                            end: Box::new(right),
                            inclusive: top_inclusive,
                            span,
                        }
                    } else {
                        Expr::Binary {
                            op: top_op.to_string(),
                            left: Box::new(left),
                            right: Box::new(right),
                            span,
                        }
                    };
                    val_stack.push(combined);
                } else {
                    break;
                }
            }

            op_stack.push((prec, is_range, inclusive, op_str));
            let next_val = self.parse_unary()?;
            val_stack.push(next_val);

            if is_range {
                // Range operators in Datara are non-associative, so we stop chaining ranges
                break;
            }
        }

        // Reduce remaining operators down to min_prec
        while let Some(&(top_prec, top_is_range, top_inclusive, top_op)) = op_stack.last() {
            if top_prec >= min_prec {
                op_stack.pop();
                let right = val_stack.pop()?;
                let left = val_stack.pop()?;
                let span = SourceSpan::new(
                    left.span().start_line,
                    left.span().start_col,
                    right.span().end_line,
                    right.span().end_col,
                    self.file.clone(),
                );
                let combined = if top_is_range {
                    Expr::Range {
                        start: Box::new(left),
                        end: Box::new(right),
                        inclusive: top_inclusive,
                        span,
                    }
                } else {
                    Expr::Binary {
                        op: top_op.to_string(),
                        left: Box::new(left),
                        right: Box::new(right),
                        span,
                    }
                };
                val_stack.push(combined);
            } else {
                break;
            }
        }

        val_stack.pop()
    }

    #[inline(always)]
    fn peek_binary_op(&self) -> Option<(u8, bool, &'static str)> {
        match &self.peek().token_type {
            TokenType::Or => Some((PREC_LOGICAL_OR, false, "||")),
            TokenType::And => Some((PREC_LOGICAL_AND, false, "&&")),
            TokenType::EqualEqual => Some((PREC_EQUALITY, false, "==")),
            TokenType::NotEqual => Some((PREC_EQUALITY, false, "!=")),
            TokenType::Less => Some((PREC_COMPARISON, false, "<")),
            TokenType::LessEqual => Some((PREC_COMPARISON, false, "<=")),
            TokenType::Greater => Some((PREC_COMPARISON, false, ">")),
            TokenType::GreaterEqual => Some((PREC_COMPARISON, false, ">=")),
            TokenType::DotDot => Some((PREC_RANGE, true, "..")),
            TokenType::DotDotEq => Some((PREC_RANGE, true, "..=")),
            TokenType::DotDotLt => Some((PREC_RANGE, true, "..<")),
            TokenType::Plus => Some((PREC_ADDITIVE, false, "+")),
            TokenType::Minus => Some((PREC_ADDITIVE, false, "-")),
            TokenType::Star => Some((PREC_MULTIPLICATIVE, false, "*")),
            TokenType::Slash => Some((PREC_MULTIPLICATIVE, false, "/")),
            TokenType::Percent => Some((PREC_MULTIPLICATIVE, false, "%")),
            _ => None,
        }
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.match_token(&TokenType::Bang)
            || self.match_token(&TokenType::Minus)
            || self.match_token(&TokenType::Await)
        {
            let op = self.previous().lexeme.clone();
            let op_span = self.previous().span.clone();
            let expr = self.parse_unary()?;
            let span = SourceSpan::new(
                op_span.start_line,
                op_span.start_col,
                expr.span().end_line,
                expr.span().end_col,
                self.file.clone(),
            );
            return Some(Expr::Unary {
                op,
                expr: Box::new(expr),
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&TokenType::LParen) {
                // Call
                let mut args = Vec::new();
                if !self.check(&TokenType::RParen) {
                    loop {
                        args.push(self.parse_expression()?);
                        if !self.match_token(&TokenType::Comma) {
                            break;
                        }
                    }
                }
                let end_token = self.consume(&TokenType::RParen, "Expected ')' after call args")?;
                let span = SourceSpan::new(
                    expr.span().start_line,
                    expr.span().start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                );
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.match_token(&TokenType::Dot) {
                // Member access
                let member = self.consume_ident_or_keyword("Expected member name after '.'")?;
                let span = SourceSpan::new(
                    expr.span().start_line,
                    expr.span().start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                );
                expr = Expr::MemberAccess {
                    object: Box::new(expr),
                    member,
                    span,
                };
            } else if self.match_token(&TokenType::LBracket) {
                // Index access `expr[index]`
                let index = self.parse_expression()?;
                let end_token = self.consume(&TokenType::RBracket, "Expected ']' after index")?;
                let span = SourceSpan::new(
                    expr.span().start_line,
                    expr.span().start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                );
                expr = Expr::IndexAccess {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else if self.match_token(&TokenType::Bang) || self.match_token(&TokenType::Question) {
                // Error propagation `!` or `?`
                let span = SourceSpan::new(
                    expr.span().start_line,
                    expr.span().start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                );
                expr = Expr::ErrorPropagate(Box::new(expr), span);
            } else {
                break;
            }
        }

        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.advance();
        match token.token_type {
            TokenType::IntLiteral(val) => Some(Expr::Literal(LiteralValue::Int(val), token.span)),
            TokenType::FloatLiteral(val) => {
                Some(Expr::Literal(LiteralValue::Float(val), token.span))
            }
            TokenType::StringLiteral(val) => {
                Some(Expr::Literal(LiteralValue::String(val), token.span))
            }
            TokenType::True => Some(Expr::Literal(LiteralValue::Bool(true), token.span)),
            TokenType::False => Some(Expr::Literal(LiteralValue::Bool(false), token.span)),
            TokenType::CharLiteral(val) => Some(Expr::Literal(LiteralValue::Char(val), token.span)),
            TokenType::None => Some(Expr::Literal(LiteralValue::None, token.span)),
            TokenType::App => Some(Expr::Identifier("app".to_string(), token.span)),
            TokenType::Role => Some(Expr::Identifier("role".to_string(), token.span)),
            TokenType::Flow => Some(Expr::Identifier("flow".to_string(), token.span)),
            TokenType::Task => Some(Expr::Identifier("task".to_string(), token.span)),
            TokenType::Cli => Some(Expr::Identifier("cli".to_string(), token.span)),
            TokenType::Command => Some(Expr::Identifier("command".to_string(), token.span)),
            TokenType::Val => Some(Expr::Identifier("val".to_string(), token.span)),
            TokenType::Using => Some(Expr::Identifier("using".to_string(), token.span)),
            TokenType::Packet => Some(Expr::Identifier("packet".to_string(), token.span)),
            TokenType::Comptime => {
                let start_span = token.span.clone();
                let inner = if self.match_token(&TokenType::LBrace) {
                    let expr = self.parse_expression()?;
                    let _ = self.consume(&TokenType::RBrace, "Expected '}' after comptime block");
                    expr
                } else {
                    self.parse_expression()?
                };
                let span = SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    inner.span().end_line,
                    inner.span().end_col,
                    self.file.clone(),
                );
                Some(Expr::Comptime {
                    expr: Box::new(inner),
                    span,
                })
            }

            TokenType::Wrapping => {
                let start_span = token.span.clone();
                if self.match_token(&TokenType::LParen) {
                    let inner = self.parse_expression()?;
                    let end_tok =
                        self.consume(&TokenType::RParen, "Expected ')' after wrapping expression")?;
                    let span = SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        end_tok.span.end_line,
                        end_tok.span.end_col,
                        self.file.clone(),
                    );
                    Some(Expr::Wrapping(Box::new(inner), span))
                } else {
                    Some(Expr::Identifier("wrapping".into(), start_span))
                }
            }

            TokenType::Saturating => {
                let start_span = token.span.clone();
                if self.match_token(&TokenType::LParen) {
                    let inner = self.parse_expression()?;
                    let end_tok = self.consume(
                        &TokenType::RParen,
                        "Expected ')' after saturating expression",
                    )?;
                    let span = SourceSpan::new(
                        start_span.start_line,
                        start_span.start_col,
                        end_tok.span.end_line,
                        end_tok.span.end_col,
                        self.file.clone(),
                    );
                    Some(Expr::Saturating(Box::new(inner), span))
                } else {
                    Some(Expr::Identifier("saturating".into(), start_span))
                }
            }

            TokenType::InterpolatedString(ref raw) => {
                let parsed = self.parse_interpolated_string_content(raw, &token.span);
                Some(parsed)
            }

            TokenType::Identifier(ref name) => {
                let name = name.clone();
                let span = token.span;
                self.parse_ident_or_object_init(span, name)
            }

            TokenType::View => Some(Expr::Identifier("view".into(), token.span)),
            TokenType::Mut => Some(Expr::Identifier("mut".into(), token.span)),
            TokenType::Err => Some(Expr::Identifier("err".into(), token.span)),
            TokenType::Out => Some(Expr::Identifier("out".into(), token.span)),

            TokenType::Decide => self.parse_decide_expr(token.span),
            TokenType::Match => self.parse_match_expr(token.span),
            TokenType::Select => self.parse_select_expr(token.span),
            TokenType::LBrace => self.parse_map_literal_expr(token.span),
            TokenType::LBracket => self.parse_bracket_expr(token.span),
            TokenType::LParen => self.parse_paren_expr(token.span),

            _ => {
                self.error(&format!("Unexpected token: {:?}", token.token_type));
                None
            }
        }
    }

    #[inline(never)]
    fn parse_ident_or_object_init(&mut self, span: SourceSpan, name: String) -> Option<Expr> {
        // Check if Lambda `x => expr`
        if self.match_token(&TokenType::FatArrow) {
            let p_span = span.clone();
            let body = Box::new(self.parse_expression()?);
            let span = SourceSpan::new(
                p_span.start_line,
                p_span.start_col,
                body.span().end_line,
                body.span().end_col,
                self.file.clone(),
            );
            return Some(Expr::Lambda {
                params: vec![Param {
                    name: name.clone(),
                    type_node: None,
                    ownership_mode: "owned".into(),
                    span: p_span,
                }],
                body,
                span,
            });
        }

        // Check if ObjectInit `User { ... }` or `Box<Int> { ... }`
        let is_capital = name.chars().next().is_some_and(|c| c.is_uppercase());
        let mut generic_args = Vec::new();

        if is_capital && self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(arg) = self.parse_type() {
                    generic_args.push(arg);
                }
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            self.consume(
                &TokenType::Greater,
                "Expected '>' after generic type arguments",
            )?;
        }

        if is_capital && self.check(&TokenType::LBrace) {
            // In condition/scrutinee position (`if Flag { ... }`), only
            // treat `Ident {` as a struct literal when the brace content
            // provably looks like field-initializer syntax; otherwise
            // parse the identifier alone and leave `{` to be consumed as
            // the block body by the statement parser.
            let allow_struct = !self.no_struct_literal_at_top || self.struct_literal_ahead();
            if allow_struct {
                self.consume(&TokenType::LBrace, "Expected '{'")?;
                let mut fields = Vec::new();

                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    let field_name = self.consume_ident_or_keyword("Expected field name")?;
                    self.consume(&TokenType::Colon, "Expected ':' after field name")?;
                    let field_val = self.parse_expression()?;
                    fields.push((field_name, field_val));
                    self.match_token(&TokenType::Comma);
                }

                let end_token = self.consume(&TokenType::RBrace, "Expected '}'")?;
                return Some(Expr::ObjectInit {
                    class_name: name,
                    generic_args,
                    fields,
                    span: SourceSpan::new(
                        span.start_line,
                        span.start_col,
                        end_token.span.end_line,
                        end_token.span.end_col,
                        self.file.clone(),
                    ),
                });
            }
        }

        Some(Expr::Identifier(name, span))
    }

    #[inline(never)]
    fn parse_decide_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        self.consume(&TokenType::LBrace, "Expected '{' after decide")?;
        let mut arms = Vec::new();
        let mut else_arm = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Else) {
                self.consume(&TokenType::FatArrow, "Expected '=>' after else")?;
                else_arm = Some(Box::new(self.parse_arm_body()?));
                self.match_token(&TokenType::Comma);
            } else {
                let condition = self.parse_expression()?;
                self.consume(&TokenType::FatArrow, "Expected '=>' after decide condition")?;
                let body = self.parse_arm_body()?;
                let span = SourceSpan::new(
                    condition.span().start_line,
                    condition.span().start_col,
                    body.span().end_line,
                    body.span().end_col,
                    self.file.clone(),
                );
                arms.push(DecideArm {
                    condition,
                    body,
                    span,
                });
                self.match_token(&TokenType::Comma);
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after decide block")?;
        Some(Expr::Decide {
            arms,
            else_arm,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    #[inline(never)]
    fn parse_match_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        let value = Box::new(self.parse_condition()?);
        self.consume(&TokenType::LBrace, "Expected '{' after match expression")?;
        let mut arms = Vec::new();

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let pattern = self.parse_pattern()?;
            let mut guard = None;
            if self.match_token(&TokenType::If) || self.match_token(&TokenType::When) {
                guard = self.parse_expression();
            }
            self.consume(&TokenType::FatArrow, "Expected '=>' after match pattern")?;
            let body = self.parse_arm_body()?;
            let span = SourceSpan::new(
                pattern.span().start_line,
                pattern.span().start_col,
                body.span().end_line,
                body.span().end_col,
                self.file.clone(),
            );
            arms.push(MatchArm {
                pattern,
                guard,
                body,
                span,
            });
            self.match_token(&TokenType::Comma);
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after match arms")?;
        Some(Expr::Match {
            value,
            arms,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    #[inline(never)]
    fn parse_select_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        self.consume(&TokenType::LBrace, "Expected '{' after select")?;
        let mut arms = Vec::new();
        let mut else_arm = None;

        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Else) {
                self.consume(&TokenType::FatArrow, "Expected '=>' after else")?;
                else_arm = Some(Box::new(self.parse_arm_body()?));
                self.match_token(&TokenType::Comma);
            } else {
                let condition = self.parse_expression()?;
                self.consume(&TokenType::FatArrow, "Expected '=>' after select condition")?;
                let body = self.parse_arm_body()?;
                let span = SourceSpan::new(
                    condition.span().start_line,
                    condition.span().start_col,
                    body.span().end_line,
                    body.span().end_col,
                    self.file.clone(),
                );
                arms.push(SelectArm {
                    condition,
                    body,
                    span,
                });
                self.match_token(&TokenType::Comma);
            }
        }

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after select block")?;
        Some(Expr::Select {
            arms,
            else_arm,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        })
    }

    #[inline(never)]
    fn parse_map_literal_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        let mut entries = Vec::new();
        while !self.check(&TokenType::RBrace) && !self.is_at_end() {
            let key = self.parse_expression()?;
            self.consume(&TokenType::Colon, "Expected ':' in map entry")?;
            let val = self.parse_expression()?;
            entries.push((key, val));
            if !self.match_token(&TokenType::Comma) {
                break;
            }
        }
        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after map literal")?;
        Some(Expr::MapLiteral(
            entries,
            SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        ))
    }

    #[inline(never)]
    fn parse_bracket_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        if self.match_token(&TokenType::RBracket) {
            return Some(Expr::ListLiteral(
                Vec::new(),
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    self.previous().span.end_line,
                    self.previous().span.end_col,
                    self.file.clone(),
                ),
            ));
        }

        let first_expr = self.parse_expression()?;

        // Check for [elem; count] (ArrayRepeatLiteral)
        if self.match_token(&TokenType::Semicolon) {
            let count = if self.negative_literal_follows() {
                self.error("Array repeat count must be non-negative");
                self.advance();
                self.advance();
                0
            } else if let TokenType::IntLiteral(c) = self.peek().token_type {
                self.advance();
                if c < 0 {
                    self.error("Array repeat count must be non-negative");
                    0
                } else {
                    c as usize
                }
            } else {
                self.error("Expected integer count after ';' in array repeat literal");
                0
            };
            let end_token = self.consume(
                &TokenType::RBracket,
                "Expected ']' after array repeat literal",
            )?;
            return Some(Expr::ArrayRepeatLiteral {
                elem: Box::new(first_expr),
                count,
                span: SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                ),
            });
        }

        // Check for ["key": value, ...] (Map literal with brackets)
        if self.match_token(&TokenType::Colon) {
            let first_val = self.parse_expression()?;
            let mut entries = vec![(first_expr, first_val)];
            while self.match_token(&TokenType::Comma)
                && !self.check(&TokenType::RBracket)
                && !self.is_at_end()
            {
                let k = self.parse_expression()?;
                self.consume(&TokenType::Colon, "Expected ':' in map entry")?;
                let v = self.parse_expression()?;
                entries.push((k, v));
            }
            let end_token = self.consume(&TokenType::RBracket, "Expected ']' after map literal")?;
            return Some(Expr::MapLiteral(
                entries,
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                ),
            ));
        }

        // Regular list literal [1, 2, 3]
        let mut items = vec![first_expr];
        while self.match_token(&TokenType::Comma)
            && !self.check(&TokenType::RBracket)
            && !self.is_at_end()
        {
            items.push(self.parse_expression()?);
        }
        let end_token = self.consume(&TokenType::RBracket, "Expected ']' after list literal")?;
        Some(Expr::ListLiteral(
            items,
            SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                end_token.span.end_line,
                end_token.span.end_col,
                self.file.clone(),
            ),
        ))
    }

    #[inline(never)]
    fn parse_paren_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
        // Collect consecutive open parentheses to parse deeply nested expressions iteratively
        let mut extra_parens = 0;
        while self.match_token(&TokenType::LParen) {
            extra_parens += 1;
        }

        if self.depth + extra_parens > MAX_PARSE_DEPTH {
            self.error_depth_limit("Expression");
            return None;
        }
        self.depth += extra_parens;

        let result = self.parse_paren_expr_inner(start_span, extra_parens);
        self.depth -= extra_parens;
        result
    }

    fn parse_paren_expr_inner(
        &mut self,
        start_span: SourceSpan,
        mut remaining_parens: usize,
    ) -> Option<Expr> {
        if self.match_token(&TokenType::RParen) {
            if self.match_token(&TokenType::FatArrow) {
                let body = Box::new(self.parse_expression()?);
                let span = SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    body.span().end_line,
                    body.span().end_col,
                    self.file.clone(),
                );
                let expr = Expr::Lambda {
                    params: Vec::new(),
                    body,
                    span,
                };
                while remaining_parens > 0 {
                    self.consume(&TokenType::RParen, "Expected ')'")?;
                    remaining_parens -= 1;
                }
                return Some(expr);
            }
            let expr = Expr::Literal(LiteralValue::None, start_span);
            while remaining_parens > 0 {
                self.consume(&TokenType::RParen, "Expected ')'")?;
                remaining_parens -= 1;
            }
            return Some(expr);
        }

        let first = self.parse_expression()?;
        if self.match_token(&TokenType::Comma) {
            let mut exprs = vec![first];
            while !self.check(&TokenType::RParen) && !self.is_at_end() {
                exprs.push(self.parse_expression()?);
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
            let end_token = self.consume(&TokenType::RParen, "Expected ')'")?;
            if self.match_token(&TokenType::FatArrow) {
                let mut params = Vec::new();
                for e in &exprs {
                    match e {
                        Expr::Identifier(name, p_span) => params.push(Param {
                            name: name.clone(),
                            type_node: None,
                            ownership_mode: "owned".into(),
                            span: p_span.clone(),
                        }),
                        _ => {
                            self.error("Lambda parameters must be plain identifiers");
                        }
                    }
                }
                let body = Box::new(self.parse_expression()?);
                let span = SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    body.span().end_line,
                    body.span().end_col,
                    self.file.clone(),
                );
                let expr = Expr::Lambda { params, body, span };
                while remaining_parens > 0 {
                    self.consume(&TokenType::RParen, "Expected ')'")?;
                    remaining_parens -= 1;
                }
                return Some(expr);
            }
            let expr = Expr::Tuple(
                exprs,
                SourceSpan::new(
                    start_span.start_line,
                    start_span.start_col,
                    end_token.span.end_line,
                    end_token.span.end_col,
                    self.file.clone(),
                ),
            );
            while remaining_parens > 0 {
                self.consume(&TokenType::RParen, "Expected ')'")?;
                remaining_parens -= 1;
            }
            return Some(expr);
        }

        let _ = self.consume(&TokenType::RParen, "Expected ')'")?;
        if self.match_token(&TokenType::FatArrow) {
            let mut params = Vec::new();
            match &first {
                Expr::Identifier(name, p_span) => params.push(Param {
                    name: name.clone(),
                    type_node: None,
                    ownership_mode: "owned".into(),
                    span: p_span.clone(),
                }),
                _ => {
                    self.error("Lambda parameters must be plain identifiers");
                }
            }
            let body = Box::new(self.parse_expression()?);
            let span = SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                body.span().end_line,
                body.span().end_col,
                self.file.clone(),
            );
            let expr = Expr::Lambda { params, body, span };
            while remaining_parens > 0 {
                self.consume(&TokenType::RParen, "Expected ')'")?;
                remaining_parens -= 1;
            }
            return Some(expr);
        }

        let expr = first;
        while remaining_parens > 0 {
            self.consume(&TokenType::RParen, "Expected ')'")?;
            remaining_parens -= 1;
        }
        Some(expr)
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let token = self.advance();
        match token.token_type {
            TokenType::Identifier(ref name) if name == "_" => Some(Pattern::Wildcard(token.span)),
            TokenType::IntLiteral(val) => {
                Some(Pattern::Literal(LiteralValue::Int(val), token.span))
            }
            TokenType::FloatLiteral(val) => {
                Some(Pattern::Literal(LiteralValue::Float(val), token.span))
            }
            TokenType::StringLiteral(val) => {
                Some(Pattern::Literal(LiteralValue::String(val), token.span))
            }
            TokenType::True => Some(Pattern::Literal(LiteralValue::Bool(true), token.span)),
            TokenType::False => Some(Pattern::Literal(LiteralValue::Bool(false), token.span)),
            TokenType::CharLiteral(val) => {
                Some(Pattern::Literal(LiteralValue::Char(val), token.span))
            }
            TokenType::None => Some(Pattern::Literal(LiteralValue::None, token.span)),
            TokenType::Identifier(ref name) => {
                let is_capital = name.chars().next().is_some_and(|c| c.is_uppercase());
                if is_capital {
                    let mut enum_name = None;
                    let mut variant_name = name.clone();
                    if self.match_token(&TokenType::Dot) {
                        enum_name = Some(name.clone());
                        variant_name = self.consume_ident("Expected variant name after '.'")?;
                    }
                    let mut bindings = Vec::new();
                    if self.match_token(&TokenType::LParen) {
                        while !self.check(&TokenType::RParen) && !self.is_at_end() {
                            if let Some(binding) =
                                self.consume_ident("Expected binding variable name")
                            {
                                bindings.push(binding);
                            }
                            if !self.match_token(&TokenType::Comma) {
                                break;
                            }
                        }
                        self.consume(&TokenType::RParen, "Expected ')' after pattern bindings")?;
                    }
                    let span = SourceSpan::new(
                        token.span.start_line,
                        token.span.start_col,
                        self.previous().span.end_line,
                        self.previous().span.end_col,
                        self.file.clone(),
                    );
                    Some(Pattern::Variant {
                        enum_name,
                        variant_name,
                        bindings,
                        span,
                    })
                } else {
                    Some(Pattern::Identifier(name.clone(), token.span))
                }
            }
            TokenType::Minus => {
                // Negative literal patterns: `match x { -1 => ... }`.
                let next = self.peek().clone();
                match next.token_type {
                    TokenType::IntLiteral(val) => {
                        self.advance();
                        Some(Pattern::Literal(
                            LiteralValue::Int(val.wrapping_neg()),
                            SourceSpan::new(
                                token.span.start_line,
                                token.span.start_col,
                                next.span.end_line,
                                next.span.end_col,
                                self.file.clone(),
                            ),
                        ))
                    }
                    TokenType::FloatLiteral(val) => {
                        self.advance();
                        Some(Pattern::Literal(
                            LiteralValue::Float(-val),
                            SourceSpan::new(
                                token.span.start_line,
                                token.span.start_col,
                                next.span.end_line,
                                next.span.end_col,
                                self.file.clone(),
                            ),
                        ))
                    }
                    _ => {
                        self.error("Expected a number after '-' in pattern");
                        None
                    }
                }
            }
            _ => {
                self.error(&format!("Expected pattern, found {:?}", token.token_type));
                None
            }
        }
    }

    fn parse_interpolated_string_content(&mut self, content: &str, span: &SourceSpan) -> Expr {
        let mut parts = Vec::new();
        let mut expressions = Vec::new();
        let mut current_lit = String::new();
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '{' {
                current_lit.push('{');
                i += 2;
                continue;
            }
            if chars[i] == '{' {
                i += 1;

                let mut expr_str = String::new();
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '{' {
                        depth += 1;
                    } else if chars[i] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    expr_str.push(chars[i]);
                    i += 1;
                }

                if depth > 0 {
                    current_lit.push('{');
                    current_lit.push_str(&expr_str);
                    continue;
                }
                if expr_str.trim().is_empty() {
                    current_lit.push_str("{}");
                    continue;
                }
                if expr_str.contains('\n') || expr_str.contains(';') {
                    current_lit.push('{');
                    current_lit.push_str(&expr_str);
                    if depth == 0 {
                        current_lit.push('}');
                    }
                    continue;
                }

                let mut sub_diag = DiagnosticEngine::new("en");
                let mut sub_lexer = Lexer::new(&expr_str, &self.file);
                let sub_tokens = sub_lexer.tokenize(&mut sub_diag);
                let mut sub_parser = Parser::new(sub_tokens, &mut sub_diag, &self.file);
                let parsed_expr = sub_parser.parse_expression();
                let has_errors = sub_parser.diag.has_errors();
                let is_end = sub_parser.is_at_end();
                if !has_errors
                    && is_end
                    && let Some(expr) = parsed_expr
                {
                    parts.push(current_lit.clone());
                    current_lit.clear();
                    expressions.push(expr);
                } else {
                    // Not a Datara expression (e.g. CSS, JSON, regex, plain text); preserve as literal text
                    current_lit.push('{');
                    current_lit.push_str(&expr_str);
                    if depth == 0 {
                        current_lit.push('}');
                    }
                }
            } else {
                current_lit.push(chars[i]);
                i += 1;
            }
        }

        if expressions.is_empty() {
            Expr::Literal(LiteralValue::String(current_lit), span.clone())
        } else {
            parts.push(current_lit);
            Expr::InterpolatedString {
                parts,
                expressions,
                span: span.clone(),
            }
        }
    }

    fn consume(&mut self, token_type: &TokenType, msg: &str) -> Option<Token> {
        if self.check(token_type) {
            Some(self.advance())
        } else {
            self.error(msg);
            None
        }
    }

    fn consume_ident(&mut self, msg: &str) -> Option<String> {
        self.consume_ident_or_keyword(msg)
    }

    fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(token_type)
        }
    }

    /// True when the next tokens are a `-` immediately followed by an integer
    /// literal, i.e. a negative literal that must not be accepted as a count,
    /// size, bit index or address (casting it to `usize` would wrap).
    fn negative_literal_follows(&self) -> bool {
        self.check(&TokenType::Minus)
            && matches!(
                self.tokens.get(self.current + 1).map(|t| &t.token_type),
                Some(TokenType::IntLiteral(_))
            )
    }

    fn check_ident_str(&self, expected: &str) -> bool {
        if self.is_at_end() {
            false
        } else if let TokenType::Identifier(s) = &self.peek().token_type {
            s == expected
        } else {
            false
        }
    }

    fn match_ident_str(&mut self, expected: &str) -> bool {
        if self.check_ident_str(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn advance(&mut self) -> Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    fn peek(&self) -> &Token {
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

    fn previous(&self) -> Token {
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

    fn error(&mut self, msg: &str) {
        let span = self.peek().span.clone();
        self.diag.error(
            ErrorCode::SyntaxUnexpectedToken,
            msg.to_string(),
            Some(span),
        );
    }

    fn error_depth_limit(&mut self, what: &str) {
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

    fn synchronize(&mut self) {
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

fn start_line_from(s: &SourceSpan) -> usize {
    s.start_line
}
fn start_col_from(s: &SourceSpan) -> usize {
    s.start_col
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
