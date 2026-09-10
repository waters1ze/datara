use crate::ast::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effect {
    Pure,
    Read,
    Write,
    IO,
    Network,
    Database,
    Unsafe,
    Parallel,
    Nondeterministic,
    Foreign,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSet {
    pub effects: HashSet<Effect>,
}

impl Default for EffectSet {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectSet {
    pub fn new() -> Self {
        Self {
            effects: HashSet::new(),
        }
    }

    pub fn pure() -> Self {
        let mut s = HashSet::new();
        s.insert(Effect::Pure);
        Self { effects: s }
    }

    pub fn add(&mut self, effect: Effect) {
        if effect != Effect::Pure {
            self.effects.remove(&Effect::Pure);
            self.effects.insert(effect);
        }
    }

    pub fn union(&mut self, other: &EffectSet) {
        for e in &other.effects {
            if *e != Effect::Pure {
                self.effects.remove(&Effect::Pure);
                self.effects.insert(*e);
            }
        }
    }

    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() || (self.effects.len() == 1 && self.effects.contains(&Effect::Pure))
    }

    pub fn is_deterministic(&self) -> bool {
        !self.effects.contains(&Effect::Nondeterministic)
            && !self.effects.contains(&Effect::Foreign)
    }

    pub fn allows_parallel(&self) -> bool {
        !self.effects.contains(&Effect::Write)
            && !self.effects.contains(&Effect::IO)
            && !self.effects.contains(&Effect::Network)
            && !self.effects.contains(&Effect::Foreign)
    }
}

impl std::fmt::Display for EffectSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_pure() {
            write!(f, "Pure")
        } else {
            let mut list: Vec<String> = self.effects.iter().map(|e| format!("{:?}", e)).collect();
            list.sort();
            write!(f, "{}", list.join(", "))
        }
    }
}

pub struct EffectAnalyzer {
    pub function_effects: HashMap<String, EffectSet>,
}

impl Default for EffectAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectAnalyzer {
    pub fn new() -> Self {
        let mut builtins = HashMap::new();

        // Built-in function effects
        let mut io_eff = EffectSet::new();
        io_eff.add(Effect::IO);
        builtins.insert("out".to_string(), io_eff.clone());
        builtins.insert("err".to_string(), io_eff.clone());
        builtins.insert("print".to_string(), io_eff);

        let mut net_eff = EffectSet::new();
        net_eff.add(Effect::Network);
        net_eff.add(Effect::IO);
        builtins.insert("fetch".to_string(), net_eff.clone());
        builtins.insert("http_get".to_string(), net_eff);

        let mut db_eff = EffectSet::new();
        db_eff.add(Effect::Database);
        db_eff.add(Effect::IO);
        builtins.insert("db_query".to_string(), db_eff.clone());
        builtins.insert("db_write".to_string(), db_eff);

        let mut non_det_eff = EffectSet::new();
        non_det_eff.add(Effect::Nondeterministic);
        builtins.insert("rand".to_string(), non_det_eff.clone());
        builtins.insert("now".to_string(), non_det_eff);

        let mut foreign_eff = EffectSet::new();
        foreign_eff.add(Effect::Foreign);
        foreign_eff.add(Effect::Nondeterministic);
        for f in &[
            "py_call",
            "py_call_1_str",
            "py_call_1_float",
            "py_eval",
            "py_eval_safe",
            "py_eval_int",
            "py_eval_float",
            "py_import",
            "py_exec",
            "py_last_error",
            "py_clear_error",
            "datara_py_eval",
            "datara_py_eval_safe",
            "datara_py_eval_int",
            "datara_py_eval_float",
            "datara_py_call",
            "datara_py_call_1_str",
            "datara_py_call_1_float",
            "datara_py_import",
            "datara_py_exec",
            "datara_py_last_error",
            "datara_py_clear_error",
            "datara_py_export_list_f64",
            "datara_py_assert_same_ptr",
        ] {
            builtins.insert(f.to_string(), foreign_eff.clone());
        }

        Self {
            function_effects: builtins,
        }
    }

