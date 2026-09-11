use super::*;
use crate::ast::*;
use crate::lexer::TokenType;

impl<'a> Parser<'a> {
    pub(crate) fn parse_param_list(&mut self) -> Option<Vec<Param>> {
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

    pub(crate) fn parse_type(&mut self) -> Option<TypeNode> {
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

    pub(crate) fn parse_type_inner(&mut self) -> Option<TypeNode> {
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

    pub(crate) fn parse_contracts(&mut self) -> (Vec<ContractClause>, Vec<ContractClause>) {
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

    pub(crate) fn find_first_ident(expr: &Expr) -> Option<String> {
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
}
