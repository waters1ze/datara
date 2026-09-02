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
    }

    pub fn allows_parallel(&self) -> bool {
        !self.effects.contains(&Effect::Write)
            && !self.effects.contains(&Effect::IO)
            && !self.effects.contains(&Effect::Network)
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

        Self {
            function_effects: builtins,
        }
    }

    pub fn analyze_program(&mut self, program: &Program) {
        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    let mut effects = EffectSet::pure();
                    let mut local_vars: HashSet<String> =
                        f.params.iter().map(|p| p.name.clone()).collect();
                    self.analyze_stmt(&f.body, &mut effects, &mut local_vars);
                    self.function_effects.insert(f.name.clone(), effects);
                }
                Decl::Class(c) => {
                    for item in &c.body_items {
                        if let ClassItem::Method(m) = item
                            && let Some(body) = &m.body {
                                let mut effects = EffectSet::pure();
                                let mut local_vars: HashSet<String> =
                                    m.params.iter().map(|p| p.name.clone()).collect();
                                self.analyze_stmt(body, &mut effects, &mut local_vars);
                                self.function_effects
                                    .insert(format!("{}.{}", c.name, m.name), effects);
                            }
                    }
                }
                Decl::Behavior(b) => {
                    for item in &b.body_items {
                        if let ClassItem::Method(m) = item
                            && let Some(body) = &m.body {
                                let mut effects = EffectSet::pure();
                                let mut local_vars: HashSet<String> =
                                    m.params.iter().map(|p| p.name.clone()).collect();
                                self.analyze_stmt(body, &mut effects, &mut local_vars);
                                self.function_effects
                                    .insert(format!("{}.{}", b.target_type, m.name), effects);
                            }
                    }
                }
                _ => {}
            }
        }
    }

    fn analyze_stmt(&self, stmt: &Stmt, effects: &mut EffectSet, local_vars: &mut HashSet<String>) {
        match stmt {
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    self.analyze_stmt(s, effects, local_vars);
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
                self.analyze_stmt(then_branch, effects, local_vars);
                if let Some(eb) = else_branch {
                    self.analyze_stmt(eb, effects, local_vars);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.analyze_expr(iterable, effects);
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.analyze_expr(condition, effects);
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::Loop { body, .. } => {
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.analyze_stmt(try_block, effects, local_vars);
                self.analyze_stmt(catch_block, effects, local_vars);
            }
            Stmt::Parallel(body, _) => {
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::ParallelFor { iterable, body, .. } => {
                self.analyze_expr(iterable, effects);
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::With { init, body, .. } => {
                self.analyze_expr(init, effects);
                self.analyze_stmt(body, effects, local_vars);
            }
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    self.analyze_expr(e, effects);
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
                    } else if name == "rand" || name == "random" || name == "timestamp" {
                        effects.add(Effect::Nondeterministic);
                    }
                } else if let Expr::MemberAccess { object, member, .. } = &**callee
                    && let Expr::Identifier(obj_name, _) = &**object {
                        let method_key = format!("{}.{}", obj_name, member);
                        if let Some(eff) = self.function_effects.get(&method_key) {
                            effects.union(eff);
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
