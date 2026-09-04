use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::resolver::Resolver;
use crate::types::{DataraType, TypeChecker};
use std::collections::{HashMap, HashSet};

pub struct SecurityVerifier<'a> {
    pub resolver: &'a Resolver,
    pub type_checker: &'a TypeChecker<'a>,
}

#[derive(Clone)]
struct FnContext {
    fn_name: String,
    requires: Vec<Expr>,
    symbols: HashMap<String, DataraType>,
    proven_non_zero: HashSet<String>,
    unsafe_justification: Option<String>,
    outer_vars: HashSet<String>,
}

impl<'a> SecurityVerifier<'a> {
    pub fn new(resolver: &'a Resolver, type_checker: &'a TypeChecker<'a>) -> Self {
        Self {
            resolver,
            type_checker,
        }
    }

    pub fn verify_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    self.verify_fn_decl(f, diag);
                }
                Decl::Class(c) => {
                    for item in &c.body_items {
                        if let ClassItem::Method(m) = item {
                            self.verify_method_decl(&c.name, m, diag);
                        }
                    }
                }
                Decl::Behavior(b) => {
                    for item in &b.body_items {
                        if let ClassItem::Method(m) = item {
                            self.verify_method_decl(&b.target_type, m, diag);
                        }
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
                .map(|t| self.type_checker.resolve_type_node(t))
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

        let mut ctx = FnContext {
            fn_name: f.name.clone(),
            requires,
            symbols,
            proven_non_zero,
            unsafe_justification: None,
            outer_vars,
        };

        self.verify_stmt(&f.body, &mut ctx, diag);
    }

    fn verify_method_decl(&mut self, target: &str, m: &MethodDecl, diag: &mut DiagnosticEngine) {
        let Some(body) = &m.body else { return };

        let fn_name = format!("{}_{}", target, m.name);
        let mut symbols = HashMap::new();
        let mut proven_non_zero = HashSet::new();
        let mut outer_vars = HashSet::new();

        symbols.insert("this".into(), DataraType::Class(target.to_string()));
        outer_vars.insert("this".into());

        for p in &m.params {
            let p_ty = p
                .type_node
                .as_ref()
                .map(|t| self.type_checker.resolve_type_node(t))
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

        let mut ctx = FnContext {
            fn_name,
            requires,
            symbols,
            proven_non_zero,
            unsafe_justification: None,
            outer_vars,
        };

        self.verify_stmt(body, &mut ctx, diag);
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
                    self.type_checker.resolve_type_node(tn)
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
                    self.verify_stmt(eb, &mut else_ctx, diag);
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
        }
    }

    fn check_parallel_block_data_race(
        &self,
        body: &Stmt,
        outer_vars: &HashSet<String>,
        diag: &mut DiagnosticEngine,
    ) {
        let Stmt::Block(stmts, _) = body else {
            return;
        };

        if stmts.len() <= 1 {
            return;
        }

        let mut branch_effects = Vec::new();
        for s in stmts {
            let mut reads = HashSet::new();
            let mut writes = HashMap::new();
            let mut local_declared = HashSet::new();
            self.collect_branch_reads_writes(
                s,
                outer_vars,
                &mut local_declared,
                &mut reads,
                &mut writes,
            );
            branch_effects.push((reads, writes));
        }

        for i in 0..branch_effects.len() {
            for j in (i + 1)..branch_effects.len() {
                let (reads_i, writes_i) = &branch_effects[i];
                let (reads_j, writes_j) = &branch_effects[j];

                // Write-Write conflict
                for (var, span) in writes_i {
                    if writes_j.contains_key(var) {
                        diag.error(
                            ErrorCode::DataRaceViolation,
                            format!(
                                "Concurrency Violation: Potential data race on mutable variable '{}' accessed concurrently across threads",
                                var
                            ),
                            Some(span.clone()),
                        );
                    }
                }

                // Write-Read conflict
                for (var, span) in writes_i {
                    if reads_j.contains(var) {
                        diag.error(
                            ErrorCode::DataRaceViolation,
                            format!(
                                "Concurrency Violation: Potential data race on mutable variable '{}' accessed concurrently across threads",
                                var
                            ),
                            Some(span.clone()),
                        );
                    }
                }

                // Read-Write conflict
                for (var, span) in writes_j {
                    if reads_i.contains(var) {
                        diag.error(
                            ErrorCode::DataRaceViolation,
                            format!(
                                "Concurrency Violation: Potential data race on mutable variable '{}' accessed concurrently across threads",
                                var
                            ),
                            Some(span.clone()),
                        );
                    }
                }
            }
        }
    }

    fn collect_branch_reads_writes(
        &self,
        stmt: &Stmt,
        outer_vars: &HashSet<String>,
        local_declared: &mut HashSet<String>,
        reads: &mut HashSet<String>,
        writes: &mut HashMap<String, SourceSpan>,
    ) {
        match stmt {
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    self.collect_branch_reads_writes(s, outer_vars, local_declared, reads, writes);
                }
            }
            Stmt::Let { name, init, .. }
            | Stmt::Const { name, init, .. }
            | Stmt::Val { name, init, .. } => {
                local_declared.insert(name.clone());
                collect_expr_reads(init, outer_vars, local_declared, reads);
            }
            Stmt::Mut {
                name, init, span, ..
            } => {
                if outer_vars.contains(name) && !local_declared.contains(name) {
                    writes.insert(name.clone(), span.clone());
                }
                local_declared.insert(name.clone());
                collect_expr_reads(init, outer_vars, local_declared, reads);
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                if let Expr::Identifier(name, _) = target
                    && outer_vars.contains(name)
                    && !local_declared.contains(name)
                {
                    writes.insert(name.clone(), span.clone());
                }
                collect_expr_reads(value, outer_vars, local_declared, reads);
            }
            Stmt::Expr(expr, _) | Stmt::Out(expr, _) | Stmt::Err(expr, _) => {
                collect_expr_reads(expr, outer_vars, local_declared, reads);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_expr_reads(condition, outer_vars, local_declared, reads);
                let mut then_declared = local_declared.clone();
                self.collect_branch_reads_writes(
                    then_branch,
                    outer_vars,
                    &mut then_declared,
                    reads,
                    writes,
                );
                if let Some(eb) = else_branch {
                    let mut else_declared = local_declared.clone();
                    self.collect_branch_reads_writes(
                        eb,
                        outer_vars,
                        &mut else_declared,
                        reads,
                        writes,
                    );
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_expr_reads(condition, outer_vars, local_declared, reads);
                self.collect_branch_reads_writes(body, outer_vars, local_declared, reads, writes);
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            } => {
                collect_expr_reads(iterable, outer_vars, local_declared, reads);
                let mut for_declared = local_declared.clone();
                for_declared.insert(var_name.clone());
                self.collect_branch_reads_writes(
                    body,
                    outer_vars,
                    &mut for_declared,
                    reads,
                    writes,
                );
            }
            Stmt::Loop { body, .. } => {
                self.collect_branch_reads_writes(body, outer_vars, local_declared, reads, writes);
            }
            Stmt::Unsafe {
                justification,
                body,
                ..
            } => {
                let justified = match justification {
                    Some(j) => !j.trim().is_empty(),
                    None => false,
                };
                if !justified {
                    self.collect_branch_reads_writes(
                        body,
                        outer_vars,
                        local_declared,
                        reads,
                        writes,
                    );
                }
            }
            Stmt::Return(Some(e), _) => {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
            _ => {}
        }
    }

    fn check_parallel_data_race(
        &self,
        body: &Stmt,
        loop_var: Option<&str>,
        outer_vars: &HashSet<String>,
        diag: &mut DiagnosticEngine,
    ) {
        let mut inner_declared = HashSet::new();
        if let Some(lv) = loop_var {
            inner_declared.insert(lv.to_string());
        }

        self.collect_and_check_data_race(body, outer_vars, &mut inner_declared, diag);
    }

    fn collect_and_check_data_race(
        &self,
        stmt: &Stmt,
        outer_vars: &HashSet<String>,
        inner_declared: &mut HashSet<String>,
        diag: &mut DiagnosticEngine,
    ) {
        match stmt {
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    self.collect_and_check_data_race(s, outer_vars, inner_declared, diag);
                }
            }
            Stmt::Let { name, .. } | Stmt::Const { name, .. } | Stmt::Val { name, .. } => {
                inner_declared.insert(name.clone());
            }
            Stmt::Mut { name, span, .. } => {
                if outer_vars.contains(name) && !inner_declared.contains(name) {
                    diag.error(
                        ErrorCode::DataRaceViolation,
                        format!(
                            "Concurrency Violation: Potential data race on mutable variable '{}' accessed concurrently across threads",
                            name
                        ),
                        Some(span.clone()),
                    );
                }
                inner_declared.insert(name.clone());
            }
            Stmt::Assign { target, span, .. } => {
                if let Expr::Identifier(name, _) = target
                    && outer_vars.contains(name)
                    && !inner_declared.contains(name)
                {
                    diag.error(
                        ErrorCode::DataRaceViolation,
                        format!(
                            "Concurrency Violation: Potential data race on mutable variable '{}' accessed concurrently across threads",
                            name
                        ),
                        Some(span.clone()),
                    );
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut then_declared = inner_declared.clone();
                self.collect_and_check_data_race(then_branch, outer_vars, &mut then_declared, diag);
                if let Some(eb) = else_branch {
                    let mut else_declared = inner_declared.clone();
                    self.collect_and_check_data_race(eb, outer_vars, &mut else_declared, diag);
                }
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } => {
                self.collect_and_check_data_race(body, outer_vars, inner_declared, diag);
            }
            Stmt::Unsafe {
                justification,
                body,
                ..
            } => {
                let justified = match justification {
                    Some(j) => !j.trim().is_empty(),
                    None => false,
                };
                if !justified {
                    self.collect_and_check_data_race(body, outer_vars, inner_declared, diag);
                }
            }
            Stmt::For { var_name, body, .. } => {
                let mut for_declared = inner_declared.clone();
                for_declared.insert(var_name.clone());
                self.collect_and_check_data_race(body, outer_vars, &mut for_declared, diag);
            }
            Stmt::Parallel(body, _) => {
                self.collect_and_check_data_race(body, outer_vars, inner_declared, diag);
            }
            Stmt::ParallelFor { var_name, body, .. } => {
                let mut pfor_declared = inner_declared.clone();
                pfor_declared.insert(var_name.clone());
                self.collect_and_check_data_race(body, outer_vars, &mut pfor_declared, diag);
            }
            Stmt::With {
                resource_name,
                body,
                ..
            } => {
                let mut with_declared = inner_declared.clone();
                with_declared.insert(resource_name.clone());
                self.collect_and_check_data_race(body, outer_vars, &mut with_declared, diag);
            }
            Stmt::CompactBind { name, .. } => {
                inner_declared.insert(name.clone());
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                ..
            } => {
                self.collect_and_check_data_race(try_block, outer_vars, inner_declared, diag);
                let mut catch_declared = inner_declared.clone();
                catch_declared.insert(err_var.clone());
                self.collect_and_check_data_race(
                    catch_block,
                    outer_vars,
                    &mut catch_declared,
                    diag,
                );
            }
            Stmt::Expr(_, _) | Stmt::Return(_, _) | Stmt::Out(_, _) | Stmt::Err(_, _) => {}
        }
    }

    fn verify_expr(&mut self, expr: &Expr, ctx: &mut FnContext, diag: &mut DiagnosticEngine) {
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
            }
            Expr::Call { callee, args, span } => {
                self.verify_expr(callee, ctx, diag);
                for a in args {
                    self.verify_expr(a, ctx, diag);
                }

                if let Expr::Identifier(callee_name, callee_span) = &**callee {
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
            }
            Expr::MemberAccess { object, member, .. } => {
                self.verify_expr(object, ctx, diag);
                let _ = member;
            }
            Expr::Unary { expr, .. } => {
                self.verify_expr(expr, ctx, diag);
            }
            Expr::IndexAccess { object, index, .. } => {
                self.verify_expr(object, ctx, diag);
                self.verify_expr(index, ctx, diag);
            }
            Expr::Range { start, end, .. } => {
                self.verify_expr(start, ctx, diag);
                self.verify_expr(end, ctx, diag);
            }
            Expr::Tuple(exprs, _) | Expr::ListLiteral(exprs, _) => {
                for e in exprs {
                    self.verify_expr(e, ctx, diag);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.verify_expr(k, ctx, diag);
                    self.verify_expr(v, ctx, diag);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
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
            Expr::ObjectInit { fields, .. } => {
                for (_, f_expr) in fields {
                    self.verify_expr(f_expr, ctx, diag);
                }
            }
            Expr::Literal(_, _) | Expr::Identifier(_, _) => {}
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
            Expr::Literal(LiteralValue::Int(n), _) => {
                if *n == 0 {
                    diag.error(
                        ErrorCode::ProofCarryingCodeViolation,
                        "Proof-Carrying Code Violation: Unproven divisor '0' may be zero. Use 'NonZeroInt' or contract 'require != 0'".to_string(),
                        Some(span.clone()),
                    );
                }
            }
            Expr::Literal(LiteralValue::Float(f), _) => {
                if *f == 0.0 {
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
                diag.error(
                    ErrorCode::ProofCarryingCodeViolation,
                    "Proof-Carrying Code Violation: Unproven divisor expression may be zero. Use 'NonZeroInt' or contract 'require != 0'".to_string(),
                    Some(span.clone()),
                );
            }
        }
    }

    fn expr_has_capability(&self, expr: &Expr, req_cap: &str, ctx: &FnContext) -> bool {
        if let Expr::Identifier(name, _) = expr
            && let Some(ty) = ctx.symbols.get(name)
        {
            return matches_capability(ty, req_cap);
        }
        false
    }
}

fn is_proven_non_zero_cond(cond: &Expr, var_name: &str) -> bool {
    match cond {
        Expr::Binary {
            op, left, right, ..
        } => {
            if op == "!=" {
                if is_var(left, var_name) && is_zero(right) {
                    return true;
                }
                if is_var(right, var_name) && is_zero(left) {
                    return true;
                }
            } else if op == ">" {
                if is_var(left, var_name) && (is_zero(right) || is_positive(right)) {
                    return true;
                }
            } else if op == ">=" {
                if is_var(left, var_name) && is_strictly_positive(right) {
                    return true;
                }
            } else if op == "<" {
                if is_var(left, var_name) && (is_zero(right) || is_negative(right)) {
                    return true;
                }
            } else if op == "<=" {
                if is_var(left, var_name) && is_strictly_negative(right) {
                    return true;
                }
            } else if op == "&&" {
                return is_proven_non_zero_cond(left, var_name)
                    || is_proven_non_zero_cond(right, var_name);
            }
            false
        }
        _ => false,
    }
}

fn is_var(expr: &Expr, var_name: &str) -> bool {
    if let Expr::Identifier(name, _) = expr {
        name == var_name
    } else {
        false
    }
}

fn is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(0), _) => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f == 0.0 => true,
        _ => false,
    }
}

