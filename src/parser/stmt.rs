use super::*;
use crate::ast::*;
use crate::lexer::TokenType;

impl<'a> Parser<'a> {
    pub(crate) fn parse_block(&mut self) -> Option<Stmt> {
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
    pub(crate) fn arm_body_is_block(&self) -> bool {
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
    pub(crate) fn parse_arm_block(&mut self) -> Option<Expr> {
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
    pub(crate) fn parse_arm_body(&mut self) -> Option<Expr> {
        if self.arm_body_is_block() {
            self.parse_arm_block()
        } else {
            self.parse_expression()
        }
    }

    pub(crate) fn parse_statement(&mut self) -> Option<Stmt> {
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

    pub(crate) fn parse_statement_inner(&mut self) -> Option<Stmt> {
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
}
