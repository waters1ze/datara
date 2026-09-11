//! Interprocedural Optimization (IPO / LTO) Engine (Phase 14)
//!
//! Provides:
//! 1. Whole-program constant propagation & argument specialization with function cloning.
//! 2. Devirtualization: resolves polymorphic / trait method calls with a single implementation
//!    into direct static calls.
//! 3. Cross-module pure inlining (DMIR-level LTO).
//! 4. Speculative inlining and dead clone elimination.

#![allow(clippy::collapsible_if, clippy::collapsible_match, clippy::map_entry)]

use crate::dmir::{Function, Inst, Module, Terminator, ValueId};
use crate::optimizer::cost_model::OptimizationDecisionTrace;
use std::collections::{HashMap, HashSet};

pub struct InterproceduralOptimizer;

impl InterproceduralOptimizer {
    /// Maximum callee instruction count eligible for constant argument specialization cloning.
    pub const SPECIALIZATION_THRESHOLD: usize = 64;

    /// Maximum total specialized function clones permitted per module to prevent code bloat.
    pub const MAX_SPECIALIZATION_BUDGET: usize = 16;

    /// Runs full IPO / LTO passes over the module.
    pub fn optimize_module(module: &mut Module, trace: &mut OptimizationDecisionTrace) -> bool {
        let mut changed = false;

        // 1. Devirtualization: single implementation -> direct static call
        if Self::devirtualize_single_impl_methods(module, trace) {
            changed = true;
        }

        // 2. Constant Argument Specialization & Function Cloning
        if Self::specialize_constant_arguments(module, trace) {
            changed = true;
        }

        // 3. Cross-Module Pure Inlining (DMIR-level LTO)
        if Self::inline_cross_module_pure(module, trace) {
            changed = true;
        }

        // 4. Dead Clone Elimination
        if Self::eliminate_dead_clones(module, trace) {
            changed = true;
        }

        changed
    }

    /// (b) Devirtualization: rewrites trait / polymorphic method calls with a single
    /// implementation into direct static calls.
    pub fn devirtualize_single_impl_methods(
        module: &mut Module,
        trace: &mut OptimizationDecisionTrace,
    ) -> bool {
        let mut changed = false;

        // Map method names to candidate implementations: e.g. "Type_method" -> candidate for "method"
        let mut method_to_impls: HashMap<String, Vec<String>> = HashMap::new();
        let mut fn_names: Vec<String> = module.functions.keys().cloned().collect();
        fn_names.sort();

        for fname in &fn_names {
            if let Some(idx) = fname.rfind('_') {
                let mname = &fname[idx + 1..];
                if !mname.is_empty() {
                    method_to_impls
                        .entry(mname.to_string())
                        .or_default()
                        .push(fname.clone());
                }
            }
        }

        let builtin_methods: HashSet<&str> = [
            "len",
            "count",
            "length",
            "byte_len",
            "char_len",
            "str_byte_at",
            "byte_at",
            "char_at",
            "append",
            "push",
            "get",
            "at",
            "set",
            "insert",
            "pop",
            "slice",
            "contains",
            "trim",
        ]
        .into_iter()
        .collect();

        for fname in &fn_names {
            let mut f = match module.functions.get_mut(fname) {
                Some(f) => f.clone(),
                None => continue,
            };

            let mut fn_changed = false;
            for block in &mut f.blocks {
                for inst in &mut block.instructions {
                    if let Inst::MethodCall {
                        dest,
                        object,
                        method,
                        args,
                        ty,
                    } = inst
                    {
                        if builtin_methods.contains(method.as_str()) {
                            continue;
                        }

                        let candidates = method_to_impls.get(method);
                        if let Some(impls) = candidates {
                            if impls.len() == 1 {
                                let target_fn = impls[0].clone();
                                let mut call_args = vec![*object];
                                call_args.extend(args.iter().copied());

                                trace.record(
                                    "Devirtualization",
                                    method,
                                    "Applied",
                                    &format!("Target: {}", target_fn),
                                    "Single trait/behavior implementation",
                                    &format!(
                                        "Devirtualized method '{}' into direct static call to '{}'",
                                        method, target_fn
                                    ),
                                );

                                *inst = Inst::Call {
                                    dest: *dest,
                                    func: target_fn,
                                    args: call_args,
                                    ty: ty.clone(),
                                };
                                fn_changed = true;
                                changed = true;
                            }
                        }
                    }
                }
            }

            if fn_changed {
                module.functions.insert(fname.clone(), f);
            }
        }

        changed
    }

