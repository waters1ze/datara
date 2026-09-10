//! Proof-Carrying Scheduler (PCS) Schedule Proof format and compiler schedule builder.
//!
//! In the Proof-Carrying Scheduler model, the compiler precomputes the execution schedule
//! as a serializable Directed Acyclic Graph (DAG) with Kahn topological wavefronts.
//! The runtime executor merely consumes and executes this precomputed schedule.

use crate::ast::{ClassItem, Decl, Program};
use crate::dmir::{Inst, Module};
use crate::effects::{Effect, EffectAnalyzer, EffectSet};
use crate::optimizer::Optimizer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub type TaskId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ScheduleEffectClass {
    Pure,
    IO,
    Network,
    Parallel,
}

impl std::fmt::Display for ScheduleEffectClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pure => write!(f, "Pure"),
            Self::IO => write!(f, "IO"),
            Self::Network => write!(f, "Network"),
            Self::Parallel => write!(f, "Parallel"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SchedulePriority {
    Cold,
    Hot,
}

impl std::fmt::Display for SchedulePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cold => write!(f, "Cold"),
            Self::Hot => write!(f, "Hot"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WorkerPoolKind {
    CPUPool,
    IOPool,
}

impl std::fmt::Display for WorkerPoolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CPUPool => write!(f, "CPUPool"),
            Self::IOPool => write!(f, "IOPool"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: TaskId,
    pub name: String,
    pub effect_class: ScheduleEffectClass,
    pub priority: SchedulePriority,
    pub deps: Vec<TaskId>,
    pub pool: WorkerPoolKind,
    pub deterministic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleProof {
    pub tasks: Vec<TaskNode>,
    /// Kahn topological wavefronts: each wave is a set of task IDs whose dependencies
    /// are completely satisfied by prior waves and which can execute concurrently.
    pub waves: Vec<Vec<TaskId>>,
    pub is_deterministic: bool,
}

/// Canonicalizes a task symbol name to match DMIR module convention (`Class_method`).
pub fn canonical_task_name(name: &str) -> String {
    if name.contains('.') {
        name.replace('.', "_")
    } else {
        name.to_string()
    }
}

impl ScheduleProof {
    /// Build a ScheduleProof deterministically from the compilation artifacts:
    /// (a) effects map from EffectAnalyzer
    /// (b) cost model's hot/cold classification
    /// (c) optimizer's parallelization decisions and DMIR call graph
    pub fn build(
        program: &Program,
        dmir_module: &Module,
        effects: &EffectAnalyzer,
        optimizer: &Optimizer,
    ) -> Self {
        // 1. Collect all declared tasks and functions in deterministic sorted order
        let mut symbol_names: BTreeSet<String> = BTreeSet::new();
        // Interning table mapping aliases (e.g. AST "Class.method") to canonical DMIR name ("Class_method")
        let mut alias_to_canonical: HashMap<String, String> = HashMap::new();

        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    let canon = canonical_task_name(&f.name);
                    alias_to_canonical.insert(f.name.clone(), canon.clone());
                    symbol_names.insert(canon);
                }
                Decl::Class(c) => {
                    for item in &c.body_items {
                        if let ClassItem::Method(m) = item {
                            let ast_name = format!("{}.{}", c.name, m.name);
                            let canon = format!("{}_{}", c.name, m.name);
                            alias_to_canonical.insert(ast_name, canon.clone());
                            alias_to_canonical.insert(canon.clone(), canon.clone());
                            symbol_names.insert(canon);
                        }
                    }
                }
                Decl::Behavior(b) => {
                    for item in &b.body_items {
                        if let ClassItem::Method(m) = item {
                            let ast_name = format!("{}.{}", b.target_type, m.name);
                            let canon = format!("{}_{}", b.target_type, m.name);
                            alias_to_canonical.insert(ast_name, canon.clone());
                            alias_to_canonical.insert(canon.clone(), canon.clone());
                            symbol_names.insert(canon);
                        }
                    }
                }
                _ => {}
            }
        }

        // Also add any functions in the DMIR module not yet collected
        for fn_name in dmir_module.functions.keys() {
            let canon = canonical_task_name(fn_name);
            alias_to_canonical.insert(fn_name.clone(), canon.clone());
            symbol_names.insert(canon);
        }

        // If no symbols exist (e.g. empty program), return an empty proof
        if symbol_names.is_empty() {
            return Self {
                tasks: Vec::new(),
                waves: Vec::new(),
                is_deterministic: true,
            };
        }

        // 2. Assign deterministic TaskId based on sorted canonical symbol names
        let mut canonical_to_id: BTreeMap<String, TaskId> = BTreeMap::new();
        let mut symbol_to_id: BTreeMap<String, TaskId> = BTreeMap::new();
        for (idx, name) in symbol_names.iter().enumerate() {
            let id = idx as TaskId;
            canonical_to_id.insert(name.clone(), id);
            symbol_to_id.insert(name.clone(), id);
        }
        // Map all registered aliases to the canonical TaskId
        for (alias, canon) in &alias_to_canonical {
            if let Some(&id) = canonical_to_id.get(canon) {
                symbol_to_id.insert(alias.clone(), id);
            }
        }

        // 3. Build each TaskNode (strictly one node per canonical symbol)
        let mut tasks: Vec<TaskNode> = Vec::with_capacity(symbol_names.len());

        for (name, &id) in &canonical_to_id {
            // (a) Determine EffectClass and Determinism from EffectAnalyzer
            let eff_opt = effects.function_effects.get(name).or_else(|| {
                // Look up potential AST alias notation (e.g. Class.method instead of Class_method)
                let alt = name.replacen('_', ".", 1);
                effects.function_effects.get(&alt)
            });
            let effect_class = Self::classify_effects(eff_opt);
            let is_deterministic = eff_opt
                .map(|e| {
                    e.is_deterministic()
                        && !e.effects.contains(&Effect::IO)
                        && !e.effects.contains(&Effect::Network)
                        && !e.effects.contains(&Effect::Database)
                })
                .unwrap_or(matches!(
                    effect_class,
                    ScheduleEffectClass::Pure | ScheduleEffectClass::Parallel
                ));

            // (b) Determine Priority from CostModel & CFG & AST
            let priority = Self::classify_priority(name, program, dmir_module, optimizer);

            // Determine Pool: CPUPool for Pure and Parallel; IOPool for IO and Network
            let pool = match effect_class {
                ScheduleEffectClass::Pure | ScheduleEffectClass::Parallel => {
                    WorkerPoolKind::CPUPool
                }
                ScheduleEffectClass::IO | ScheduleEffectClass::Network => WorkerPoolKind::IOPool,
            };

            // (c) Determine Dependencies from DMIR call graph
            let mut deps = Vec::new();
            if let Some(dmir_func) = dmir_module.functions.get(name) {
                for blk in &dmir_func.blocks {
                    for inst in &blk.instructions {
                        match inst {
                            Inst::Call { func, .. } => {
                                if let Some(&dep_id) = symbol_to_id.get(func) {
                                    if dep_id != id {
                                        deps.push(dep_id);
                                    }
                                }
                            }
                            Inst::MethodCall { method, .. } => {
                                if let Some(&dep_id) = symbol_to_id.get(method) {
                                    if dep_id != id {
                                        deps.push(dep_id);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            deps.sort();
            deps.dedup();

            tasks.push(TaskNode {
                id,
                name: name.clone(),
                effect_class,
                priority,
                deps,
                pool,
                deterministic: is_deterministic,
            });
        }

        debug_assert_eq!(
            tasks.len(),
            symbol_names.len(),
            "Task deduplication invariant violated: task count must equal unique canonical symbol count"
        );

        // 4. Compute Kahn topological wavefront levels
        let waves = Self::compute_kahn_waves(&tasks);

        // Overall determinism: true if all tasks in the DAG are deterministic
        let is_deterministic = tasks.iter().all(|t| t.deterministic);

        Self {
            tasks,
            waves,
            is_deterministic,
        }
    }

    fn classify_effects(eff_opt: Option<&EffectSet>) -> ScheduleEffectClass {
        if let Some(eff) = eff_opt {
            if eff.effects.contains(&Effect::Network) {
                ScheduleEffectClass::Network
            } else if eff.effects.contains(&Effect::IO) || eff.effects.contains(&Effect::Database) {
                ScheduleEffectClass::IO
            } else if eff.effects.contains(&Effect::Parallel) {
                ScheduleEffectClass::Parallel
            } else {
                ScheduleEffectClass::Pure
            }
        } else {
            ScheduleEffectClass::Pure
        }
    }

    fn classify_priority(
        name: &str,
        program: &Program,
        dmir_module: &Module,
        optimizer: &Optimizer,
    ) -> SchedulePriority {
        // 1. Check optimizer decision trace for PGO hotness or loop parallelization
        for rec in &optimizer.report.decision_trace {
            if rec.candidate.contains(name) && (rec.pass == "PGO" || rec.pass == "LoopParallel") {
                return SchedulePriority::Hot;
            }
        }

        // 2. Check if DMIR function has CFG loops or exceeds instruction threshold
        if let Some(f) = dmir_module.functions.get(name) {
            let cfg = crate::dmir::cfg::ControlFlowGraph::build(f);
            if !cfg.loops.is_empty() {
                return SchedulePriority::Hot;
            }
            let total_insts: usize = f.blocks.iter().map(|b| b.instructions.len()).sum();
            if total_insts > optimizer.cost_model.inlining_threshold.max(20) {
                return SchedulePriority::Hot;
            }
        }

        // 3. Check AST declaration in case of early inlining/elimination
        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) if f.name == name => {
                    if Self::stmt_contains_loop(&f.body) {
                        return SchedulePriority::Hot;
                    }
                }
                _ => {}
            }
        }

        SchedulePriority::Cold
    }

    fn stmt_contains_loop(stmt: &crate::ast::Stmt) -> bool {
        use crate::ast::Stmt;
        match stmt {
            Stmt::While { .. } | Stmt::For { .. } | Stmt::Parallel { .. } => true,
            Stmt::Block(stmts, _) => stmts.iter().any(Self::stmt_contains_loop),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::stmt_contains_loop(then_branch)
                    || else_branch
                        .as_ref()
                        .is_some_and(|b| Self::stmt_contains_loop(b))
            }
            _ => false,
        }
    }

    /// Kahn's algorithm computing topological waves (wavefront levels).
    /// Wave 0 consists of all tasks with zero in-degree (no dependencies).
    /// Each subsequent wave consists of tasks whose dependencies are resolved by previous waves.
    pub fn compute_kahn_waves(tasks: &[TaskNode]) -> Vec<Vec<TaskId>> {
        if tasks.is_empty() {
            return Vec::new();
        }

        let mut in_degree: BTreeMap<TaskId, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();

        for t in tasks {
            in_degree.insert(t.id, 0);
            dependents.insert(t.id, Vec::new());
        }

        for t in tasks {
            for &dep in &t.deps {
                if in_degree.contains_key(&dep) {
                    *in_degree.entry(t.id).or_insert(0) += 1;
                    dependents.entry(dep).or_default().push(t.id);
                }
            }
        }

        let mut waves: Vec<Vec<TaskId>> = Vec::new();
        let mut current_wave: Vec<TaskId> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&id, _)| id)
            .collect();
        current_wave.sort();

        let mut visited: BTreeSet<TaskId> = BTreeSet::new();

        while !current_wave.is_empty() {
            for &id in &current_wave {
                visited.insert(id);
            }

            let mut next_wave: Vec<TaskId> = Vec::new();
            for &id in &current_wave {
                if let Some(succs) = dependents.get(&id) {
                    for &succ in succs {
                        if let Some(deg) = in_degree.get_mut(&succ) {
                            if *deg > 0 {
                                *deg -= 1;
                                if *deg == 0 && !visited.contains(&succ) {
                                    next_wave.push(succ);
                                }
                            }
                        }
                    }
                }
            }

            waves.push(current_wave);
            next_wave.sort();
            next_wave.dedup();
            current_wave = next_wave;
        }

        // Handle any cyclic or disconnected residual tasks by appending them in sorted order
        if visited.len() < tasks.len() {
            let mut residual: Vec<TaskId> = tasks
                .iter()
                .filter(|t| !visited.contains(&t.id))
                .map(|t| t.id)
                .collect();
            residual.sort();
            if !residual.is_empty() {
                waves.push(residual);
            }
        }

        waves
    }

    /// Serialize this ScheduleProof as a pretty JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Look up a task by name or alias (e.g. "Class.method" or "Class_method")
    pub fn find_task(&self, name: &str) -> Option<&TaskNode> {
        let canonical = canonical_task_name(name);
        self.tasks
            .iter()
            .find(|t| t.name == name || t.name == canonical)
    }

    /// Look up a task by ID
    pub fn task_by_id(&self, id: TaskId) -> Option<&TaskNode> {
        self.tasks.iter().find(|t| t.id == id)
    }
}
