//! Abstract interpretation over DMIR for graduated ownership guarantees.
//!
//! Provides:
//! - Per-variable abstract state: `Uninit < Owned < Moved` alongside `Borrowed{n}`.
//! - Dataflow: forward, may-analysis for borrows, must-analysis for moves.
//! - Iteration to saturation with widening at loop headers (cap at 32 iterations).
//! - Graduated dual-mode lowering: proven (zero-cost) vs guarded (runtime refcount guards).

use crate::ast::Param;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use crate::resolver::Resolver;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VarBaseState {
    Uninit,
    Owned,
    Moved {
        at_inst: Option<String>,
        reason: String,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbstractVarState {
    pub base: VarBaseState,
    pub borrows: usize,
    pub mut_borrows: usize,
    pub active_borrows: HashSet<String>,
}

impl AbstractVarState {
    pub fn uninit() -> Self {
        Self {
            base: VarBaseState::Uninit,
            borrows: 0,
            mut_borrows: 0,
            active_borrows: HashSet::new(),
        }
    }

    pub fn owned() -> Self {
        Self {
            base: VarBaseState::Owned,
            borrows: 0,
            mut_borrows: 0,
            active_borrows: HashSet::new(),
        }
    }

    pub fn moved(reason: String, at_inst: Option<String>) -> Self {
        Self {
            base: VarBaseState::Moved { at_inst, reason },
            borrows: 0,
            mut_borrows: 0,
            active_borrows: HashSet::new(),
        }
    }

    pub fn unknown() -> Self {
        Self {
            base: VarBaseState::Unknown,
            borrows: 0,
            mut_borrows: 0,
            active_borrows: HashSet::new(),
        }
    }

    pub fn is_defined(&self) -> bool {
        !matches!(self.base, VarBaseState::Unknown)
    }

    pub fn is_moved(&self) -> bool {
        matches!(self.base, VarBaseState::Moved { .. })
    }

    pub fn is_owned(&self) -> bool {
        matches!(self.base, VarBaseState::Owned)
    }

    /// Dataflow join:
    /// - May-analysis for borrows (union active borrow sets)
    /// - Must-analysis for moves (Moved on all paths => Moved, else Unknown/Owned)
    pub fn join(&self, other: &Self) -> Self {
        let mut active_borrows = self.active_borrows.clone();
        active_borrows.extend(other.active_borrows.iter().cloned());
        let borrows = active_borrows.len().max(self.borrows).max(other.borrows);
        let mut_borrows = self.mut_borrows.max(other.mut_borrows);

        let base = match (&self.base, &other.base) {
            // Uninit is lattice bottom (neutral element for join)
            (VarBaseState::Uninit, other_b) | (other_b, VarBaseState::Uninit) => other_b.clone(),
            // Unknown is lattice top (widened / unproven)
            (VarBaseState::Unknown, _) | (_, VarBaseState::Unknown) => VarBaseState::Unknown,
            // Must-analysis for moves: both branches moved => Moved
            (VarBaseState::Moved { at_inst, reason }, VarBaseState::Moved { .. }) => {
                VarBaseState::Moved {
                    at_inst: at_inst.clone(),
                    reason: reason.clone(),
                }
            }
            // Both branches owned => Owned
            (VarBaseState::Owned, VarBaseState::Owned) => VarBaseState::Owned,
            // One branch moved and one owned => NOT moved on all paths => Unknown
            (VarBaseState::Owned, VarBaseState::Moved { .. })
            | (VarBaseState::Moved { .. }, VarBaseState::Owned) => VarBaseState::Unknown,
        };

        Self {
            base,
            borrows,
            mut_borrows,
            active_borrows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDataflowState {
    pub vars: HashMap<String, AbstractVarState>,
    pub values: HashMap<ValueId, AbstractVarState>,
    pub val_origins: HashMap<ValueId, String>,
}

impl FunctionDataflowState {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            values: HashMap::new(),
            val_origins: HashMap::new(),
        }
    }

    pub fn join(&self, other: &Self) -> Self {
        let mut joined_vars = HashMap::new();
        let all_vars: HashSet<&String> = self.vars.keys().chain(other.vars.keys()).collect();
        for var in all_vars {
            let s1 = self
                .vars
                .get(var)
                .cloned()
                .unwrap_or_else(AbstractVarState::uninit);
            let s2 = other
                .vars
                .get(var)
                .cloned()
                .unwrap_or_else(AbstractVarState::uninit);
            joined_vars.insert(var.clone(), s1.join(&s2));
        }

        let mut joined_values = HashMap::new();
        let all_vals: HashSet<&ValueId> = self.values.keys().chain(other.values.keys()).collect();
        for val in all_vals {
            let s1 = self
                .values
                .get(val)
                .cloned()
                .unwrap_or_else(AbstractVarState::uninit);
            let s2 = other
                .values
                .get(val)
                .cloned()
                .unwrap_or_else(AbstractVarState::uninit);
            joined_values.insert(*val, s1.join(&s2));
        }

        let mut val_origins = self.val_origins.clone();
        for (k, v) in &other.val_origins {
            val_origins.entry(*k).or_insert_with(|| v.clone());
        }

        Self {
            vars: joined_vars,
            values: joined_values,
            val_origins,
        }
    }
}

// -----------------------------------------------------------------------------------------
// Wave B: Modern Dense Dataflow Engine (Dense indexing, RPO worklist, hash-consed borrows)
// -----------------------------------------------------------------------------------------

/// Hash-consed borrow set interner:
/// Maps sets of borrow origin strings to compact, zero-cost `u32` identifiers.
/// Set ID 0 always represents the empty borrow set.
#[derive(Debug, Clone)]
pub struct BorrowSetInterner {
    sets: Vec<BTreeSet<String>>,
    map: HashMap<BTreeSet<String>, u32>,
}

impl Default for BorrowSetInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowSetInterner {
    pub fn new() -> Self {
        let mut interner = Self {
            sets: Vec::with_capacity(16),
            map: HashMap::with_capacity(16),
        };
        let empty = BTreeSet::new();
        interner.sets.push(empty.clone());
        interner.map.insert(empty, 0);
        interner
    }

    #[inline]
    pub fn len(&self, id: u32) -> usize {
        self.sets.get(id as usize).map(|s| s.len()).unwrap_or(0)
    }

    pub fn intern(&mut self, set: BTreeSet<String>) -> u32 {
        if let Some(&id) = self.map.get(&set) {
            return id;
        }
        let id = self.sets.len() as u32;
        self.sets.push(set.clone());
        self.map.insert(set, id);
        id
    }

    #[inline]
    pub fn union(&mut self, a: u32, b: u32) -> u32 {
        if a == b || b == 0 {
            return a;
        }
        if a == 0 {
            return b;
        }
        let mut u = self.sets[a as usize].clone();
        for item in &self.sets[b as usize] {
            u.insert(item.clone());
        }
        self.intern(u)
    }

    #[inline]
    pub fn insert(&mut self, base: u32, item: String) -> u32 {
        if (base as usize) < self.sets.len() && self.sets[base as usize].contains(&item) {
            return base;
        }
        let mut s = if (base as usize) < self.sets.len() {
            self.sets[base as usize].clone()
        } else {
            BTreeSet::new()
        };
        s.insert(item);
        self.intern(s)
    }

    pub fn to_hash_set(&self, id: u32) -> HashSet<String> {
        if let Some(set) = self.sets.get(id as usize) {
            set.iter().cloned().collect()
        } else {
            HashSet::new()
        }
    }
}

/// Compact per-variable abstract state with hash-consed borrow set ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenseVarState {
    pub base: VarBaseState,
    pub borrows: usize,
    pub mut_borrows: usize,
    pub borrow_set_id: u32,
}

impl DenseVarState {
    pub const UNINIT: DenseVarState = DenseVarState {
        base: VarBaseState::Uninit,
        borrows: 0,
        mut_borrows: 0,
        borrow_set_id: 0,
    };

    #[inline]
    pub fn uninit() -> Self {
        Self::UNINIT
    }

    #[inline]
    pub fn owned() -> Self {
        Self {
            base: VarBaseState::Owned,
            borrows: 0,
            mut_borrows: 0,
            borrow_set_id: 0,
        }
    }

    #[inline]
    pub fn moved(reason: String, at_inst: Option<String>) -> Self {
        Self {
            base: VarBaseState::Moved { at_inst, reason },
            borrows: 0,
            mut_borrows: 0,
            borrow_set_id: 0,
        }
    }

    #[inline]
    pub fn unknown() -> Self {
        Self {
            base: VarBaseState::Unknown,
            borrows: 0,
            mut_borrows: 0,
            borrow_set_id: 0,
        }
    }

    #[inline]
    pub fn is_defined(&self) -> bool {
        !matches!(self.base, VarBaseState::Unknown)
    }

    #[inline]
    pub fn is_moved(&self) -> bool {
        matches!(self.base, VarBaseState::Moved { .. })
    }

    #[inline]
    pub fn is_owned(&self) -> bool {
        matches!(self.base, VarBaseState::Owned)
    }

    pub fn join(&self, other: &Self, interner: &mut BorrowSetInterner) -> Self {
        if self == other {
            return self.clone();
        }
        let borrow_set_id = interner.union(self.borrow_set_id, other.borrow_set_id);
        let active_count = interner.len(borrow_set_id);
        let borrows = active_count.max(self.borrows).max(other.borrows);
        let mut_borrows = self.mut_borrows.max(other.mut_borrows);

        let base = match (&self.base, &other.base) {
            (VarBaseState::Uninit, other_b) | (other_b, VarBaseState::Uninit) => other_b.clone(),
            (VarBaseState::Unknown, _) | (_, VarBaseState::Unknown) => VarBaseState::Unknown,
            (VarBaseState::Moved { at_inst, reason }, VarBaseState::Moved { .. }) => {
                VarBaseState::Moved {
                    at_inst: at_inst.clone(),
                    reason: reason.clone(),
                }
            }
            (VarBaseState::Owned, VarBaseState::Owned) => VarBaseState::Owned,
            (VarBaseState::Owned, VarBaseState::Moved { .. })
            | (VarBaseState::Moved { .. }, VarBaseState::Owned) => VarBaseState::Unknown,
        };

        Self {
            base,
            borrows,
            mut_borrows,
            borrow_set_id,
        }
    }

    pub fn to_abstract(&self, interner: &BorrowSetInterner) -> AbstractVarState {
        AbstractVarState {
            base: self.base.clone(),
            borrows: self.borrows,
            mut_borrows: self.mut_borrows,
            active_borrows: interner.to_hash_set(self.borrow_set_id),
        }
    }
}

/// Dense function dataflow state holding flat vectors for fast indexing and cache locality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenseFunctionDataflowState {
    pub vars: Vec<DenseVarState>,
    pub values: Vec<DenseVarState>,
    pub val_origins: Vec<Option<u32>>,
}

impl DenseFunctionDataflowState {
    pub fn new(var_count: usize, val_count: usize) -> Self {
        Self {
            vars: vec![DenseVarState::uninit(); var_count],
            values: vec![DenseVarState::uninit(); val_count],
            val_origins: vec![None; val_count],
        }
    }

    #[inline]
    pub fn get_var(&self, idx: usize) -> &DenseVarState {
        self.vars.get(idx).unwrap_or(&DenseVarState::UNINIT)
    }

    #[inline]
    pub fn set_var(&mut self, idx: usize, st: DenseVarState) {
        if idx >= self.vars.len() {
            self.vars.resize(idx + 1, DenseVarState::uninit());
        }
        self.vars[idx] = st;
    }

    #[inline]
    pub fn get_value(&self, val: usize) -> &DenseVarState {
        self.values.get(val).unwrap_or(&DenseVarState::UNINIT)
    }

    #[inline]
    pub fn set_value(&mut self, val: usize, st: DenseVarState) {
        if val >= self.values.len() {
            self.values.resize(val + 1, DenseVarState::uninit());
            self.val_origins.resize(val + 1, None);
        }
        self.values[val] = st;
    }

    #[inline]
    pub fn get_val_origin(&self, val: usize) -> Option<u32> {
        self.val_origins.get(val).copied().flatten()
    }

    #[inline]
    pub fn set_val_origin(&mut self, val: usize, orig: Option<u32>) {
        if val >= self.val_origins.len() {
            self.values.resize(val + 1, DenseVarState::uninit());
            self.val_origins.resize(val + 1, None);
        }
        self.val_origins[val] = orig;
    }

    /// Flat join with slice-equality early exit for rapid convergence on loops and joins.
    pub fn join(&self, other: &Self, interner: &mut BorrowSetInterner) -> Self {
        if self == other {
            return self.clone();
        }

        let max_vars = self.vars.len().max(other.vars.len());
        let mut joined_vars = Vec::with_capacity(max_vars);
        for i in 0..max_vars {
            let s1 = self.vars.get(i).unwrap_or(&DenseVarState::UNINIT);
            let s2 = other.vars.get(i).unwrap_or(&DenseVarState::UNINIT);
            if s1 == s2 {
                joined_vars.push(s1.clone());
            } else {
                joined_vars.push(s1.join(s2, interner));
            }
        }

        let max_vals = self.values.len().max(other.values.len());
        let mut joined_values = Vec::with_capacity(max_vals);
        for i in 0..max_vals {
            let s1 = self.values.get(i).unwrap_or(&DenseVarState::UNINIT);
            let s2 = other.values.get(i).unwrap_or(&DenseVarState::UNINIT);
            if s1 == s2 {
                joined_values.push(s1.clone());
            } else {
                joined_values.push(s1.join(s2, interner));
            }
        }

        let mut val_origins = self.val_origins.clone();
        if val_origins.len() < max_vals {
            val_origins.resize(max_vals, None);
        }
        for (i, &opt_orig) in other.val_origins.iter().enumerate() {
            if i < val_origins.len() && val_origins[i].is_none() && opt_orig.is_some() {
                val_origins[i] = opt_orig;
            }
        }

        Self {
            vars: joined_vars,
            values: joined_values,
            val_origins,
        }
    }
}

/// Compute Reverse Post-Order (RPO) traversal of basic blocks starting from entry.
fn compute_rpo(
    entry: BasicBlockId,
    successors: &HashMap<BasicBlockId, Vec<BasicBlockId>>,
) -> Vec<BasicBlockId> {
    let mut visited = HashSet::new();
    let mut post_order = Vec::new();
    let mut stack = vec![(entry, false)];

    while let Some((node, processed)) = stack.pop() {
        if processed {
            post_order.push(node);
        } else if visited.insert(node) {
            stack.push((node, true));
            if let Some(succs) = successors.get(&node) {
                // Traverse successors in reverse so leftmost branch is processed first
                for &s in succs.iter().rev() {
                    if !visited.contains(&s) {
                        stack.push((s, false));
                    }
                }
            }
        }
    }

    post_order.reverse();
    post_order
}

#[derive(Debug, Clone, Default)]
pub struct OwnershipFunctionReport {
    pub func_name: String,
    pub total_states: usize,
    pub defined_states: usize,
    pub guarded_states: usize,
    pub rejected_states: usize,
    pub proven_ratio: f64,
    pub is_proven: bool,
    pub trace_line: String,
}

pub struct DmirOwnershipAnalyzer<'a> {
    pub resolver: &'a Resolver,
    pub fn_signatures: HashMap<String, Vec<Param>>,
    pub class_names: HashSet<String>,
}

