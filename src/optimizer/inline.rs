use super::*;
use crate::dmir::{Function, Inst, Module, Terminator, ValueId};
use std::collections::{HashMap, HashSet};

impl Optimizer {
    pub(crate) fn max_value_id_in_function(&self, f: &Function) -> usize {
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

    pub(crate) fn visit_inst_vids<F: FnMut(&ValueId)>(&self, inst: &Inst, f: &mut F) {
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
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                for ci in condition_insts {
                    self.visit_inst_vids(ci, f);
                }
                for bi in body_insts {
                    self.visit_inst_vids(bi, f);
                }
                f(cond_val);
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for ti in try_insts {
                    self.visit_inst_vids(ti, f);
                }
                for ci in catch_insts {
                    self.visit_inst_vids(ci, f);
                }
            }
            Inst::Return { value } => {
                if let Some(v) = value {
                    f(v);
                }
            }
            Inst::InlineAsm {
                outputs, inputs, ..
            } => {
                for (_, d) in outputs {
                    f(d);
                }
                for (_, i) in inputs {
                    f(i);
                }
            }
        }
    }

    /// Rewrite every operand of `inst` through `subst`, leaving unknown ids alone.
    ///
    /// This must handle *every* `Inst` variant. Silently dropping a variant
    /// would delete an instruction the verifier still expects to exist.
    pub(crate) fn substitute_operands(inst: &mut Inst, subst: &HashMap<ValueId, ValueId>) {
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
            Inst::InlineAsm { outputs, .. } => {
                for (_, d) in outputs {
                    fix(d);
                }
            }
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
            Inst::InlineAsm { inputs, .. } => {
                for (_, i) in inputs {
                    fix(i);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn substitute_terminator(t: &mut Terminator, subst: &HashMap<ValueId, ValueId>) {
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
                            || s.effects.contains(&crate::effects::Effect::Parallel)
                            || s.effects.contains(&crate::effects::Effect::Foreign)
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

            // ValueIds are module-wide, so freshly minted ids must start past
            // every id already allocated in the caller and advance by the
            // callee's own id span to keep successive inlines disjoint.
            let mut fresh_base = self.max_value_id_in_function(caller_fn) + 1;
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

            let mut fn_subst: HashMap<ValueId, ValueId> = HashMap::new();

            for block in caller_fn.blocks.iter_mut() {
                // call `dest` -> value produced by the inlined body. Applied to
                // every later instruction *and* to the block terminator, which
                // is where the call result is usually consumed.
                let mut new_insts: Vec<Inst> = Vec::with_capacity(block.instructions.len());

                for mut inst in std::mem::take(&mut block.instructions) {
                    Self::substitute_operands(&mut inst, &fn_subst);

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
                        Inst::UnOp {
                            dest, op, operand, ..
                        } if op == "copy" => {
                            if let Some(c) = val_to_class.get(operand) {
                                val_to_class.insert(*dest, c.clone());
                            }
                        }
                        _ => {}
                    }

                    let inlined_candidate = match &inst {
                        Inst::Call {
                            dest, func, args, ..
                        } => candidates
                            .get(func)
                            // An arity mismatch means this call resolves to
                            // something else; splicing would bind wrong params.
                            .filter(|c| c.params.len() == args.len())
                            .map(|c| (*dest, c, args.clone())),
                        Inst::MethodCall {
                            dest,
                            object,
                            method,
                            args,
                            ..
                        } => {
                            let mut all_args = vec![*object];
                            all_args.extend(args.iter().copied());

                            // Without a known receiver class, do not guess:
                            // a suffix match can pick a different class's
                            // method, so only an exact Class_method candidate
                            // (or a free function of the same name) qualifies.
                            let direct_name = val_to_class
                                .get(object)
                                .map(|c| format!("{}_{}", c, method));
                            let callee = direct_name
                                .as_ref()
                                .and_then(|name| candidates.get(name))
                                .filter(|c| c.params.len() == all_args.len())
                                .or_else(|| {
                                    candidates
                                        .get(method)
                                        .filter(|c| c.params.len() == all_args.len())
                                });
                            callee.map(|c| (*dest, c, all_args))
                        }
                        _ => None,
                    };

                    if let Some((call_dest, callee, inlined_args)) = inlined_candidate {
                        let callee_max = self.max_value_id_in_function(callee);
                        let stride = callee_max + 1;
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
                        let assigned_params: HashSet<String> = callee.blocks[0]
                            .instructions
                            .iter()
                            .filter_map(|i| {
                                if let Inst::AssignVar { name, .. } = i {
                                    if param_names.contains(name) {
                                        return Some(name.clone());
                                    }
                                }
                                None
                            })
                            .collect();

                        // Callee locals are renamed so they can never
                        // capture a same-named variable in the caller.
                        let local_prefix = format!("__il{}_", fresh_base);

                        // If any parameters are assigned in callee, initialize their
                        // local variables with the incoming argument values first.
                        for (idx, (p_name, _, _)) in callee.params.iter().enumerate() {
                            if assigned_params.contains(p_name) {
                                if let Some(arg) = inlined_args.get(idx) {
                                    new_insts.push(Inst::AssignVar {
                                        name: format!("{}{}", local_prefix, p_name),
                                        value: *arg,
                                    });
                                }
                            }
                        }

                        let pure_param_names: HashSet<String> =
                            param_names.difference(&assigned_params).cloned().collect();

                        for (idx, (_, _, p_val)) in callee.params.iter().enumerate() {
                            if idx < inlined_args.len() {
                                val_map.insert(*p_val, inlined_args[idx]);
                            }
                        }

                        for c_inst in &callee.blocks[0].instructions {
                            if let Inst::LoadVar { dest, name } = c_inst
                                && pure_param_names.contains(name)
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
                                &pure_param_names,
                                &local_prefix,
                            ) {
                                new_insts.push(spliced);
                            }
                        }

                        if let Some(ret_v) = Self::callee_return_value(callee, &val_map) {
                            let resolved_ret = fn_subst.get(&ret_v).copied().unwrap_or(ret_v);
                            fn_subst.insert(call_dest, resolved_ret);
                        } else {
                            new_insts.push(Inst::ConstInt {
                                dest: call_dest,
                                value: 0,
                            });
                        }

                        fresh_base += stride;
                        inlined_set.insert(callee.name.clone());
                        self.report.functions_inlined += 1;
                        continue;
                    }

                    new_insts.push(inst);
                }

                Self::substitute_terminator(&mut block.terminator, &fn_subst);
                block.instructions = new_insts;
            }

            if !fn_subst.is_empty() {
                for block in caller_fn.blocks.iter_mut() {
                    for inst in block.instructions.iter_mut() {
                        Self::substitute_operands(inst, &fn_subst);
                    }
                    Self::substitute_terminator(&mut block.terminator, &fn_subst);
                }
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
}