fn is_positive(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n >= 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f >= 0.0 => true,
        _ => false,
    }
}

fn is_strictly_positive(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n >= 1 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f > 0.0 => true,
        _ => false,
    }
}

fn is_negative(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n <= 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f <= 0.0 => true,
        _ => false,
    }
}

fn is_strictly_negative(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n <= -1 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f < 0.0 => true,
        _ => false,
    }
}

fn is_non_zero_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n != 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f != 0.0 => true,
        _ => false,
    }
}

fn required_capability_for_op(callee: &str) -> Option<&'static str> {
    match callee {
        "fs_open" | "fs_read" | "read_file" | "file_read" => Some("Capability<FileRead>"),
        "fs_write" | "file_write" | "write_file" | "file_append" => Some("Capability<FileWrite>"),
        "net_connect" | "socket_connect" => Some("Capability<NetworkConnect>"),
        "net_listen" | "socket_listen" | "socket_bind" => Some("Capability<NetworkListen>"),
        "proc_spawn" | "process_run" | "system" | "exec" | "process_output" => {
            Some("Capability<ProcessExec>")
        }
        _ => None,
    }
}

fn has_capability(symbols: &HashMap<String, DataraType>, req_cap: &str) -> bool {
    for ty in symbols.values() {
        if matches_capability(ty, req_cap) {
            return true;
        }
    }
    false
}

