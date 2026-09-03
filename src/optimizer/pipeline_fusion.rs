use crate::dmir::{Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::HashMap;

pub struct PipelineFusionOptimizer;

impl PipelineFusionOptimizer {
    /// Applies high-level pipeline fusion optimizations:
    /// 1. N-ary String Polyhedral Fusion: collapses chained binary string
    ///    concatenations into single atomic 3/4/5-ary runtime calls with a single allocation.
    /// 2. Arithmetic Pipeline Reassociation: collapses multi-stage associative
    ///    constant operations ((x + C1) + C2 => x + (C1 + C2)).
    pub fn fuse_pipelines(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut total_fused = 0;

        // Count global uses of each ValueId in the function
        let uses = Self::count_uses(f);

        // 1. String Concatenation Fusion
        total_fused += Self::fuse_string_concatenations(f, &uses, trace);

        // 2. Arithmetic Pipeline Reassociation
        total_fused += Self::fuse_arithmetic_chains(f, &uses, trace);

        total_fused
    }

    fn count_uses(f: &Function) -> HashMap<ValueId, usize> {
        let mut uses = HashMap::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                Self::for_each_inst_use(inst, |vid| {
                    *uses.entry(vid).or_insert(0) += 1;
                });
            }
            match &b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        *uses.entry(*a).or_insert(0) += 1;
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    *uses.entry(*cond).or_insert(0) += 1;
                    for a in then_args {
                        *uses.entry(*a).or_insert(0) += 1;
                    }
                    for a in else_args {
                        *uses.entry(*a).or_insert(0) += 1;
                    }
                }
                Terminator::Return { value: Some(v) } => {
                    *uses.entry(*v).or_insert(0) += 1;
                }
                _ => {}
            }
        }
        uses
    }

    fn for_each_inst_use<F: FnMut(ValueId)>(inst: &Inst, mut f: F) {
        match inst {
            Inst::BinOp { left, right, .. } => {
                f(*left);
                f(*right);
            }
            Inst::UnOp { operand, .. } => f(*operand),
            Inst::Call { args, .. } => {
                for a in args {
                    f(*a);
                }
            }
            Inst::MethodCall { object, args, .. } => {
                f(*object);
                for a in args {
                    f(*a);
                }
            }
            Inst::AssignVar { value, .. } => f(*value),
            Inst::Out { value } => f(*value),
            Inst::Err { value } => f(*value),
            Inst::StructInit { fields, .. } => {
                for (_, v) in fields {
                    f(*v);
                }
            }
            Inst::GetField { object, .. } => f(*object),
            Inst::SetField { object, value, .. } => {
                f(*object);
                f(*value);
            }
            Inst::FormatStr { values, .. } => {
                for v in values {
                    f(*v);
                }
            }
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => {
                f(*cond);
                f(*then_val);
                f(*else_val);
            }
            Inst::Decide { arms, else_val, .. } => {
                for (cond, val) in arms {
                    f(*cond);
                    f(*val);
                }
                if let Some(ev) = else_val {
                    f(*ev);
                }
            }
            _ => {}
        }
    }

    /// Fuses trees of binary string concatenations into N-ary calls
    fn fuse_string_concatenations(
        f: &mut Function,
        uses: &HashMap<ValueId, usize>,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut fused_count = 0;

        let mut val_types: HashMap<ValueId, String> = HashMap::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                match inst {
                    Inst::ConstStr { dest, .. } => {
                        val_types.insert(*dest, "String".to_string());
                    }
                    Inst::ConstInt { dest, .. } => {
                        val_types.insert(*dest, "Int".to_string());
                    }
                    Inst::ConstFloat { dest, .. } => {
                        val_types.insert(*dest, "Float".to_string());
                    }
                    Inst::BinOp { dest, ty, .. } => {
                        val_types.insert(*dest, ty.clone());
                    }
                    Inst::Call { dest, ty, .. } => {
                        val_types.insert(*dest, ty.clone());
                    }
                    _ => {}
                }
            }
        }

        for block in &mut f.blocks {
            // Map each dest of a concat to its (left, right) or arg list
            let mut concat_map: HashMap<ValueId, Vec<ValueId>> = HashMap::new();

            for inst in &mut block.instructions {
                let (dest, args) = match inst {
                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ty,
                    } if op == "+" && (ty == "String" || ty.contains("Str")) => {
                        (*dest, vec![*left, *right])
                    }
                    Inst::Call {
                        dest, func, args, ..
                    } if func == "datara_rt_str_concat" && args.len() == 2 => (*dest, args.clone()),
                    Inst::Call {
                        dest, func, args, ..
                    } if func == "datara_rt_str_concat_3" && args.len() == 3 => {
                        (*dest, args.clone())
                    }
                    Inst::Call {
                        dest, func, args, ..
                    } if func == "datara_rt_str_concat_4" && args.len() == 4 => {
                        (*dest, args.clone())
                    }
                    _ => continue,
                };

                // Check if the first argument is itself a single-use string concat
                let first_arg = args[0];
                if uses.get(&first_arg).copied().unwrap_or(0) <= 1
                    && let Some(prev_args) = concat_map.get(&first_arg)
                {
                    let mut combined = prev_args.clone();
                    combined.extend(&args[1..]);

                    if combined.len() <= 5 {
                        // Check for direct-slot string+int wire-blit pattern [str, int, str, int]
                        let is_sisi = combined.len() == 4
                            && (val_types
                                .get(&combined[0])
                                .map(|s| s == "String")
                                .unwrap_or(true))
                            && val_types
                                .get(&combined[1])
                                .map(|s| s == "Int")
                                .unwrap_or(false)
                            && (val_types
                                .get(&combined[2])
                                .map(|s| s == "String")
                                .unwrap_or(true))
                            && val_types
                                .get(&combined[3])
                                .map(|s| s == "Int")
                                .unwrap_or(false);

                        // Rewrite the current instruction into an N-ary concat or direct-slot format call
                        let new_func = if is_sisi {
                            "datara_rt_format_str_i64_str_i64"
                        } else {
                            match combined.len() {
                                3 => "datara_rt_str_concat_3",
                                4 => "datara_rt_str_concat_4",
                                5 => "datara_rt_str_concat_5",
                                _ => "datara_rt_str_concat",
                            }
                        };

                        *inst = Inst::Call {
                            dest,
                            func: new_func.to_string(),
                            args: combined.clone(),
                            ty: "String".to_string(),
                        };

                        concat_map.insert(dest, combined);
                        fused_count += 1;

                        trace.record(
                                "StringConcatPolyhedralFusion",
                                &format!("{}:bb{}", f.name, block.id.0),
                                "Applied",
                                &format!("Fused string tree into {}", new_func),
                                "Eliminated intermediate string buffer allocations",
                                "Transformed chained string concatenation into single-allocation N-ary runtime call",
                            );
                        continue;
                    }
                }

                concat_map.insert(dest, args);
            }
        }

        fused_count
    }

    /// Reassociates arithmetic pipelines:
    /// ((x + C1) + C2 => x + (C1 + C2))
    /// ((x * C1) * C2 => x * (C1 * C2))
    /// ((x & C1) & C2 => x & (C1 & C2))
    /// ((x | C1) | C2 => x | (C1 | C2))
    fn fuse_arithmetic_chains(
        f: &mut Function,
        uses: &HashMap<ValueId, usize>,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut fused_count = 0;

        // Collect all ConstInt values in the function
        let mut const_ints = HashMap::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::ConstInt { dest, value } = inst {
                    const_ints.insert(*dest, *value);
                }
            }
        }

        let mut next_vid = Self::max_vid(f) + 1;

        for block in &mut f.blocks {
            // Map dest -> (op, base_var, constant_value)
            let mut affine_chains: HashMap<ValueId, (String, ValueId, i64)> = HashMap::new();
            let mut new_instructions = Vec::new();

            for inst in &block.instructions {
                if let Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ty,
                } = inst
                    && (ty == "Int" || ty == "i64")
                {
                    let is_c_right = const_ints.get(right).copied();
                    let is_c_left = const_ints.get(left).copied();

                    let is_commutative = matches!(op.as_str(), "+" | "*" | "&" | "|" | "^");

                    let (var, c_val) = match (is_c_right, is_c_left) {
                        (Some(c), None) => (*left, c),
                        (None, Some(c)) if is_commutative => (*right, c),
                        _ => (ValueId(usize::MAX), 0),
                    };

                    if var.0 != usize::MAX {
                        if uses.get(&var).copied().unwrap_or(0) <= 1
                            && let Some((prev_op, base_var, prev_c)) = affine_chains.get(&var)
                            && prev_op == op
                        {
                            let combined = match op.as_str() {
                                "+" => Some(prev_c.wrapping_add(c_val)),
                                "*" => Some(prev_c.wrapping_mul(c_val)),
                                "&" => Some(*prev_c & c_val),
                                "|" => Some(*prev_c | c_val),
                                "^" => Some(*prev_c ^ c_val),
                                _ => None,
                            };

                            if let Some(new_k) = combined {
                                let new_c_vid = ValueId(next_vid);
                                next_vid += 1;
                                const_ints.insert(new_c_vid, new_k);

                                new_instructions.push(Inst::ConstInt {
                                    dest: new_c_vid,
                                    value: new_k,
                                });

                                new_instructions.push(Inst::BinOp {
                                    dest: *dest,
                                    op: op.clone(),
                                    left: *base_var,
                                    right: new_c_vid,
                                    ty: ty.clone(),
                                });

                                affine_chains.insert(*dest, (op.clone(), *base_var, new_k));
                                fused_count += 1;

                                trace.record(
                                                "ArithmeticPipelineReassociation",
                                                &format!("{}:bb{}", f.name, block.id.0),
                                                "Applied",
                                                &format!("Folded associative pipeline on {}", op),
                                                "Eliminated chained arithmetic instruction latency",
                                                "Reassociated multi-stage constant pipeline into single constant operation",
                                            );
                                continue;
                            }
                        }

                        affine_chains.insert(*dest, (op.clone(), var, c_val));
                    }
                }

                new_instructions.push(inst.clone());
            }

            block.instructions = new_instructions;
        }

        fused_count
    }

    fn max_vid(f: &Function) -> usize {
        let mut m = 0;
        for b in &f.blocks {
            for p in &b.params {
                m = m.max(p.val.0);
            }
            for inst in &b.instructions {
                Self::for_each_inst_dest(inst, |vid| {
                    m = m.max(vid.0);
                });
            }
        }
        m
    }

    fn for_each_inst_dest<F: FnMut(ValueId)>(inst: &Inst, mut f: F) {
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
            | Inst::Select { dest, .. }
            | Inst::Decide { dest, .. }
            | Inst::GetFuncAddr { dest, .. } => f(*dest),
            _ => {}
        }
    }
}
