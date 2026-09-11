pub(crate) mod capabilities;
pub(crate) mod concurrency;
pub(crate) mod verify_expr;

use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::resolver::Resolver;
use crate::types::{DataraType, TypeChecker};
pub(crate) use capabilities::*;
use std::collections::{HashMap, HashSet};

pub struct SecurityVerifier<'a> {
    pub resolver: &'a Resolver,
    pub type_checker: &'a TypeChecker<'a>,
    pub function_decls: HashMap<String, (Vec<Param>, Vec<ContractClause>)>,
}

#[derive(Clone)]
pub(crate) struct FnContext {
    fn_name: String,
    requires: Vec<Expr>,
    symbols: HashMap<String, DataraType>,
    proven_non_zero: HashSet<String>,
    unsafe_justification: Option<String>,
    outer_vars: HashSet<String>,
    is_no_alloc: bool,
    is_no_panic: bool,
}

impl<'a> SecurityVerifier<'a> {
    pub fn new(resolver: &'a Resolver, type_checker: &'a TypeChecker<'a>) -> Self {
        Self {
            resolver,
            type_checker,
            function_decls: HashMap::new(),
        }
    }

    pub fn verify_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        self.function_decls.clear();
        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    self.function_decls
                        .insert(f.name.clone(), (f.params.clone(), f.requires.clone()));
                }
                Decl::Class(c) => {
                    for item in &c.body_items {
                        if let ClassItem::Method(m) = item {
                            self.function_decls.insert(
                                format!("{}_{}", c.name, m.name),
                                (m.params.clone(), m.requires.clone()),
                            );
                            self.function_decls
                                .insert(m.name.clone(), (m.params.clone(), m.requires.clone()));
                        }
                    }
                }
                Decl::Behavior(b) => {
                    for item in &b.body_items {
                        if let ClassItem::Method(m) = item {
                            self.function_decls.insert(
                                format!("{}_{}", b.target_type, m.name),
                                (m.params.clone(), m.requires.clone()),
                            );
                            self.function_decls
                                .insert(m.name.clone(), (m.params.clone(), m.requires.clone()));
                        }
                    }
                }
                Decl::Impl(i) => {
                    for m in &i.methods {
                        self.function_decls
                            .insert(m.name.clone(), (m.params.clone(), m.requires.clone()));
                    }
                }
                _ => {}
            }
        }

        let mut class_invariants: HashMap<String, Vec<Expr>> = HashMap::new();
        for decl in &program.declarations {
            if let Decl::Class(c) = decl
                && !c.invariants.is_empty()
            {
                class_invariants.insert(c.name.clone(), c.invariants.clone());
            }
        }

        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    self.verify_fn_decl(f, diag);
                }
                Decl::Class(c) => {
                    let invs = class_invariants.get(&c.name).cloned().unwrap_or_default();
                    for item in &c.body_items {
                        if let ClassItem::Method(m) = item {
                            self.verify_method_decl(&c.name, m, &invs, diag);
                        }
                    }
                }
                Decl::Behavior(b) => {
                    let invs = class_invariants
                        .get(&b.target_type)
                        .cloned()
                        .unwrap_or_default();
                    for item in &b.body_items {
                        if let ClassItem::Method(m) = item {
                            self.verify_method_decl(&b.target_type, m, &invs, diag);
                        }
                    }
                }
                Decl::Impl(i) => {
                    for m in &i.methods {
                        self.verify_fn_decl(m, diag);
                    }
                }
                _ => {}
            }
        }
    }

    fn verify_fn_decl(&mut self, f: &FunctionDecl, diag: &mut DiagnosticEngine) {
        let mut symbols = HashMap::new();
        let mut proven_non_zero = HashSet::new();
        let mut outer_vars = HashSet::new();

        for p in &f.params {
            let p_ty = p
                .type_node
                .as_ref()
                .map(|t| self.type_checker.resolve_type_node(t, diag))
                .unwrap_or(DataraType::Int);
            symbols.insert(p.name.clone(), p_ty);
            outer_vars.insert(p.name.clone());

            // Check if parameter has refinement proving non-zero
            if let Some(tn) = &p.type_node
                && (tn.name == "NonZeroInt" || self.refinement_proves_non_zero(tn, &p.name))
            {
                proven_non_zero.insert(p.name.clone());
            }
        }

        let requires: Vec<Expr> = f.requires.iter().map(|r| r.condition.clone()).collect();
        for req in &requires {
            for p in &f.params {
                if is_proven_non_zero_cond(req, &p.name) {
                    proven_non_zero.insert(p.name.clone());
                }
            }
        }

        let is_no_alloc = f.attributes.iter().any(|a| a.name == "no_alloc");
        let is_no_panic = f.attributes.iter().any(|a| a.name == "no_panic");

        let mut ctx = FnContext {
            fn_name: f.name.clone(),
            requires,
            symbols,
            proven_non_zero,
            unsafe_justification: None,
            outer_vars,
            is_no_alloc,
            is_no_panic,
        };

        self.verify_totality_and_termination(
            &f.name,
            &f.attributes,
            &f.params,
            &f.decreases,
            &f.body,
            diag,
        );

        self.verify_stmt(&f.body, &mut ctx, diag);
    }

    fn verify_method_decl(
        &mut self,
        target: &str,
        m: &MethodDecl,
        invariants: &[Expr],
        diag: &mut DiagnosticEngine,
    ) {
        let Some(body) = &m.body else { return };

        self.verify_totality_and_termination(
            &m.name,
            &m.attributes,
            &m.params,
            &m.decreases,
            body,
            diag,
        );

        if !invariants.is_empty() {
            self.verify_class_invariants(target, m, invariants, body, diag);
        }

        let fn_name = format!("{}_{}", target, m.name);
        let mut symbols = HashMap::new();
        let mut proven_non_zero = HashSet::new();
        let mut outer_vars = HashSet::new();

        symbols.insert("this".into(), DataraType::Class(target.to_string()));
        outer_vars.insert("this".into());

        if let Some(cls_sym) = self.type_checker.resolver.classes.get(target) {
            for (f_name, f_sym) in &cls_sym.fields {
                let f_ty = f_sym
                    .type_node
                    .as_ref()
                    .map(|t| self.type_checker.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Int);
                symbols.insert(f_name.clone(), f_ty);
                outer_vars.insert(f_name.clone());
            }
        }

        for p in &m.params {
            let p_ty = p
                .type_node
                .as_ref()
                .map(|t| self.type_checker.resolve_type_node(t, diag))
                .unwrap_or(DataraType::Int);
            symbols.insert(p.name.clone(), p_ty);
            outer_vars.insert(p.name.clone());

            if let Some(tn) = &p.type_node
                && (tn.name == "NonZeroInt" || self.refinement_proves_non_zero(tn, &p.name))
            {
                proven_non_zero.insert(p.name.clone());
            }
        }

        let requires: Vec<Expr> = m.requires.iter().map(|r| r.condition.clone()).collect();
        for req in &requires {
            for p in &m.params {
                if is_proven_non_zero_cond(req, &p.name) {
                    proven_non_zero.insert(p.name.clone());
                }
            }
        }

        let is_no_alloc = m.attributes.iter().any(|a| a.name == "no_alloc");
        let is_no_panic = m.attributes.iter().any(|a| a.name == "no_panic");

        let mut ctx = FnContext {
            fn_name,
            requires,
            symbols,
            proven_non_zero,
            unsafe_justification: None,
            outer_vars,
            is_no_alloc,
            is_no_panic,
        };

        self.verify_stmt(body, &mut ctx, diag);
    }

    fn verify_class_invariants(
        &self,
        class_name: &str,
        m: &MethodDecl,
        invariants: &[Expr],
        body: &Stmt,
        diag: &mut DiagnosticEngine,
    ) {
        let mut assignments = Vec::new();
        Self::collect_assignments(body, &mut assignments);

        for inv in invariants {
            if let Expr::Binary {
                op, left, right, ..
            } = inv
            {
                let field_name = match &**left {
                    Expr::Identifier(id, _) => Some(id.clone()),
                    Expr::MemberAccess { member, .. } => Some(member.clone()),
                    _ => None,
                };

                let Some(f_name) = field_name else {
                    continue;
                };

                for (target_expr, val_expr, assign_span) in &assignments {
                    let is_target_field = match target_expr {
                        Expr::Identifier(id, _) => id == &f_name,
                        Expr::MemberAccess { member, .. } => member == &f_name,
                        _ => false,
                    };

                    if !is_target_field {
                        continue;
                    }

                    // 1. Literal constant assignment check
                    let val_int = match val_expr {
                        Expr::Literal(LiteralValue::Int(n), _) => Some(*n),
                        Expr::Unary { op: u_op, expr, .. } if u_op == "-" => {
                            if let Expr::Literal(LiteralValue::Int(n), _) = &**expr {
                                n.checked_neg()
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(n) = val_int {
                        let violates = match op.as_str() {
                            ">=" => match &**right {
                                Expr::Literal(LiteralValue::Int(limit), _) => n < *limit,
                                _ => n < 0,
                            },
                            ">" => match &**right {
                                Expr::Literal(LiteralValue::Int(limit), _) => n <= *limit,
                                _ => n <= 0,
                            },
                            "<=" => match &**right {
                                Expr::Literal(LiteralValue::Int(limit), _) => n > *limit,
                                _ => false,
                            },
                            "<" => match &**right {
                                Expr::Literal(LiteralValue::Int(limit), _) => n >= *limit,
                                _ => false,
                            },
                            _ => false,
                        };

                        if violates {
                            diag.error_with_help(
                                ErrorCode::InvariantViolation,
                                format!(
                                    "Class '{}' invariant violation: method '{}' assigns invalid constant {} to field '{}'",
                                    class_name, m.name, n, f_name
                                ),
                                Some(assign_span.clone()),
                                Some("Ensure assigned value satisfies class invariant".to_string()),
                            );
                        }
                    }

                    // 2. Subtraction check (e.g. self.field = self.field - amount)
                    if (op == ">=" || op == ">")
                        && let Expr::Binary { op: sub_op, .. } = val_expr
                        && sub_op == "-"
                    {
                        let has_precondition = m.requires.iter().any(|req| {
                            let mut reads = HashSet::new();
                            collect_expr_reads(
                                &req.condition,
                                &HashSet::new(),
                                &HashSet::new(),
                                &mut reads,
                            );
                            reads.contains(&f_name)
                                || reads.contains("self")
                                || reads.contains("this")
                        });

                        if !has_precondition {
                            diag.error_with_help(
                                ErrorCode::InvariantViolation,
                                format!(
                                    "Class '{}' invariant violation: method '{}' decrements field '{}' without a contract precondition ensuring the invariant holds",
                                    class_name, m.name, f_name
                                ),
                                Some(assign_span.clone()),
                                Some(format!(
                                    "Add 'require' clause to method '{}' to guarantee invariant preservation",
                                    m.name
                                )),
                            );
                        }
                    }
                }
            }
        }
    }

    fn collect_assignments<'b>(stmt: &'b Stmt, out: &mut Vec<(&'b Expr, &'b Expr, SourceSpan)>) {
        match stmt {
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                out.push((target, value, span.clone()));
            }
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    Self::collect_assignments(s, out);
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_assignments(then_branch, out);
                if let Some(eb) = else_branch {
                    Self::collect_assignments(eb, out);
                }
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } => {
                Self::collect_assignments(body, out);
            }
            Stmt::For { body, .. } => {
                Self::collect_assignments(body, out);
            }
            Stmt::Parallel(body, _) | Stmt::ParallelFor { body, .. } | Stmt::With { body, .. } => {
                Self::collect_assignments(body, out);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::collect_assignments(try_block, out);
                Self::collect_assignments(catch_block, out);
            }
            Stmt::Unsafe { body, .. } => {
                Self::collect_assignments(body, out);
            }
            _ => {}
        }
    }

    fn verify_totality_and_termination(
        &self,
        fn_name: &str,
        attributes: &[Attribute],
        params: &[Param],
        decreases: &Option<Expr>,
        body: &Stmt,
        diag: &mut DiagnosticEngine,
    ) {
        let is_pure = attributes.iter().any(|a| a.name == "pure");
        if !is_pure && decreases.is_none() {
            return;
        }

        // 1. Verify loops terminate
        Self::check_loops_termination(body, is_pure, diag);

        // 2. Verify recursion termination metric
        let mut recursive_calls = Vec::new();
        Self::collect_recursive_calls(body, fn_name, &mut recursive_calls);

        if !recursive_calls.is_empty() {
            if decreases.is_none() && is_pure {
                diag.error_with_help(
                    ErrorCode::TerminationViolation,
                    format!(
                        "Recursive pure function '{}' requires a 'decreases <metric>' annotation to guarantee termination",
                        fn_name
                    ),
                    Some(recursive_calls[0].1.clone()),
                    Some("Specify a termination metric, e.g. 'decreases n'".to_string()),
                );
            } else if let Some(dec_expr) = decreases {
                let metric_name = match dec_expr {
                    Expr::Identifier(id, _) => Some(id.as_str()),
                    _ => None,
                };

                if let Some(m_name) = metric_name
                    && let Some(param_idx) = params.iter().position(|p| p.name == m_name)
                {
                    for (call_args, call_span) in &recursive_calls {
                        if let Some(arg) = call_args.get(param_idx) {
                            let decreases_ok = match arg {
                                Expr::Binary {
                                    op, left, right, ..
                                } if op == "-" => {
                                    if let Expr::Identifier(id, _) = &**left {
                                        id == m_name
                                            && match &**right {
                                                Expr::Literal(LiteralValue::Int(k), _) => *k > 0,
                                                _ => true,
                                            }
                                    } else {
                                        false
                                    }
                                }
                                _ => false,
                            };

                            if !decreases_ok {
                                diag.error_with_help(
                                        ErrorCode::TerminationViolation,
                                        format!(
                                            "Recursive call in function '{}' does not strictly decrease termination metric '{}'",
                                            fn_name, m_name
                                        ),
                                        Some(call_span.clone()),
                                        Some(format!(
                                            "Pass a strictly decreasing metric, e.g. '{} - 1'",
                                            m_name
                                        )),
                                    );
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_loops_termination(stmt: &Stmt, is_pure: bool, diag: &mut DiagnosticEngine) {
        match stmt {
            Stmt::Loop { span, body } => {
                if is_pure {
                    diag.error_with_help(
                        ErrorCode::TerminationViolation,
                        "Unconditional 'loop' construct in pure function violates totality/termination guarantees".to_string(),
                        Some(span.clone()),
                        Some("Use a terminating for loop or while loop with a decreasing metric".to_string()),
                    );
                }
                Self::check_loops_termination(body, is_pure, diag);
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                if is_pure && let Expr::Literal(LiteralValue::Bool(true), _) = condition {
                    diag.error_with_help(
                            ErrorCode::TerminationViolation,
                            "Infinite while(true) loop in pure function violates totality/termination guarantees".to_string(),
                            Some(span.clone()),
                            Some("Ensure loop condition terminates".to_string()),
                        );
                }
                Self::check_loops_termination(body, is_pure, diag);
            }
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    Self::check_loops_termination(s, is_pure, diag);
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::check_loops_termination(then_branch, is_pure, diag);
                if let Some(eb) = else_branch {
                    Self::check_loops_termination(eb, is_pure, diag);
                }
            }
            Stmt::For { body, .. }
            | Stmt::Parallel(body, _)
            | Stmt::ParallelFor { body, .. }
            | Stmt::With { body, .. } => {
                Self::check_loops_termination(body, is_pure, diag);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::check_loops_termination(try_block, is_pure, diag);
                Self::check_loops_termination(catch_block, is_pure, diag);
            }
            Stmt::Unsafe { body, .. } => {
                Self::check_loops_termination(body, is_pure, diag);
            }
            _ => {}
        }
    }

    fn collect_recursive_calls<'b>(
        stmt: &'b Stmt,
        fn_name: &str,
        out: &mut Vec<(&'b Vec<Expr>, SourceSpan)>,
    ) {
        match stmt {
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    Self::collect_recursive_calls(s, fn_name, out);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_expr_calls(condition, fn_name, out);
                Self::collect_recursive_calls(then_branch, fn_name, out);
                if let Some(eb) = else_branch {
                    Self::collect_recursive_calls(eb, fn_name, out);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                Self::collect_expr_calls(condition, fn_name, out);
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::For { iterable, body, .. } => {
                Self::collect_expr_calls(iterable, fn_name, out);
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::Parallel(body, _) => {
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::ParallelFor { iterable, body, .. } => {
                Self::collect_expr_calls(iterable, fn_name, out);
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::With { init, body, .. } => {
                Self::collect_expr_calls(init, fn_name, out);
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::collect_recursive_calls(try_block, fn_name, out);
                Self::collect_recursive_calls(catch_block, fn_name, out);
            }
            Stmt::Loop { body, .. } => {
                Self::collect_recursive_calls(body, fn_name, out);
            }
            Stmt::Return(Some(expr), _) => {
                Self::collect_expr_calls(expr, fn_name, out);
            }
            Stmt::Expr(expr, _) | Stmt::Out(expr, _) | Stmt::Err(expr, _) => {
                Self::collect_expr_calls(expr, fn_name, out);
            }
            Stmt::Let { init, .. }
            | Stmt::Const { init, .. }
            | Stmt::Mut { init, .. }
            | Stmt::Val { init, .. }
            | Stmt::CompactBind { init, .. } => {
                Self::collect_expr_calls(init, fn_name, out);
            }
            Stmt::Assign { target, value, .. } => {
                Self::collect_expr_calls(target, fn_name, out);
                Self::collect_expr_calls(value, fn_name, out);
            }
            Stmt::Unsafe { body, .. } => {
                Self::collect_recursive_calls(body, fn_name, out);
            }
            _ => {}
        }
    }

    fn collect_expr_calls<'b>(
        expr: &'b Expr,
        fn_name: &str,
        out: &mut Vec<(&'b Vec<Expr>, SourceSpan)>,
    ) {
        match expr {
            Expr::Call { callee, args, span } => {
                let is_match = match &**callee {
                    Expr::Identifier(id, _) => id == fn_name,
                    Expr::MemberAccess { member, .. } => member == fn_name,
                    _ => false,
                };
                if is_match {
                    out.push((args, span.clone()));
                }
                for a in args {
                    Self::collect_expr_calls(a, fn_name, out);
                }
            }
            Expr::Binary { left, right, .. } => {
                Self::collect_expr_calls(left, fn_name, out);
                Self::collect_expr_calls(right, fn_name, out);
            }
            Expr::Unary { expr, .. } => {
                Self::collect_expr_calls(expr, fn_name, out);
            }
            Expr::Match { value, arms, .. } => {
                Self::collect_expr_calls(value, fn_name, out);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        Self::collect_expr_calls(g, fn_name, out);
                    }
                    Self::collect_expr_calls(&arm.body, fn_name, out);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for arm in arms {
                    Self::collect_expr_calls(&arm.condition, fn_name, out);
                    Self::collect_expr_calls(&arm.body, fn_name, out);
                }
                if let Some(eb) = else_arm {
                    Self::collect_expr_calls(eb, fn_name, out);
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for arm in arms {
                    Self::collect_expr_calls(&arm.condition, fn_name, out);
                    Self::collect_expr_calls(&arm.body, fn_name, out);
                }
                if let Some(eb) = else_arm {
                    Self::collect_expr_calls(eb, fn_name, out);
                }
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    Self::collect_expr_calls(s, fn_name, out);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    Self::collect_expr_calls(e, fn_name, out);
                }
            }
            Expr::ObjectInit { fields, .. } => {
                for (_, f_expr) in fields {
                    Self::collect_expr_calls(f_expr, fn_name, out);
                }
            }
            Expr::MemberAccess { object, .. } => {
                Self::collect_expr_calls(object, fn_name, out);
            }
            Expr::IndexAccess { object, index, .. } => {
                Self::collect_expr_calls(object, fn_name, out);
                Self::collect_expr_calls(index, fn_name, out);
            }
            Expr::Range { start, end, .. } => {
                Self::collect_expr_calls(start, fn_name, out);
                Self::collect_expr_calls(end, fn_name, out);
            }
            Expr::ErrorPropagate(inner, _) => {
                Self::collect_expr_calls(inner, fn_name, out);
            }
            Expr::Lambda { body, .. } => {
                Self::collect_expr_calls(body, fn_name, out);
            }
            Expr::ListLiteral(items, _) => {
                for item in items {
                    Self::collect_expr_calls(item, fn_name, out);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    Self::collect_expr_calls(k, fn_name, out);
                    Self::collect_expr_calls(v, fn_name, out);
                }
            }
            Expr::Tuple(items, _) => {
                for item in items {
                    Self::collect_expr_calls(item, fn_name, out);
                }
            }
            Expr::Block(stmts, value, _) => {
                for s in stmts {
                    Self::collect_recursive_calls(s, fn_name, out);
                }
                if let Some(v) = value {
                    Self::collect_expr_calls(v, fn_name, out);
                }
            }
            _ => {}
        }
    }

    fn refinement_proves_non_zero(&self, tn: &TypeNode, var_name: &str) -> bool {
        if tn.name == "NonZeroInt" || tn.name == "NonZero" {
            return true;
        }
        if let Some(alias) = self.resolver.type_aliases.get(&tn.name)
            && self.refinement_proves_non_zero(&alias.base_type, var_name)
        {
            return true;
        }
        if let Some(ref ref_kind) = tn.refinement {
            match ref_kind {
                Refinement::Predicate {
                    var_name: ref_var,
                    predicate,
                } => {
                    if is_proven_non_zero_cond(predicate, ref_var)
                        || is_proven_non_zero_cond(predicate, "val")
                        || is_proven_non_zero_cond(predicate, var_name)
                    {
                        return true;
                    }
                }
                Refinement::Range {
                    start, inclusive, ..
                } => match start.as_ref() {
                    Expr::Literal(LiteralValue::Int(n), _) => {
                        if *n > 0 || (*inclusive && *n >= 1) {
                            return true;
                        }
                    }
                    Expr::Literal(LiteralValue::Float(f), _) if *f > 0.0 => {
                        return true;
                    }
                    _ => {}
                },
            }
        }
        false
    }

    fn verify_stmt(&mut self, stmt: &Stmt, ctx: &mut FnContext, diag: &mut DiagnosticEngine) {
        match stmt {
            Stmt::Block(stmts, _) => {
                let prev_symbols = ctx.symbols.clone();
                let prev_proven = ctx.proven_non_zero.clone();
                let prev_outer = ctx.outer_vars.clone();

                for s in stmts {
                    self.verify_stmt(s, ctx, diag);
                }

                ctx.symbols = prev_symbols;
                ctx.proven_non_zero = prev_proven;
                ctx.outer_vars = prev_outer;
            }
            Stmt::Let {
                name,
                type_node,
                init,
                ..
            }
            | Stmt::Const {
                name,
                type_node,
                init,
                ..
            }
            | Stmt::Val {
                name,
                type_node,
                init,
                ..
            }
            | Stmt::Mut {
                name,
                type_node,
                init,
                ..
            } => {
                self.verify_expr(init, ctx, diag);

                let init_ty = if let Some(tn) = type_node {
                    self.type_checker.resolve_type_node(tn, diag)
                } else if let Some(ty) = self.type_checker.symbol_types.get(name) {
                    ty.clone()
                } else {
                    DataraType::Int
                };

                ctx.symbols.insert(name.clone(), init_ty);
                ctx.outer_vars.insert(name.clone());

                // Proven non-zero tracking
                let mut proven = false;
                if let Some(tn) = type_node
                    && (tn.name == "NonZeroInt" || self.refinement_proves_non_zero(tn, name))
                {
                    proven = true;
                }
                if is_non_zero_literal(init) {
                    proven = true;
                } else if let Expr::Identifier(from_var, _) = init {
                    if ctx.proven_non_zero.contains(from_var) {
                        proven = true;
                    }
                }
                if proven {
                    ctx.proven_non_zero.insert(name.clone());
                } else {
                    ctx.proven_non_zero.remove(name);
                }
            }
            Stmt::Assign {
                target,
                value,
                span: _,
            } => {
                self.verify_expr(target, ctx, diag);
                self.verify_expr(value, ctx, diag);

                if let Expr::Identifier(var_name, _) = target {
                    if is_non_zero_literal(value) {
                        ctx.proven_non_zero.insert(var_name.clone());
                    } else if let Expr::Identifier(from_var, _) = value
                        && ctx.proven_non_zero.contains(from_var)
                    {
                        ctx.proven_non_zero.insert(var_name.clone());
                    } else if !ctx
                        .requires
                        .iter()
                        .any(|r| is_proven_non_zero_cond(r, var_name))
                    {
                        ctx.proven_non_zero.remove(var_name);
                    }
                }
            }
            Stmt::Expr(expr, _) => {
                // If this expression is a contract / assertion: `require b != 0` or `assert(b != 0)`
                if let Expr::Call { callee, args, .. } = expr
                    && let Expr::Identifier(cname, _) = &**callee
                    && (cname == "require" || cname == "assert")
                    && !args.is_empty()
                {
                    for var_name in ctx.symbols.keys().cloned().collect::<Vec<_>>() {
                        if is_proven_non_zero_cond(&args[0], &var_name) {
                            ctx.proven_non_zero.insert(var_name);
                        }
                    }
                }
                self.verify_expr(expr, ctx, diag);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.verify_expr(condition, ctx, diag);

                // In then_branch, any variable guarded by condition != 0 is proven
                let mut then_ctx = ctx.clone();
                for var_name in ctx.symbols.keys() {
                    if is_proven_non_zero_cond(condition, var_name) {
                        then_ctx.proven_non_zero.insert(var_name.clone());
                    }
                }
                self.verify_stmt(then_branch, &mut then_ctx, diag);

                if let Some(eb) = else_branch {
                    let mut else_ctx = ctx.clone();
                    for var_name in ctx.symbols.keys() {
                        if is_equality_zero_cond(condition, var_name) {
                            else_ctx.proven_non_zero.insert(var_name.clone());
                        }
                    }
                    self.verify_stmt(eb, &mut else_ctx, diag);
                } else if stmt_always_returns(then_branch) {
                    for var_name in ctx.symbols.keys().cloned().collect::<Vec<_>>() {
                        if is_equality_zero_cond(condition, &var_name) {
                            ctx.proven_non_zero.insert(var_name);
                        }
                    }
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.verify_expr(condition, ctx, diag);
                let mut loop_ctx = ctx.clone();
                for var_name in ctx.symbols.keys() {
                    if is_proven_non_zero_cond(condition, var_name) {
                        loop_ctx.proven_non_zero.insert(var_name.clone());
                    }
                }
                self.verify_stmt(body, &mut loop_ctx, diag);
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            } => {
                self.verify_expr(iterable, ctx, diag);
                let mut for_ctx = ctx.clone();
                for_ctx.symbols.insert(var_name.clone(), DataraType::Int);
                for_ctx.outer_vars.insert(var_name.clone());
                self.verify_stmt(body, &mut for_ctx, diag);
            }
            Stmt::Loop { body, .. } => {
                let mut loop_ctx = ctx.clone();
                self.verify_stmt(body, &mut loop_ctx, diag);
            }
            Stmt::Parallel(body, _) => {
                // Concurrency Violation Gate: data race check
                if ctx.unsafe_justification.is_none() {
                    self.check_parallel_block_data_race(body, &ctx.outer_vars, diag);
                }
                self.verify_stmt(body, ctx, diag);
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                self.verify_expr(iterable, ctx, diag);
                // Concurrency Violation Gate: data race check
                if ctx.unsafe_justification.is_none() {
                    self.check_parallel_data_race(body, Some(var_name), &ctx.outer_vars, diag);
                }

                let mut pfor_ctx = ctx.clone();
                pfor_ctx.symbols.insert(var_name.clone(), DataraType::Int);
                pfor_ctx.outer_vars.insert(var_name.clone());
                self.verify_stmt(body, &mut pfor_ctx, diag);
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                self.verify_expr(init, ctx, diag);
                let mut with_ctx = ctx.clone();
                with_ctx
                    .symbols
                    .insert(resource_name.clone(), DataraType::Int);
                with_ctx.outer_vars.insert(resource_name.clone());
                self.verify_stmt(body, &mut with_ctx, diag);
            }
            Stmt::Unsafe {
                justification,
                body,
                ..
            } => {
                let prev_just = ctx.unsafe_justification.clone();
                ctx.unsafe_justification = justification.clone();
                self.verify_stmt(body, ctx, diag);
                ctx.unsafe_justification = prev_just;
            }
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    self.verify_expr(e, ctx, diag);
                }
            }
            Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.verify_expr(e, ctx, diag);
            }
            Stmt::CompactBind { name, init, .. } => {
                self.verify_expr(init, ctx, diag);
                ctx.symbols.insert(name.clone(), DataraType::Int);
                ctx.outer_vars.insert(name.clone());
                if is_non_zero_literal(init) {
                    ctx.proven_non_zero.insert(name.clone());
                }
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                ..
            } => {
                self.verify_stmt(try_block, ctx, diag);
                let mut catch_ctx = ctx.clone();
                catch_ctx
                    .symbols
                    .insert(err_var.clone(), DataraType::String);
                catch_ctx.outer_vars.insert(err_var.clone());
                self.verify_stmt(catch_block, &mut catch_ctx, diag);
            }
            Stmt::Asm { .. } => {}
        }
    }
}
