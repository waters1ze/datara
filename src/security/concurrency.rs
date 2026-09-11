use super::*;
use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use std::collections::HashSet;

impl<'a> SecurityVerifier<'a> {
    pub(crate) fn check_parallel_block_data_race(
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
            Stmt::Parallel(body, _) => {
                self.collect_branch_reads_writes(body, outer_vars, local_declared, reads, writes);
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                collect_expr_reads(iterable, outer_vars, local_declared, reads);
                let mut pfor_declared = local_declared.clone();
                pfor_declared.insert(var_name.clone());
                self.collect_branch_reads_writes(
                    body,
                    outer_vars,
                    &mut pfor_declared,
                    reads,
                    writes,
                );
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                collect_expr_reads(init, outer_vars, local_declared, reads);
                let mut with_declared = local_declared.clone();
                with_declared.insert(resource_name.clone());
                self.collect_branch_reads_writes(
                    body,
                    outer_vars,
                    &mut with_declared,
                    reads,
                    writes,
                );
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                ..
            } => {
                let mut try_declared = local_declared.clone();
                self.collect_branch_reads_writes(
                    try_block,
                    outer_vars,
                    &mut try_declared,
                    reads,
                    writes,
                );
                let mut catch_declared = local_declared.clone();
                catch_declared.insert(err_var.clone());
                self.collect_branch_reads_writes(
                    catch_block,
                    outer_vars,
                    &mut catch_declared,
                    reads,
                    writes,
                );
            }
            Stmt::Return(Some(e), _) => {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
            _ => {}
        }
    }

    pub(crate) fn check_parallel_data_race(
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
            Stmt::Expr(_, _)
            | Stmt::Return(_, _)
            | Stmt::Out(_, _)
            | Stmt::Err(_, _)
            | Stmt::Asm { .. } => {}
        }
    }
}
