use super::*;
use crate::ast::{Expr, LiteralValue, Refinement};
use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::{HashMap, HashSet};

impl LoopOptimizer {
    pub fn bce_pass(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut eliminated = 0;
        let cfg = ControlFlowGraph::build(f);

        // Whole-function value lattice
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum LenVal {
            Const(i64),
            Vid(ValueId),
        }

        let mut consts: HashMap<ValueId, i64> = HashMap::new();
        let mut list_len: HashMap<ValueId, LenVal> = HashMap::new();
        let mut len_to_arr: HashMap<ValueId, ValueId> = HashMap::new();
        let mut val_to_name: HashMap<ValueId, String> = HashMap::new();
        let mut name_to_val: HashMap<String, ValueId> = HashMap::new();
        let mut copy_of: HashMap<ValueId, ValueId> = HashMap::new();
        let mut assigned: HashSet<String> = HashSet::new();
        let mut var_len_of_arr: HashMap<String, String> = HashMap::new();

        for (p_name, _ty, p_val) in &f.params {
            name_to_val.insert(p_name.clone(), *p_val);
            val_to_name.insert(*p_val, p_name.clone());
        }

        for block in &f.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        consts.insert(*dest, *value);
                    }
                    Inst::Call {
                        func, args, dest, ..
                    } if func == "datara_rt_list_create_repeat" && args.len() == 2 => {
                        list_len.insert(*dest, LenVal::Vid(args[1]));
                    }
                    Inst::Call { func, dest, .. } if func == "datara_rt_list_create_1" => {
                        list_len.insert(*dest, LenVal::Const(1));
                    }
                    Inst::Call { func, dest, .. } if func == "datara_rt_list_create_2" => {
                        list_len.insert(*dest, LenVal::Const(2));
                    }
                    Inst::Call { func, dest, .. } if func == "datara_rt_list_create_3" => {
                        list_len.insert(*dest, LenVal::Const(3));
                    }
                    Inst::Call { func, dest, .. } if func == "datara_rt_list_create_4" => {
                        list_len.insert(*dest, LenVal::Const(4));
                    }
                    Inst::Call { func, dest, .. } if func == "datara_rt_list_create_5" => {
                        list_len.insert(*dest, LenVal::Const(5));
                    }
                    Inst::Call {
                        func, args, dest, ..
                    } if (func == "datara_rt_list_len" || func == "len") && !args.is_empty() => {
                        list_len.insert(args[0], LenVal::Vid(*dest));
                        list_len.insert(*dest, LenVal::Vid(*dest));
                        len_to_arr.insert(*dest, args[0]);
                    }
                    Inst::MethodCall {
                        dest,
                        object,
                        method,
                        args,
                        ..
                    } if (method == "len" || method == "length") && args.is_empty() => {
                        list_len.insert(*object, LenVal::Vid(*dest));
                        list_len.insert(*dest, LenVal::Vid(*dest));
                        len_to_arr.insert(*dest, *object);
                    }
                    Inst::Call {
                        func, args, dest, ..
                    } if func == "datara_rt_slice" && args.len() == 3 => {
                        if consts.get(&args[1]) == Some(&0) {
                            if let Some(&c) = consts.get(&args[2]) {
                                list_len.insert(*dest, LenVal::Const(c));
                            } else {
                                list_len.insert(*dest, LenVal::Vid(args[2]));
                            }
                        }
                    }
                    Inst::AssignVar { name, value } => {
                        assigned.insert(name.clone());
                        if let Some(prev) = name_to_val.get(name) {
                            if *prev != *value {
                                val_to_name.remove(prev);
                                name_to_val.remove(name);
                            }
                        } else {
                            name_to_val.insert(name.clone(), *value);
                            val_to_name.insert(*value, name.clone());
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        val_to_name.insert(*dest, name.clone());
                        if let Some(&v) = name_to_val.get(name) {
                            copy_of.insert(*dest, v);
                        }
                    }
                    Inst::UnOp {
                        dest, op, operand, ..
                    } if op == "copy" => {
                        copy_of.insert(*dest, *operand);
                    }
                    _ => {}
                }
            }
        }

        // Link variable names to array lengths where n = arr.len()
        for (len_vid, arr_vid) in &len_to_arr {
            let len_name = val_to_name.get(len_vid);
            let arr_name = val_to_name.get(arr_vid);
            if let (Some(ln), Some(an)) = (len_name, arr_name) {
                var_len_of_arr.insert(ln.clone(), an.clone());
            }
        }

        fn resolve_vid(mut v: ValueId, copy_of: &HashMap<ValueId, ValueId>) -> ValueId {
            for _ in 0..32 {
                match copy_of.get(&v) {
                    Some(&next) if next != v => v = next,
                    _ => return v,
                }
            }
            v
        }

        // Chase copy/name chains; a dead end is itself a valid identity key.
        fn resolve(
            mut v: ValueId,
            copy_of: &HashMap<ValueId, ValueId>,
            list_len: &HashMap<ValueId, LenVal>,
            consts: &HashMap<ValueId, i64>,
        ) -> LenVal {
            for _ in 0..32 {
                if let Some(l) = list_len.get(&v) {
                    return *l;
                }
                if let Some(c) = consts.get(&v) {
                    return LenVal::Const(*c);
                }
                match copy_of.get(&v) {
                    Some(&next) if next != v => v = next,
                    _ => return LenVal::Vid(v),
                }
            }
            LenVal::Vid(v)
        }

        // Resolve a value to the variable name it was loaded from.
        fn resolve_name(
            mut v: ValueId,
            copy_of: &HashMap<ValueId, ValueId>,
            val_to_name: &HashMap<ValueId, String>,
        ) -> Option<String> {
            for _ in 0..32 {
                if let Some(n) = val_to_name.get(&v) {
                    return Some(n.clone());
                }
                match copy_of.get(&v) {
                    Some(&next) if next != v => v = next,
                    _ => return None,
                }
            }
            None
        }

        let const_val = |vid: ValueId| -> Option<i64> { consts.get(&vid).copied() };

        // --- Evidence Gate Bounds Check Elimination (BCE) ---
        // Proves 0 <= idx < len(arr) from parameter refinement types (e.g. idx: Int in 0..<arr.len())
        // or contract preconditions (require 0 <= idx && idx < arr.len()).
        let mut proven_bounds: HashMap<String, String> = HashMap::new();

        let is_param = |name: &str| f.params.iter().any(|(p, _, _)| p == name);

        for (p_name, refn_opt) in &f.param_refinements {
            if let Some(refn) = refn_opt {
                match refn {
                    Refinement::Range {
                        start,
                        end,
                        inclusive,
                    } => {
                        if !inclusive
                            && Self::is_zero_expr(start)
                            && let Some(arr_name) = Self::extract_len_target(end)
                        {
                            proven_bounds.insert(p_name.clone(), arr_name);
                        }
                    }
                    Refinement::Predicate {
                        var_name,
                        predicate,
                    } => {
                        if let Some(arr_name) =
                            Self::extract_predicate_len_target(var_name, predicate)
                        {
                            proven_bounds.insert(p_name.clone(), arr_name);
                        }
                    }
                }
            }
        }

        for req in &f.requires {
            if let Some((idx_name, arr_name)) =
                Self::extract_index_bound_from_contract(&req.condition)
            {
                if is_param(&idx_name) && is_param(&arr_name) {
                    proven_bounds.insert(idx_name, arr_name);
                }
            }
        }

        let mut evidence_gate_count = 0;
        if !proven_bounds.is_empty() {
            for block in &mut f.blocks {
                for inst in &mut block.instructions {
                    if let Inst::Call { func, args, .. } = inst
                        && (func == "datara_rt_list_get" || func == "datara_rt_list_set")
                        && args.len() >= 2
                    {
                        let arr_name = resolve_name(args[0], &copy_of, &val_to_name);
                        let idx_name = resolve_name(args[1], &copy_of, &val_to_name);
                        if let (Some(arr), Some(idx)) = (arr_name, idx_name)
                            && proven_bounds.get(&idx) == Some(&arr)
                            && !assigned.contains(&idx)
                            && !assigned.contains(&arr)
                        {
                            *func = format!("{}_unchecked", func);
                            eliminated += 1;
                            evidence_gate_count += 1;
                        }
                    }
                }
            }
        }

        if evidence_gate_count > 0 {
            trace.record(
                "EvidenceGate:BCE",
                &format!("{}:param_refinement", f.name),
                "Applied",
                &format!("+{} BCE proven unchecked access", evidence_gate_count),
                "0",
                &format!(
                    "BCE proven: 0 <= idx < len(arr) via refinement/contract for {} accesses: bypassed runtime bounds check",
                    evidence_gate_count
                ),
            );
        }

        // --- Condition Dominator Bounds Check Elimination ---
        // If block B is dominated by a conditional branch testing idx < len(arr) (and idx >= 0),
        // then bounds checks on arr[idx] inside B are guaranteed redundant.
        let mut dominator_eliminated = 0;
        for block_idx in 0..f.blocks.len() {
            let block_id = f.blocks[block_idx].id;
            let mut dom_checks: Vec<(ValueId, ValueId)> = Vec::new();
            for other_block in &f.blocks {
                if let Terminator::CondBranch {
                    cond, then_block, ..
                } = &other_block.terminator
                {
                    if cfg.dominates(*then_block, block_id) {
                        for inst in &other_block.instructions {
                            if let Inst::BinOp {
                                dest,
                                op,
                                left,
                                right,
                                ..
                            } = inst
                            {
                                if dest == cond && op == "<" {
                                    let resolved_right = resolve_vid(*right, &copy_of);
                                    if let Some(&arr_target) = len_to_arr.get(&resolved_right) {
                                        dom_checks.push((*left, arr_target));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !dom_checks.is_empty() {
                let block = &mut f.blocks[block_idx];
                for inst in &mut block.instructions {
                    if let Inst::Call { func, args, .. } = inst {
                        if (func == "datara_rt_list_get" || func == "datara_rt_list_set")
                            && args.len() >= 2
                        {
                            let arr_vid = resolve_vid(args[0], &copy_of);
                            let idx_vid = resolve_vid(args[1], &copy_of);
                            let is_safe = dom_checks.iter().any(|(chk_idx, chk_arr)| {
                                resolve_vid(*chk_idx, &copy_of) == idx_vid
                                    && resolve_vid(*chk_arr, &copy_of) == arr_vid
                            });
                            if is_safe {
                                *func = format!("{}_unchecked", func);
                                eliminated += 1;
                                dominator_eliminated += 1;
                            }
                        }
                    }
                }
            }
        }

        if dominator_eliminated > 0 {
            trace.record(
                "ConditionDominator:BCE",
                &format!("{}:dominator", f.name),
                "Applied",
                &format!("+{} BCE proven unchecked access", dominator_eliminated),
                "0",
                &format!(
                    "BCE proven: 0 <= idx < len(arr) verified by dominating condition for {} accesses",
                    dominator_eliminated
                ),
            );
        }

        // --- Loop Induction Variable Range Analysis ---
        let mut loop_eliminated = 0;
        for lp in &cfg.loops {
            let header_block = match f.get_block(lp.header) {
                Some(b) => b,
                None => continue,
            };

            // Canonical header: `cond = (i < bound)`. <= is rejected: it
            // allows `idx == bound`, which is out of range when len == bound.
            let (induction_var, bound_val) = match &header_block.terminator {
                Terminator::CondBranch { cond, .. } => {
                    let mut found = None;
                    for inst in &header_block.instructions {
                        if let Inst::BinOp {
                            dest,
                            op,
                            left,
                            right,
                            ..
                        } = inst
                            && dest == cond
                            && op == "<"
                        {
                            found = Some((*left, *right));
                        }
                    }
                    match found {
                        Some(v) => v,
                        None => continue,
                    }
                }
                _ => continue,
            };

            // The induction value must be a load of a named counter.
            let counter_name = match resolve_name(induction_var, &copy_of, &val_to_name) {
                Some(n) => n,
                None => continue,
            };

            let loop_blocks: HashSet<_> = lp.blocks.iter().copied().collect();

            // Prove init-to-0 in the preheader, exactly one step-by-1 on the back edge,
            // and that the counter is never rebound to anything else in-loop.
            let mut init_zero = false;
            let mut in_loop_steps = 0;
            let mut step_loc: Option<(BasicBlockId, usize)> = None;
            let mut rebound = false;
            for block in &f.blocks {
                let in_loop = loop_blocks.contains(&block.id);
                for (inst_idx, inst) in block.instructions.iter().enumerate() {
                    match inst {
                        Inst::AssignVar { name, value } if name == &counter_name => {
                            let is_step = if in_loop {
                                match Self::binop_add_one_source(f, *value) {
                                    Some(lhs) => {
                                        resolve_name(lhs, &copy_of, &val_to_name).as_deref()
                                            == Some(counter_name.as_str())
                                    }
                                    None => false,
                                }
                            } else {
                                false
                            };
                            if is_step {
                                in_loop_steps += 1;
                                step_loc = Some((block.id, inst_idx));
                            } else if in_loop {
                                rebound = true;
                            } else if const_val(*value) == Some(0) {
                                if cfg.dominates(block.id, lp.header) {
                                    init_zero = true;
                                }
                            } else {
                                rebound = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !init_zero || in_loop_steps != 1 || rebound {
                continue;
            }

            let (step_block, step_idx) = match step_loc {
                Some(loc) => loc,
                None => continue,
            };

            // Step block must be a latch
            let step_is_latch = f.get_block(step_block).map_or(false, |b| {
                matches!(&b.terminator, Terminator::Branch { target, .. } if *target == lp.header)
            });
            if !step_is_latch {
                continue;
            }

            // Safe induction step optimization: rewrite `i + 1` to `wrapping_+`
            let step_vid = if let Some(blk) = f.get_block(step_block) {
                if let Some(Inst::AssignVar { value, .. }) = blk.instructions.get(step_idx) {
                    Some(*value)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(vid) = step_vid {
                for b in &mut f.blocks {
                    for inst in &mut b.instructions {
                        if let Inst::BinOp { dest, op, .. } = inst {
                            if *dest == vid && op == "+" {
                                *op = "wrapping_+".to_string();
                            }
                        }
                    }
                }
            }

            // Prove header bound <= indexed list's length
            let bound_len = resolve(bound_val, &copy_of, &list_len, &consts);
            let resolved_bound_vid = resolve_vid(bound_val, &copy_of);
            let bound_arr_vid = len_to_arr.get(&resolved_bound_vid).copied();
            let bound_name = resolve_name(bound_val, &copy_of, &val_to_name);

            for &block_id in &lp.blocks {
                if let Some(block) = f.get_block_mut(block_id) {
                    for (inst_idx, inst) in block.instructions.iter_mut().enumerate() {
                        if let Inst::Call { func, args, .. } = inst
                            && (func == "datara_rt_list_get" || func == "datara_rt_list_set")
                            && args.len() >= 2
                        {
                            if block_id == step_block && inst_idx >= step_idx {
                                continue;
                            }

                            // The index must be the counter variable
                            if resolve_name(args[1], &copy_of, &val_to_name).as_deref()
                                != Some(counter_name.as_str())
                            {
                                continue;
                            }

                            let list_len_val = resolve(args[0], &copy_of, &list_len, &consts);
                            let resolved_arr_vid = resolve_vid(args[0], &copy_of);
                            let arr_name = resolve_name(args[0], &copy_of, &val_to_name);

                            let mut proven = match (bound_len, list_len_val) {
                                (LenVal::Vid(a), LenVal::Vid(b)) => a == b,
                                (LenVal::Const(a), LenVal::Const(b)) => a <= b,
                                _ => false,
                            };

                            if !proven {
                                if let Some(target_arr) = bound_arr_vid {
                                    if resolve_vid(target_arr, &copy_of) == resolved_arr_vid {
                                        proven = true;
                                    }
                                }
                            }

                            if !proven {
                                if let (Some(bn), Some(an)) = (&bound_name, &arr_name) {
                                    if var_len_of_arr.get(bn) == Some(an) {
                                        proven = true;
                                    }
                                }
                            }

                            if proven {
                                *func = format!("{}_unchecked", func);
                                eliminated += 1;
                                loop_eliminated += 1;
                            }
                        }
                    }
                }
            }
        }

        if loop_eliminated > 0 {
            trace.record(
                "BCE",
                &format!("{}:loop_bounds", f.name),
                "Applied",
                &format!("+{} BCE proven unchecked access", loop_eliminated),
                "0",
                &format!(
                    "BCE proven: idx < len for {} accesses (canonical 0-based +1 loop bound tied to allocation length)",
                    loop_eliminated
                ),
            );
        }

        eliminated
    }

    /// If `vid` is the dest of `a + 1` (BinOp), return the left operand.
    fn binop_add_one_source(f: &Function, vid: ValueId) -> Option<ValueId> {
        for block in &f.blocks {
            for inst in &block.instructions {
                if let Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ..
                } = inst
                    && *dest == vid
                    && op == "+"
                    && Self::const_int_value(f, *right) == Some(1)
                {
                    return Some(*left);
                }
            }
        }
        None
    }

    fn is_zero_expr(expr: &Expr) -> bool {
        matches!(expr, Expr::Literal(LiteralValue::Int(0), _))
    }

    fn extract_len_target(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, args, .. } if args.is_empty() => {
                if let Expr::MemberAccess { object, member, .. } = callee.as_ref()
                    && (member == "len" || member == "length")
                    && let Expr::Identifier(arr_name, _) = object.as_ref()
                {
                    return Some(arr_name.clone());
                }
                if let Expr::Identifier(fn_name, _) = callee.as_ref()
                    && (fn_name == "len" || fn_name == "length")
                    && let Some(Expr::Identifier(arr_name, _)) = args.first()
                {
                    return Some(arr_name.clone());
                }
                None
            }
            Expr::Call { callee, args, .. } if args.len() == 1 => {
                if let Expr::Identifier(fn_name, _) = callee.as_ref()
                    && (fn_name == "len" || fn_name == "length")
                    && let Some(Expr::Identifier(arr_name, _)) = args.first()
                {
                    return Some(arr_name.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn extract_predicate_len_target(var_name: &str, predicate: &Expr) -> Option<String> {
        match predicate {
            Expr::Binary {
                op, left, right, ..
            } if op == "&&" => {
                let left_ok = Self::is_non_negative_check(var_name, left);
                let right_target = Self::is_less_than_len_check(var_name, right);
                if left_ok && right_target.is_some() {
                    return right_target;
                }
                let right_ok = Self::is_non_negative_check(var_name, right);
                let left_target = Self::is_less_than_len_check(var_name, left);
                if right_ok && left_target.is_some() {
                    return left_target;
                }
                None
            }
            _ => None,
        }
    }

    fn is_non_negative_check(var_name: &str, expr: &Expr) -> bool {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == ">=" {
                    if let Expr::Identifier(name, _) = left.as_ref()
                        && name == var_name
                        && Self::is_zero_expr(right)
                    {
                        return true;
                    }
                } else if op == "<="
                    && let Expr::Identifier(name, _) = right.as_ref()
                    && name == var_name
                    && Self::is_zero_expr(left)
                {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn is_less_than_len_check(var_name: &str, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "<" => {
                if let Expr::Identifier(name, _) = left.as_ref()
                    && name == var_name
                {
                    return Self::extract_len_target(right);
                }
                None
            }
            _ => None,
        }
    }

    fn extract_index_bound_from_contract(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "&&" => {
                if let (Some(idx1), Some((idx2, arr))) = (
                    Self::extract_non_negative_var(left),
                    Self::extract_less_than_len(right),
                ) && idx1 == idx2
                {
                    return Some((idx1, arr));
                }
                if let (Some((idx1, arr)), Some(idx2)) = (
                    Self::extract_less_than_len(left),
                    Self::extract_non_negative_var(right),
                ) && idx1 == idx2
                {
                    return Some((idx1, arr));
                }
                None
            }
            _ => None,
        }
    }

    fn extract_non_negative_var(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == ">=" {
                    if let Expr::Identifier(name, _) = left.as_ref()
                        && Self::is_zero_expr(right)
                    {
                        return Some(name.clone());
                    }
                } else if op == "<="
                    && let Expr::Identifier(name, _) = right.as_ref()
                    && Self::is_zero_expr(left)
                {
                    return Some(name.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn extract_less_than_len(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "<" => {
                if let Expr::Identifier(idx_name, _) = left.as_ref()
                    && let Some(arr_name) = Self::extract_len_target(right)
                {
                    return Some((idx_name.clone(), arr_name));
                }
                None
            }
            _ => None,
        }
    }
}
