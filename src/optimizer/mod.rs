use crate::dmir::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub mod adaptive;
pub mod cost_model;
pub mod evidence;
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
    pub runtime_modules_linked: Vec<String>,
    pub runtime_modules_stripped: Vec<String>,
    pub decision_trace: Vec<cost_model::DecisionRecord>,
    pub adaptation_records: Vec<adaptive::AdaptationRecord>,
}

pub struct Optimizer {
    pub mode: String,
    pub report: OptimizationReport,
    pub cost_model: CostModel,
    pub trace: OptimizationDecisionTrace,
    pub sae: SemanticAdaptationEngine,
    pub function_effects: HashMap<String, crate::effects::EffectSet>,
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
                runtime_modules_linked: vec!["core".into()],
                runtime_modules_stripped: vec![
                    "network".into(),
                    "database".into(),
                    "reflection".into(),
                ],
                decision_trace: Vec::new(),
                adaptation_records: Vec::new(),
            },
            cost_model,
            trace: OptimizationDecisionTrace::new(),
            sae,
        }
    }

    pub fn optimize_module(&mut self, module: &mut Module) {
        if let Err(error) = crate::dmir::verify_module(module) {
            panic!("DMIR verification failed before optimization: {}", error);
        }
        self.report.symbols_analyzed = module.functions.len();

        if self.mode == "debug"
            || self.mode == "quick"
            || self.mode == "start"
            || self.mode == "check"
        {
            // Debug, quick, start, and check modes preserve verbatim IR for debugging/fast turnaround
            self.report.reachable_symbols = module.functions.len();
            return;
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
                });
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
            });

            // 2. Intra-procedural optimizations (SROA, Constant Folding, DCE, LoopFold)
            self.run_mutating_pass("intraproc", module, |opt, m| {
                let mut fn_names: Vec<String> = m.functions.keys().cloned().collect();
                fn_names.sort();
                for name in fn_names {
                    if let Some(f) = m.functions.get_mut(&name) {
                        opt.optimize_function(f);
                    }
                }
            });

            // Sibling recursion elimination (domain mode)
            if self.mode == "domain" {
                self.run_mutating_pass("tail_recursion", module, |opt, m| {
                    let mut fn_names: Vec<String> = m.functions.keys().cloned().collect();
                    fn_names.sort();
                    for name in fn_names {
                        let is_pure = opt
                            .function_effects
                            .get(&name)
                            .map(|s| s.is_pure())
                            .unwrap_or(true);
                        if is_pure
                            && let Some(f) = m.functions.get_mut(&name)
                                && recursion::eliminate_sibling_recursion(f) {
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
                });
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
            });
        }

        if let Err(error) = crate::dmir::verify_module(module) {
            panic!("DMIR verification failed after optimization: {}", error);
        }

        // Finalize decision trace
        self.report.decision_trace = self.trace.records.clone();
    }

    /// Evidence gate around one mutating pass.
    ///
    /// 1. Fingerprint the IR before the pass.
    /// 2. Run the pass.
    /// 3. Verify the IR (fail-closed): a pass that corrupts DMIR aborts the
    ///    build instead of being tolerated.
    /// 4. Fingerprint the IR after the pass. If it is unchanged, every
    ///    `Applied` record the pass emitted during this invocation is
    ///    downgraded to `Rejected` and every counter movement is reverted.
    ///
    /// This is the mechanical enforcement of the project rule: "a line in the
    /// trace does not prove an optimization". `Applied` now requires a
    /// physical IR delta to survive the gate.
    pub fn run_mutating_pass<F>(&mut self, label: &str, module: &mut Module, pass: F)
    where
        F: FnOnce(&mut Self, &mut Module),
    {
        let before = evidence::ir_fingerprint(module);
        let records_start = self.trace.records.len();
        let counters = evidence::CountersSnapshot::capture(&self.report);

        pass(self, module);

        if let Err(error) = crate::dmir::verify_module(module) {
            panic!(
                "DMIR verification failed after optimizer pass '{}': {}",
                label, error
            );
        }

        let after = evidence::ir_fingerprint(module);
        if after == before {
            let downgraded =
                evidence::downgrade_applied_without_delta(&mut self.trace.records, records_start);
            counters.restore(&mut self.report);
            self.report.evidence_downgrades += downgraded;
        }
    }

    fn max_value_id_in_function(&self, f: &Function) -> usize {
        let mut max_id = 0;
        for (_, _, p_val) in &f.params {
            if p_val.0 > max_id {
                max_id = p_val.0;
            }
        }
        for b in &f.blocks {
            // Block parameters and terminator operands are definitions/uses too.
            // Ignoring them lets a freshly minted id collide with an existing one.
            for param in &b.params {
                if param.val.0 > max_id {
                    max_id = param.val.0;
                }
            }
            for inst in &b.instructions {
                self.visit_inst_vids(inst, &mut |v| {
                    if v.0 > max_id {
                        max_id = v.0;
                    }
                });
            }
            let mut bump = |v: &ValueId| {
                if v.0 > max_id {
                    max_id = v.0;
                }
            };
            match &b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        bump(a);
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    bump(cond);
                    for a in then_args.iter().chain(else_args.iter()) {
                        bump(a);
                    }
                }
                Terminator::Return { value: Some(v) } => bump(v),
                Terminator::Return { value: None } | Terminator::Unreachable => {}
            }
        }
        max_id
    }

    fn visit_inst_vids<F: FnMut(&ValueId)>(&self, inst: &Inst, f: &mut F) {
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::GetFuncAddr { dest, .. } => f(dest),
            Inst::LoadVar { dest, .. } => f(dest),
            Inst::AssignVar { value, .. } => f(value),
            Inst::BinOp {
                dest, left, right, ..
            } => {
                f(dest);
                f(left);
                f(right);
            }
            Inst::UnOp { dest, operand, .. } => {
                f(dest);
                f(operand);
            }
            Inst::Call { dest, args, .. } => {
                f(dest);
                for a in args {
                    f(a);
                }
            }
            Inst::MethodCall {
                dest, object, args, ..
            } => {
                f(dest);
                f(object);
                for a in args {
                    f(a);
                }
            }
            Inst::StructInit { dest, fields, .. } => {
                f(dest);
                for (_, v) in fields {
                    f(v);
                }
            }
            Inst::GetField { dest, object, .. } => {
                f(dest);
                f(object);
            }
            Inst::SetField { object, value, .. } => {
                f(object);
                f(value);
            }
            Inst::Out { value } | Inst::Err { value } => f(value),
            Inst::FormatStr { dest, values, .. } => {
                f(dest);
                for v in values {
                    f(v);
                }
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ..
            } => {
                f(dest);
                for (c, v) in arms {
                    f(c);
                    f(v);
                }
                if let Some(ev) = else_val {
                    f(ev);
                }
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ..
            } => {
                f(dest);
                f(cond);
                f(then_val);
                f(else_val);
            }
            Inst::WhileLoop { cond_val, .. } => f(cond_val),
            Inst::TryCatch { .. } => {}
            Inst::Return { value } => {
                if let Some(v) = value {
                    f(v);
                }
            }
        }
    }

    /// Rewrite every operand of `inst` through `subst`, leaving unknown ids alone.
    ///
    /// This must handle *every* `Inst` variant. Silently dropping a variant
    /// would delete an instruction the verifier still expects to exist.
    fn substitute_operands(inst: &mut Inst, subst: &HashMap<ValueId, ValueId>) {
        if subst.is_empty() {
            return;
        }
        let fix = |v: &mut ValueId| {
            if let Some(new) = subst.get(v) {
                *v = *new;
            }
        };
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::LoadVar { dest, .. }
            | Inst::BinOp { dest, .. }
            | Inst::UnOp { dest, .. }
            | Inst::Call { dest, .. }
            | Inst::MethodCall { dest, .. }
            | Inst::StructInit { dest, .. }
            | Inst::GetField { dest, .. }
            | Inst::FormatStr { dest, .. }
            | Inst::GetFuncAddr { dest, .. }
            | Inst::Select { dest, .. }
            | Inst::Decide { dest, .. } => fix(dest),
            Inst::AssignVar { .. }
            | Inst::SetField { .. }
            | Inst::Out { .. }
            | Inst::Err { .. }
            | Inst::Return { .. }
            | Inst::WhileLoop { .. }
            | Inst::TryCatch { .. } => {}
        }
        match inst {
            Inst::AssignVar { value, .. } | Inst::Out { value } | Inst::Err { value } => fix(value),
            Inst::BinOp { left, right, .. } => {
                fix(left);
                fix(right);
            }
            Inst::UnOp { operand, .. } => fix(operand),
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => {
                fix(cond);
                fix(then_val);
                fix(else_val);
            }
            Inst::Call { args, .. } => {
                for a in args {
                    fix(a);
                }
            }
            Inst::MethodCall { object, args, .. } => {
                fix(object);
                for a in args {
                    fix(a);
                }
            }
            Inst::StructInit { fields, .. } => {
                for (_, v) in fields {
                    fix(v);
                }
            }
            Inst::GetField { object, .. } => fix(object),
            Inst::SetField { object, value, .. } => {
                fix(object);
                fix(value);
            }
            Inst::FormatStr { values, .. } => {
                for v in values {
                    fix(v);
                }
            }
            Inst::Decide { arms, else_val, .. } => {
                for (c, v) in arms {
                    fix(c);
                    fix(v);
                }
                if let Some(ev) = else_val {
                    fix(ev);
                }
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                for ci in condition_insts.iter_mut() {
                    Self::substitute_operands(ci, subst);
                }
                for bi in body_insts.iter_mut() {
                    Self::substitute_operands(bi, subst);
                }
                fix(cond_val);
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for ti in try_insts.iter_mut() {
                    Self::substitute_operands(ti, subst);
                }
                for ci in catch_insts.iter_mut() {
                    Self::substitute_operands(ci, subst);
                }
            }
            Inst::Return { value: Some(v) } => fix(v),
            _ => {}
        }
    }

    fn substitute_terminator(t: &mut Terminator, subst: &HashMap<ValueId, ValueId>) {
        if subst.is_empty() {
            return;
        }
        let fix = |v: &mut ValueId| {
            if let Some(new) = subst.get(v) {
                *v = *new;
            }
        };
        match t {
            Terminator::Branch { args, .. } => {
                for a in args {
                    fix(a);
                }
            }
            Terminator::CondBranch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                fix(cond);
                for a in then_args.iter_mut().chain(else_args.iter_mut()) {
                    fix(a);
                }
            }
            Terminator::Return { value } => {
                if let Some(v) = value {
                    fix(v);
                }
            }
            Terminator::Unreachable => {}
        }
    }

    /// The value a single-block callee returns, mapped into the caller.
    ///
    /// The real lowering returns through `Terminator::Return`; the compound
    /// `Inst::Return` is legacy and is only consulted as a fallback. Reading
    /// `Inst::Return` alone is why inlining used to leave the call's `dest`
    /// undefined whenever the result was used.
    fn callee_return_value(
        callee: &Function,
        val_map: &HashMap<ValueId, ValueId>,
    ) -> Option<ValueId> {
        let raw = match &callee.blocks[0].terminator {
            Terminator::Return { value: Some(v) } => Some(*v),
            _ => callee.blocks[0]
                .instructions
                .iter()
                .rev()
                .find_map(|i| match i {
                    Inst::Return { value: Some(v) } => Some(*v),
                    _ => None,
                }),
        }?;
        Some(val_map.get(&raw).copied().unwrap_or(raw))
    }

    /// Rewrite one callee instruction for splicing into the caller.
    ///
    /// Returns `None` for instructions that only express control flow and must
    /// not be copied. `skip_names` holds callee parameter names: loading one is
    /// replaced by a direct bind to the incoming argument, so no instruction is
    /// emitted at all.
    fn splice_callee_inst(
        &self,
        c_inst: &Inst,
        val_map: &HashMap<ValueId, ValueId>,
        param_names: &HashSet<String>,
        local_prefix: &str,
    ) -> Option<Inst> {
        let lookup = |v: &ValueId| val_map.get(v).copied().unwrap_or(*v);
        let rename = |n: &String| format!("{}{}", local_prefix, n);

        match c_inst {
            Inst::LoadVar { dest, name } => {
                if param_names.contains(name) {
                    // Bound straight to the argument; `val_map[dest]` already
                    // carries it, so nothing needs to be emitted.
                    None
                } else {
                    Some(Inst::LoadVar {
                        dest: lookup(dest),
                        name: rename(name),
                    })
                }
            }
            Inst::AssignVar { name, value } => Some(Inst::AssignVar {
                name: rename(name),
                value: lookup(value),
            }),
            Inst::ConstInt { dest, value } => Some(Inst::ConstInt {
                dest: lookup(dest),
                value: *value,
            }),
            Inst::ConstFloat { dest, value } => Some(Inst::ConstFloat {
                dest: lookup(dest),
                value: *value,
            }),
            Inst::ConstStr { dest, value } => Some(Inst::ConstStr {
                dest: lookup(dest),
                value: value.clone(),
            }),
            Inst::ConstBool { dest, value } => Some(Inst::ConstBool {
                dest: lookup(dest),
                value: *value,
            }),
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } => Some(Inst::BinOp {
                dest: lookup(dest),
                op: op.clone(),
                left: lookup(left),
                right: lookup(right),
                ty: ty.clone(),
            }),
            Inst::UnOp {
                dest,
                op,
                operand,
                ty,
            } => Some(Inst::UnOp {
                dest: lookup(dest),
                op: op.clone(),
                operand: lookup(operand),
                ty: ty.clone(),
            }),
            Inst::GetField {
                dest,
                object,
                field,
                ty,
            } => Some(Inst::GetField {
                dest: lookup(dest),
                object: lookup(object),
                field: field.clone(),
                ty: ty.clone(),
            }),
            Inst::Decide {
                dest,
                arms,
                else_val,
                ty,
            } => Some(Inst::Decide {
                dest: lookup(dest),
                arms: arms.iter().map(|(c, v)| (lookup(c), lookup(v))).collect(),
                else_val: else_val.as_ref().map(&lookup),
                ty: ty.clone(),
            }),
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ty,
            } => Some(Inst::Select {
                dest: lookup(dest),
                cond: lookup(cond),
                then_val: lookup(then_val),
                else_val: lookup(else_val),
                ty: ty.clone(),
            }),
            Inst::Return { .. } => None,
            // Anything else is not part of a "pure leaf" body. Refusing to copy
            // it keeps inlining from silently deleting an effect.
            _ => None,
        }
    }

    pub fn set_function_effects(&mut self, effects: HashMap<String, crate::effects::EffectSet>) {
        self.function_effects = effects;
    }

    pub fn inline_pure_functions(&mut self, module: &mut Module) {
        let mut candidates: HashMap<String, Function> = HashMap::new();
        let mut candidate_records: Vec<(String, String, String, String)> = Vec::new();

        let mut fn_names: Vec<String> = module.functions.keys().cloned().collect();
        fn_names.sort();

        for name in &fn_names {
            let f = &module.functions[name];
            if name == "main" {
                continue;
            }
            if f.blocks.len() == 1 {
                let inst_count = f.blocks[0].instructions.len();
                let is_inst_pure = f.blocks[0].instructions.iter().all(|i| {
                    matches!(
                        i,
                        Inst::ConstInt { .. }
                            | Inst::ConstFloat { .. }
                            | Inst::ConstStr { .. }
                            | Inst::ConstBool { .. }
                            | Inst::LoadVar { .. }
                            | Inst::AssignVar { .. }
                            | Inst::BinOp { .. }
                            | Inst::UnOp { .. }
                            | Inst::GetField { .. }
                            | Inst::Select { .. }
                            | Inst::Decide { .. }
                            | Inst::Return { .. }
                    )
                });

                // Effect lattice inspection:
                let lattice_pure = self
                    .function_effects
                    .get(name)
                    .map(|s| s.is_pure())
                    .unwrap_or(true);
                let has_side_effects = self
                    .function_effects
                    .get(name)
                    .map(|s| {
                        s.effects.contains(&crate::effects::Effect::IO)
                            || s.effects.contains(&crate::effects::Effect::Network)
                            || s.effects.contains(&crate::effects::Effect::Database)
                            || s.effects.contains(&crate::effects::Effect::Unsafe)
                            || s.effects
                                .contains(&crate::effects::Effect::Nondeterministic)
                    })
                    .unwrap_or(false);

                let is_pure = !has_side_effects && is_inst_pure;
                let multiplier = if is_pure && lattice_pure { 2 } else { 1 };

                let is_recursive = f.blocks[0].instructions.iter().any(|i| match i {
                    Inst::Call { func, .. } => func == name,
                    _ => false,
                });

                let (should_inline, benefit, cost, reason) =
                    self.cost_model.evaluate_inlining_effect_guided(
                        name,
                        inst_count,
                        is_pure,
                        is_recursive,
                        multiplier,
                    );
                if should_inline {
                    candidate_records.push((name.clone(), benefit, cost, reason));
                    candidates.insert(name.clone(), f.clone());
                } else {
                    self.trace
                        .record("Inlining", name, "Rejected", &benefit, &cost, &reason);
                }
            }
        }

        if candidates.is_empty() {
            for (name, benefit, cost, _) in candidate_records {
                self.trace.record(
                    "Inlining",
                    &name,
                    "Rejected",
                    &benefit,
                    &cost,
                    "Pure leaf function within budget, but no call sites were present in callers",
                );
            }
            return;
        }

        let mut inlined_set: HashSet<String> = HashSet::new();

        let mut caller_names: Vec<String> = module.functions.keys().cloned().collect();
        caller_names.sort();

        for caller_name in &caller_names {
            if candidates.contains_key(caller_name) {
                continue;
            }
            let caller_fn = module.functions.get_mut(caller_name).unwrap();

            let mut fresh_base = self.max_value_id_in_function(caller_fn) + 1000;
            let mut val_to_class: HashMap<ValueId, String> = HashMap::new();
            let mut var_to_class: HashMap<String, String> = HashMap::new();

            for (p_name, p_ty, p_val) in &caller_fn.params {
                if !p_ty.is_empty()
                    && p_ty != "Int"
                    && p_ty != "Float"
                    && p_ty != "Bool"
                    && p_ty != "String"
                {
                    val_to_class.insert(*p_val, p_ty.clone());
                    var_to_class.insert(p_name.clone(), p_ty.clone());
                }
            }

            for block in caller_fn.blocks.iter_mut() {
                // call `dest` -> value produced by the inlined body. Applied to
                // every later instruction *and* to the block terminator, which
                // is where the call result is usually consumed.
                let mut subst: HashMap<ValueId, ValueId> = HashMap::new();
                let mut new_insts: Vec<Inst> = Vec::with_capacity(block.instructions.len());

                for mut inst in std::mem::take(&mut block.instructions) {
                    Self::substitute_operands(&mut inst, &subst);

                    // Track classes for method dispatch
                    match &inst {
                        Inst::StructInit {
                            dest, class_name, ..
                        } => {
                            val_to_class.insert(*dest, class_name.clone());
                        }
                        Inst::AssignVar { name, value } => {
                            if let Some(c) = val_to_class.get(value) {
                                var_to_class.insert(name.clone(), c.clone());
                            }
                        }
                        Inst::LoadVar { dest, name } => {
                            if let Some(c) = var_to_class.get(name) {
                                val_to_class.insert(*dest, c.clone());
                            }
                        }
                        _ => {}
                    }

                    let inlined_candidate = match &inst {
                        Inst::Call {
                            dest, func, args, ..
                        } => candidates.get(func).map(|c| (*dest, c, args.clone())),
                        Inst::MethodCall {
                            dest,
                            object,
                            method,
                            args,
                            ..
                        } => {
                            let mut all_args = vec![*object];
                            all_args.extend(args.iter().copied());

                            let direct_name = val_to_class
                                .get(object)
                                .map(|c| format!("{}_{}", c, method));
                            let callee = direct_name
                                .as_ref()
                                .and_then(|name| candidates.get(name))
                                .or_else(|| candidates.get(method))
                                .or_else(|| {
                                    let suffix = format!("_{}", method);
                                    let mut matches: Vec<&Function> = candidates
                                        .iter()
                                        .filter(|(k, _)| k.ends_with(&suffix))
                                        .map(|(_, f)| f)
                                        .collect();
                                    if matches.len() == 1 {
                                        Some(matches.remove(0))
                                    } else {
                                        None
                                    }
                                });
                            callee.map(|c| (*dest, c, all_args))
                        }
                        _ => None,
                    };

                    if let Some((call_dest, callee, inlined_args)) = inlined_candidate {
                        let mut val_map: HashMap<ValueId, ValueId> = HashMap::new();
                        for b in &callee.blocks {
                            for ci in &b.instructions {
                                self.visit_inst_vids(ci, &mut |v| {
                                    if !val_map.contains_key(v) {
                                        val_map.insert(*v, ValueId(fresh_base + v.0));
                                    }
                                });
                            }
                        }

                        let param_names: HashSet<String> =
                            callee.params.iter().map(|(n, _, _)| n.clone()).collect();
                        for (idx, (_, _, p_val)) in callee.params.iter().enumerate() {
                            if idx < inlined_args.len() {
                                val_map.insert(*p_val, inlined_args[idx]);
                            }
                        }

                        // Callee locals are renamed so they can never
                        // capture a same-named variable in the caller.
                        let local_prefix = format!("__il{}_", fresh_base);

                        for c_inst in &callee.blocks[0].instructions {
                            if let Inst::LoadVar { dest, name } = c_inst
                                && param_names.contains(name)
                                && !inlined_args.is_empty()
                            {
                                // Bind the load straight to the argument.
                                let idx = callee.params.iter().position(|(n, _, _)| n == name);
                                if let Some(i) = idx
                                    && let Some(arg) = inlined_args.get(i)
                                {
                                    val_map.insert(*dest, *arg);
                                }
                            }
                            if let Some(spliced) = self.splice_callee_inst(
                                c_inst,
                                &val_map,
                                &param_names,
                                &local_prefix,
                            ) {
                                new_insts.push(spliced);
                            }
                        }

                        if let Some(ret_v) = Self::callee_return_value(callee, &val_map) {
                            subst.insert(call_dest, ret_v);
                        }

                        fresh_base += 1000;
                        inlined_set.insert(callee.name.clone());
                        self.report.functions_inlined += 1;
                        continue;
                    }

                    new_insts.push(inst);
                }

                Self::substitute_terminator(&mut block.terminator, &subst);
                block.instructions = new_insts;
            }
        }

        for (name, benefit, cost, reason) in candidate_records {
            if inlined_set.contains(&name) {
                self.trace
                    .record("Inlining", &name, "Applied", &benefit, &cost, &reason);
            } else {
                self.trace.record(
                    "Inlining",
                    &name,
                    "Rejected",
                    &benefit,
                    &cost,
                    "Pure leaf function within budget, but no call sites were present in callers",
                );
            }
        }
    }

    fn dead_symbol_elimination(&mut self, module: &mut Module) {
        let mut reachable: HashSet<String> = HashSet::new();
        let mut worklist: Vec<String> = Vec::new();

        if module.functions.contains_key("main") {
            reachable.insert("main".to_string());
            worklist.push("main".to_string());
        }

        while let Some(current_fn) = worklist.pop() {
            if let Some(f) = module.functions.get(&current_fn) {
                for block in &f.blocks {
                    self.collect_calls(&block.instructions, module, &mut reachable, &mut worklist);
                }
            }
        }

        self.report.reachable_symbols = reachable.len();
        let initial_count = module.functions.len();

        if !reachable.is_empty() {
            module.functions.retain(|name, _| reachable.contains(name));
            self.report.removed_symbols = initial_count - module.functions.len();
        } else {
            self.report.reachable_symbols = initial_count;
        }
    }

    fn collect_calls(
        &self,
        instructions: &[Inst],
        module: &Module,
        reachable: &mut HashSet<String>,
        worklist: &mut Vec<String>,
    ) {
        for inst in instructions {
            match inst {
                Inst::Call { func, .. } => {
                    if module.functions.contains_key(func) && !reachable.contains(func) {
                        reachable.insert(func.clone());
                        worklist.push(func.clone());
                    }
                }
                Inst::GetFuncAddr { func_name, .. } => {
                    if module.functions.contains_key(func_name) && !reachable.contains(func_name) {
                        reachable.insert(func_name.clone());
                        worklist.push(func_name.clone());
                    }
                }
                Inst::MethodCall { method, .. } => {
                    if module.functions.contains_key(method) && !reachable.contains(method) {
                        reachable.insert(method.clone());
                        worklist.push(method.clone());
                    }
                    for f_name in module.functions.keys() {
                        if f_name.ends_with(&format!("_{}", method)) && !reachable.contains(f_name)
                        {
                            reachable.insert(f_name.clone());
                            worklist.push(f_name.clone());
                        }
                    }
                }
                Inst::WhileLoop {
                    condition_insts,
                    body_insts,
                    ..
                } => {
                    self.collect_calls(condition_insts, module, reachable, worklist);
                    self.collect_calls(body_insts, module, reachable, worklist);
                }
                Inst::TryCatch {
                    try_insts,
                    catch_insts,
                    ..
                } => {
                    self.collect_calls(try_insts, module, reachable, worklist);
                    self.collect_calls(catch_insts, module, reachable, worklist);
                }
                _ => {}
            }
        }
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
            if LoopOptimizer::optimize_loops(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if PipelineFusionOptimizer::fuse_pipelines(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if MemoryOptimizer::scalarize_structures(f, &self.cost_model, &mut self.trace) > 0 {
                changed = true;
            }
            if self.scalarize_structures(f) {
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

        let mut merge_candidate: Option<(usize, BasicBlockId)> = None;
        for (i, b) in f.blocks.iter().enumerate() {
            if let Terminator::Branch { target, args } = &b.terminator
                && args.is_empty()
                && *target != b.id
                && *target != f.entry_block
                && preds.get(target).copied().unwrap_or(0) == 1
                && let Some(target_b) = f.blocks.iter().find(|blk| blk.id == *target)
                && target_b.params.is_empty()
            {
                merge_candidate = Some((i, *target));
                break;
            }
        }

        if let Some((a_idx, target_id)) = merge_candidate {
            let target_pos = f.blocks.iter().position(|b| b.id == target_id).unwrap();
            let target_b = f.blocks.remove(target_pos);
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

            // Infer the select operand type from constant-producing arm
            // instructions so a Float/String diamond is not mislabelled "Int".
            let mut val_ty: HashMap<ValueId, &'static str> = HashMap::new();
            for inst in then_b.instructions.iter().chain(else_b.instructions.iter()) {
                match inst {
                    Inst::ConstInt { dest, .. } => {
                        val_ty.insert(*dest, "Int");
                    }
                    Inst::ConstFloat { dest, .. } => {
                        val_ty.insert(*dest, "Float");
                    }
                    Inst::ConstBool { dest, .. } => {
                        val_ty.insert(*dest, "Bool");
                    }
                    Inst::ConstStr { dest, .. } => {
                        val_ty.insert(*dest, "String");
                    }
                    _ => {}
                }
            }

            let block = &mut f.blocks[b_idx];

            // Append instructions from both arms
            block.instructions.extend(then_b.instructions);
            block.instructions.extend(else_b.instructions);

            let mut merged_args = Vec::new();
            for (t_val, e_val) in then_args.iter().zip(else_args.iter()) {
                if t_val == e_val {
                    merged_args.push(*t_val);
                } else {
                    max_id += 1;
                    let sel_dest = ValueId(max_id);
                    let sel_ty = val_ty
                        .get(t_val)
                        .or_else(|| val_ty.get(e_val))
                        .copied()
                        .unwrap_or("Int");
                    block.instructions.push(Inst::Select {
                        dest: sel_dest,
                        cond,
                        then_val: *t_val,
                        else_val: *e_val,
                        ty: sel_ty.into(),
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
                    _ => {}
                }
            }

            // Terminators are uses too. A `return` of a struct keeps the
            // allocation alive: returns live in `block.terminator`, not in
            // `instructions`, so the old instruction-only scan never saw
            // them and this pass deleted returned objects outright — the
            // caller then read freed/undefined memory as the "result".
            if let Terminator::Return { value: Some(v) } = &block.terminator
                && let Some(s_id) = val_to_struct.get(v)
            {
                escaping_structs.insert(*s_id);
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

    fn constant_fold(&mut self, f: &mut Function) -> bool {
        let mut changed = false;
        let mut int_constants: HashMap<ValueId, i64> = HashMap::new();
        let mut str_constants: HashMap<ValueId, String> = HashMap::new();
        let mut bool_constants: HashMap<ValueId, bool> = HashMap::new();

        for block in &mut f.blocks {
            let mut block_var_ints: HashMap<String, i64> = HashMap::new();
            let mut block_var_strs: HashMap<String, String> = HashMap::new();
            let mut block_var_bools: HashMap<String, bool> = HashMap::new();
            let mut new_instructions = Vec::new();

            for inst in &block.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        int_constants.insert(*dest, *value);
                        new_instructions.push(inst.clone());
                    }
                    Inst::ConstStr { dest, value } => {
                        str_constants.insert(*dest, value.clone());
                        new_instructions.push(inst.clone());
                    }
                    Inst::ConstBool { dest, value } => {
                        bool_constants.insert(*dest, *value);
                        new_instructions.push(inst.clone());
                    }
                    Inst::AssignVar { name, value } => {
                        if let Some(v) = int_constants.get(value) {
                            block_var_ints.insert(name.clone(), *v);
                        } else {
                            block_var_ints.remove(name);
                        }
                        if let Some(v) = str_constants.get(value) {
                            block_var_strs.insert(name.clone(), v.clone());
                        } else {
                            block_var_strs.remove(name);
                        }
                        if let Some(v) = bool_constants.get(value) {
                            block_var_bools.insert(name.clone(), *v);
                        } else {
                            block_var_bools.remove(name);
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(v) = block_var_ints.get(name) {
                            int_constants.insert(*dest, *v);
                        }
                        if let Some(v) = block_var_strs.get(name) {
                            str_constants.insert(*dest, v.clone());
                        }
                        if let Some(v) = block_var_bools.get(name) {
                            bool_constants.insert(*dest, *v);
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ty: _,
                    } => {
                        if let (Some(l_val), Some(r_val)) =
                            (int_constants.get(left), int_constants.get(right))
                        {
                            let folded = match op.as_str() {
                                "+" => Some(l_val.wrapping_add(*r_val)),
                                "-" => Some(l_val.wrapping_sub(*r_val)),
                                "*" => Some(l_val.wrapping_mul(*r_val)),
                                "/" if *r_val != 0 => Some(l_val.wrapping_div(*r_val)),
                                "%" if *r_val != 0 => Some(l_val.wrapping_rem(*r_val)),
                                _ => None,
                            };
                            if let Some(res) = folded {
                                int_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }

                            let bool_folded = match op.as_str() {
                                "<" => Some(l_val < r_val),
                                "<=" => Some(l_val <= r_val),
                                ">" => Some(l_val > r_val),
                                ">=" => Some(l_val >= r_val),
                                "==" => Some(l_val == r_val),
                                "!=" => Some(l_val != r_val),
                                _ => None,
                            };
                            if let Some(res) = bool_folded {
                                bool_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::Decide {
                        dest,
                        arms,
                        else_val,
                        ty: _,
                    } => {
                        let mut resolved: Option<ValueId> = None;
                        let mut can_resolve = true;
                        for (cond, val) in arms {
                            if let Some(b) = bool_constants.get(cond) {
                                if *b {
                                    resolved = Some(*val);
                                    break;
                                }
                            } else if let Some(i) = int_constants.get(cond) {
                                if *i != 0 {
                                    resolved = Some(*val);
                                    break;
                                }
                            } else {
                                can_resolve = false;
                                break;
                            }
                        }
                        if resolved.is_none() && can_resolve {
                            resolved = *else_val;
                        }

                        if let Some(res_vid) = resolved {
                            if let Some(ival) = int_constants.get(&res_vid).copied() {
                                int_constants.insert(*dest, ival);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: ival,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(sval) = str_constants.get(&res_vid).cloned() {
                                str_constants.insert(*dest, sval.clone());
                                new_instructions.push(Inst::ConstStr {
                                    dest: *dest,
                                    value: sval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(bval) = bool_constants.get(&res_vid).copied() {
                                bool_constants.insert(*dest, bval);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: bval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::Select {
                        dest,
                        cond,
                        then_val,
                        else_val,
                        ty: _,
                    } => {
                        if let Some(b) = bool_constants.get(cond) {
                            let chosen = if *b { *then_val } else { *else_val };
                            if let Some(ival) = int_constants.get(&chosen).copied() {
                                int_constants.insert(*dest, ival);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: ival,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(sval) = str_constants.get(&chosen).cloned() {
                                str_constants.insert(*dest, sval.clone());
                                new_instructions.push(Inst::ConstStr {
                                    dest: *dest,
                                    value: sval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(bval) = bool_constants.get(&chosen).copied() {
                                bool_constants.insert(*dest, bval);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: bval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::FormatStr {
                        dest,
                        parts,
                        values,
                    } => {
                        let all_known = values.iter().all(|v| {
                            int_constants.contains_key(v)
                                || str_constants.contains_key(v)
                                || bool_constants.contains_key(v)
                        });
                        if all_known {
                            let mut res_str = String::new();
                            for (i, p) in parts.iter().enumerate() {
                                res_str.push_str(p);
                                if i < values.len() {
                                    let v_id = &values[i];
                                    if let Some(c) = int_constants.get(v_id) {
                                        res_str.push_str(&c.to_string());
                                    } else if let Some(c) = str_constants.get(v_id) {
                                        res_str.push_str(c);
                                    } else if let Some(c) = bool_constants.get(v_id) {
                                        res_str.push_str(if *c { "true" } else { "false" });
                                    }
                                }
                            }
                            str_constants.insert(*dest, res_str.clone());
                            new_instructions.push(Inst::ConstStr {
                                dest: *dest,
                                value: res_str,
                            });
                            self.report.constants_folded += 1;
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

    fn collect_used_values(
        &self,
        inst: &Inst,
        used_values: &mut HashSet<ValueId>,
        loaded_vars: &HashSet<String>,
    ) {
        match inst {
            Inst::AssignVar { name, value } => {
                if loaded_vars.contains(name) {
                    used_values.insert(*value);
                }
            }
            Inst::BinOp { left, right, .. } => {
                used_values.insert(*left);
                used_values.insert(*right);
            }
            Inst::UnOp { operand, .. } => {
                used_values.insert(*operand);
            }
            Inst::Call { args, .. } => {
                for a in args {
                    used_values.insert(*a);
                }
            }
            Inst::MethodCall { object, args, .. } => {
                used_values.insert(*object);
                for a in args {
                    used_values.insert(*a);
                }
            }
            Inst::StructInit { fields, .. } => {
                for (_, fval) in fields {
                    used_values.insert(*fval);
                }
            }
            Inst::GetField { object, .. } => {
                used_values.insert(*object);
            }
            Inst::SetField { object, value, .. } => {
                used_values.insert(*object);
                used_values.insert(*value);
            }
            Inst::Out { value } | Inst::Err { value } => {
                used_values.insert(*value);
            }
            Inst::FormatStr { values, .. } => {
                for v in values {
                    used_values.insert(*v);
                }
            }
            Inst::Decide { arms, else_val, .. } => {
                for (c, v) in arms {
                    used_values.insert(*c);
                    used_values.insert(*v);
                }
                if let Some(ev) = else_val {
                    used_values.insert(*ev);
                }
            }
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => {
                used_values.insert(*cond);
                used_values.insert(*then_val);
                used_values.insert(*else_val);
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                used_values.insert(*cond_val);
                for ci in condition_insts {
                    self.collect_used_values(ci, used_values, loaded_vars);
                }
                for bi in body_insts {
                    self.collect_used_values(bi, used_values, loaded_vars);
                }
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for ti in try_insts {
                    self.collect_used_values(ti, used_values, loaded_vars);
                }
                for ci in catch_insts {
                    self.collect_used_values(ci, used_values, loaded_vars);
                }
            }
            Inst::Return { value: Some(v) } => {
                used_values.insert(*v);
            }
            _ => {}
        }
    }

    /// Whether deleting this instruction could delete an observable fault.
    ///
    /// Integer `/` and `%` lower to `sdiv`/`srem`, which trap on a zero divisor
    /// and on `MIN / -1`. Dropping a dead one removes a trap the program would
    /// have raised, so it is not side-effect free. Float division produces
    /// inf/NaN instead of faulting and stays removable.
    fn may_trap(inst: &Inst) -> bool {
        matches!(inst, Inst::BinOp { op, ty, .. } if (op == "/" || op == "%") && ty != "Float")
    }

    fn dead_code_elimination(&mut self, f: &mut Function) -> bool {
        let mut changed = false;
        let mut used_values: HashSet<ValueId> = HashSet::new();

        let mut loaded_vars: HashSet<String> = HashSet::new();
        fn scan_loads(inst: &Inst, loaded: &mut HashSet<String>) {
            match inst {
                Inst::LoadVar { name, .. } => {
                    loaded.insert(name.clone());
                }
                Inst::WhileLoop {
                    condition_insts,
                    body_insts,
                    ..
                } => {
                    for ci in condition_insts {
                        scan_loads(ci, loaded);
                    }
                    for bi in body_insts {
                        scan_loads(bi, loaded);
                    }
                }
                Inst::TryCatch {
                    try_insts,
                    catch_insts,
                    ..
                } => {
                    for ti in try_insts {
                        scan_loads(ti, loaded);
                    }
                    for ci in catch_insts {
                        scan_loads(ci, loaded);
                    }
                }
                _ => {}
            }
        }
        for block in &f.blocks {
            for inst in &block.instructions {
                scan_loads(inst, &mut loaded_vars);
            }
        }

        for block in &f.blocks {
            for inst in &block.instructions {
                self.collect_used_values(inst, &mut used_values, &loaded_vars);
            }
            // Block parameters and terminator operands are uses too. After
            // mem2reg, branch arguments reference live SSA definitions;
            // ignoring them would let DCE delete a definition a branch still
            // consumes, producing undefined uses the verifier rejects.
            match &block.terminator {
                Terminator::Branch { args, .. } => {
                    for v in args {
                        used_values.insert(*v);
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    used_values.insert(*cond);
                    for v in then_args.iter().chain(else_args) {
                        used_values.insert(*v);
                    }
                }
                Terminator::Return { value: Some(v) } => {
                    used_values.insert(*v);
                }
                _ => {}
            }
        }

        for block in &mut f.blocks {
            let mut new_instructions = Vec::new();
            for inst in &block.instructions {
                if let Inst::AssignVar { name, .. } = inst
                    && !loaded_vars.contains(name)
                {
                    self.report.dead_instructions_removed += 1;
                    changed = true;
                    continue;
                }
                let is_pure_call = match inst {
                    Inst::Call { func, .. } => {
                        func.starts_with("datara_rt_str_concat")
                            || func == "datara_rt_format_str_i64_str_i64"
                            || func == "datara_rt_int_to_str"
                            || func == "datara_rt_float_to_str"
                            || func == "datara_rt_len"
                            || func == "abs"
                            || func == "min"
                            || func == "max"
                    }
                    _ => false,
                };
                let is_pure = is_pure_call
                    || matches!(
                        inst,
                        Inst::ConstInt { .. }
                            | Inst::ConstFloat { .. }
                            | Inst::ConstStr { .. }
                            | Inst::ConstBool { .. }
                            | Inst::BinOp { .. }
                            | Inst::UnOp { .. }
                    );
                if is_pure && !Self::may_trap(inst) {
                    let dest_id = match inst {
                        Inst::ConstInt { dest, .. }
                        | Inst::ConstFloat { dest, .. }
                        | Inst::ConstStr { dest, .. }
                        | Inst::ConstBool { dest, .. }
                        | Inst::BinOp { dest, .. }
                        | Inst::UnOp { dest, .. }
                        | Inst::Call { dest, .. } => dest,
                        _ => unreachable!(),
                    };
                    if !used_values.contains(dest_id) {
                        self.report.dead_instructions_removed += 1;
                        changed = true;
                        continue;
                    }
                }
                new_instructions.push(inst.clone());
            }
            block.instructions = new_instructions;
        }

        changed
    }
}