fn matches_capability(ty: &DataraType, req_cap: &str) -> bool {
    match ty {
        DataraType::GenericInstance { name, args } if name == "Capability" => {
            if let Some(first_arg) = args.first() {
                let inner = match first_arg {
                    DataraType::Class(c) => c.as_str(),
                    _ => "",
                };
                let formatted = format!("Capability<{}>", inner);
                if formatted == req_cap {
                    return true;
                }
            }
        }
        DataraType::Class(c) if c == req_cap || c == "SystemCapabilities" => {
            return true;
        }
        _ => {}
    }
    false
}

fn collect_expr_reads(
    expr: &Expr,
    outer_vars: &HashSet<String>,
    local_declared: &HashSet<String>,
    reads: &mut HashSet<String>,
) {
    match expr {
        Expr::Identifier(name, _) => {
            if outer_vars.contains(name) && !local_declared.contains(name) {
                reads.insert(name.clone());
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_reads(left, outer_vars, local_declared, reads);
            collect_expr_reads(right, outer_vars, local_declared, reads);
        }
        Expr::Unary { expr, .. } => {
            collect_expr_reads(expr, outer_vars, local_declared, reads);
        }
        Expr::Call { callee, args, .. } => {
            collect_expr_reads(callee, outer_vars, local_declared, reads);
            for a in args {
                collect_expr_reads(a, outer_vars, local_declared, reads);
            }
        }
        Expr::MemberAccess { object, .. } => {
            collect_expr_reads(object, outer_vars, local_declared, reads);
        }
        Expr::IndexAccess { object, index, .. } => {
            collect_expr_reads(object, outer_vars, local_declared, reads);
            collect_expr_reads(index, outer_vars, local_declared, reads);
        }
        Expr::Tuple(exprs, _) | Expr::ListLiteral(exprs, _) => {
            for e in exprs {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
        }
        Expr::InterpolatedString { expressions, .. } => {
            for e in expressions {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
        }
        _ => {}
    }
}
