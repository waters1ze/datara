use super::*;
use crate::dmir::{Function, Inst, Module, Terminator, ValueId};
use std::collections::HashSet;

impl Optimizer {
    pub(crate) fn dead_symbol_elimination(&mut self, module: &mut Module) {
        // Conservative guard: reachability is seeded from `main` only. For a
        // module without a `main` (e.g. a library), every function is a
        // potential entry point, so keep them all.
        if !module.functions.contains_key("main") {
            self.report.reachable_symbols = module.functions.len();
            self.report.removed_symbols = 0;
            return;
        }

        let mut reachable: HashSet<String> = HashSet::new();
        let mut worklist: Vec<String> = Vec::new();

        let mut method_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_fn_names: Vec<String> = module.functions.keys().cloned().collect();
        all_fn_names.sort();
        for f_name in &all_fn_names {
            let mut start = 0;
            while let Some(idx) = f_name[start..].find('_') {
                let actual_idx = start + idx;
                let suffix = &f_name[actual_idx + 1..];
                method_map
                    .entry(suffix.to_string())
                    .or_default()
                    .push(f_name.clone());
                start = actual_idx + 1;
            }
        }

        if module.functions.contains_key("main") {
            reachable.insert("main".to_string());
            worklist.push("main".to_string());
        }

        while let Some(current_fn) = worklist.pop() {
            if let Some(f) = module.functions.get(&current_fn) {
                for block in &f.blocks {
                    self.collect_calls(
                        &block.instructions,
                        module,
                        &method_map,
                        &mut reachable,
                        &mut worklist,
                    );
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
        method_map: &HashMap<String, Vec<String>>,
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
                    if let Some(funcs) = method_map.get(method) {
                        for f_name in funcs {
                            if !reachable.contains(f_name) {
                                reachable.insert(f_name.clone());
                                worklist.push(f_name.clone());
                            }
                        }
                    }
                }
                Inst::WhileLoop {
                    condition_insts,
                    body_insts,
                    ..
                } => {
                    self.collect_calls(condition_insts, module, method_map, reachable, worklist);
                    self.collect_calls(body_insts, module, method_map, reachable, worklist);
                }
                Inst::TryCatch {
                    try_insts,
                    catch_insts,
                    ..
                } => {
                    self.collect_calls(try_insts, module, method_map, reachable, worklist);
                    self.collect_calls(catch_insts, module, method_map, reachable, worklist);
                }
                _ => {}
            }
        }
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
    pub(crate) fn may_trap(inst: &Inst) -> bool {
        matches!(inst, Inst::BinOp { op, ty, .. } if (op == "/" || op == "%") && ty != "Float")
    }

    pub(crate) fn dead_code_elimination(&mut self, f: &mut Function) -> bool {
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
            for (inst_idx, inst) in block.instructions.iter().enumerate() {
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
                // Hybrid-SSA variable reads have no side effects in this IR:
                // a LoadVar only loads from the logical variable store, so a
                // dead one is removable (its feeding store is removed by the
                // AssignVar rule above).
                let is_load_var = matches!(inst, Inst::LoadVar { .. });
                // StructInit is a pure allocation: removing a dead one is
                // allocation elision.
                let is_struct_init = matches!(inst, Inst::StructInit { .. });
                // FormatStr / Select / Decide are pure value computations.
                let is_pure_value = matches!(
                    inst,
                    Inst::FormatStr { .. } | Inst::Select { .. } | Inst::Decide { .. }
                );
                let is_pure = is_pure_call
                    || matches!(
                        inst,
                        Inst::ConstInt { .. }
                            | Inst::ConstFloat { .. }
                            | Inst::ConstStr { .. }
                            | Inst::ConstBool { .. }
                            | Inst::BinOp { .. }
                            | Inst::UnOp { .. }
                    )
                    || is_load_var
                    || is_struct_init
                    || is_pure_value;
                if is_pure && !Self::may_trap(inst) {
                    let dest_id = match inst {
                        Inst::ConstInt { dest, .. }
                        | Inst::ConstFloat { dest, .. }
                        | Inst::ConstStr { dest, .. }
                        | Inst::ConstBool { dest, .. }
                        | Inst::BinOp { dest, .. }
                        | Inst::UnOp { dest, .. }
                        | Inst::Call { dest, .. }
                        | Inst::LoadVar { dest, .. }
                        | Inst::StructInit { dest, .. }
                        | Inst::FormatStr { dest, .. }
                        | Inst::Select { dest, .. }
                        | Inst::Decide { dest, .. } => dest,
                        _ => continue,
                    };
                    if !used_values.contains(dest_id) {
                        self.report.dead_instructions_removed += 1;
                        changed = true;
                        continue;
                    }
                }
                // GetField can fault on a null object, so it is only removable
                // when the object was produced by a StructInit earlier in the
                // same block (known non-null, no cross-block uncertainty).
                if let Inst::GetField { dest, object, .. } = inst
                    && !used_values.contains(dest)
                    && block.instructions[..inst_idx]
                        .iter()
                        .any(|prev| matches!(prev, Inst::StructInit { dest: d, .. } if d == object))
                {
                    self.report.dead_instructions_removed += 1;
                    changed = true;
                    continue;
                }
                new_instructions.push(inst.clone());
            }
            block.instructions = new_instructions;
        }

        changed
    }
}