    /// (a) Constant argument specialization & cloning:
    /// When a function is called with literal constant argument(s), creates a specialized
    /// clone with the constant baked in, folds the clone body, and points caller to clone.
    pub fn specialize_constant_arguments(
        module: &mut Module,
        trace: &mut OptimizationDecisionTrace,
    ) -> bool {
        let mut changed = false;
        let mut new_clones: Vec<Function> = Vec::new();
        let mut rewrites: Vec<(String, usize, usize, String)> = Vec::new(); // (caller_name, block_idx, inst_idx, clone_name)

        let current_specializations = module
            .functions
            .keys()
            .filter(|k| k.contains("__spec_"))
            .count();
        if current_specializations >= Self::MAX_SPECIALIZATION_BUDGET {
            return false;
        }

        let fn_names: Vec<String> = module.functions.keys().cloned().collect();

        for caller_name in &fn_names {
            let caller = match module.functions.get(caller_name) {
                Some(f) => f,
                None => continue,
            };

            // Track constants in caller
            let mut val_constants: HashMap<ValueId, (String, i64)> = HashMap::new(); // (type, int_val)
            for block in &caller.blocks {
                for inst in &block.instructions {
                    match inst {
                        Inst::ConstInt { dest, value } => {
                            val_constants.insert(*dest, ("Int".into(), *value));
                        }
                        Inst::ConstBool { dest, value } => {
                            val_constants
                                .insert(*dest, ("Bool".into(), if *value { 1 } else { 0 }));
                        }
                        _ => {}
                    }
                }
            }

            for (b_idx, block) in caller.blocks.iter().enumerate() {
                for (i_idx, inst) in block.instructions.iter().enumerate() {
                    if let Inst::Call { func, args, .. } = inst {
                        // Candidate callee check
                        if func.contains("__spec_") || func.starts_with("datara_rt_") {
                            continue;
                        }

                        let callee = match module.functions.get(func) {
                            Some(f) => f,
                            None => continue,
                        };

                        if caller_name.contains("__spec_") {
                            continue;
                        }

                        // Check if callee is recursive (recursive functions cannot be cloned safely)
                        let is_recursive = callee.blocks.iter().any(|b| {
                            b.instructions.iter().any(|i| match i {
                                Inst::Call { func: f, .. } => {
                                    f == func || f.starts_with(&format!("{func}__spec_"))
                                }
                                _ => false,
                            })
                        });
                        if is_recursive {
                            continue;
                        }

                        // Check size threshold using full recursive count
                        let total_insts: usize = Self::count_function_instructions(callee);
                        if total_insts > Self::SPECIALIZATION_THRESHOLD || total_insts == 0 {
                            continue;
                        }

                        // Check if any argument is a constant
                        for (arg_idx, arg_val) in args.iter().enumerate() {
                            if let Some((const_ty, const_val)) = val_constants.get(arg_val) {
                                let param_name = callee
                                    .params
                                    .get(arg_idx)
                                    .map(|p| p.0.clone())
                                    .unwrap_or_else(|| format!("p{}", arg_idx));

                                let clone_name = format!("{func}__spec_{param_name}_{const_val}");

                                if current_specializations + new_clones.len()
                                    >= Self::MAX_SPECIALIZATION_BUDGET
                                {
                                    break;
                                }

                                // Clone and specialize callee
                                if !module.functions.contains_key(&clone_name)
                                    && !new_clones.iter().any(|c| c.name == clone_name)
                                {
                                    let mut clone = callee.clone();
                                    clone.name = clone_name.clone();

                                    // Allocate a fresh ValueId in the clone for the constant, strictly > max
                                    let max_vid = Self::max_value_id_in_function(&clone);
                                    let const_vid = ValueId(max_vid + 1);

                                    // Replace uses of callee parameter with const_vid
                                    if let Some(param) = callee.params.get(arg_idx) {
                                        let param_vid = param.2;
                                        if let Some(entry) = clone.blocks.first_mut() {
                                            if const_ty == "Bool" {
                                                entry.instructions.insert(
                                                    0,
                                                    Inst::ConstBool {
                                                        dest: const_vid,
                                                        value: *const_val != 0,
                                                    },
                                                );
                                            } else {
                                                entry.instructions.insert(
                                                    0,
                                                    Inst::ConstInt {
                                                        dest: const_vid,
                                                        value: *const_val,
                                                    },
                                                );
                                            }
                                        }

                                        let mut subst = HashMap::new();
                                        subst.insert(param_vid, const_vid);
                                        Self::substitute_uses_in_function(&mut clone, &subst);
                                    }

                                    // Run local constant folding on the clone
                                    Self::fold_constants_in_function(&mut clone);

                                    trace.record(
                                        "ArgumentSpecialization",
                                        func,
                                        "Applied",
                                        &format!("Clone: {}", clone_name),
                                        "Constant argument propagation",
                                        &format!(
                                            "Specialized '{}' with parameter '{}' = {} into clone '{}'",
                                            func, param_name, const_val, clone_name
                                        ),
                                    );

                                    new_clones.push(clone);
                                }

                                rewrites.push((
                                    caller_name.clone(),
                                    b_idx,
                                    i_idx,
                                    clone_name.clone(),
                                ));
                                changed = true;
                                break; // Specialize first constant arg
                            }
                        }
                    }
                }
            }
        }

        // Add all new clones to module
        for clone in new_clones {
            module.functions.insert(clone.name.clone(), clone);
        }

        // Apply rewrites
        for (c_name, b_idx, i_idx, clone_name) in rewrites {
            if let Some(caller) = module.functions.get_mut(&c_name) {
                if let Some(b) = caller.blocks.get_mut(b_idx) {
                    if let Some(inst) = b.instructions.get_mut(i_idx) {
                        if let Inst::Call { func, .. } = inst {
                            *func = clone_name;
                        }
                    }
                }
            }
        }

        changed
    }

