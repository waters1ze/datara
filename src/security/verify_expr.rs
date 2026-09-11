use super::*;
use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::types::DataraType;

impl<'a> SecurityVerifier<'a> {
    pub(crate) fn verify_expr(
        &mut self,
        expr: &Expr,
        ctx: &mut FnContext,
        diag: &mut DiagnosticEngine,
    ) {
        match expr {
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                self.verify_expr(left, ctx, diag);

                let mut right_ctx = ctx.clone();
                if op == "&&" {
                    for var_name in ctx.symbols.keys() {
                        if is_proven_non_zero_cond(left, var_name) {
                            right_ctx.proven_non_zero.insert(var_name.clone());
                        }
                    }
                }

                self.verify_expr(right, &mut right_ctx, diag);

                // Proof-Carrying Code Gate: Division by zero gate
                if op == "/" || op == "%" {
                    self.verify_division_gate(right, span, ctx, diag);
                }

                if ctx.is_no_alloc && op == "+" {
                    let is_str = matches!(
                        (&**left, &**right),
                        (Expr::Literal(LiteralValue::String(_), _), _)
                            | (_, Expr::Literal(LiteralValue::String(_), _))
                    );
                    if is_str {
                        diag.error(
                            ErrorCode::AllocationViolation,
                            format!(
                                "[E0950] Allocation Violation: String concatenation allocates on the heap in '@no_alloc' function '{}'",
                                ctx.fn_name
                            ),
                            Some(span.clone()),
                        );
                    }
                }
            }
            Expr::Call { callee, args, span } => {
                self.verify_expr(callee, ctx, diag);
                for a in args {
                    self.verify_expr(a, ctx, diag);
                }

                if let Expr::Identifier(callee_name, callee_span) = &**callee {
                    if ctx.is_no_panic && callee_name == "panic" {
                        diag.error(
                            ErrorCode::PanicViolation,
                            format!(
                                "[E0951] Panic Violation: Explicit 'panic' call is forbidden in '@no_panic' function '{}'",
                                ctx.fn_name
                            ),
                            Some(callee_span.clone()),
                        );
                    }

                    if ctx.is_no_alloc
                        && matches!(
                            callee_name.as_str(),
                            "http_get"
                                | "http_post"
                                | "db_query"
                                | "read_file"
                                | "write_file"
                                | "format"
                        )
                    {
                        diag.error(
                            ErrorCode::AllocationViolation,
                            format!(
                                "[E0950] Allocation Violation: Dynamic allocator call '{}' is forbidden in '@no_alloc' function '{}'",
                                callee_name, ctx.fn_name
                            ),
                            Some(callee_span.clone()),
                        );
                    }

                    // Gate 3: Unchecked FFI Gate
                    if self.resolver.extern_functions.contains_key(callee_name) {
                        let justified = match &ctx.unsafe_justification {
                            Some(j) => !j.trim().is_empty(),
                            None => false,
                        };
                        if !justified {
                            diag.error(
                                ErrorCode::UncheckedFFIViolation,
                                format!(
                                    "Security Violation: Foreign call to extern function '{}' requires 'unsafe(justification: \"...\")' block",
                                    callee_name
                                ),
                                Some(callee_span.clone()),
                            );
                        }
                    }

                    // Gate 1: Capability Security OS gate
                    if let Some(req_cap) = required_capability_for_op(callee_name) {
                        let justified = match &ctx.unsafe_justification {
                            Some(j) => !j.trim().is_empty(),
                            None => false,
                        };
                        let is_stdlib = span.file.contains("stdlib");
                        let has_cap = is_stdlib
                            || justified
                            || has_capability(&ctx.symbols, req_cap)
                            || args
                                .iter()
                                .any(|arg| self.expr_has_capability(arg, req_cap, ctx));
                        if !has_cap {
                            diag.error(
                                ErrorCode::SecurityViolation,
                                format!(
                                    "Security Violation: Operation '{}' requires '{}'",
                                    callee_name, req_cap
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }

                // Gate 4: Static Contract Precondition Gate (E0948)
                let callee_id_opt = match &**callee {
                    Expr::Identifier(id, _) => Some(id.as_str()),
                    Expr::MemberAccess { member, .. } => Some(member.as_str()),
                    _ => None,
                };
                if let Some(callee_id) = callee_id_opt {
                    if let Some((params, requires)) = self.function_decls.get(callee_id) {
                        if args.len() == params.len() && !requires.is_empty() {
                            let mut arg_env = HashMap::new();
                            for (p, arg) in params.iter().zip(args.iter()) {
                                let val = eval_static_expr(arg, &HashMap::new());
                                if val != StaticVal::Unknown {
                                    arg_env.insert(p.name.clone(), val);
                                }
                            }
                            for req in requires {
                                let res = eval_static_expr(&req.condition, &arg_env);
                                if res == StaticVal::Bool(false) {
                                    diag.error(
                                        ErrorCode::ContractViolation,
                                        format!(
                                            "Contract Violation: Call to '{}' statically violates precondition: {}",
                                            callee_id,
                                            req.message.as_deref().unwrap_or("Precondition failed")
                                        ),
                                        Some(span.clone()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Expr::MemberAccess { object, member, .. } => {
                self.verify_expr(object, ctx, diag);
                let _ = member;
            }
            Expr::Unary { expr, .. } => {
                self.verify_expr(expr, ctx, diag);
            }
            Expr::IndexAccess {
                object,
                index,
                span,
            } => {
                self.verify_expr(object, ctx, diag);
                self.verify_expr(index, ctx, diag);
                if ctx.is_no_panic {
                    diag.error(
                        ErrorCode::PanicViolation,
                        format!(
                            "[E0951] Panic Violation: Unchecked index access may panic out of bounds in '@no_panic' function '{}'",
                            ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
            }
            Expr::Range { start, end, .. } => {
                self.verify_expr(start, ctx, diag);
                self.verify_expr(end, ctx, diag);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.verify_expr(e, ctx, diag);
                }
            }
            Expr::ListLiteral(exprs, span) => {
                if ctx.is_no_alloc {
                    diag.error(
                        ErrorCode::AllocationViolation,
                        format!(
                            "[E0950] Allocation Violation: Dynamic collection literal allocates on the heap in '@no_alloc' function '{}'",
                            ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
                for e in exprs {
                    self.verify_expr(e, ctx, diag);
                }
            }
            Expr::MapLiteral(entries, span) => {
                if ctx.is_no_alloc {
                    diag.error(
                        ErrorCode::AllocationViolation,
                        format!(
                            "[E0950] Allocation Violation: Dynamic map literal allocates on the heap in '@no_alloc' function '{}'",
                            ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
                for (k, v) in entries {
                    self.verify_expr(k, ctx, diag);
                    self.verify_expr(v, ctx, diag);
                }
            }
            Expr::InterpolatedString {
                expressions, span, ..
            } => {
                if ctx.is_no_alloc {
                    diag.error(
                        ErrorCode::AllocationViolation,
                        format!(
                            "[E0950] Allocation Violation: String interpolation allocates on the heap in '@no_alloc' function '{}'",
                            ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
                for e in expressions {
                    self.verify_expr(e, ctx, diag);
                }
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    self.verify_expr(s, ctx, diag);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for arm in arms {
                    self.verify_expr(&arm.condition, ctx, diag);
                    self.verify_expr(&arm.body, ctx, diag);
                }
                if let Some(eb) = else_arm {
                    self.verify_expr(eb, ctx, diag);
                }
            }
            Expr::Match { arms, value, .. } => {
                self.verify_expr(value, ctx, diag);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.verify_expr(g, ctx, diag);
                    }
                    self.verify_expr(&arm.body, ctx, diag);
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for arm in arms {
                    self.verify_expr(&arm.condition, ctx, diag);
                    self.verify_expr(&arm.body, ctx, diag);
                }
                if let Some(eb) = else_arm {
                    self.verify_expr(eb, ctx, diag);
                }
            }
            Expr::Lambda { body, .. } => {
                self.verify_expr(body, ctx, diag);
            }
            Expr::ErrorPropagate(inner, _) => {
                self.verify_expr(inner, ctx, diag);
            }
            Expr::OrRecovery { expr, arms, .. } => {
                self.verify_expr(expr, ctx, diag);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.verify_expr(g, ctx, diag);
                    }
                    self.verify_expr(&arm.body, ctx, diag);
                }
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                self.verify_expr(elem, ctx, diag);
            }
            Expr::Comptime { expr, .. } => {
                self.verify_expr(expr, ctx, diag);
            }
            Expr::Wrapping(expr, _) | Expr::Saturating(expr, _) => {
                self.verify_expr(expr, ctx, diag);
            }
            Expr::ObjectInit {
                class_name,
                fields,
                span,
                ..
            } => {
                if ctx.is_no_alloc && !class_name.contains("Arena") && !class_name.contains("Stack")
                {
                    diag.error(
                        ErrorCode::AllocationViolation,
                        format!(
                            "[E0950] Allocation Violation: Heap allocation via object instantiation '{}' is forbidden in '@no_alloc' function '{}'",
                            class_name, ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
                for (_, f_expr) in fields {
                    self.verify_expr(f_expr, ctx, diag);
                }
            }
            Expr::Literal(_, _) | Expr::Identifier(_, _) => {}
            Expr::Block(stmts, value, _) => {
                // Arm-body block: walk the nested statements' expressions so
                // they still pass through the security gates.
                for s in stmts {
                    match s {
                        Stmt::Expr(e, _)
                        | Stmt::Out(e, _)
                        | Stmt::Err(e, _)
                        | Stmt::Let { init: e, .. }
                        | Stmt::Mut { init: e, .. }
                        | Stmt::Val { init: e, .. }
                        | Stmt::Const { init: e, .. }
                        | Stmt::CompactBind { init: e, .. } => self.verify_expr(e, ctx, diag),
                        Stmt::Assign {
                            target, value: v, ..
                        } => {
                            self.verify_expr(target, ctx, diag);
                            self.verify_expr(v, ctx, diag);
                        }
                        Stmt::Return(Some(e), _) => self.verify_expr(e, ctx, diag),
                        _ => {}
                    }
                }
                if let Some(v) = value {
                    self.verify_expr(v, ctx, diag);
                }
            }
        }
    }

    fn verify_division_gate(
        &self,
        divisor: &Expr,
        span: &SourceSpan,
        ctx: &FnContext,
        diag: &mut DiagnosticEngine,
    ) {
        match divisor {
            Expr::Binary {
                op, left, right, ..
            } if op == "*" => {
                if is_non_zero_literal(right) {
                    self.verify_division_gate(left, span, ctx, diag);
                    return;
                } else if is_non_zero_literal(left) {
                    self.verify_division_gate(right, span, ctx, diag);
                    return;
                }
            }
            Expr::Literal(LiteralValue::Int(n), _) => {
                if *n == 0 {
                    if ctx.is_no_panic {
                        diag.error(
                            ErrorCode::PanicViolation,
                            format!(
                                "[E0951] Panic Violation: Division by zero constant in '@no_panic' function '{}'",
                                ctx.fn_name
                            ),
                            Some(span.clone()),
                        );
                    }
                    diag.error(
                        ErrorCode::ProofCarryingCodeViolation,
                        "Proof-Carrying Code Violation: Unproven divisor '0' may be zero. Use 'NonZeroInt' or contract 'require != 0'".to_string(),
                        Some(span.clone()),
                    );
                }
            }
            Expr::Literal(LiteralValue::Float(f), _) => {
                if *f == 0.0 {
                    if ctx.is_no_panic {
                        diag.error(
                            ErrorCode::PanicViolation,
                            format!(
                                "[E0951] Panic Violation: Division by 0.0 in '@no_panic' function '{}'",
                                ctx.fn_name
                            ),
                            Some(span.clone()),
                        );
                    }
                    diag.error(
                        ErrorCode::ProofCarryingCodeViolation,
                        "Proof-Carrying Code Violation: Unproven divisor '0.0' may be zero. Use 'NonZeroInt' or contract 'require != 0'".to_string(),
                        Some(span.clone()),
                    );
                }
            }
            Expr::Identifier(var_name, _) => {
                let is_proven = ctx.proven_non_zero.contains(var_name)
                    || ctx
                        .requires
                        .iter()
                        .any(|r| is_proven_non_zero_cond(r, var_name))
                    || self
                        .type_checker
                        .var_refinements
                        .get(var_name)
                        .map(|tn| self.refinement_proves_non_zero(tn, var_name))
                        .unwrap_or(false)
                    || self
                        .type_checker
                        .function_param_nodes
                        .get(&ctx.fn_name)
                        .map(|params| {
                            params.iter().any(|opt_tn| {
                                if let Some(tn) = opt_tn {
                                    tn.name == "NonZeroInt"
                                        || self.refinement_proves_non_zero(tn, var_name)
                                } else {
                                    false
                                }
                            })
                        })
                        .unwrap_or(false)
                    || self
                        .type_checker
                        .symbol_types
                        .get(var_name)
                        .map(|t| t == &DataraType::Class("NonZeroInt".into()))
                        .unwrap_or(false);

                if !is_proven {
                    if ctx.is_no_panic {
                        diag.error(
                            ErrorCode::PanicViolation,
                            format!(
                                "[E0951] Panic Violation: Unproven division by zero in '@no_panic' function '{}'",
                                ctx.fn_name
                            ),
                            Some(span.clone()),
                        );
                    }
                    diag.error(
                        ErrorCode::ProofCarryingCodeViolation,
                        format!(
                            "Proof-Carrying Code Violation: Unproven divisor '{}' may be zero. Use 'NonZeroInt' or contract 'require {} != 0'",
                            var_name, var_name
                        ),
                        Some(span.clone()),
                    );
                }
            }
            _ => {
                if ctx.is_no_panic {
                    diag.error(
                        ErrorCode::PanicViolation,
                        format!(
                            "[E0951] Panic Violation: Unproven division expression may panic in '@no_panic' function '{}'",
                            ctx.fn_name
                        ),
                        Some(span.clone()),
                    );
                }
                diag.error(
                    ErrorCode::ProofCarryingCodeViolation,
                    "Proof-Carrying Code Violation: Unproven divisor expression may be zero. Use 'NonZeroInt' or contract 'require != 0'".to_string(),
                    Some(span.clone()),
                );
            }
        }
    }
}
