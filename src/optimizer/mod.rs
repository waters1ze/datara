use crate::dmir::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub mod adaptive;
pub mod const_fold;
pub mod cost_model;
pub mod dce;
pub mod evidence;
pub mod inline;
pub mod ipo;
pub mod loops;
pub mod mem2reg;
pub mod memory;
pub mod pipeline_fusion;
pub mod recursion;
pub mod scalar;

use adaptive::SemanticAdaptationEngine;
use cost_model::{CostModel, OptimizationDecisionTrace};
use loops::LoopOptimizer;
use memory::MemoryOptimizer;
use pipeline_fusion::PipelineFusionOptimizer;
use scalar::ScalarOptimizer;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptimizationReport {
    pub modules_analyzed: usize,
    pub symbols_analyzed: usize,
    pub reachable_symbols: usize,
    pub removed_symbols: usize,
    pub generic_specializations: Vec<String>,
    pub constants_folded: usize,
    pub dead_instructions_removed: usize,
    pub functions_inlined: usize,
    pub allocations_eliminated: usize,
    /// Named scalar variables promoted into SSA values / block parameters by
    /// the mem2reg pass. Zero means the IR was already fully SSA (or the pass
    /// could not prove a sound promotion).
    pub variables_promoted: usize,
    /// Records mechanically downgraded Applied -> Rejected because the pass
    /// left the IR unchanged. Always zero or more; never a silent event.
    pub evidence_downgrades: usize,
    /// Bounds checks proven safe and eliminated into unchecked accesses.
    #[serde(default)]
    pub bce_proven: usize,
    pub runtime_modules_linked: Vec<String>,
    pub runtime_modules_stripped: Vec<String>,
    pub decision_trace: Vec<cost_model::DecisionRecord>,
    pub adaptation_records: Vec<adaptive::AdaptationRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub ownership_reports: HashMap<String, String>,
}

pub struct Optimizer {
    pub mode: String,
    pub report: OptimizationReport,
    pub cost_model: CostModel,
    pub trace: OptimizationDecisionTrace,
    pub sae: SemanticAdaptationEngine,
    pub function_effects: HashMap<String, crate::effects::EffectSet>,
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
}

impl Optimizer {
    pub fn new(mode: &str) -> Self {
        let cost_model = CostModel::new(mode);
        let sae = SemanticAdaptationEngine::new(mode);

        Self {
            mode: mode.to_string(),
            function_effects: HashMap::new(),
            report: OptimizationReport {
                modules_analyzed: 1,
                symbols_analyzed: 0,
                reachable_symbols: 0,
                removed_symbols: 0,
                generic_specializations: Vec::new(),
                constants_folded: 0,
                dead_instructions_removed: 0,
                functions_inlined: 0,
                allocations_eliminated: 0,
                variables_promoted: 0,
                evidence_downgrades: 0,
                bce_proven: 0,
                runtime_modules_linked: vec!["core".into()],
                runtime_modules_stripped: vec![
                    "network".into(),
                    "database".into(),
                    "reflection".into(),
                ],
                decision_trace: Vec::new(),
                adaptation_records: Vec::new(),
                ownership: None,
                ownership_reports: HashMap::new(),
            },
            cost_model,
            trace: OptimizationDecisionTrace::new(),
            sae,
            diagnostics: Vec::new(),
        }
    }

    pub fn record_ownership_report(&mut self, fn_name: &str, trace_line: &str) {
        self.report
            .ownership_reports
            .insert(fn_name.to_string(), trace_line.to_string());
        if self.report.ownership.is_none() || fn_name == "main" {
            self.report.ownership = Some(trace_line.to_string());
        }
        self.trace.record(
            "ownership",
            fn_name,
            "Graduated Analysis",
            "Zero-cost / Guarded",
            "None",
            trace_line,
        );
    }