impl<'a> DmirOwnershipAnalyzer<'a> {
    pub fn new(resolver: &'a Resolver) -> Self {
        let mut class_names = HashSet::new();
        for name in resolver.classes.keys() {
            class_names.insert(name.clone());
        }

        Self {
            resolver,
            fn_signatures: HashMap::new(),
            class_names,
        }
    }

    pub fn set_signatures(&mut self, sigs: HashMap<String, Vec<Param>>) {
        self.fn_signatures = sigs;
    }

    /// Run abstract interpretation and dual-mode lowering on all functions in the module.
    pub fn analyze_and_lower_module(
        &self,
        module: &mut Module,
        diag: &mut DiagnosticEngine,
    ) -> HashMap<String, OwnershipFunctionReport> {
        let mut reports = HashMap::new();
        let mut func_names: Vec<String> = module.functions.keys().cloned().collect();
        func_names.sort();

        for name in func_names {
            if let Some(func) = module.functions.get_mut(&name) {
                let rep = self.analyze_and_lower_function(func, diag);
                reports.insert(name, rep);
            }
        }

        reports
    }

    pub fn analyze_and_lower_function(
        &self,
        func: &mut Function,
        diag: &mut DiagnosticEngine,
    ) -> OwnershipFunctionReport {
        let cfg = ControlFlowGraph::build(func);
        let loop_headers: HashSet<BasicBlockId> = cfg.loops.iter().map(|l| l.header).collect();

        // 1. Pre-index all variable names and find max ValueId for dense vector allocation
        let mut var_names = Vec::new();
        let mut var_index = HashMap::new();

        let mut register_var = |name: &str| {
            if !var_index.contains_key(name) {
                let idx = var_names.len();
                var_names.push(name.to_string());
                var_index.insert(name.to_string(), idx);
            }
        };

        for (p_name, _, _) in &func.params {
            register_var(p_name);
        }
        for blk in &func.blocks {
            for p in &blk.params {
                if let Some(ref name) = p.name {
                    register_var(name);
                }
            }
            for inst in &blk.instructions {
                match inst {
                    Inst::AssignVar { name, .. } => register_var(name),
                    Inst::LoadVar { name, .. } => register_var(name),
                    _ => {}
                }
            }
        }

        let mut max_val_id = 0usize;
        for (_, _, p_val) in &func.params {
            max_val_id = max_val_id.max(p_val.0);
        }
        for blk in &func.blocks {
            for p in &blk.params {
                max_val_id = max_val_id.max(p.val.0);
            }
            for inst in &blk.instructions {
                self.visit_inst_val_ids(inst, |v| {
                    max_val_id = max_val_id.max(v.0);
                });
            }
            match &blk.terminator {
                Terminator::Return { value: Some(val) } => {
                    max_val_id = max_val_id.max(val.0);
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    max_val_id = max_val_id.max(cond.0);
                    for a in then_args.iter().chain(else_args.iter()) {
                        max_val_id = max_val_id.max(a.0);
                    }
                }
                Terminator::Branch { args, .. } => {
                    for a in args {
                        max_val_id = max_val_id.max(a.0);
                    }
                }
                _ => {}
            }
        }

        let var_count = var_names.len();
        let val_count = max_val_id + 1;

        // Wave B: Hash-consed borrow set interner for this function
        let mut interner = BorrowSetInterner::new();

        // 2. Dataflow forward fixpoint iteration
        let mut in_states: HashMap<BasicBlockId, DenseFunctionDataflowState> = HashMap::new();
        let mut out_states: HashMap<BasicBlockId, DenseFunctionDataflowState> = HashMap::new();
        let mut loop_iterations: HashMap<BasicBlockId, usize> = HashMap::new();

        // Initialize entry block with function parameters as Owned
        let mut entry_state = DenseFunctionDataflowState::new(var_count, val_count);
        for (p_name, _p_ty, p_val) in &func.params {
            let state = DenseVarState::owned();
            if let Some(&var_idx) = var_index.get(p_name) {
                entry_state.set_var(var_idx, state.clone());
                entry_state.set_val_origin(p_val.0, Some(var_idx as u32));
            }
            entry_state.set_value(p_val.0, state);
        }
        in_states.insert(func.entry_block, entry_state);

        // Wave B: Reverse Post-Order (RPO) worklist with dirty bitset
        let rpo = compute_rpo(func.entry_block, &cfg.successors);
        let mut rpo_index: HashMap<BasicBlockId, usize> = HashMap::new();
        for (idx, &bb) in rpo.iter().enumerate() {
            rpo_index.insert(bb, idx);
        }

        let num_rpo = rpo.len();
        let mut dirty = vec![false; num_rpo];
        let mut dirty_count = 0usize;

        if let Some(&entry_idx) = rpo_index.get(&func.entry_block) {
            dirty[entry_idx] = true;
            dirty_count = 1;
        }

        while dirty_count > 0 {
            // Wave B: Pick lowest RPO index for forward topological processing
            let rpo_idx = dirty.iter().position(|&d| d).expect("dirty block exists");
            dirty[rpo_idx] = false;
            dirty_count -= 1;
            let bb_id = rpo[rpo_idx];

            // Compute in_state from predecessors
            let preds = cfg.predecessors.get(&bb_id).cloned().unwrap_or_default();
            let mut cur_in = if bb_id == func.entry_block && preds.is_empty() {
                in_states
                    .get(&bb_id)
                    .cloned()
                    .unwrap_or_else(|| DenseFunctionDataflowState::new(var_count, val_count))
            } else {
                let mut joined = DenseFunctionDataflowState::new(var_count, val_count);
                let mut first = true;
                for p in &preds {
                    if let Some(p_out) = out_states.get(p) {
                        let mut transferred = p_out.clone();
                        if let Some(p_blk) = func.get_block(*p) {
                            let branch_args = match &p_blk.terminator {
                                Terminator::Branch { target, args } if *target == bb_id => {
                                    Some(args)
                                }
                                Terminator::CondBranch {
                                    then_block,
                                    then_args,
                                    else_block,
                                    else_args,
                                    ..
                                } => {
                                    if *then_block == bb_id {
                                        Some(then_args)
                                    } else if *else_block == bb_id {
                                        Some(else_args)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            if let Some(args) = branch_args {
                                if let Some(target_blk) = func.get_block(bb_id) {
                                    for (i, param) in target_blk.params.iter().enumerate() {
                                        if let Some(arg_val) = args.get(i) {
                                            let st = p_out.get_value(arg_val.0).clone();
                                            transferred.set_value(param.val.0, st.clone());
                                            if let Some(ref name) = param.name {
                                                if let Some(&var_idx) = var_index.get(name) {
                                                    transferred.set_var(var_idx, st);
                                                    transferred.set_val_origin(
                                                        param.val.0,
                                                        Some(var_idx as u32),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if first {
                            joined = transferred;
                            first = false;
                        } else {
                            joined = joined.join(&transferred, &mut interner);
                        }
                    }
                }
                if first {
                    in_states
                        .get(&bb_id)
                        .cloned()
                        .unwrap_or_else(|| DenseFunctionDataflowState::new(var_count, val_count))
                } else {
                    joined
                }
            };

            // Widening at loop headers: cap iterations at 32
            if loop_headers.contains(&bb_id) {
                let count = loop_iterations.entry(bb_id).or_insert(0);
                *count += 1;
                if *count > 32 {
                    if let Some(prev_in) = in_states.get(&bb_id) {
                        for i in 0..cur_in.vars.len() {
                            if prev_in.vars.get(i) != cur_in.vars.get(i) {
                                cur_in.vars[i].base = VarBaseState::Unknown;
                            }
                        }
                        for i in 0..cur_in.values.len() {
                            if prev_in.values.get(i) != cur_in.values.get(i) {
                                cur_in.values[i].base = VarBaseState::Unknown;
                            }
                        }
                    }
                }
            }

            in_states.insert(bb_id, cur_in.clone());

            // Run transfer functions through instructions in bb (no use_states or diag overhead on hot path)
            let mut cur_state = cur_in;
            let block = func.get_block(bb_id).cloned();
            if let Some(blk) = block {
                for (inst_idx, inst) in blk.instructions.iter().enumerate() {
                    self.apply_transfer(
                        inst,
                        inst_idx,
                        bb_id,
                        &mut cur_state,
                        &var_names,
                        &var_index,
                        &mut interner,
                        None,
                        None,
                    );
                }
                self.check_terminator(
                    &blk.terminator,
                    blk.instructions.len(),
                    bb_id,
                    &mut cur_state,
                    &var_names,
                    &interner,
                    None,
                    None,
                );
            }

            let prev_out = out_states.get(&bb_id);
            if prev_out != Some(&cur_state) {
                out_states.insert(bb_id, cur_state);
                let succs = cfg.successors.get(&bb_id).cloned().unwrap_or_default();
                for s in succs {
                    if let Some(&s_idx) = rpo_index.get(&s) {
                        if !dirty[s_idx] {
                            dirty[s_idx] = true;
                            dirty_count += 1;
                        }
                    }
                }
            }
        }

        // Wave B+: Bourdoncle narrowing pass (bounded refinement of widened states)
        let has_widened = loop_iterations.values().any(|&c| c > 32);
        if has_widened {
            for _iteration in 0..2 {
                let mut changed = false;
                for &bb_id in &rpo {
                    if !loop_headers.contains(&bb_id) {
                        continue;
                    }
                    let preds = cfg.predecessors.get(&bb_id).cloned().unwrap_or_default();
                    if preds.is_empty() {
                        continue;
                    }

                    let mut candidate_in = DenseFunctionDataflowState::new(var_count, val_count);
                    let mut first = true;
                    for p in &preds {
                        if let Some(p_out) = out_states.get(p) {
                            let mut transferred = p_out.clone();
                            if let Some(p_blk) = func.get_block(*p) {
                                let branch_args = match &p_blk.terminator {
                                    Terminator::Branch { target, args } if *target == bb_id => {
                                        Some(args)
                                    }
                                    Terminator::CondBranch {
                                        then_block,
                                        then_args,
                                        else_block,
                                        else_args,
                                        ..
                                    } => {
                                        if *then_block == bb_id {
                                            Some(then_args)
                                        } else if *else_block == bb_id {
                                            Some(else_args)
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                };
                                if let Some(args) = branch_args {
                                    if let Some(target_blk) = func.get_block(bb_id) {
                                        for (i, param) in target_blk.params.iter().enumerate() {
                                            if let Some(arg_val) = args.get(i) {
                                                let st = p_out.get_value(arg_val.0).clone();
                                                transferred.set_value(param.val.0, st.clone());
                                                if let Some(ref name) = param.name {
                                                    if let Some(&var_idx) = var_index.get(name) {
                                                        transferred.set_var(var_idx, st);
                                                        transferred.set_val_origin(
                                                            param.val.0,
                                                            Some(var_idx as u32),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if first {
                                candidate_in = transferred;
                                first = false;
                            } else {
                                candidate_in = candidate_in.join(&transferred, &mut interner);
                            }
                        }
                    }

                    if let Some(cur_in) = in_states.get_mut(&bb_id) {
                        for i in 0..cur_in.vars.len() {
                            if cur_in.vars[i].base == VarBaseState::Unknown {
                                if let Some(cand_st) = candidate_in.vars.get(i) {
                                    if cand_st.base != VarBaseState::Unknown
                                        && cand_st.base != VarBaseState::Uninit
                                    {
                                        cur_in.vars[i].base = cand_st.base.clone();
                                        changed = true;
                                    }
                                }
                            }
                        }
                        for i in 0..cur_in.values.len() {
                            if cur_in.values[i].base == VarBaseState::Unknown {
                                if let Some(cand_st) = candidate_in.values.get(i) {
                                    if cand_st.base != VarBaseState::Unknown
                                        && cand_st.base != VarBaseState::Uninit
                                    {
                                        cur_in.values[i].base = cand_st.base.clone();
                                        changed = true;
                                    }
                                }
                            }
                        }

                        if changed {
                            let mut cur_state = cur_in.clone();
                            if let Some(blk) = func.get_block(bb_id) {
                                for (inst_idx, inst) in blk.instructions.iter().enumerate() {
                                    self.apply_transfer(
                                        inst,
                                        inst_idx,
                                        bb_id,
                                        &mut cur_state,
                                        &var_names,
                                        &var_index,
                                        &mut interner,
                                        None,
                                        None,
                                    );
                                }
                                self.check_terminator(
                                    &blk.terminator,
                                    blk.instructions.len(),
                                    bb_id,
                                    &mut cur_state,
                                    &var_names,
                                    &interner,
                                    None,
                                    None,
                                );
                            }
                            out_states.insert(bb_id, cur_state);
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        // 3. Post-convergence diagnostic and lowering pass
        let mut use_states_per_inst: HashMap<(BasicBlockId, usize, ValueId), AbstractVarState> =
            HashMap::new();

        for blk in &func.blocks {
            if let Some(in_state) = in_states.get(&blk.id) {
                let mut cur_state = in_state.clone();
                for (inst_idx, inst) in blk.instructions.iter().enumerate() {
                    self.apply_transfer(
                        inst,
                        inst_idx,
                        blk.id,
                        &mut cur_state,
                        &var_names,
                        &var_index,
                        &mut interner,
                        Some(&mut use_states_per_inst),
                        Some(diag),
                    );
                }
                self.check_terminator(
                    &blk.terminator,
                    blk.instructions.len(),
                    blk.id,
                    &mut cur_state,
                    &var_names,
                    &interner,
                    Some(&mut use_states_per_inst),
                    Some(diag),
                );
            }
        }

        // 4. Compute proven_ratio & dual-mode lowering
        let mut total_states = 0usize;
        let mut defined_states = 0usize;
        let mut guarded_states = 0usize;
        let mut rejected_states = 0usize;

        let mut values_to_guard: HashSet<(BasicBlockId, usize, ValueId)> = HashSet::new();

        for ((bb, inst_idx, val), state) in &use_states_per_inst {
            total_states += 1;
            if state.is_defined() && !state.is_moved() {
                defined_states += 1;
            } else if state.is_moved() {
                rejected_states += 1;
            } else {
                // Unknown / unproven state
                guarded_states += 1;
                values_to_guard.insert((*bb, *inst_idx, *val));
            }
        }

        let proven_ratio = if total_states == 0 {
            1.0
        } else {
            defined_states as f64 / total_states as f64
        };
        let is_proven = proven_ratio >= 1.0;

        let proven_pct = if total_states == 0 {
            100.0
        } else {
            (defined_states as f64 / total_states as f64) * 100.0
        };
        let guarded_pct = if total_states == 0 {
            0.0
        } else {
            (guarded_states as f64 / total_states as f64) * 100.0
        };
        let rejected_pct = if total_states == 0 {
            0.0
        } else {
            (rejected_states as f64 / total_states as f64) * 100.0
        };

        let format_pct = |v: f64| -> String {
            if v.fract().abs() < 1e-6 {
                format!("{:.0}", v)
            } else {
                format!("{:.1}", v)
            }
        };

        let trace_line = format!(
            "Ownership: {}% proven, {}% guarded, {}% rejected (compile error only for definite use-after-move proven by the fixpoint)",
            format_pct(proven_pct),
            format_pct(guarded_pct),
            format_pct(rejected_pct)
        );

        // 5. Lowering: If guarded, insert runtime ownership guards
        if !is_proven && !values_to_guard.is_empty() {
            self.insert_runtime_guards(func, &values_to_guard);
        }

        OwnershipFunctionReport {
            func_name: func.name.clone(),
            total_states,
            defined_states,
            guarded_states,
            rejected_states,
            proven_ratio,
            is_proven,
            trace_line,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_transfer(
        &self,
        inst: &Inst,
        inst_idx: usize,
        bb_id: BasicBlockId,
        state: &mut DenseFunctionDataflowState,
        var_names: &[String],
        var_index: &HashMap<String, usize>,
        interner: &mut BorrowSetInterner,
        mut use_states: Option<&mut HashMap<(BasicBlockId, usize, ValueId), AbstractVarState>>,
        mut diag: Option<&mut DiagnosticEngine>,
    ) {
        match inst {
            Inst::AssignVar { name, value } => {
                let val_state = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), val_state.to_abstract(interner));
                }

                // Check active borrow conflict on re-assignment
                if let Some(&var_idx) = var_index.get(name) {
                    let prev = state.get_var(var_idx);
                    if prev.borrows > 0 {
                        if let Some(ref mut d) = diag {
                            d.error_with_help(
                                ErrorCode::BorrowConflictActiveView,
                                format!(
                                    "Cannot mutate or reassign '{}' while active borrow exists",
                                    name
                                ),
                                None,
                                Some(format!(
                                    "ensure view is finished before modifying '{}'",
                                    name
                                )),
                            );
                        }
                    }

                    // Killed state replaced
                    state.set_var(var_idx, val_state);
                }
            }

            Inst::LoadVar { dest, name } => {
                let (var_idx, var_state) = if let Some(&idx) = var_index.get(name) {
                    (Some(idx), state.get_var(idx).clone())
                } else {
                    (None, DenseVarState::uninit())
                };

                // Check if variable was moved
                if let VarBaseState::Moved { at_inst, reason } = &var_state.base {
                    let at_info = at_inst.as_deref().unwrap_or("earlier call");
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!(
                                "Use of moved value '{}'. Value was moved at {} ({})",
                                name, at_info, reason
                            ),
                            None,
                        );
                    }
                } else if var_state.mut_borrows > 0 {
                    if let Some(ref mut d) = diag {
                        d.error_with_help(
                            ErrorCode::BorrowConflict,
                            format!(
                                "Cannot read '{}' because it is actively mutably borrowed",
                                name
                            ),
                            None,
                            Some(format!(
                                "ensure mutable view is finished before reading '{}'",
                                name
                            )),
                        );
                    }
                }

                if let Some(idx) = var_idx {
                    state.set_val_origin(dest.0, Some(idx as u32));
                }
                state.set_value(dest.0, var_state);
            }

            Inst::Call {
                dest,
                func,
                args,
                ty: _,
            } => {
                let is_destroy = func == "destroy" || func == "drop";
                let is_view = func == "view" || func == "borrow";
                let is_mut_view = func == "mut_view" || func == "mutView";

                let sig_params = self.fn_signatures.get(func).cloned();

                for (arg_idx, arg_val) in args.iter().enumerate() {
                    let arg_state = state.get_value(arg_val.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *arg_val), arg_state.to_abstract(interner));
                    }

                    let origin_name = state
                        .get_val_origin(arg_val.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned();

                    let is_owned_param = is_destroy
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| {
                                let mode = p.ownership_mode.as_str();
                                let is_class = p
                                    .type_node
                                    .as_ref()
                                    .is_some_and(|t| self.class_names.contains(&t.name));
                                (mode == "owned" || mode == "own") && is_class
                            })
                            .unwrap_or(false);

                    let is_borrow_param = is_view
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| p.ownership_mode == "view")
                            .unwrap_or(false);

                    let is_mut_borrow_param = is_mut_view
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| p.ownership_mode == "mut-view")
                            .unwrap_or(false);

                    if is_owned_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!("Cannot move '{}' because it was already moved", name),
                                    None,
                                );
                            }
                        } else if arg_state.borrows > 0 || arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowConflictActiveView,
                                    format!(
                                        "Cannot move '{}' because it is actively borrowed",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else {
                            let moved_st = DenseVarState::moved(
                                format!("consumed by call to '{}'", func),
                                Some(format!("call '{}'", func)),
                            );
                            state.set_value(arg_val.0, moved_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, moved_st);
                            }
                        }
                    } else if is_mut_borrow_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!(
                                        "Cannot borrow '{}' because it was previously moved",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else if arg_state.borrows > 0 || arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error_with_help(
                                    ErrorCode::BorrowMultipleMutableViews,
                                    format!("Cannot borrow '{}' as mutable because it is already borrowed", name),
                                    None,
                                    Some(format!("Datara enforces XOR view semantics: only one mutable view of '{}' can exist at a time", name)),
                                );
                            }
                        } else {
                            let mut borrowed_st = arg_state.clone();
                            borrowed_st.borrows += 1;
                            borrowed_st.mut_borrows += 1;
                            borrowed_st.borrow_set_id =
                                interner.insert(borrowed_st.borrow_set_id, func.clone());
                            state.set_value(arg_val.0, borrowed_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, borrowed_st);
                            }
                        }
                    } else if is_borrow_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!(
                                        "Cannot borrow '{}' because it was previously moved",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else if arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error_with_help(
                                    ErrorCode::BorrowConflictActiveView,
                                    format!("Cannot borrow '{}' as immutable because it is already mutably borrowed", name),
                                    None,
                                    Some(format!("ensure mutable view is released before creating immutable view of '{}'", name)),
                                );
                            }
                        } else {
                            let mut borrowed_st = arg_state.clone();
                            borrowed_st.borrows += 1;
                            borrowed_st.borrow_set_id =
                                interner.insert(borrowed_st.borrow_set_id, func.clone());
                            state.set_value(arg_val.0, borrowed_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, borrowed_st);
                            }
                        }
                    }
                }

                // Return value produces Owned temporary
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::MethodCall {
                dest,
                object,
                method,
                args,
                ty: _,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}' in method call '{}'", name, method),
                            None,
                        );
                    }
                }

                for arg in args {
                    let a_st = state.get_value(arg.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *arg), a_st.to_abstract(interner));
                    }
                }

                if method == "view" {
                    let mut b_st = obj_state;
                    b_st.borrows += 1;
                    b_st.borrow_set_id = interner.insert(b_st.borrow_set_id, "method_view".into());
                    state.set_value(object.0, b_st.clone());
                    if let Some(var_idx) = state.get_val_origin(object.0) {
                        state.set_var(var_idx as usize, b_st);
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::SetField {
                object,
                field: _,
                value,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                let val_state = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), val_state.to_abstract(interner));
                }

                // Conservative: state of base unchanged, but check active borrows on base
                if obj_state.borrows > 0 {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowConflictActiveView,
                            format!(
                                "Cannot mutate field of '{}' while active borrow exists",
                                name
                            ),
                            None,
                        );
                    }
                }
                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
            }

            Inst::GetField {
                dest,
                object,
                field: _,
                ty: _,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::BinOp {
                dest,
                op: _,
                left,
                right,
                ty: _,
            } => {
                let l_st = state.get_value(left.0).clone();
                let r_st = state.get_value(right.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *left), l_st.to_abstract(interner));
                    uses.insert((bb_id, inst_idx, *right), r_st.to_abstract(interner));
                }

                if l_st.is_moved() {
                    let name = state
                        .get_val_origin(left.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
                if r_st.is_moved() {
                    let name = state
                        .get_val_origin(right.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::UnOp {
                dest,
                op: _,
                operand,
                ty: _,
            } => {
                let op_st = state.get_value(operand.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *operand), op_st.to_abstract(interner));
                }

                if op_st.is_moved() {
                    let name = state
                        .get_val_origin(operand.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. } => {
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::GetFuncAddr { dest, .. } => {
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::InlineAsm { outputs, .. } => {
                for (_, dest) in outputs {
                    state.set_value(dest.0, DenseVarState::owned());
                }
            }

            Inst::Out { value } | Inst::Err { value } => {
                let v_st = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), v_st.to_abstract(interner));
                }

                if v_st.is_moved() {
                    let name = state
                        .get_val_origin(value.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "value".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
            }

            Inst::FormatStr {
                dest,
                parts: _,
                values,
            } => {
                for v in values {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                    if v_st.is_moved() {
                        let name = state
                            .get_val_origin(v.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "value".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::StructInit {
                dest,
                class_name: _,
                fields,
            } => {
                for (_, f_val) in fields {
                    let f_st = state.get_value(f_val.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *f_val), f_st.to_abstract(interner));
                    }
                    if f_st.is_moved() {
                        let name = state
                            .get_val_origin(f_val.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "field".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ty: _,
            } => {
                for v in [cond, then_val, else_val] {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Decide {
                dest,
                arms,
                else_val,
                ty: _,
            } => {
                for (cond, body) in arms {
                    for v in [cond, body] {
                        let v_st = state.get_value(v.0).clone();
                        if let Some(ref mut uses) = use_states {
                            uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                        }
                    }
                }
                if let Some(ev) = else_val {
                    let v_st = state.get_value(ev.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *ev), v_st.to_abstract(interner));
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Return { value } => {
                if let Some(v) = value {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                    if v_st.is_moved() {
                        let name = state
                            .get_val_origin(v.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "return value".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
            }

            _ => {}
        }
    }

    fn check_terminator(
        &self,
        terminator: &crate::dmir::Terminator,
        inst_idx: usize,
        bb_id: BasicBlockId,
        state: &mut DenseFunctionDataflowState,
        var_names: &[String],
        interner: &BorrowSetInterner,
        mut use_states: Option<&mut HashMap<(BasicBlockId, usize, ValueId), AbstractVarState>>,
        mut diag: Option<&mut DiagnosticEngine>,
    ) {
        match terminator {
            crate::dmir::Terminator::Return { value: Some(val) } => {
                let v_st = state.get_value(val.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *val), v_st.to_abstract(interner));
                }
                if v_st.is_moved() {
                    let name = state
                        .get_val_origin(val.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "return value".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
            }
            crate::dmir::Terminator::CondBranch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                let c_st = state.get_value(cond.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *cond), c_st.to_abstract(interner));
                }
                if c_st.is_moved() {
                    let name = state
                        .get_val_origin(cond.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "condition".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
                for a in then_args.iter().chain(else_args.iter()) {
                    let a_st = state.get_value(a.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *a), a_st.to_abstract(interner));
                    }
                    if a_st.is_moved() {
                        let name = state
                            .get_val_origin(a.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "argument".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
            }
            crate::dmir::Terminator::Branch { args, .. } => {
                for a in args {
                    let a_st = state.get_value(a.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *a), a_st.to_abstract(interner));
                    }
                    if a_st.is_moved() {
                        let name = state
                            .get_val_origin(a.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "argument".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Insert runtime ownership guards for unproven value uses:
    /// `datara_rt_own_acquire(val)` before use and `datara_rt_own_release(val)` after use.
    fn insert_runtime_guards(
        &self,
        func: &mut Function,
        values_to_guard: &HashSet<(BasicBlockId, usize, ValueId)>,
    ) {
        let mut max_val = 0usize;
        for (_, _, p_val) in &func.params {
            max_val = max_val.max(p_val.0);
        }
        for blk in &func.blocks {
            for p in &blk.params {
                max_val = max_val.max(p.val.0);
            }
            for inst in &blk.instructions {
                self.visit_inst_val_ids(inst, |v| {
                    max_val = max_val.max(v.0);
                });
            }
            match &blk.terminator {
                crate::dmir::Terminator::Return { value: Some(val) } => {
                    max_val = max_val.max(val.0);
                }
                crate::dmir::Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    max_val = max_val.max(cond.0);
                    for a in then_args.iter().chain(else_args.iter()) {
                        max_val = max_val.max(a.0);
                    }
                }
                crate::dmir::Terminator::Branch { args, .. } => {
                    for a in args {
                        max_val = max_val.max(a.0);
                    }
                }
                _ => {}
            }
        }

        let mut next_val_id = max_val + 1;
        let mut alloc_val = || {
            let id = ValueId(next_val_id);
            next_val_id += 1;
            id
        };

        for blk in &mut func.blocks {
            let orig_len = blk.instructions.len();
            let mut new_insts = Vec::new();
            for (inst_idx, inst) in blk.instructions.drain(..).enumerate() {
                // Check if any value used in this instruction needs guarding
                let mut guarded_in_inst = Vec::new();
                for (bb, idx, val) in values_to_guard {
                    if *bb == blk.id && *idx == inst_idx {
                        guarded_in_inst.push(*val);
                    }
                }

                // Insert acquire before instruction
                for val in &guarded_in_inst {
                    let acq_dest = alloc_val();
                    new_insts.push(Inst::Call {
                        dest: acq_dest,
                        func: "datara_rt_own_acquire".into(),
                        args: vec![*val],
                        ty: "Int".into(),
                    });
                }

                new_insts.push(inst);

                // Insert release after instruction
                for val in &guarded_in_inst {
                    let rel_dest = alloc_val();
                    new_insts.push(Inst::Call {
                        dest: rel_dest,
                        func: "datara_rt_own_release".into(),
                        args: vec![*val],
                        ty: "Unit".into(),
                    });
                }
            }

            // Check if any value used in terminator needs guarding
            let mut guarded_in_term = Vec::new();
            for (bb, idx, val) in values_to_guard {
                if *bb == blk.id && *idx >= orig_len {
                    guarded_in_term.push(*val);
                }
            }
            for val in &guarded_in_term {
                let acq_dest = alloc_val();
                new_insts.push(Inst::Call {
                    dest: acq_dest,
                    func: "datara_rt_own_acquire".into(),
                    args: vec![*val],
                    ty: "Int".into(),
                });
                let rel_dest = alloc_val();
                new_insts.push(Inst::Call {
                    dest: rel_dest,
                    func: "datara_rt_own_release".into(),
                    args: vec![*val],
                    ty: "Unit".into(),
                });
            }

            blk.instructions = new_insts;
        }
    }

    fn visit_inst_val_ids<F: FnMut(ValueId)>(&self, inst: &Inst, mut f: F) {
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::GetFuncAddr { dest, .. }
            | Inst::LoadVar { dest, .. } => f(*dest),
            Inst::AssignVar { value, .. } => f(*value),
            Inst::BinOp {
                dest, left, right, ..
            } => {
                f(*dest);
                f(*left);
                f(*right);
            }
            Inst::UnOp { dest, operand, .. } => {
                f(*dest);
                f(*operand);
            }
            Inst::Call { dest, args, .. } => {
                f(*dest);
                for a in args {
                    f(*a);
                }
            }
            Inst::MethodCall {
                dest, object, args, ..
            } => {
                f(*dest);
                f(*object);
                for a in args {
                    f(*a);
                }
            }
            Inst::StructInit { dest, fields, .. } => {
                f(*dest);
                for (_, v) in fields {
                    f(*v);
                }
            }
            Inst::GetField { dest, object, .. } => {
                f(*dest);
                f(*object);
            }
            Inst::SetField { object, value, .. } => {
                f(*object);
                f(*value);
            }
            Inst::Out { value } | Inst::Err { value } => f(*value),
            Inst::FormatStr { dest, values, .. } => {
                f(*dest);
                for v in values {
                    f(*v);
                }
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ..
            } => {
                f(*dest);
                f(*cond);
                f(*then_val);
                f(*else_val);
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ..
            } => {
                f(*dest);
                for (c, b) in arms {
                    f(*c);
                    f(*b);
                }
                if let Some(ev) = else_val {
                    f(*ev);
                }
            }
            Inst::Return { value } => {
                if let Some(v) = value {
                    f(*v);
                }
            }
            Inst::InlineAsm {
                outputs, inputs, ..
            } => {
                for (_, d) in outputs {
                    f(*d);
                }
                for (_, i) in inputs {
                    f(*i);
                }
            }
            _ => {}
        }
    }
}
