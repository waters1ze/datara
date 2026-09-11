use super::*;

impl Resolver {
    pub(crate) fn resolve_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                self.enter_scope(&format!("fn_{}", f.name));
                for p in &f.params {
                    self.define_local(&p.name, SymbolKind::Param, false, &p.span);
                }
                self.resolve_stmt(&f.body, diag);
                self.exit_scope();
            }
            Decl::Class(c) => {
                self.current_target_type = Some(c.name.clone());
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item {
                        self.resolve_method(m, diag);
                    }
                }
                self.current_target_type = None;
            }
            Decl::Behavior(b) => {
                self.current_target_type = Some(b.target_type.clone());
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item {
                        self.resolve_method(m, diag);
                    }
                }
                self.current_target_type = None;
            }
            Decl::Impl(i) => {
                self.current_target_type = Some(i.target_type.clone());
                for m in &i.methods {
                    self.enter_scope(&format!("method_{}_{}", i.target_type, m.name));
                    self.define_local("this", SymbolKind::Param, false, &m.span);
                    self.define_local("self", SymbolKind::Param, false, &m.span);
                    for p in &m.params {
                        self.define_local(&p.name, SymbolKind::Param, false, &p.span);
                    }
                    self.resolve_stmt(&m.body, diag);
                    self.exit_scope();
                }
                self.current_target_type = None;
            }
            _ => {}
        }
    }

    fn resolve_method(&mut self, m: &MethodDecl, diag: &mut DiagnosticEngine) {
        self.enter_scope(&format!("method_{}", m.name));
        self.define_local("this", SymbolKind::Param, false, &m.span);
        for p in &m.params {
            self.define_local(&p.name, SymbolKind::Param, false, &p.span);
        }
        if let Some(body) = &m.body {
            self.resolve_stmt(body, diag);
        }
        self.exit_scope();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt, diag: &mut DiagnosticEngine) {
        match stmt {
            Stmt::Block(stmts, _) => {
                self.enter_scope("block");
                for s in stmts {
                    self.resolve_stmt(s, diag);
                }
                self.exit_scope();
            }
            Stmt::Let {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::Mut {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, true, span);
            }
            Stmt::Val {
                name,
                init,
                is_mut,
                span,
                ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, *is_mut, span);
            }
            Stmt::Const {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::CompactBind { name, init, span } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::Assign {
                target,
                value,
                span,
                ..
            } => {
                if let Expr::Identifier(name, id_span) = target {
                    if self.resolve_symbol(name).is_none() {
                        let mut candidates: Vec<&str> = Vec::new();
                        for s in self.scopes.iter().rev() {
                            for k in s.symbols.keys() {
                                candidates.push(k.as_str());
                            }
                        }
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
                            Some(id_span.clone()),
                            Some(help_msg),
                        );
                    }
                } else {
                    self.resolve_expr(target, diag);
                }
                let _ = span;
                self.resolve_expr(value, diag);
            }
            Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.resolve_expr(e, diag);
            }
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    self.resolve_expr(e, diag);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expr(condition, diag);
                self.resolve_stmt(then_branch, diag);
                if let Some(eb) = else_branch {
                    self.resolve_stmt(eb, diag);
                }
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                span,
            } => {
                self.resolve_expr(iterable, diag);
                self.enter_scope("for");
                self.define_local(var_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.resolve_expr(condition, diag);
                self.resolve_stmt(body, diag);
            }
            Stmt::Loop { body, .. } => {
                self.resolve_stmt(body, diag);
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                span,
            } => {
                self.resolve_stmt(try_block, diag);
                self.enter_scope("catch");
                self.define_local(err_var, SymbolKind::Variable, false, span);
                self.resolve_stmt(catch_block, diag);
                self.exit_scope();
            }
            Stmt::Parallel(body, _) => {
                self.resolve_stmt(body, diag);
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                span,
            } => {
                self.resolve_expr(iterable, diag);
                self.enter_scope("parallel_for");
                self.define_local(var_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::With {
                resource_name,
                init,
                body,
                span,
            } => {
                self.resolve_expr(init, diag);
                self.enter_scope("with");
                self.define_local(resource_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::Unsafe { body, .. } => {
                self.resolve_stmt(body, diag);
            }
            Stmt::Asm { .. } => {}
        }
    }

    fn resolve_expr(&mut self, expr: &Expr, diag: &mut DiagnosticEngine) {
        match expr {
            Expr::Identifier(name, span) => {
                let lookup_name = if name == "Self" {
                    self.current_target_type.as_deref().unwrap_or(name)
                } else {
                    name.as_str()
                };
                if let Some(sym) = self.resolve_symbol(lookup_name) {
                    if !sym.is_export
                        && !sym.span.file.is_empty()
                        && !span.file.is_empty()
                        && !crate::diagnostics::is_same_file_or_module(&sym.span.file, &span.file)
                        && !sym.span.file.contains("stdlib")
                    {
                        diag.error(
                            ErrorCode::PrivateItemAccess,
                            format!(
                                "Cannot access private item '{}' declared in '{}'",
                                lookup_name, sym.span.file
                            ),
                            Some(span.clone()),
                        );
                    }
                } else {
                    let has_field = self.scopes.iter().any(|s| {
                        if s.get("this").is_some() {
                            for cls in self.classes.values() {
                                if cls.fields.contains_key(lookup_name) {
                                    return true;
                                }
                            }
                        }
                        false
                    });

                    if !has_field {
                        let mut candidates: Vec<&str> = Vec::new();
                        for s in self.scopes.iter().rev() {
                            for k in s.symbols.keys() {
                                candidates.push(k.as_str());
                            }
                        }
                        for f in self.functions.keys() {
                            candidates.push(f.as_str());
                        }
                        for c in self.classes.keys() {
                            candidates.push(c.as_str());
                        }
                        candidates.sort_unstable();

                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(
                                lookup_name,
                                candidates,
                            ) {
                            format!("a symbol with a similar name exists: '{}'", similar)
                        } else {
                            format!(
                                "ensure '{}' is declared with 'let'/'mut' or imported via 'use'",
                                lookup_name
                            )
                        };

                        diag.error_with_help(
                            ErrorCode::ResolveUndefinedSymbol,
                            format!("Undefined symbol '{}'", lookup_name),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                }
            }
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left, diag);
                self.resolve_expr(right, diag);
            }
            Expr::Unary { expr, .. } | Expr::ErrorPropagate(expr, _) => {
                self.resolve_expr(expr, diag);
            }
            Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee, diag);
                for a in args {
                    self.resolve_expr(a, diag);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.resolve_expr(object, diag);
            }
            Expr::ObjectInit {
                class_name,
                span,
                fields,
                ..
            } => {
                let resolved_name = if class_name == "Self" {
                    self.current_target_type.as_deref().unwrap_or(class_name)
                } else {
                    class_name.as_str()
                };
                if let Some(cls) = self.classes.get(resolved_name) {
                    if !cls.is_export
                        && !cls.span.file.is_empty()
                        && !span.file.is_empty()
                        && !crate::diagnostics::is_same_file_or_module(&cls.span.file, &span.file)
                        && !cls.span.file.contains("stdlib")
                    {
                        diag.error(
                            ErrorCode::PrivateItemAccess,
                            format!(
                                "Cannot access private class '{}' declared in '{}'",
                                resolved_name, cls.span.file
                            ),
                            Some(span.clone()),
                        );
                    }
                } else {
                    let mut candidates: Vec<&str> =
                        self.classes.keys().map(|s| s.as_str()).collect();
                    candidates.sort_unstable();
                    let help_msg = if let Some(similar) =
                        crate::diagnostics::suggestions::find_best_match(resolved_name, candidates)
                    {
                        format!("a class with a similar name exists: '{}'", similar)
                    } else {
                        format!("define class '{}' or import it via 'use'", resolved_name)
                    };
                    diag.error_with_help(
                        ErrorCode::ResolveUndefinedSymbol,
                        format!(
                            "Unknown class '{}' in initialization (is a matching 'use' import missing?)",
                            resolved_name
                        ),
                        Some(span.clone()),
                        Some(help_msg),
                    );
                }
                for (_, f_expr) in fields {
                    self.resolve_expr(f_expr, diag);
                }
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    self.resolve_expr(s, diag);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for arm in arms {
                    self.resolve_expr(&arm.condition, diag);
                    self.resolve_expr(&arm.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.resolve_expr(eb, diag);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.resolve_expr(value, diag);
                for arm in arms {
                    self.enter_scope("match_arm");
                    match &arm.pattern {
                        Pattern::Identifier(id, span) => {
                            self.define_local(id, SymbolKind::Variable, false, span);
                        }
                        Pattern::Variant { bindings, span, .. } => {
                            for b in bindings {
                                self.define_local(b, SymbolKind::Variable, false, span);
                            }
                        }
                        _ => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.resolve_expr(g, diag);
                    }
                    self.resolve_expr(&arm.body, diag);
                    self.exit_scope();
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for arm in arms {
                    self.resolve_expr(&arm.condition, diag);
                    self.resolve_expr(&arm.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.resolve_expr(eb, diag);
                }
            }
            Expr::Lambda { params, body, .. } => {
                self.enter_scope("lambda");
                for p in params {
                    self.define_local(&p.name, SymbolKind::Param, false, &p.span);
                }
                self.resolve_expr(body, diag);
                self.exit_scope();
            }
            Expr::ListLiteral(items, _) => {
                for item in items {
                    self.resolve_expr(item, diag);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.resolve_expr(k, diag);
                    self.resolve_expr(v, diag);
                }
            }
            Expr::IndexAccess { object, index, .. } => {
                self.resolve_expr(object, diag);
                self.resolve_expr(index, diag);
            }
            Expr::Range { start, end, .. } => {
                self.resolve_expr(start, diag);
                self.resolve_expr(end, diag);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.resolve_expr(e, diag);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.resolve_expr(e, diag);
                }
            }
            Expr::OrRecovery { expr, arms, .. } => {
                self.resolve_expr(expr, diag);
                for arm in arms {
                    self.resolve_expr(&arm.body, diag);
                }
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                self.resolve_expr(elem, diag);
            }
            Expr::Comptime { expr, .. } => {
                self.resolve_expr(expr, diag);
            }
            Expr::Wrapping(expr, _) | Expr::Saturating(expr, _) => {
                self.resolve_expr(expr, diag);
            }
            _ => {}
        }
    }
}