    pub fn optimize_module(
        &mut self,
        module: &mut Module,
    ) -> Result<(), crate::diagnostics::Diagnostic> {
        if let Err(error) = crate::dmir::verify_module(module) {
            let diag = crate::diagnostics::Diagnostic::error(
                crate::diagnostics::ErrorCode::InternalVerification,
                format!(
                    "[E0901] DMIR verification failed before optimization: {}",
                    error
                ),
                None,
            );
            self.diagnostics.push(diag.clone());
            return Err(diag);
        }
        self.report.symbols_analyzed = module.functions.len();

        if self.mode == "debug"
            || self.mode == "quick"
            || self.mode == "start"
            || self.mode == "check"
        {
            // Debug, quick, start, and check modes preserve verbatim IR for debugging/fast turnaround
            self.report.reachable_symbols = module.functions.len();
            return Ok(());
        }

        // 0. Semantic Adaptation Engine (SAE) pass
        self.sae.adapt_module(module);
        self.report.adaptation_records = self.sae.log.records.clone();

        let max_iterations = if self.mode == "domain" || self.mode == "release" {
            3
        } else {
            1
        };

        for _iter in 0..max_iterations {
            let fp_before = evidence::ir_fingerprint(module);

            // 1. Inlining pass (Inter-procedural optimization)
            if self.cost_model.inlining_threshold > 0 {
                self.run_mutating_pass("inline", module, |opt, m| {
                    opt.inline_pure_functions(m);
                })?;
            }

            // 1.5 Mem2Reg: promote named scalar variables into SSA values with
            // block parameters. Running it after inlining means inlined bodies are
            // promoted too, and before every other pass so the whole pipeline
            // operates on dominance-provable IR. `variables_promoted` is reported
            // honestly: the pass restores a function unchanged whenever it cannot
            // prove its own output well-formed.
            let mut promoted = 0usize;
            self.run_mutating_pass("mem2reg", module, |opt, m| {
                promoted = mem2reg::promote_module(m);
                if promoted > 0 {
                    opt.report.variables_promoted += promoted;
                    opt.trace.record(
                        "Mem2Reg",
                        "module:scalar_vars",
                        "Applied",
                        &format!("{} named variables promoted", promoted),
                        "None (single linear pass)",
                        "LoadVar/AssignVar pairs rewritten to SSA block parameters; \
                         every load dominated by a definition was proven before rewriting",
                    );
                }
            })?;

            // 2. Intra-procedural optimizations (SROA, Constant Folding, DCE, LoopFold)
            self.run_mutating_pass("intraproc", module, |opt, m| {
                let mut fn_names: Vec<String> = m.functions.keys().cloned().collect();
                fn_names.sort();
                for name in fn_names {
                    if let Some(f) = m.functions.get_mut(&name) {
                        opt.optimize_function(f);
                    }
                }
            })?;

            // 2.2 Interprocedural Optimization (Phase 14: Specialization, Devirtualization, Cross-Module Pure Inlining)
            if self.mode == "domain" || self.mode == "release" {
                self.run_mutating_pass("ipo", module, |opt, m| {
                    ipo::InterproceduralOptimizer::optimize_module(m, &mut opt.trace);
                })?;
            }

            // Tail recursion & sibling recursion elimination (domain and release mode)
            if self.mode == "domain" || self.mode == "release" {
                self.run_mutating_pass("tail_recursion", module, |opt, m| {
                    let mut fn_names: Vec<String> = m.functions.keys().cloned().collect();
                    fn_names.sort();
                    for name in fn_names {
                        let is_pure = opt
                            .function_effects
                            .get(&name)
                            .map(|s| s.is_pure())
                            .unwrap_or(true);
                        if let Some(f) = m.functions.get_mut(&name) {
                            if recursion::eliminate_tail_recursion(f) {
                                opt.trace.record(
                                    "TailRecursionElimination",
                                    &name,
                                    "Applied",
                                    "Tail-recursive call converted to iterative loop",
                                    "O(1) stack frames",
                                    "Tail call converted into loop with block parameters; eliminates stack overflow",
                                );
                            } else if is_pure && recursion::eliminate_sibling_recursion(f) {
                                opt.trace.record(
                                    "TailRecursionElimination",
                                    &name,
                                    "Applied",
                                    "Binary recursion sibling call converted to loop accumulator",
                                    "O(1) extra block parameters",
                                    "Additive binary recursion converted into single recursive loop; eliminates 50% call overhead",
                                );
                            }
                        }
                    }
                })?;
            }

            let fp_after = evidence::ir_fingerprint(module);
            if fp_before == fp_after {
                break;
            }
        }

        // 3. Reachability Analysis & Dead Symbol Elimination (in domain/release mode)
        if self.mode == "domain" || self.mode == "release" {
            self.run_mutating_pass("dead_symbol_elimination", module, |opt, m| {
                opt.dead_symbol_elimination(m);
            })?;
        }

        if let Err(error) = crate::dmir::verify_module(module) {
            let diag = crate::diagnostics::Diagnostic::error(
                crate::diagnostics::ErrorCode::InternalVerification,
                format!(
                    "[E0901] DMIR verification failed after optimization: {}",
                    error
                ),
                None,
            );
            self.diagnostics.push(diag.clone());
            return Err(diag);
        }

        // Finalize decision trace
        self.report.decision_trace = self.trace.records.clone();
        Ok(())
    }