    /// (c) Cross-Module Pure Inlining (DMIR-level LTO):
    /// Inlines small pure functions across module boundaries.
    pub fn inline_cross_module_pure(
        module: &mut Module,
        trace: &mut OptimizationDecisionTrace,
    ) -> bool {
        let mut changed = false;
        let mut inline_candidates: HashMap<String, Function> = HashMap::new();

        let fn_names: Vec<String> = module.functions.keys().cloned().collect();
        for fname in &fn_names {
            if fname == "main" || fname.starts_with("datara_rt_") {
                continue;
            }
            if let Some(f) = module.functions.get(fname) {
                // Must be single-block pure function <= 15 instructions
                if f.blocks.len() == 1 {
                    let b = &f.blocks[0];
                    let is_pure = b.instructions.iter().all(|i| {
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
                        )
                    });
                    if is_pure && b.instructions.len() <= 15 {
                        inline_candidates.insert(fname.clone(), f.clone());
                    }
                }
            }
        }

        for fname in &fn_names {
            if inline_candidates.contains_key(fname) {
                continue; // Do not inline into self
            }
            let mut f = match module.functions.get_mut(fname) {
                Some(f) => f.clone(),
                None => continue,
            };

            let mut fn_changed = false;
            let caller_max = Self::max_value_id_in_function(&f);
            let fresh_base = caller_max + 1;

            for block in &mut f.blocks {
                let mut new_insts = Vec::new();
                for inst in &block.instructions {
                    if let Inst::Call {
                        dest,
                        func,
                        args,
                        ty,
                    } = inst
                    {
                        if let Some(callee) = inline_candidates.get(func) {
                            if callee.blocks.len() == 1 {
                                let callee_blk = &callee.blocks[0];
                                // Map callee params to call args
                                let mut val_subst: HashMap<ValueId, ValueId> = HashMap::new();
                                for (idx, (_, _, pval)) in callee.params.iter().enumerate() {
                                    if let Some(arg_vid) = args.get(idx) {
                                        val_subst.insert(*pval, *arg_vid);
                                    }
                                }

                                // Remap internal definitions in callee to fresh value IDs
                                for c_inst in &callee_blk.instructions {
                                    Self::visit_inst_vids(c_inst, &mut |v| {
                                        if !val_subst.contains_key(&v) {
                                            val_subst.insert(v, ValueId(fresh_base + v.0));
                                        }
                                    });
                                }

                                // Emit cloned callee instructions with substituted operands
                                for c_inst in &callee_blk.instructions {
                                    let mut cloned_inst = c_inst.clone();
                                    Self::substitute_inst_vids(&mut cloned_inst, &val_subst);
                                    new_insts.push(cloned_inst);
                                }

                                // If callee returns a value, copy it to dest
                                if let Terminator::Return {
                                    value: Some(ret_vid),
                                } = &callee_blk.terminator
                                {
                                    let subst_ret =
                                        val_subst.get(ret_vid).copied().unwrap_or(*ret_vid);
                                    new_insts.push(Inst::UnOp {
                                        dest: *dest,
                                        op: "copy".into(),
                                        operand: subst_ret,
                                        ty: ty.clone(),
                                    });
                                } else {
                                    new_insts.push(Inst::ConstInt {
                                        dest: *dest,
                                        value: 0,
                                    });
                                }

                                trace.record(
                                    "PureLTOInlining",
                                    func,
                                    "Applied",
                                    &format!("Inlined into {}", fname),
                                    "DMIR-level LTO cross-module inlining",
                                    &format!(
                                        "Inlined pure function '{}' ({} instructions) into '{}'",
                                        func,
                                        callee_blk.instructions.len(),
                                        fname
                                    ),
                                );

                                fn_changed = true;
                                changed = true;
                                continue;
                            }
                        }
                    }
                    new_insts.push(inst.clone());
                }
                block.instructions = new_insts;
            }

            if fn_changed {
                module.functions.insert(fname.clone(), f);
            }
        }

