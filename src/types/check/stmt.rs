use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::types::{DataraType, MutabilityKind, TypeChecker};

impl<'a> TypeChecker<'a> {
    pub fn check_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        // Pre-register enum variants and FFI `use` imports so they resolve
        // regardless of declaration order. The resolver types FFI imports as
        // dynamic `Val` symbols.
        for decl in &program.declarations {
            match decl {
                Decl::Enum(e) => {
                    let enum_type = DataraType::Class(e.name.clone());
                    for v in &e.variants {
                        self.symbol_types.insert(v.name.clone(), enum_type.clone());
                        self.symbol_types
                            .insert(format!("{}.{}", e.name, v.name), enum_type.clone());
                    }
                }
                Decl::Use(u) => {
                    let first_seg = u.path.first().map(|s| s.as_str());
                    if matches!(
                        first_seg,
                        Some("python" | "rust" | "c" | "cpp" | "cxx" | "npm" | "js" | "ts")
                    ) {
                        let alias = u
                            .alias
                            .clone()
                            .unwrap_or_else(|| u.path.last().cloned().unwrap_or_default());
                        if !alias.is_empty() {
                            self.symbol_types.entry(alias).or_insert(DataraType::Val);
                        }
                    }
                }
                _ => {}
            }
        }

        // Collect function signatures first
        for decl in &program.declarations {
            if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl {
                let p_types: Vec<DataraType> = f
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Int)
                    })
                    .collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                let gen_params: Vec<String> = f.generic_params.clone();
                self.function_signatures
                    .insert(f.name.clone(), (p_types, ret, gen_params));
                let p_nodes: Vec<Option<TypeNode>> =
                    f.params.iter().map(|p| p.type_node.clone()).collect();
                self.function_param_nodes.insert(f.name.clone(), p_nodes);
            } else if let Decl::ExternFn(ef) = decl {
                let p_types: Vec<DataraType> = ef
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Int)
                    })
                    .collect();
                let ret = ef
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                self.function_signatures
                    .insert(ef.name.clone(), (p_types, ret, Vec::new()));
            }
        }

        for decl in &program.declarations {
            self.check_decl(decl, diag);
        }
    }

    fn check_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Type(td) => {
                let _ = self.resolve_type_node(&td.base_type, diag);
            }
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                self.current_fn_name = Some(f.name.clone());
                for p in &f.params {
                    let p_type = p
                        .type_node
                        .as_ref()
                        .map(|t| self.resolve_type_node(t, diag))
                        .unwrap_or(DataraType::Int);
                    self.symbol_types.insert(p.name.clone(), p_type.clone());
                    self.fn_symbol_types
                        .insert((f.name.clone(), p.name.clone()), p_type.erasure());
                    if let Some(tn) = &p.type_node {
                        self.var_refinements.insert(p.name.clone(), tn.clone());
                    }
                }
                if let Some(rt) = &f.return_type {
                    self.validate_error_channels(rt, diag);
                }
                let expected = f
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                self.current_return_type = Some(expected.clone());

                for req in &f.requires {
                    self.check_expr(&req.condition, diag);
                }

                self.symbol_types.insert("result".into(), expected.clone());
                for ens in &f.ensures {
                    self.check_expr(&ens.condition, diag);
                }
                self.symbol_types.remove("result");

                let body_type = self.check_stmt(&f.body, diag);
                self.current_return_type = None;
                self.current_fn_name = None;

                if f.is_expression_body
                    && !body_type.is_compatible_with_args(&expected, Some(self.resolver))
                    && expected != DataraType::Unit
                    && !matches!(expected, DataraType::TypeParam(_))
                {
                    let help_msg = Self::suggest_type_fix(&expected, &body_type);
                    diag.error_with_help(
                        ErrorCode::TypeMismatch,
                        format!(
                            "Type mismatch: expected '{}', got '{}'",
                            expected, body_type
                        ),
                        Some(f.span.clone()),
                        help_msg,
                    );
                }
            }
            Decl::Class(c) => {
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let m_fn_name = format!("{}_{}", c.name, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());
                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(c.name.clone()));
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(c.name.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(|t| self.resolve_type_node(t, diag))
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                            if let Some(tn) = &p.type_node {
                                self.var_refinements.insert(p.name.clone(), tn.clone());
                            }
                        }
                        if let Some(rt) = &m.return_type {
                            self.validate_error_channels(rt, diag);
                        }
                        let expected = m
                            .return_type
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Unit);
                        self.current_return_type = Some(expected.clone());

                        for req in &m.requires {
                            self.check_expr(&req.condition, diag);
                        }

                        self.symbol_types.insert("result".into(), expected.clone());
                        for ens in &m.ensures {
                            self.check_expr(&ens.condition, diag);
                        }
                        self.symbol_types.remove("result");

                        self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;
                    }
                }
            }
            Decl::Enum(e) => {
                let enum_type = DataraType::Class(e.name.clone());
                for v in &e.variants {
                    self.symbol_types.insert(v.name.clone(), enum_type.clone());
                    self.symbol_types
                        .insert(format!("{}.{}", e.name, v.name), enum_type.clone());
                }
            }
            Decl::Behavior(b) => {
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let m_fn_name = format!("{}_{}", b.target_type, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());
                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(b.target_type.clone()));
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(b.target_type.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(|t| self.resolve_type_node(t, diag))
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                            if let Some(tn) = &p.type_node {
                                self.var_refinements.insert(p.name.clone(), tn.clone());
                            }
                        }
                        if let Some(rt) = &m.return_type {
                            self.validate_error_channels(rt, diag);
                        }
                        let expected = m
                            .return_type
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Unit);
                        self.current_return_type = Some(expected.clone());

                        for req in &m.requires {
                            self.check_expr(&req.condition, diag);
                        }

                        self.symbol_types.insert("result".into(), expected.clone());
                        for ens in &m.ensures {
                            self.check_expr(&ens.condition, diag);
                        }
                        self.symbol_types.remove("result");

                        self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn record_var_type(&mut self, name: &str, ty: DataraType) {
        if let Some(ref fn_name) = self.current_fn_name {
            // DMIR/codegen only understand the erased class form
            // (`Class("List")`); keep the parametric type checker-internal.
            self.fn_symbol_types
                .insert((fn_name.clone(), name.to_string()), ty.erasure());
        }
        self.symbol_types.insert(name.to_string(), ty);
    }

    pub fn check_stmt(&mut self, stmt: &Stmt, diag: &mut DiagnosticEngine) -> DataraType {
        match stmt {
            Stmt::Block(stmts, _) => {
                // Lexical scope: declarations inside the block must not leak
                // into sibling blocks or the enclosing scope.
                let saved_types = self.symbol_types.clone();
                let saved_mut = self.symbol_mutability.clone();
                let saved_elem = self.var_element_types.clone();
                let saved_refinements = self.var_refinements.clone();
                let saved_lengths = self.var_array_lengths.clone();
                let mut last = DataraType::Unit;
                for s in stmts {
                    last = self.check_stmt(s, diag);
                }
                self.symbol_types = saved_types;
                self.symbol_mutability = saved_mut;
                self.var_element_types = saved_elem;
                self.var_refinements = saved_refinements;
                self.var_array_lengths = saved_lengths;
                last
            }
            Stmt::Let {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Immutable);
                final_ty
            }
            Stmt::Const {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Immutable);
                final_ty
            }
            Stmt::Mut {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::MutableFixed);
                final_ty
            }
            Stmt::Val {
                name,
                type_node,
                init,
                is_mut,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else if !*is_mut {
                    // Type promotion: immutable `val` promotes directly to its concrete scalar SSA register type
                    init_type
                } else {
                    DataraType::Val
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Dynamic { is_mut: *is_mut });
                final_ty
            }
            Stmt::CompactBind { name, init, .. } => {
                let init_type = self.check_expr(init, diag);
                self.record_var_type(name, init_type.clone());
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::MutableFixed);
                init_type
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let val_type = self.check_expr(value, diag);
                if let Expr::Identifier(name, _) = target {
                    if let Some(tn) = self.var_refinements.get(name).cloned() {
                        self.check_refinement(&tn, value, span, diag);
                    }
                    if let Some(mut_kind) = self.symbol_mutability.get(name) {
                        match mut_kind {
                            MutabilityKind::Immutable => {
                                diag.error_with_help(
                                    ErrorCode::BorrowCannotMutateImmutable,
                                    format!("Cannot assign twice to immutable variable '{}'", name),
                                    Some(span.clone()),
                                    Some(format!(
                                        "consider declaring '{}' as mutable: 'mut {} = ...'",
                                        name, name
                                    )),
                                );
                            }
                            MutabilityKind::Dynamic { is_mut: false } => {
                                diag.error_with_help(
                                    ErrorCode::BorrowCannotMutateImmutable,
                                    format!(
                                        "Cannot assign to immutable val '{}'",
                                        name
                                    ),
                                    Some(span.clone()),
                                    Some(format!("'val' constants cannot be reassigned; use 'mut val {}' or 'mut {}' if mutation is required", name, name)),
                                );
                            }
                            MutabilityKind::MutableFixed => {
                                if let Some(existing) = self.symbol_types.get(name).cloned() {
                                    self.check_range_and_measure_assignment(
                                        &existing, &val_type, value, span, diag,
                                    );
                                    if !val_type.is_compatible_with_refined_with_args(
                                        &existing,
                                        Some(self.resolver),
                                    ) {
                                        let help_msg = Self::suggest_type_fix(&existing, &val_type);
                                        diag.error_with_help(
                                            ErrorCode::TypeMismatch,
                                            format!(
                                                "Type mismatch in assignment to mutable variable '{}': expected '{}', got '{}'",
                                                name, existing, val_type
                                            ),
                                            Some(span.clone()),
                                            help_msg,
                                        );
                                    }
                                }
                            }
                            MutabilityKind::Dynamic { is_mut: true } => {
                                if let Some(existing) = self.symbol_types.get(name).cloned() {
                                    self.check_range_and_measure_assignment(
                                        &existing, &val_type, value, span, diag,
                                    );
                                }
                                // `mut val` bindings stay dynamically typed:
                                // do NOT re-type the symbol to the assigned
                                // value's concrete type, otherwise a concrete
                                // type would flow into checked contexts while
                                // the runtime still treats it as `Val`.
                            }
                        }
                    } else {
                        let mut candidates: Vec<&str> =
                            self.symbol_types.keys().map(|s| s.as_str()).collect();
                        candidates.sort_unstable();
                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(name, candidates)
                        {
                            format!(
                                "a variable with a similar name exists: '{}'. Or declare with 'let {} = ...' / 'mut {} = ...'",
                                similar, name, name
                            )
                        } else {
                            format!(
                                "declare '{}' with 'let' (immutable) or 'mut' (mutable) before assigning to it",
                                name
                            )
                        };
                        diag.error_with_help(
                            ErrorCode::ResolveUndefinedSymbol,
                            format!("Assignment to undeclared variable '{}'", name),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                } else {
                    let tgt_type = self.check_expr(target, diag);
                    if !val_type.is_compatible_with_args(&tgt_type, Some(self.resolver)) {
                        let help_msg = Self::suggest_type_fix(&tgt_type, &val_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in assignment: expected '{}', got '{}'",
                                tgt_type, val_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                }
                val_type
            }
            Stmt::Expr(e, _) => self.check_expr(e, diag),
            Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.check_expr(e, diag);
                DataraType::Unit
            }
            Stmt::Return(opt_e, span) => {
                let t = if let Some(e) = opt_e {
                    self.check_expr(e, diag)
                } else {
                    DataraType::Unit
                };
                // When the enclosing signature is Result/Option-like, the
                // returned value must be the same kind with a compatible
                // payload — there is no implicit wrapping or coercion.
                if let Some(expected) = self.current_return_type.clone() {
                    let enc_res = expected.result_like();
                    let enc_opt = expected.option_like();
                    if enc_res.is_some() || enc_opt.is_some() {
                        let matches = if let Some((eok, eerr)) = &enc_res {
                            match t.result_like() {
                                Some((tok, terr)) => {
                                    tok.is_compatible_with_args(eok, Some(self.resolver))
                                        && terr.is_compatible_with_args(eerr, Some(self.resolver))
                                }
                                None => false,
                            }
                        } else if let Some(einner) = &enc_opt {
                            match t.option_like() {
                                Some(tinner) => {
                                    tinner.is_compatible_with_args(einner, Some(self.resolver))
                                }
                                None => false,
                            }
                        } else {
                            false
                        };
                        if !matches {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "function signature returns '{}' but the return statement produces '{}'; construct it explicitly, e.g. Outcome<T> {{ is_success: .., value: .., error_msg: .. }}",
                                    expected, t
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }
                t
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let cond_type = self.check_expr(condition, diag);
                if !cond_type.is_compatible_with_args(&DataraType::Bool, Some(self.resolver)) {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!("Condition must be Bool, got '{}'", cond_type),
                        Some(span.clone()),
                    );
                }

                // Smart Type Narrowing for Option/Maybe:
                let is_neq_none = match condition {
                    Expr::Binary {
                        left, op, right, ..
                    } if op == "!=" => {
                        if let Expr::Identifier(var_name, _) = &**left {
                            if matches!(&**right, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else if let Expr::Identifier(var_name, _) = &**right {
                            if matches!(&**left, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                let is_eq_none = match condition {
                    Expr::Binary {
                        left, op, right, ..
                    } if op == "==" => {
                        if let Expr::Identifier(var_name, _) = &**left {
                            if matches!(&**right, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else if let Expr::Identifier(var_name, _) = &**right {
                            if matches!(&**left, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(ref v) = is_neq_none {
                    let orig = self.symbol_types.get(v).cloned();
                    if let Some(DataraType::Option(inner)) = &orig {
                        self.symbol_types.insert(v.clone(), *inner.clone());
                    }
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(o) = orig {
                        self.symbol_types.insert(v.clone(), o);
                    }
                    if let Some(eb) = else_branch {
                        let orig_else = self.symbol_types.get(v).cloned();
                        self.symbol_types
                            .insert(v.clone(), DataraType::Option(Box::new(DataraType::Unit)));
                        self.check_stmt(eb, diag);
                        if let Some(o) = orig_else {
                            self.symbol_types.insert(v.clone(), o);
                        }
                    }
                    res
                } else if let Some(ref v) = is_eq_none {
                    let orig = self.symbol_types.get(v).cloned();
                    self.symbol_types
                        .insert(v.clone(), DataraType::Option(Box::new(DataraType::Unit)));
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(o) = orig {
                        self.symbol_types.insert(v.clone(), o);
                    }
                    if let Some(eb) = else_branch {
                        let orig_else = self.symbol_types.get(v).cloned();
                        if let Some(DataraType::Option(inner)) = &orig_else {
                            self.symbol_types.insert(v.clone(), *inner.clone());
                        }
                        self.check_stmt(eb, diag);
                        if let Some(o) = orig_else {
                            self.symbol_types.insert(v.clone(), o);
                        }
                    }
                    res
                } else {
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(eb) = else_branch {
                        self.check_stmt(eb, diag);
                    }
                    res
                }
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            } => {
                let iter_type = self.check_expr(iterable, diag);
                // Parametric collections carry their element types into the
                // loop variable: `for x in List(t)` binds `x: t`. Maps have no
                // key/value iterator protocol (the runtime lowers iteration to
                // the list protocol), so their elements stay dynamic `Val`.
                let elem_type = match &iter_type {
                    DataraType::List(elem) => (**elem).clone(),
                    DataraType::Map(..) => DataraType::Val,
                    DataraType::GenericInstance { name, args }
                        if name == "List" && !args.is_empty() =>
                    {
                        args[0].clone()
                    }
                    DataraType::Class(c) if c == "Range" => DataraType::Int,
                    DataraType::String => DataraType::Char,
                    DataraType::Class(c) if c == "List" => {
                        // Erased collections: fall back to the recorded element
                        // type, else the dynamic type — never a silent Int.
                        if let Expr::Identifier(n, _) = iterable {
                            self.var_element_types
                                .get(n)
                                .cloned()
                                .unwrap_or(DataraType::Val)
                        } else {
                            self.last_list_element.clone().unwrap_or(DataraType::Val)
                        }
                    }
                    _ => DataraType::Val,
                };
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), var_name.clone()), elem_type.clone());
                }
                let prev = self.symbol_types.insert(var_name.clone(), elem_type);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(var_name.clone(), p);
                } else {
                    self.symbol_types.remove(var_name);
                }
                DataraType::Unit
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                let cond_type = self.check_expr(condition, diag);
                if !cond_type.is_compatible_with_args(&DataraType::Bool, Some(self.resolver)) {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!("While condition must be Bool, got '{}'", cond_type),
                        Some(span.clone()),
                    );
                }
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::Loop { body, .. } => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                ..
            } => {
                self.check_stmt(try_block, diag);
                self.symbol_types
                    .insert(err_var.clone(), DataraType::String);
                self.check_stmt(catch_block, diag);
                DataraType::Unit
            }
            Stmt::Parallel(body, _) => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                let iter_type = self.check_expr(iterable, diag);
                // Same element-type rules as `Stmt::For`: parametric List
                // carries its element type; Map elements are dynamic `Val`
                // (no key/value iterator protocol); erased collections fall
                // back to the recorded element type, never a silent Int.
                let elem_type = match &iter_type {
                    DataraType::List(elem) => (**elem).clone(),
                    DataraType::Map(..) => DataraType::Val,
                    DataraType::GenericInstance { name, args }
                        if name == "List" && !args.is_empty() =>
                    {
                        args[0].clone()
                    }
                    DataraType::Class(c) if c == "Range" => DataraType::Int,
                    DataraType::String => DataraType::Char,
                    DataraType::Class(c) if c == "List" => {
                        if let Expr::Identifier(n, _) = iterable {
                            self.var_element_types
                                .get(n)
                                .cloned()
                                .unwrap_or(DataraType::Val)
                        } else {
                            self.last_list_element.clone().unwrap_or(DataraType::Val)
                        }
                    }
                    _ => DataraType::Val,
                };
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), var_name.clone()), elem_type.clone());
                }
                let prev = self.symbol_types.insert(var_name.clone(), elem_type);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(var_name.clone(), p);
                } else {
                    self.symbol_types.remove(var_name);
                }
                DataraType::Unit
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                let init_type = self.check_expr(init, diag);
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), resource_name.clone()), init_type.clone());
                }
                self.symbol_types.insert(resource_name.clone(), init_type);
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::Unsafe { body, .. } => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::Asm { .. } => DataraType::Unit,
        }
    }
}
