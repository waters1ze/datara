use crate::dmir::{BasicBlockId, ValueId};
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
pub(crate) fn compute_rpo(
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