    /// Evidence gate around one mutating pass.
    ///
    /// 1. Fingerprint the IR before the pass.
    /// 2. Run the pass.
    /// 3. Verify the IR (fail-closed): a pass that corrupts DMIR records
    ///    a diagnostic error instead of crashing the process.
    /// 4. Fingerprint the IR after the pass. If it is unchanged, every
    ///    `Applied` record the pass emitted during this invocation is
    ///    downgraded to `Rejected` and every counter movement is reverted.
    pub fn run_mutating_pass<F>(
        &mut self,
        label: &str,
        module: &mut Module,
        pass: F,
    ) -> Result<(), crate::diagnostics::Diagnostic>
    where
        F: FnOnce(&mut Self, &mut Module),
    {
        let before = evidence::ir_fingerprint(module);
        let records_start = self.trace.records.len();
        let counters = evidence::CountersSnapshot::capture(&self.report);

        pass(self, module);

        if let Err(error) = crate::dmir::verify_module(module) {
            let diag = crate::diagnostics::Diagnostic::error(
                crate::diagnostics::ErrorCode::InternalVerification,
                format!(
                    "[E0901] DMIR verification failed after optimizer pass '{}': {}",
                    label, error
                ),
                None,
            );
            self.diagnostics.push(diag.clone());
            return Err(diag);
        }

        let after = evidence::ir_fingerprint(module);
        if after == before {
            let downgraded =
                evidence::downgrade_applied_without_delta(&mut self.trace.records, records_start);
            counters.restore(&mut self.report);
            self.report.evidence_downgrades += downgraded;
        }
        Ok(())
    }

