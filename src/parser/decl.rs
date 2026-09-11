use super::*;
use crate::ast::*;
use crate::lexer::TokenType;

impl<'a> Parser<'a> {
    pub(crate) fn parse_attributes(&mut self) -> Vec<Attribute> {
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

    pub(crate) fn consume_literal_or_ident(&mut self) -> String {
        let token = self.advance();
        match &token.token_type {
            TokenType::StringLiteral(s) => s.clone(),
            TokenType::IntLiteral(n) => n.to_string(),
            TokenType::FloatLiteral(f) => f.to_string(),
            TokenType::Identifier(id) => id.clone(),
            _ => token.lexeme.clone(),
        }
    }

    pub(crate) fn check_ident_lexeme(&self, expected: &str) -> bool {
        if self.is_at_end() {
            return false;
        }
        match &self.peek().token_type {
            TokenType::Identifier(id) => id == expected,
            _ => false,
        }
    }

    pub(crate) fn parse_c_import_decl(&mut self) -> Option<CImportDecl> {
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

    pub(crate) fn parse_declaration_with_attrs(&mut self, attrs: Vec<Attribute>) -> Option<Decl> {
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

    pub(crate) fn parse_register_decl(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Option<RegisterDecl> {
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

    pub(crate) fn parse_type_decl(&mut self, is_export: bool) -> Option<TypeDecl> {
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

    pub(crate) fn consume_ident_or_keyword(&mut self, message: &str) -> Option<String> {
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

    pub(crate) fn consume_import_name(&mut self, message: &str) -> Option<String> {
        self.consume_ident_or_keyword(message)
    }

    pub(crate) fn parse_use_decl(&mut self) -> Option<UseDecl> {
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

    pub(crate) fn parse_class_decl(
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

    pub(crate) fn parse_enum_decl(&mut self, is_export: bool) -> Option<EnumDecl> {
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

    pub(crate) fn parse_behavior_decl(&mut self) -> Option<BehaviorDecl> {
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

    pub(crate) fn parse_component_decl(&mut self, is_export: bool) -> Option<ComponentDecl> {
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

    pub(crate) fn parse_role_decl(&mut self, is_export: bool) -> Option<RoleDecl> {
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

    pub(crate) fn parse_trait_decl(&mut self, is_export: bool) -> Option<TraitDef> {
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

    pub(crate) fn parse_trait_method_sig(&mut self) -> Option<TraitMethodSignature> {
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

    pub(crate) fn parse_impl_block(&mut self) -> Option<ImplBlock> {
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

    pub(crate) fn parse_class_item(&mut self) -> Option<ClassItem> {
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

    pub(crate) fn parse_function_decl(
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

    pub(crate) fn parse_packet_decl(&mut self) -> Option<PacketDecl> {
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

    pub(crate) fn parse_extern_fn_decl(&mut self) -> Option<ExternFnDecl> {
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
}