    pub fn analyze_program(&mut self, program: &Program) {
        let mut changed = true;
        let mut iter_count = 0;
        while changed && iter_count < 32 {
            changed = false;
            iter_count += 1;

            for decl in &program.declarations {
                match decl {
                    Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                        let mut effects = EffectSet::pure();
                        let mut local_vars: HashSet<String> =
                            f.params.iter().map(|p| p.name.clone()).collect();
                        self.analyze_stmt(&f.body, &mut effects, &mut local_vars);
                        if let Some(old) = self.function_effects.get(&f.name) {
                            if old.effects != effects.effects {
                                changed = true;
                                self.function_effects.insert(f.name.clone(), effects);
                            }
                        } else {
                            changed = true;
                            self.function_effects.insert(f.name.clone(), effects);
                        }
                    }
                    Decl::Class(c) => {
                        for item in &c.body_items {
                            if let ClassItem::Method(m) = item
                                && let Some(body) = &m.body
                            {
                                let mut effects = EffectSet::pure();
                                let mut local_vars: HashSet<String> =
                                    m.params.iter().map(|p| p.name.clone()).collect();
                                self.analyze_stmt(body, &mut effects, &mut local_vars);
                                let key = format!("{}.{}", c.name, m.name);
                                if let Some(old) = self.function_effects.get(&key) {
                                    if old.effects != effects.effects {
                                        changed = true;
                                        self.function_effects.insert(key, effects);
                                    }
                                } else {
                                    changed = true;
                                    self.function_effects.insert(key, effects);
                                }
                            }
                        }
                    }
                    Decl::Behavior(b) => {
                        for item in &b.body_items {
                            if let ClassItem::Method(m) = item
                                && let Some(body) = &m.body
                            {
                                let mut effects = EffectSet::pure();
                                let mut local_vars: HashSet<String> =
                                    m.params.iter().map(|p| p.name.clone()).collect();
                                self.analyze_stmt(body, &mut effects, &mut local_vars);
                                let key = format!("{}.{}", b.target_type, m.name);
                                if let Some(old) = self.function_effects.get(&key) {
                                    if old.effects != effects.effects {
                                        changed = true;
                                        self.function_effects.insert(key, effects);
                                    }
                                } else {
                                    changed = true;
                                    self.function_effects.insert(key, effects);
                                }
                            }
                        }
                    }
                    Decl::Impl(i) => {
                        for m in &i.methods {
                            let mut effects = EffectSet::pure();
                            let mut local_vars: HashSet<String> =
                                m.params.iter().map(|p| p.name.clone()).collect();
                            local_vars.insert("this".to_string());
                            local_vars.insert("self".to_string());
                            self.analyze_stmt(&m.body, &mut effects, &mut local_vars);
                            let key = format!("{}.{}", i.target_type, m.name);
                            if let Some(old) = self.function_effects.get(&key) {
                                if old.effects != effects.effects {
                                    changed = true;
                                    self.function_effects.insert(key, effects);
                                }
                            } else {
                                changed = true;
                                self.function_effects.insert(key, effects);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn analyze_stmt(&self, stmt: &Stmt, effects: &mut EffectSet, local_vars: &mut HashSet<String>) {
        match stmt {
            Stmt::Block(stmts, _) => {
                let mut block_locals = local_vars.clone();
                for s in stmts {
                    self.analyze_stmt(s, effects, &mut block_locals);
                }
            }
            Stmt::Out(e, _) | Stmt::Err(e, _) => {
                effects.add(Effect::IO);
                self.analyze_expr(e, effects);
            }
            Stmt::Assign { target, value, .. } => {
                let is_local_id = if let Expr::Identifier(id_name, _) = target {
                    local_vars.contains(id_name)
                } else {
                    false
                };
                if !is_local_id {
                    effects.add(Effect::Write);
                }
                self.analyze_expr(target, effects);
                self.analyze_expr(value, effects);
            }
            Stmt::Let { name, init, .. }
            | Stmt::Mut { name, init, .. }
            | Stmt::Const { name, init, .. }
            | Stmt::Val { name, init, .. }
            | Stmt::CompactBind { name, init, .. } => {
                local_vars.insert(name.clone());
                self.analyze_expr(init, effects);
            }
            Stmt::Expr(init, _) => {
                self.analyze_expr(init, effects);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.analyze_expr(condition, effects);
                let mut then_locals = local_vars.clone();
                self.analyze_stmt(then_branch, effects, &mut then_locals);
                if let Some(eb) = else_branch {
                    let mut else_locals = local_vars.clone();
                    self.analyze_stmt(eb, effects, &mut else_locals);
                }
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            } => {
                self.analyze_expr(iterable, effects);
                let mut loop_locals = local_vars.clone();
                loop_locals.insert(var_name.clone());
                self.analyze_stmt(body, effects, &mut loop_locals);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.analyze_expr(condition, effects);
                let mut body_locals = local_vars.clone();
                self.analyze_stmt(body, effects, &mut body_locals);
            }
            Stmt::Loop { body, .. } => {
                let mut body_locals = local_vars.clone();
                self.analyze_stmt(body, effects, &mut body_locals);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                err_var,
                ..
            } => {
                let mut try_locals = local_vars.clone();
                self.analyze_stmt(try_block, effects, &mut try_locals);
                let mut catch_locals = local_vars.clone();
                catch_locals.insert(err_var.clone());
                self.analyze_stmt(catch_block, effects, &mut catch_locals);
            }
            Stmt::Parallel(body, _) => {
                effects.add(Effect::Parallel);
                let mut par_locals = local_vars.clone();
                self.analyze_stmt(body, effects, &mut par_locals);
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                effects.add(Effect::Parallel);
                self.analyze_expr(iterable, effects);
                let mut loop_locals = local_vars.clone();
                loop_locals.insert(var_name.clone());
                self.analyze_stmt(body, effects, &mut loop_locals);
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                self.analyze_expr(init, effects);
                let mut with_locals = local_vars.clone();
                with_locals.insert(resource_name.clone());
                self.analyze_stmt(body, effects, &mut with_locals);
            }
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    self.analyze_expr(e, effects);
                }
            }
            Stmt::Unsafe { body, .. } => {
                effects.add(Effect::Unsafe);
                let mut unsafe_locals = local_vars.clone();
                self.analyze_stmt(body, effects, &mut unsafe_locals);
            }
            Stmt::Asm { options, .. } => {
                effects.add(Effect::Unsafe);
                if !options.iter().any(|o| o == "pure") {
                    effects.add(Effect::IO);
                }
            }
        }
    }

    fn analyze_expr(&self, expr: &Expr, effects: &mut EffectSet) {
        match expr {
            Expr::Call { callee, args, .. } => {
                self.analyze_expr(callee, effects);
                for a in args {
                    self.analyze_expr(a, effects);
                }
                if let Expr::Identifier(name, _) = &**callee {
                    if let Some(eff) = self.function_effects.get(name) {
                        effects.union(eff);
                    } else if name.starts_with("http_") || name.starts_with("net_") {
                        effects.add(Effect::Network);
                        effects.add(Effect::IO);
                    } else if name.starts_with("db_") || name.starts_with("sql_") {
                        effects.add(Effect::Database);
                        effects.add(Effect::IO);
                    } else if name.starts_with("py_") || name.starts_with("datara_py_") {
                        effects.add(Effect::Foreign);
                        effects.add(Effect::Nondeterministic);
                    } else if name == "rand" || name == "random" || name == "timestamp" {
                        effects.add(Effect::Nondeterministic);
                    }
                } else if let Expr::MemberAccess { object, member, .. } = &**callee {
                    if let Expr::Identifier(obj_name, _) = &**object {
                        let method_key = format!("{}.{}", obj_name, member);
                        if let Some(eff) = self.function_effects.get(&method_key) {
                            effects.union(eff);
                        }
                    }
                    if member.starts_with("py_")
                        || member == "eval"
                        || member == "eval_int"
                        || member == "eval_float"
                        || member == "call"
                        || member == "exec"
                        || member == "bind_buffer"
                    {
                        effects.add(Effect::Foreign);
                        effects.add(Effect::Nondeterministic);
                    } else if member.starts_with("http_")
                        || member.starts_with("net_")
                        || member == "fetch"
                    {
                        effects.add(Effect::Network);
                        effects.add(Effect::IO);
                    } else if member.starts_with("db_")
                        || member.starts_with("sql_")
                        || member == "query"
                    {
                        effects.add(Effect::Database);
                        effects.add(Effect::IO);
                    } else if member == "rand" || member == "random" {
                        effects.add(Effect::Nondeterministic);
                    }
                }
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left, effects);
                self.analyze_expr(right, effects);
            }
            Expr::Unary { expr, .. } | Expr::ErrorPropagate(expr, _) => {
                self.analyze_expr(expr, effects);
            }
            Expr::MemberAccess { object, .. } => {
                effects.add(Effect::Read);
                self.analyze_expr(object, effects);
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    self.analyze_expr(s, effects);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for a in arms {
                    self.analyze_expr(&a.condition, effects);
                    self.analyze_expr(&a.body, effects);
                }
                if let Some(eb) = else_arm {
                    self.analyze_expr(eb, effects);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.analyze_expr(value, effects);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.analyze_expr(g, effects);
                    }
                    self.analyze_expr(&a.body, effects);
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for a in arms {
                    self.analyze_expr(&a.condition, effects);
                    self.analyze_expr(&a.body, effects);
                }
                if let Some(eb) = else_arm {
                    self.analyze_expr(eb, effects);
                }
            }
            Expr::Lambda { body, .. } => {
                self.analyze_expr(body, effects);
            }
            Expr::ListLiteral(items, _) => {
                for item in items {
                    self.analyze_expr(item, effects);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.analyze_expr(k, effects);
                    self.analyze_expr(v, effects);
                }
            }
            Expr::IndexAccess { object, index, .. } => {
                effects.add(Effect::Read);
                self.analyze_expr(object, effects);
                self.analyze_expr(index, effects);
            }
            Expr::Range { start, end, .. } => {
                self.analyze_expr(start, effects);
                self.analyze_expr(end, effects);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.analyze_expr(e, effects);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.analyze_expr(e, effects);
                }
            }
            _ => {}
        }
    }
}