    pub fn optimize_function(&mut self, f: &mut Function) {
        let mut changed = true;
        let mut iterations = 0;
        let max_iterations = if self.mode == "domain" { 10 } else { 3 };

        while changed && iterations < max_iterations {
            changed = false;
            iterations += 1;

            // Loop-idiom recognition runs first: if the pattern is incomplete
            // because dead code still occupies the loop body, the later DCE
            // cleans it and the next iteration folds successfully.
            if LoopOptimizer::fold_loops(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if ScalarOptimizer::eliminate_common_subexpressions(
                f,
                &self.cost_model,
                &mut self.trace,
            ) > 0
            {
                changed = true;
            }
            if ScalarOptimizer::apply_strength_reduction(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if MemoryOptimizer::scalarize_structures(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if self.scalarize_structures(f) {
                changed = true;
            }
            if LoopOptimizer::optimize_loops(
                f,
                &self.cost_model,
                &mut self.trace,
                &mut self.report.bce_proven,
            ) > 0
            {
                changed = true;
            }
            if PipelineFusionOptimizer::fuse_pipelines(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if self.constant_fold(f) {
                changed = true;
            }
            if self.dead_code_elimination(f) {
                changed = true;
            }
            if self.convert_branches_to_select(f) {
                changed = true;
            }
            if self.merge_blocks(f) {
                changed = true;
            }
        }
    }

    fn merge_blocks(&mut self, f: &mut Function) -> bool {
        if f.blocks.len() <= 1 {
            return false;
        }

        let mut preds: HashMap<BasicBlockId, usize> = HashMap::new();
        for b in &f.blocks {
            match &b.terminator {
                Terminator::Branch { target, .. } => {
                    *preds.entry(*target).or_insert(0) += 1;
                }
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => {
                    *preds.entry(*then_block).or_insert(0) += 1;
                    *preds.entry(*else_block).or_insert(0) += 1;
                }
                _ => {}
            }
        }

        let mut merge_candidate: Option<(usize, BasicBlockId, Vec<ValueId>)> = None;
        for (i, b) in f.blocks.iter().enumerate() {
            if let Terminator::Branch { target, args } = &b.terminator
                && *target != b.id
                && *target != f.entry_block
                && preds.get(target).copied().unwrap_or(0) == 1
                && let Some(target_b) = f.blocks.iter().find(|blk| blk.id == *target)
                && target_b.params.len() == args.len()
            {
                merge_candidate = Some((i, *target, args.clone()));
                break;
            }
        }

        if let Some((a_idx, target_id, branch_args)) = merge_candidate {
            let target_pos = f.blocks.iter().position(|b| b.id == target_id).unwrap();
            let mut target_b = f.blocks.remove(target_pos);
            let mut subst: HashMap<ValueId, ValueId> = HashMap::new();
            for (p, arg) in target_b.params.iter().zip(branch_args.iter()) {
                subst.insert(p.val, *arg);
            }
            if !subst.is_empty() {
                for b in &mut f.blocks {
                    for inst in &mut b.instructions {
                        Self::substitute_operands(inst, &subst);
                    }
                    Self::substitute_terminator(&mut b.terminator, &subst);
                }
                for inst in &mut target_b.instructions {
                    Self::substitute_operands(inst, &subst);
                }
                Self::substitute_terminator(&mut target_b.terminator, &subst);
            }

            let actual_a_idx = if target_pos < a_idx { a_idx - 1 } else { a_idx };
            let a = &mut f.blocks[actual_a_idx];
            a.instructions.extend(target_b.instructions);
            a.terminator = target_b.terminator;
            return true;
        }

        false
    }

    /// If-Conversion pass: detects diamond CFG patterns and collapses them into
    /// branchless `Inst::Select` (lowering to CMOV / CSEL), eliminating branch
    /// mispredictions.
    fn convert_branches_to_select(&mut self, f: &mut Function) -> bool {
        if f.blocks.len() <= 2 {
            return false;
        }

        // Count predecessors
        let mut preds: HashMap<BasicBlockId, usize> = HashMap::new();
        for b in &f.blocks {
            match &b.terminator {
                Terminator::Branch { target, .. } => {
                    *preds.entry(*target).or_insert(0) += 1;
                }
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => {
                    *preds.entry(*then_block).or_insert(0) += 1;
                    *preds.entry(*else_block).or_insert(0) += 1;
                }
                _ => {}
            }
        }

        let mut candidate: Option<(usize, BasicBlockId, BasicBlockId, BasicBlockId, ValueId)> =
            None;

        for (b_idx, b) in f.blocks.iter().enumerate() {
            if let Terminator::CondBranch {
                cond,
                then_block,
                then_args,
                else_block,
                else_args,
            } = &b.terminator
            {
                if !then_args.is_empty() || !else_args.is_empty() {
                    continue;
                }
                if then_block == else_block {
                    continue;
                }
                if preds.get(then_block).copied().unwrap_or(0) != 1
                    || preds.get(else_block).copied().unwrap_or(0) != 1
                {
                    continue;
                }

                let then_b = match f.blocks.iter().find(|blk| blk.id == *then_block) {
                    Some(blk) if blk.params.is_empty() && blk.instructions.len() <= 4 => blk,
                    _ => continue,
                };
                let else_b = match f.blocks.iter().find(|blk| blk.id == *else_block) {
                    Some(blk) if blk.params.is_empty() && blk.instructions.len() <= 4 => blk,
                    _ => continue,
                };

                let (then_target, then_branch_args) = match &then_b.terminator {
                    Terminator::Branch { target, args } => (*target, args.clone()),
                    _ => continue,
                };
                let (else_target, else_branch_args) = match &else_b.terminator {
                    Terminator::Branch { target, args } => (*target, args.clone()),
                    _ => continue,
                };

                // Both must merge to the same target block with matching argument counts
                if then_target != else_target || then_branch_args.len() != else_branch_args.len() {
                    continue;
                }

                // Check that all instructions in both blocks are pure
                let is_pure = |inst: &Inst| {
                    matches!(
                        inst,
                        Inst::ConstInt { .. }
                            | Inst::ConstFloat { .. }
                            | Inst::ConstBool { .. }
                            | Inst::ConstStr { .. }
                            | Inst::BinOp { .. }
                            | Inst::UnOp { .. }
                            | Inst::Select { .. }
                    )
                };

                if !then_b.instructions.iter().all(is_pure)
                    || !else_b.instructions.iter().all(is_pure)
                {
                    continue;
                }

                // Both arms execute unconditionally after conversion, so a
                // trapping op (`/`, `%` on integers) in either arm could fault
                // on inputs the original program never executed.
                if then_b.instructions.iter().any(Self::may_trap)
                    || else_b.instructions.iter().any(Self::may_trap)
                {
                    continue;
                }

                candidate = Some((b_idx, *then_block, *else_block, then_target, *cond));
                break;
            }
        }

        if let Some((b_idx, then_id, else_id, merge_target, cond)) = candidate {
            let head_id = f.blocks[b_idx].id;

            // A diamond whose merge point is one of the removed blocks or the
            // head block itself is a loop, not a straight-line select: leave
            // it alone.
            if merge_target == head_id || merge_target == then_id || merge_target == else_id {
                return false;
            }

            let mut max_id = self.max_value_id_in_function(f);

            let then_pos = f.blocks.iter().position(|b| b.id == then_id).unwrap();
            let then_b = f.blocks[then_pos].clone();

            let else_pos = f.blocks.iter().position(|b| b.id == else_id).unwrap();
            let else_b = f.blocks[else_pos].clone();

            let (then_args, else_args) = match (&then_b.terminator, &else_b.terminator) {
                (
                    Terminator::Branch { args: t_args, .. },
                    Terminator::Branch { args: e_args, .. },
                ) => (t_args.clone(), e_args.clone()),
                _ => return false,
            };

            // Infer the select operand types from value-producing arm
            // instructions so a Float/String diamond is not mislabelled "Int".
            let mut val_ty: HashMap<ValueId, String> = HashMap::new();
            for (_, p_ty, p_val) in &f.params {
                if !p_ty.is_empty() {
                    val_ty.insert(*p_val, p_ty.clone());
                }
            }
            for inst in f.blocks[b_idx]
                .instructions
                .iter()
                .chain(then_b.instructions.iter())
                .chain(else_b.instructions.iter())
            {
                match inst {
                    Inst::ConstInt { dest, .. } => {
                        val_ty.insert(*dest, "Int".to_string());
                    }
                    Inst::ConstFloat { dest, .. } => {
                        val_ty.insert(*dest, "Float".to_string());
                    }
                    Inst::ConstBool { dest, .. } => {
                        val_ty.insert(*dest, "Bool".to_string());
                    }
                    Inst::ConstStr { dest, .. } => {
                        val_ty.insert(*dest, "String".to_string());
                    }
                    Inst::BinOp { dest, ty, .. } | Inst::GetField { dest, ty, .. } => {
                        val_ty.insert(*dest, ty.clone());
                    }
                    Inst::UnOp {
                        dest, op, operand, ..
                    } if op == "copy" => {
                        if let Some(t) = val_ty.get(operand) {
                            val_ty.insert(*dest, t.clone());
                        }
                    }
                    _ => {}
                }
            }

            let merge_b = f.blocks.iter().find(|blk| blk.id == merge_target);

            // Determine the Select type for every differing arg pair up front.
            // If a pair's type cannot be proven — or the two arms disagree —
            // refuse the transformation instead of guessing a type.
            let mut sel_types: Vec<Option<String>> = Vec::new();
            for (idx, (t_val, e_val)) in then_args.iter().zip(else_args.iter()).enumerate() {
                if t_val == e_val {
                    sel_types.push(None);
                    continue;
                }
                let target_param_ty = merge_b
                    .and_then(|mb| mb.params.get(idx))
                    .map(|p| p.ty.clone())
                    .filter(|ty| !ty.is_empty());
                let sel_ty = match (val_ty.get(t_val), val_ty.get(e_val)) {
                    (Some(t), Some(e)) if t == e => Some(t.clone()),
                    _ => target_param_ty,
                };
                if sel_ty.is_none() {
                    return false;
                }
                sel_types.push(sel_ty);
            }

            let block = &mut f.blocks[b_idx];

            // Append instructions from both arms
            block.instructions.extend(then_b.instructions);
            block.instructions.extend(else_b.instructions);

            let mut merged_args = Vec::new();
            for ((t_val, e_val), sel_ty) in then_args.iter().zip(else_args.iter()).zip(sel_types) {
                if t_val == e_val {
                    merged_args.push(*t_val);
                } else if let Some(sel_ty) = sel_ty {
                    max_id += 1;
                    let sel_dest = ValueId(max_id);
                    block.instructions.push(Inst::Select {
                        dest: sel_dest,
                        cond,
                        then_val: *t_val,
                        else_val: *e_val,
                        ty: sel_ty,
                    });
                    merged_args.push(sel_dest);
                }
            }

            block.terminator = Terminator::Branch {
                target: merge_target,
                args: merged_args,
            };

            // Remove the then and else blocks (in reverse index order)
            let mut remove_indices = [then_pos, else_pos];
            remove_indices.sort();
            f.blocks.remove(remove_indices[1]);
            f.blocks.remove(remove_indices[0]);

            // b_idx may have shifted after the removals above; the head block
            // itself is unchanged, so report by its stable id.
            self.trace.record(
                "IfConversion",
                &format!("bb{}", head_id.0),
                "Applied",
                "Diamond control flow collapsed to branchless Select",
                "1 Select instruction",
                "Eliminated conditional branch and 2 basic blocks; zero branch mispredictions",
            );

            return true;
        }

        false
    }

    fn scalarize_structures(&mut self, f: &mut Function) -> bool {
        let mut changed = false;
        let mut struct_inits: HashMap<ValueId, HashMap<String, ValueId>> = HashMap::new();
        let mut var_to_struct: HashMap<String, ValueId> = HashMap::new();
        let mut val_to_struct: HashMap<ValueId, ValueId> = HashMap::new();
        let mut escaping_structs: HashSet<ValueId> = HashSet::new();
        // Variables bound more than once (struct -> struct or struct ->
        // scalar) must never be scalarized: this pass is order-insensitive,
        // so a second binding could make an earlier LoadVar forward to the
        // wrong struct value.
        let mut var_ambiguous: HashSet<String> = HashSet::new();

        // Pass 1: Collect StructInits and variable bindings
        for block in &f.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::StructInit { dest, fields, .. } => {
                        let mut map = HashMap::new();
                        for (fname, fval) in fields {
                            map.insert(fname.clone(), *fval);
                            if let Some(s_id) = val_to_struct.get(fval) {
                                escaping_structs.insert(*s_id);
                            }
                        }
                        struct_inits.insert(*dest, map);
                        val_to_struct.insert(*dest, *dest);
                    }
                    Inst::AssignVar { name, value } => {
                        if let Some(s_id) = val_to_struct.get(value) {
                            if var_to_struct.contains_key(name) {
                                var_ambiguous.insert(name.clone());
                                escaping_structs.insert(var_to_struct[name]);
                                escaping_structs.insert(*s_id);
                            } else {
                                var_to_struct.insert(name.clone(), *s_id);
                            }
                        } else if var_to_struct.contains_key(name) {
                            // Reassigned to a non-struct value: later LoadVars
                            // must read the scalar, so disqualify the variable.
                            var_ambiguous.insert(name.clone());
                            escaping_structs.insert(var_to_struct[name]);
                            var_to_struct.remove(name);
                        }
                    }
                    // A field store mutates the struct: any forwarded GetField
                    // would read the stale initial field value. Treat it as an
                    // escape so the allocation (and honest field reads) survive.
                    Inst::SetField { object, value, .. } => {
                        if f.blocks.len() > 1
                            && let Some(s_id) = val_to_struct.get(object)
                        {
                            escaping_structs.insert(*s_id);
                        }
                        if let Some(s_id) = val_to_struct.get(value) {
                            escaping_structs.insert(*s_id);
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(s_id) = var_to_struct.get(name) {
                            val_to_struct.insert(*dest, *s_id);
                        }
                    }
                    Inst::UnOp {
                        dest, op, operand, ..
                    } if op == "copy" => {
                        if let Some(s_id) = val_to_struct.get(operand) {
                            val_to_struct.insert(*dest, *s_id);
                        }
                    }
                    Inst::MethodCall { object, args, .. } => {
                        if let Some(s_id) = val_to_struct.get(object) {
                            escaping_structs.insert(*s_id);
                        }
                        for a in args {
                            if let Some(s_id) = val_to_struct.get(a) {
                                escaping_structs.insert(*s_id);
                            }
                        }
                    }
                    Inst::Call { args, .. } => {
                        for a in args {
                            if let Some(s_id) = val_to_struct.get(a) {
                                escaping_structs.insert(*s_id);
                            }
                        }
                    }
                    Inst::Out { value } | Inst::Err { value } => {
                        if let Some(s_id) = val_to_struct.get(value) {
                            escaping_structs.insert(*s_id);
                        }
                    }
                    Inst::Return { value: Some(v) } => {
                        if let Some(s_id) = val_to_struct.get(v) {
                            escaping_structs.insert(*s_id);
                        }
                    }
                    Inst::FormatStr { values, .. } => {
                        for v in values {
                            if let Some(s_id) = val_to_struct.get(v) {
                                escaping_structs.insert(*s_id);
                            }
                        }
                    }
                    Inst::WhileLoop {
                        condition_insts,
                        cond_val,
                        body_insts,
                    } => {
                        if let Some(s_id) = val_to_struct.get(cond_val) {
                            escaping_structs.insert(*s_id);
                        }
                        for ci in condition_insts {
                            self.visit_inst_vids(ci, &mut |v| {
                                if let Some(s_id) = val_to_struct.get(v) {
                                    escaping_structs.insert(*s_id);
                                }
                            });
                        }
                        for bi in body_insts {
                            self.visit_inst_vids(bi, &mut |v| {
                                if let Some(s_id) = val_to_struct.get(v) {
                                    escaping_structs.insert(*s_id);
                                }
                            });
                        }
                    }
                    Inst::TryCatch {
                        try_insts,
                        catch_insts,
                        ..
                    } => {
                        for ti in try_insts {
                            self.visit_inst_vids(ti, &mut |v| {
                                if let Some(s_id) = val_to_struct.get(v) {
                                    escaping_structs.insert(*s_id);
                                }
                            });
                        }
                        for ci in catch_insts {
                            self.visit_inst_vids(ci, &mut |v| {
                                if let Some(s_id) = val_to_struct.get(v) {
                                    escaping_structs.insert(*s_id);
                                }
                            });
                        }
                    }
                    _ => {}
                }
            }

            // Terminators are uses too. A `return` of a struct keeps the
            // allocation alive: returns live in `block.terminator`, not in
            // `instructions`. Likewise, passing a struct as a branch argument
            // across block boundaries keeps it alive as a block parameter.
            match &block.terminator {
                Terminator::Return { value: Some(v) } => {
                    if let Some(s_id) = val_to_struct.get(v) {
                        escaping_structs.insert(*s_id);
                    }
                }
                Terminator::Branch { args, .. } => {
                    for a in args {
                        if let Some(s_id) = val_to_struct.get(a) {
                            escaping_structs.insert(*s_id);
                        }
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    if let Some(s_id) = val_to_struct.get(cond) {
                        escaping_structs.insert(*s_id);
                    }
                    for a in then_args.iter().chain(else_args) {
                        if let Some(s_id) = val_to_struct.get(a) {
                            escaping_structs.insert(*s_id);
                        }
                    }
                }
                _ => {}
            }
        }

        // Retain only non-escaping struct initializations
        struct_inits.retain(|k, _| !escaping_structs.contains(k));
        if struct_inits.is_empty() {
            return false;
        }

        // Pass 2: Eliminate StructInit and scalarize GetField
        for block in &mut f.blocks {
            let mut new_instructions = Vec::new();
            for inst in &block.instructions {
                match inst {
                    Inst::StructInit { dest, .. } if struct_inits.contains_key(dest) => {
                        self.report.allocations_eliminated += 1;
                        changed = true;
                        continue;
                    }
                    Inst::AssignVar { name, value }
                        if !var_ambiguous.contains(name)
                            && val_to_struct.contains_key(value)
                            && struct_inits.contains_key(&val_to_struct[value]) =>
                    {
                        // Struct variable assignment eliminated
                        changed = true;
                        continue;
                    }
                    Inst::LoadVar { dest, name }
                        if !var_ambiguous.contains(name)
                            && var_to_struct.contains_key(name)
                            && struct_inits.contains_key(&var_to_struct[name]) =>
                    {
                        // Struct load eliminated
                        changed = true;
                        continue;
                    }
                    Inst::SetField {
                        object,
                        field,
                        value,
                    } => {
                        let actual_struct_id = val_to_struct
                            .get(object)
                            .or_else(|| struct_inits.get(object).map(|_| object));
                        if let Some(s_id) = actual_struct_id
                            && let Some(field_map) = struct_inits.get_mut(s_id)
                        {
                            field_map.insert(field.clone(), *value);
                            changed = true;
                            continue;
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::GetField {
                        dest,
                        object,
                        field,
                        ty,
                    } => {
                        let actual_struct_id = val_to_struct
                            .get(object)
                            .or_else(|| struct_inits.get(object).map(|_| object));
                        if let Some(s_id) = actual_struct_id
                            && let Some(field_map) = struct_inits.get(s_id)
                            && let Some(actual_val) = field_map.get(field)
                        {
                            // Register copy with an explicit dest: the
                            // forwarded value must stay bound to the
                            // GetField's ValueId. The old synthetic
                            // `AssignVar { name: "v_N" }` never bound
                            // `dest` in the backend's value map, so
                            // every forwarded read compiled to 0.
                            new_instructions.push(Inst::UnOp {
                                dest: *dest,
                                op: "copy".to_string(),
                                operand: *actual_val,
                                ty: ty.clone(),
                            });
                            changed = true;
                            continue;
                        }
                        new_instructions.push(inst.clone());
                    }
                    _ => {
                        new_instructions.push(inst.clone());
                    }
                }
            }
            block.instructions = new_instructions;
        }

        changed
    }
}
