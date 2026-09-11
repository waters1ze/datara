//! Abstract interpretation over DMIR for graduated ownership guarantees.
//!
//! Provides:
//! - Per-variable abstract state: Uninit < Owned < Moved alongside Borrowed{n}.
//! - Dataflow: forward, may-analysis for borrows, must-analysis for moves.
//! - Iteration to saturation with widening at loop headers (cap at 32 iterations).
//! - Graduated dual-mode lowering: proven (zero-cost) vs guarded (runtime refcount guards).

use crate::ast::Param;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use crate::resolver::Resolver;
use std::collections::{HashMap, HashSet};

pub use super::domain::*;
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
