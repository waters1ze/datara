use super::*;
use crate::diagnostics::{DiagnosticEngine, SourceSpan};
use crate::lexer::TokenType;

impl<'a> Parser<'a> {
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
    pub(crate) fn parse_condition(&mut self) -> Option<Expr> {
        self.no_struct_literal_at_top = true;
        let expr = self.parse_expression();
        self.no_struct_literal_at_top = false;
        expr
    }

    /// Lookahead: assuming `self.current` points at `{`, does the brace content
    /// look like struct-literal field-initializer syntax (`{}` or `{ Ident :`)?
    pub(crate) fn struct_literal_ahead(&self) -> bool {
        match self.tokens.get(self.current + 1).map(|t| &t.token_type) {
            Some(TokenType::RBrace) => true,
            Some(TokenType::Identifier(_)) => matches!(
                self.tokens.get(self.current + 2).map(|t| &t.token_type),
                Some(TokenType::Colon)
            ),
            _ => false,
        }
    }

    pub(crate) fn parse_pipeline(&mut self) -> Option<Expr> {
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
    pub(crate) fn parse_pipeline_tail(&mut self, expr: Expr) -> Option<Expr> {
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
    pub(crate) fn parse_or_recovery_tail(&mut self, mut result_expr: Expr) -> Option<Expr> {
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
    pub(crate) fn parse_binary_climbing(&mut self, min_prec: u8) -> Option<Expr> {
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
    pub(crate) fn peek_binary_op(&self) -> Option<(u8, bool, &'static str)> {
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

    pub(crate) fn parse_unary(&mut self) -> Option<Expr> {
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

    pub(crate) fn parse_postfix(&mut self) -> Option<Expr> {
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

    pub(crate) fn parse_primary(&mut self) -> Option<Expr> {
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
    pub(crate) fn parse_ident_or_object_init(
        &mut self,
        span: SourceSpan,
        name: String,
    ) -> Option<Expr> {
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
    pub(crate) fn parse_decide_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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
    pub(crate) fn parse_match_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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
    pub(crate) fn parse_select_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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
    pub(crate) fn parse_map_literal_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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
    pub(crate) fn parse_bracket_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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
    pub(crate) fn parse_paren_expr(&mut self, start_span: SourceSpan) -> Option<Expr> {
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

    pub(crate) fn parse_paren_expr_inner(
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

    pub(crate) fn parse_pattern(&mut self) -> Option<Pattern> {
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

    pub(crate) fn parse_interpolated_string_content(
        &mut self,
        content: &str,
        span: &SourceSpan,
    ) -> Expr {
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
}