        changed
    }

    /// Cleans up dead / uncalled specialized clones.
    pub fn eliminate_dead_clones(
        module: &mut Module,
        trace: &mut OptimizationDecisionTrace,
    ) -> bool {
        let mut referenced_funcs: HashSet<String> = HashSet::new();
        for f in module.functions.values() {
            for b in &f.blocks {
                for inst in &b.instructions {
                    if let Inst::Call { func, .. } = inst {
                        referenced_funcs.insert(func.clone());
                    }
                }
            }
        }

        let mut to_remove = Vec::new();
        for fname in module.functions.keys() {
            if fname.contains("__spec_") && !referenced_funcs.contains(fname) {
                to_remove.push(fname.clone());
            }
        }

        let changed = !to_remove.is_empty();
        for dead_clone in to_remove {
            module.functions.remove(&dead_clone);
            trace.record(
                "DeadCloneElimination",
                &dead_clone,
                "Applied",
                "Removed unreferenced clone",
                "Dead code elimination",
                "Specialized clone has 0 callers remaining",
            );
        }

        changed
    }

    pub fn count_function_instructions(f: &Function) -> usize {
        f.blocks
            .iter()
            .map(|b| {
                b.instructions
                    .iter()
                    .map(Self::count_instruction)
                    .sum::<usize>()
            })
            .sum()
    }

    fn count_instruction(inst: &Inst) -> usize {
        match inst {
            Inst::WhileLoop {
                condition_insts,
                body_insts,
                ..
            } => {
                1 + condition_insts
                    .iter()
                    .map(Self::count_instruction)
                    .sum::<usize>()
                    + body_insts
                        .iter()
                        .map(Self::count_instruction)
                        .sum::<usize>()
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                1 + try_insts.iter().map(Self::count_instruction).sum::<usize>()
                    + catch_insts
                        .iter()
                        .map(Self::count_instruction)
                        .sum::<usize>()
            }
            _ => 1,
        }
    }

    pub fn max_value_id_in_function(f: &Function) -> usize {
        let mut max_id = 0;
        for (_, _, v) in &f.params {
            if v.0 > max_id {
                max_id = v.0;
            }
        }
        for b in &f.blocks {
            for p in &b.params {
                if p.val.0 > max_id {
                    max_id = p.val.0;
                }
            }
            for inst in &b.instructions {
                Self::visit_inst_vids(inst, &mut |v| {
                    if v.0 > max_id {
                        max_id = v.0;
                    }
                });
            }
            match &b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        if a.0 > max_id {
                            max_id = a.0;
                        }
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    if cond.0 > max_id {
                        max_id = cond.0;
                    }
                    for a in then_args.iter().chain(else_args.iter()) {
                        if a.0 > max_id {
                            max_id = a.0;
                        }
                    }
                }
                Terminator::Return { value: Some(v) } => {
                    if v.0 > max_id {
                        max_id = v.0;
                    }
                }
                _ => {}
            }
        }
        max_id
    }

    fn visit_inst_vids<F: FnMut(ValueId)>(inst: &Inst, f: &mut F) {
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::GetFuncAddr { dest, .. } => f(*dest),
            Inst::LoadVar { dest, .. } => f(*dest),
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
                for (c, v) in arms {
                    f(*c);
                    f(*v);
                }
                if let Some(e) = else_val {
                    f(*e);
                }
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                f(*cond_val);
                for i in condition_insts {
                    Self::visit_inst_vids(i, f);
                }
                for i in body_insts {
                    Self::visit_inst_vids(i, f);
                }
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for i in try_insts {
                    Self::visit_inst_vids(i, f);
                }
                for i in catch_insts {
                    Self::visit_inst_vids(i, f);
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
            Inst::Return { value: Some(v) } => f(*v),
            Inst::Return { value: None } => {}
        }
    }

    fn substitute_uses_in_function(f: &mut Function, subst: &HashMap<ValueId, ValueId>) {
        for b in &mut f.blocks {
            for inst in &mut b.instructions {
                Self::substitute_inst_uses(inst, subst);
            }
            match &mut b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        if let Some(new_a) = subst.get(a) {
                            *a = *new_a;
                        }
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    if let Some(new_c) = subst.get(cond) {
                        *cond = *new_c;
                    }
                    for a in then_args {
                        if let Some(new_a) = subst.get(a) {
                            *a = *new_a;
                        }
                    }
                    for a in else_args {
                        if let Some(new_a) = subst.get(a) {
                            *a = *new_a;
                        }
                    }
                }
                Terminator::Return { value: Some(v) } => {
                    if let Some(new_v) = subst.get(v) {
                        *v = *new_v;
                    }
                }
                _ => {}
            }
        }
    }

    fn substitute_inst_uses(inst: &mut Inst, map: &HashMap<ValueId, ValueId>) {
        match inst {
            Inst::BinOp { left, right, .. } => {
                if let Some(new_l) = map.get(left) {
                    *left = *new_l;
                }
                if let Some(new_r) = map.get(right) {
                    *right = *new_r;
                }
            }
            Inst::UnOp { operand, .. } => {
                if let Some(new_op) = map.get(operand) {
                    *operand = *new_op;
                }
            }
            Inst::AssignVar { value, .. } => {
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            Inst::Call { args, .. } => {
                for a in args {
                    if let Some(new_a) = map.get(a) {
                        *a = *new_a;
                    }
                }
            }
            Inst::MethodCall { object, args, .. } => {
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
                for a in args {
                    if let Some(new_a) = map.get(a) {
                        *a = *new_a;
                    }
                }
            }
            Inst::StructInit { fields, .. } => {
                for (_, v) in fields {
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
            }
            Inst::GetField { object, .. } => {
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
            }
            Inst::SetField { object, value, .. } => {
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => {
                if let Some(new_c) = map.get(cond) {
                    *cond = *new_c;
                }
                if let Some(new_t) = map.get(then_val) {
                    *then_val = *new_t;
                }
                if let Some(new_e) = map.get(else_val) {
                    *else_val = *new_e;
                }
            }
            Inst::Decide { arms, else_val, .. } => {
                for (c, v) in arms {
                    if let Some(new_c) = map.get(c) {
                        *c = *new_c;
                    }
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
                if let Some(e) = else_val {
                    if let Some(new_e) = map.get(e) {
                        *e = *new_e;
                    }
                }
            }
            Inst::FormatStr { values, .. } => {
                for v in values {
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                if let Some(new_c) = map.get(cond_val) {
                    *cond_val = *new_c;
                }
                for i in condition_insts {
                    Self::substitute_inst_uses(i, map);
                }
                for i in body_insts {
                    Self::substitute_inst_uses(i, map);
                }
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for i in try_insts {
                    Self::substitute_inst_uses(i, map);
                }
                for i in catch_insts {
                    Self::substitute_inst_uses(i, map);
                }
            }
            Inst::InlineAsm { inputs, .. } => {
                for (_, i) in inputs {
                    if let Some(new_i) = map.get(i) {
                        *i = *new_i;
                    }
                }
            }
            Inst::Return { value: Some(v) } => {
                if let Some(new_v) = map.get(v) {
                    *v = *new_v;
                }
            }
            Inst::Out { value } | Inst::Err { value } => {
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            _ => {}
        }
    }

    fn substitute_inst_vids(inst: &mut Inst, map: &HashMap<ValueId, ValueId>) {
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::GetFuncAddr { dest, .. }
            | Inst::LoadVar { dest, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
            }
            Inst::AssignVar { value, .. } => {
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            Inst::BinOp {
                dest, left, right, ..
            } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                if let Some(new_l) = map.get(left) {
                    *left = *new_l;
                }
                if let Some(new_r) = map.get(right) {
                    *right = *new_r;
                }
            }
            Inst::UnOp { dest, operand, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                if let Some(new_op) = map.get(operand) {
                    *operand = *new_op;
                }
            }
            Inst::Call { dest, args, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                for a in args {
                    if let Some(new_a) = map.get(a) {
                        *a = *new_a;
                    }
                }
            }
            Inst::MethodCall {
                dest, object, args, ..
            } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
                for a in args {
                    if let Some(new_a) = map.get(a) {
                        *a = *new_a;
                    }
                }
            }
            Inst::StructInit { dest, fields, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                for (_, v) in fields {
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
            }
            Inst::GetField { dest, object, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
            }
            Inst::SetField { object, value, .. } => {
                if let Some(new_o) = map.get(object) {
                    *object = *new_o;
                }
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            Inst::FormatStr { dest, values, .. } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                for v in values {
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ..
            } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                if let Some(new_c) = map.get(cond) {
                    *cond = *new_c;
                }
                if let Some(new_t) = map.get(then_val) {
                    *then_val = *new_t;
                }
                if let Some(new_e) = map.get(else_val) {
                    *else_val = *new_e;
                }
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ..
            } => {
                if let Some(new_d) = map.get(dest) {
                    *dest = *new_d;
                }
                for (c, v) in arms {
                    if let Some(new_c) = map.get(c) {
                        *c = *new_c;
                    }
                    if let Some(new_v) = map.get(v) {
                        *v = *new_v;
                    }
                }
                if let Some(e) = else_val {
                    if let Some(new_e) = map.get(e) {
                        *e = *new_e;
                    }
                }
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                if let Some(new_c) = map.get(cond_val) {
                    *cond_val = *new_c;
                }
                for i in condition_insts {
                    Self::substitute_inst_vids(i, map);
                }
                for i in body_insts {
                    Self::substitute_inst_vids(i, map);
                }
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for i in try_insts {
                    Self::substitute_inst_vids(i, map);
                }
                for i in catch_insts {
                    Self::substitute_inst_vids(i, map);
                }
            }
            Inst::InlineAsm {
                outputs, inputs, ..
            } => {
                for (_, d) in outputs {
                    if let Some(new_d) = map.get(d) {
                        *d = *new_d;
                    }
                }
                for (_, i) in inputs {
                    if let Some(new_i) = map.get(i) {
                        *i = *new_i;
                    }
                }
            }
            Inst::Return { value: Some(v) } => {
                if let Some(new_v) = map.get(v) {
                    *v = *new_v;
                }
            }
            Inst::Out { value } | Inst::Err { value } => {
                if let Some(new_v) = map.get(value) {
                    *value = *new_v;
                }
            }
            _ => {}
        }
    }

    fn fold_constants_in_function(f: &mut Function) {
        let mut int_consts: HashMap<ValueId, i64> = HashMap::new();
        for b in &mut f.blocks {
            let mut new_insts = Vec::new();
            for inst in &b.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        int_consts.insert(*dest, *value);
                        new_insts.push(inst.clone());
                    }
                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ..
                    } => {
                        if let (Some(l), Some(r)) = (int_consts.get(left), int_consts.get(right)) {
                            let folded = match op.as_str() {
                                "+" => Some(l.wrapping_add(*r)),
                                "-" => Some(l.wrapping_sub(*r)),
                                "*" => Some(l.wrapping_mul(*r)),
                                "/" if *r != 0 => Some(l.wrapping_div(*r)),
                                _ => None,
                            };
                            if let Some(val) = folded {
                                int_consts.insert(*dest, val);
                                new_insts.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: val,
                                });
                                continue;
                            }
                        }
                        new_insts.push(inst.clone());
                    }
                    _ => new_insts.push(inst.clone()),
                }
            }
            b.instructions = new_insts;
        }
    }
}
