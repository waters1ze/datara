use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::lexer::{Lexer, Token, TokenType};

pub struct Parser<'a> {
    tokens: Vec<Token>,
    current: usize,
    diag: &'a mut DiagnosticEngine,
    file: String,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>, diag: &'a mut DiagnosticEngine, file: &str) -> Self {
        Self {
            tokens,
            current: 0,
            diag,
            file: file.to_string(),
        }
    }

    pub fn parse_program(&mut self) -> Program {
        let mut declarations = Vec::new();
        while !self.is_at_end() {
            if let Some(decl) = self.parse_declaration() {
                declarations.push(decl);
            } else {
                self.synchronize();
            }
        }
        Program {
            declarations,
            file: self.file.clone(),
        }
    }

    fn parse_declaration(&mut self) -> Option<Decl> {
        let is_export = self.match_token(&TokenType::Export);

        if self.match_token(&TokenType::Use) || self.match_token(&TokenType::Import) || self.match_token(&TokenType::Using) {
            return self.parse_use_decl().map(Decl::Use);
        }
        if self.match_token(&TokenType::Class) || self.match_token(&TokenType::Entity) {
            return self.parse_class_decl(is_export).map(Decl::Class);
        }
        if self.match_token(&TokenType::Enum) {
            return self.parse_enum_decl(is_export).map(Decl::Enum);
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
        if self.match_token(&TokenType::Fn) || self.match_token(&TokenType::Function) {
            return self.parse_function_decl(is_export).map(Decl::Function);
        }
        if self.match_token(&TokenType::Flow) {
            self.error(
                "SyntaxError: 'flow' is not a top-level declaration. Use 'fn' to define functions, and 'flow' inside pipelines: '|> flow Stage'.",
            );
            return None;
        }
        if self.match_token(&TokenType::Process) {
            return self.parse_function_decl(is_export).map(Decl::Flow);
        }
        if self.match_token(&TokenType::Task) {
            return self.parse_function_decl(is_export).map(Decl::Task);
        }
        if self.match_token(&TokenType::Packet) {
            return self.parse_packet_decl().map(Decl::Packet);
        }
        if self.match_token(&TokenType::Extern) {
            return self.parse_extern_fn_decl().map(Decl::ExternFn);
        }

        self.error(
            "Expected top-level declaration (class, entity, behavior, fn, process, component, role, packet, extern, use)",
        );
        None
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

    fn parse_class_decl(&mut self, is_export: bool) -> Option<ClassDecl> {
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
            if let Some(comp) = self.consume_ident("Expected component or role name") {
                compositions.push(comp);
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

        let end_token = self.consume(&TokenType::RBrace, "Expected '}' after class body")?;
        Some(ClassDecl {
            name,
            generic_params,
            base_type,
            compositions,
            body_items,
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

    fn parse_class_item(&mut self) -> Option<ClassItem> {
        if self.match_token(&TokenType::Using) {
            let name = self.consume_ident("Expected class name after 'using'")?;
            return Some(ClassItem::Using(name, self.previous().span.clone()));
        }

        let _ = self.match_token(&TokenType::Fn) || self.match_token(&TokenType::Function);
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
                generic_params: Vec::new(),
                params,
                return_type,
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

            let mut default_value = None;
            if self.match_token(&TokenType::Equal) {
                default_value = self.parse_expression();
            }

            Some(ClassItem::Field(FieldDecl {
                name,
                type_node,
                default_value,
                is_mut,
                span: self.previous().span.clone(),
            }))
        }
    }

    fn parse_function_decl(&mut self, is_export: bool) -> Option<FunctionDecl> {
        let start_span = self.previous().span.clone();
        let name = self.consume_ident("Expected function name")?;

        let mut generic_params = Vec::new();
        let mut generic_constraints = Vec::new();
        if self.match_token(&TokenType::Less) {
            while !self.check(&TokenType::Greater) && !self.is_at_end() {
                if let Some(param) = self.consume_ident("Expected generic parameter name") {
                    if self.match_token(&TokenType::Colon)
                        && let Some(constraint) =
                            self.consume_ident("Expected role constraint name")
                        {
                            generic_constraints.push((param.clone(), constraint));
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
            generic_params,
            generic_constraints,
            params,
            return_type,
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
            let bits = if let TokenType::IntLiteral(val) = self.peek().token_type {
                self.advance();
                val as usize
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

                let name = self.consume_ident("Expected parameter name")?;
                self.consume(&TokenType::Colon, "Expected ':' after parameter name")?;
                let type_node = self.parse_type();

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
        let start_span = self.peek().span.clone();
        let name = self.consume_ident("Expected type name")?;

        let mut generic_args = Vec::new();
        if self.match_token(&TokenType::Less) {
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

        let is_option = self.match_token(&TokenType::Question);

        let mut error_type = None;
        if self.match_token(&TokenType::Bang) {
            error_type = self.parse_type().map(Box::new);
        }

        Some(TypeNode {
            name,
            generic_args,
            is_option,
            error_type,
            span: SourceSpan::new(
                start_span.start_line,
                start_span.start_col,
                self.previous().span.end_line,
                self.previous().span.end_col,
                self.file.clone(),
            ),
        })
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

    fn parse_statement(&mut self) -> Option<Stmt> {
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

        if self.match_token(&TokenType::Return) {
            let expr = if !self.check(&TokenType::RBrace) && !self.is_at_end() {
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
            let condition = self.parse_expression()?;
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
            let condition = self.parse_expression()?;
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
        self.parse_pipeline()
    }

    fn parse_pipeline(&mut self) -> Option<Expr> {
        let mut expr = self.parse_logical_or()?;

        if self.check(&TokenType::Pipe) || self.check(&TokenType::Then) {
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
            expr = Expr::Pipeline {
                stages,
                span: SourceSpan::new(
                    start.start_line,
                    start.start_col,
                    end.end_line,
                    end.end_col,
                    self.file.clone(),
                ),
            };
        }

        let mut result_expr = expr;
        if self.match_token(&TokenType::OrKeyword) {
            let start_span = result_expr.span().clone();
            self.consume(&TokenType::LBrace, "Expected '{' after 'or'")?;
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
        }

        Some(result_expr)
    }

    fn parse_logical_or(&mut self) -> Option<Expr> {
        let mut expr = self.parse_logical_and()?;
        while self.match_token(&TokenType::Or) {
            let right = self.parse_logical_and()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op: "||".into(),
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_logical_and(&mut self) -> Option<Expr> {
        let mut expr = self.parse_equality()?;
        while self.match_token(&TokenType::And) {
            let right = self.parse_equality()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op: "&&".into(),
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_equality(&mut self) -> Option<Expr> {
        let mut expr = self.parse_comparison()?;
        while self.check(&TokenType::EqualEqual) || self.check(&TokenType::NotEqual) {
            let op = if self.match_token(&TokenType::EqualEqual) {
                "=="
            } else {
                self.advance();
                "!="
            }
            .to_string();
            let right = self.parse_comparison()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_comparison(&mut self) -> Option<Expr> {
        let mut expr = self.parse_range()?;
        while self.check(&TokenType::Less)
            || self.check(&TokenType::LessEqual)
            || self.check(&TokenType::Greater)
            || self.check(&TokenType::GreaterEqual)
        {
            let op = match self.advance().token_type {
                TokenType::Less => "<",
                TokenType::LessEqual => "<=",
                TokenType::Greater => ">",
                TokenType::GreaterEqual => ">=",
                _ => unreachable!(),
            }
            .to_string();
            let right = self.parse_range()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_range(&mut self) -> Option<Expr> {
        let mut expr = self.parse_term()?;
        if self.match_token(&TokenType::DotDot) {
            let end = self.parse_term()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                end.span().end_line,
                end.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Range {
                start: Box::new(expr),
                end: Box::new(end),
                span,
            };
        }
        Some(expr)
    }

    fn parse_term(&mut self) -> Option<Expr> {
        let mut expr = self.parse_factor()?;
        while self.check(&TokenType::Plus) || self.check(&TokenType::Minus) {
            let op = match self.advance().token_type {
                TokenType::Plus => "+",
                TokenType::Minus => "-",
                _ => unreachable!(),
            }
            .to_string();
            let right = self.parse_factor()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_factor(&mut self) -> Option<Expr> {
        let mut expr = self.parse_unary()?;
        while self.check(&TokenType::Star)
            || self.check(&TokenType::Slash)
            || self.check(&TokenType::Percent)
        {
            let op = match self.advance().token_type {
                TokenType::Star => "*",
                TokenType::Slash => "/",
                TokenType::Percent => "%",
                _ => unreachable!(),
            }
            .to_string();
            let right = self.parse_unary()?;
            let span = SourceSpan::new(
                expr.span().start_line,
                expr.span().start_col,
                right.span().end_line,
                right.span().end_col,
                self.file.clone(),
            );
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                span,
            };
        }
        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.match_token(&TokenType::Bang) || self.match_token(&TokenType::Minus) {
            let op = self.previous().lexeme.clone();
            let expr = self.parse_unary()?;
            let span = SourceSpan::new(
                self.previous().span.start_line,
                self.previous().span.start_col,
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

            TokenType::InterpolatedString(ref raw) => {
                let parsed = self.parse_interpolated_string_content(raw, &token.span);
                Some(parsed)
            }

            TokenType::Identifier(ref name) => {
                // Check if Lambda `x => expr`
                if self.match_token(&TokenType::FatArrow) {
                    let p_span = token.span.clone();
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
                        class_name: name.clone(),
                        generic_args,
                        fields,
                        span: SourceSpan::new(
                            token.span.start_line,
                            token.span.start_col,
                            end_token.span.end_line,
                            end_token.span.end_col,
                            self.file.clone(),
                        ),
                    });
                }

                Some(Expr::Identifier(name.clone(), token.span))
            }

            TokenType::View => Some(Expr::Identifier("view".into(), token.span)),
            TokenType::Mut => Some(Expr::Identifier("mut".into(), token.span)),
            TokenType::Err => Some(Expr::Identifier("err".into(), token.span)),
            TokenType::Out => Some(Expr::Identifier("out".into(), token.span)),

            TokenType::Decide => {
                let start_span = token.span.clone();
                self.consume(&TokenType::LBrace, "Expected '{' after decide")?;
                let mut arms = Vec::new();
                let mut else_arm = None;

                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    if self.match_token(&TokenType::Else) {
                        self.consume(&TokenType::FatArrow, "Expected '=>' after else")?;
                        else_arm = Some(Box::new(self.parse_expression()?));
                        self.match_token(&TokenType::Comma);
                    } else {
                        let condition = self.parse_expression()?;
                        self.consume(&TokenType::FatArrow, "Expected '=>' after decide condition")?;
                        let body = self.parse_expression()?;
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

                let end_token =
                    self.consume(&TokenType::RBrace, "Expected '}' after decide block")?;
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

            TokenType::Match => {
                let start_span = token.span.clone();
                let value = Box::new(self.parse_expression()?);
                self.consume(&TokenType::LBrace, "Expected '{' after match expression")?;
                let mut arms = Vec::new();

                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    let pattern = self.parse_pattern()?;
                    let mut guard = None;
                    if self.match_token(&TokenType::If) || self.match_token(&TokenType::When) {
                        guard = self.parse_expression();
                    }
                    self.consume(&TokenType::FatArrow, "Expected '=>' after match pattern")?;
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

                let end_token =
                    self.consume(&TokenType::RBrace, "Expected '}' after match arms")?;
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

            TokenType::Select => {
                let start_span = token.span.clone();
                self.consume(&TokenType::LBrace, "Expected '{' after select")?;
                let mut arms = Vec::new();
                let mut else_arm = None;

                while !self.check(&TokenType::RBrace) && !self.is_at_end() {
                    if self.match_token(&TokenType::Else) {
                        self.consume(&TokenType::FatArrow, "Expected '=>' after else")?;
                        else_arm = Some(Box::new(self.parse_expression()?));
                        self.match_token(&TokenType::Comma);
                    } else {
                        let condition = self.parse_expression()?;
                        self.consume(&TokenType::FatArrow, "Expected '=>' after select condition")?;
                        let body = self.parse_expression()?;
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

                let end_token =
                    self.consume(&TokenType::RBrace, "Expected '}' after select block")?;
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

            TokenType::LBrace => {
                let start_span = token.span.clone();
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
                let end_token =
                    self.consume(&TokenType::RBrace, "Expected '}' after map literal")?;
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

            TokenType::LBracket => {
                let start_span = token.span.clone();
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
                    let count = if let TokenType::IntLiteral(c) = self.peek().token_type {
                        self.advance();
                        c as usize
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
                    let end_token =
                        self.consume(&TokenType::RBracket, "Expected ']' after map literal")?;
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
                let end_token =
                    self.consume(&TokenType::RBracket, "Expected ']' after list literal")?;
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

            TokenType::LParen => {
                let start_span = token.span.clone();
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
                        return Some(Expr::Lambda {
                            params: Vec::new(),
                            body,
                            span,
                        });
                    }
                    return Some(Expr::Literal(LiteralValue::None, start_span));
                }

                let mut exprs = Vec::new();
                let first = self.parse_expression()?;
                exprs.push(first);
                while self.match_token(&TokenType::Comma) {
                    if self.check(&TokenType::RParen) {
                        break;
                    }
                    exprs.push(self.parse_expression()?);
                }
                let end_token = self.consume(&TokenType::RParen, "Expected ')'")?;

                if self.match_token(&TokenType::FatArrow) {
                    let mut params = Vec::new();
                    for e in exprs {
                        if let Expr::Identifier(name, p_span) = e {
                            params.push(Param {
                                name,
                                type_node: None,
                                ownership_mode: "owned".into(),
                                span: p_span,
                            });
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
                    return Some(Expr::Lambda { params, body, span });
                }

                if exprs.len() == 1 {
                    Some(exprs.remove(0))
                } else {
                    Some(Expr::Tuple(
                        exprs,
                        SourceSpan::new(
                            start_span.start_line,
                            start_span.start_col,
                            end_token.span.end_line,
                            end_token.span.end_col,
                            self.file.clone(),
                        ),
                    ))
                }
            }

            _ => {
                self.error(&format!("Unexpected token: {:?}", token.token_type));
                None
            }
        }
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
                            LiteralValue::Int(-val),
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
                if !has_errors && is_end
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
        &self.tokens[self.current]
    }

    fn previous(&self) -> Token {
        self.tokens[self.current.saturating_sub(1)].clone()
    }

    fn error(&mut self, msg: &str) {
        let span = self.peek().span.clone();
        self.diag.error(
            ErrorCode::SyntaxUnexpectedToken,
            msg.to_string(),
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
                | TokenType::Let
                | TokenType::Mut
                | TokenType::If
                | TokenType::For
                | TokenType::While
                | TokenType::Return => return,
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
