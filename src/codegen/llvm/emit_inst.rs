use super::*;
use crate::dmir::*;
use std::collections::HashMap;

impl<'a> LlvmEmitter<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_instruction(
        &self,
        inst: &Inst,
        module: &Module,
        strings: &HashMap<String, usize>,
        value_types: &mut HashMap<ValueId, &'static str>,
        var_types: &mut HashMap<String, &'static str>,
        value_classes: &mut HashMap<ValueId, String>,
        bool_vids: &mut HashSet<ValueId>,
        bool_vars: &mut HashSet<String>,
        out: &mut String,
        fn_name: &str,
        types: &TypeChecker,
        range_metadata: &mut HashMap<(i64, i64), usize>,
        next_meta_id: &mut usize,
        address_taken: &HashSet<String>,
    ) {
        match inst {
            Inst::ConstInt { dest, value } => {
                value_types.insert(*dest, "i64");
                out.push_str(&format!("  %v{} = add i64 0, {}\n", dest.0, value));
            }
            Inst::ConstFloat { dest, value } => {
                value_types.insert(*dest, "double");
                out.push_str(&format!(
                    "  %v{} = fadd double 0.0, {:.17}\n",
                    dest.0, value
                ));
            }
            Inst::ConstBool { dest, value } => {
                value_types.insert(*dest, "i64");
                bool_vids.insert(*dest);
                let b_val = if *value { 1 } else { 0 };
                out.push_str(&format!("  %v{} = add i64 0, {}\n", dest.0, b_val));
            }
            Inst::ConstStr { dest, value } => {
                value_types.insert(*dest, "ptr");
                let str_id = strings.get(value).copied().unwrap_or(0);
                out.push_str(&format!(
                    "  %v{} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                    dest.0, str_id
                ));
            }
            Inst::LoadVar { dest, name } => {
                let vty = var_types.get(name).copied().unwrap_or("i64");
                value_types.insert(*dest, vty);
                if bool_vars.contains(name) {
                    bool_vids.insert(*dest);
                }
                let align = if vty == "<4 x float>" { 16 } else { 8 };
                if let Some((min, max)) = get_range_for_var(fn_name, name, None, types) {
                    let high = max.saturating_add(1);
                    let meta_id = *range_metadata.entry((min, high)).or_insert_with(|| {
                        let id = *next_meta_id;
                        *next_meta_id += 1;
                        id
                    });
                    out.push_str(&format!(
                        "  %v{} = load {}, ptr %var_{}, align {}, !range !{}\n",
                        dest.0, vty, name, align, meta_id
                    ));
                } else {
                    out.push_str(&format!(
                        "  %v{} = load {}, ptr %var_{}, align {}\n",
                        dest.0, vty, name, align
                    ));
                }
            }
            Inst::AssignVar { name, value } => {
                let vty = value_types.get(value).copied().unwrap_or("i64");
                var_types.insert(name.clone(), vty);
                if bool_vids.contains(value) {
                    bool_vars.insert(name.clone());
                } else {
                    bool_vars.remove(name);
                }
                let align = if vty == "<4 x float>" { 16 } else { 8 };
                out.push_str(&format!(
                    "  store {} %v{}, ptr %var_{}, align {}\n",
                    vty, value.0, name, align
                ));
                if let Some((min, max)) = get_range_for_var(fn_name, name, None, types) {
                    out.push_str(&format!(
                        "  %fvrp_amin_{}_{} = icmp sge i64 %v{}, {}\n",
                        name, value.0, value.0, min
                    ));
                    out.push_str(&format!(
                        "  call void @llvm.assume(i1 %fvrp_amin_{}_{})\n",
                        name, value.0
                    ));
                    out.push_str(&format!(
                        "  %fvrp_amax_{}_{} = icmp sle i64 %v{}, {}\n",
                        name, value.0, value.0, max
                    ));
                    out.push_str(&format!(
                        "  call void @llvm.assume(i1 %fvrp_amax_{}_{})\n",
                        name, value.0
                    ));
                }
            }
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } => {
                let l_ty = value_types.get(left).copied().unwrap_or("i64");
                let r_ty = value_types.get(right).copied().unwrap_or("i64");
                let is_float = ty == "Float" || l_ty == "double" || r_ty == "double";
                let is_str = ty == "Str" || ty == "String";

                if (is_str || l_ty == "ptr" || r_ty == "ptr") && op == "+" {
                    value_types.insert(*dest, "ptr");
                    let left_s = if l_ty != "ptr" {
                        let tmp = format!("%str_conv_l_{}", dest.0);
                        if l_ty == "double" {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_float_to_str(double %v{})\n",
                                tmp, left.0
                            ));
                        } else {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                                tmp, left.0
                            ));
                        }
                        tmp
                    } else {
                        format!("%v{}", left.0)
                    };
                    let right_s = if r_ty != "ptr" {
                        let tmp = format!("%str_conv_r_{}", dest.0);
                        if r_ty == "double" {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_float_to_str(double %v{})\n",
                                tmp, right.0
                            ));
                        } else {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                                tmp, right.0
                            ));
                        }
                        tmp
                    } else {
                        format!("%v{}", right.0)
                    };
                    out.push_str(&format!(
                        "  %v{} = call ptr @datara_rt_str_concat(ptr {}, ptr {})\n",
                        dest.0, left_s, right_s
                    ));
                } else if is_float {
                    let left_v = if l_ty == "i64" {
                        let tmp = format!("%fconv_l_{}", dest.0);
                        out.push_str(&format!("  {} = sitofp i64 %v{} to double\n", tmp, left.0));
                        tmp
                    } else {
                        format!("%v{}", left.0)
                    };
                    let right_v = if r_ty == "i64" {
                        let tmp = format!("%fconv_r_{}", dest.0);
                        out.push_str(&format!("  {} = sitofp i64 %v{} to double\n", tmp, right.0));
                        tmp
                    } else {
                        format!("%v{}", right.0)
                    };

                    match op.as_str() {
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            value_types.insert(*dest, "i64");
                            bool_vids.insert(*dest);
                            let fcmp_op = match op.as_str() {
                                "==" => "oeq",
                                "!=" => "one",
                                "<" => "olt",
                                "<=" => "ole",
                                ">" => "ogt",
                                ">=" => "oge",
                                _ => "oeq",
                            };
                            let cmp_temp = format!("%fcmp_{}", dest.0);
                            out.push_str(&format!(
                                "  {} = fcmp {} double {}, {}\n",
                                cmp_temp, fcmp_op, left_v, right_v
                            ));
                            out.push_str(&format!(
                                "  %v{} = zext i1 {} to i64\n",
                                dest.0, cmp_temp
                            ));
                        }
                        _ => {
                            value_types.insert(*dest, "double");
                            let llvm_op = match op.as_str() {
                                "+" => "fadd",
                                "-" => "fsub",
                                "*" => "fmul",
                                "/" => "fdiv",
                                _ => "fadd",
                            };
                            out.push_str(&format!(
                                "  %v{} = {} double {}, {}\n",
                                dest.0, llvm_op, left_v, right_v
                            ));
                        }
                    }
                } else {
                    match op.as_str() {
                        "+" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_checked_add(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "-" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_checked_sub(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "*" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_checked_mul(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "/" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_checked_div(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "%" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_checked_rem(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "wrapping_+" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = add i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "wrapping_-" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = sub i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "wrapping_*" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = mul i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "saturating_+" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_saturating_add(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "saturating_-" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_saturating_sub(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "saturating_*" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = call i64 @datara_rt_saturating_mul(i64 %v{}, i64 %v{})\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            value_types.insert(*dest, "i64");
                            bool_vids.insert(*dest);
                            let cmp_op = match op.as_str() {
                                "==" => "eq",
                                "!=" => "ne",
                                "<" => "slt",
                                "<=" => "sle",
                                ">" => "sgt",
                                ">=" => "sge",
                                _ => "eq",
                            };
                            let cmp_temp = format!("%cmp_{}", dest.0);
                            out.push_str(&format!(
                                "  {} = icmp {} i64 %v{}, %v{}\n",
                                cmp_temp, cmp_op, left.0, right.0
                            ));
                            out.push_str(&format!(
                                "  %v{} = zext i1 {} to i64\n",
                                dest.0, cmp_temp
                            ));
                        }
                        "&" | "&&" => {
                            value_types.insert(*dest, "i64");
                            if bool_vids.contains(left) || bool_vids.contains(right) {
                                bool_vids.insert(*dest);
                            }
                            out.push_str(&format!(
                                "  %v{} = and i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "|" | "||" => {
                            value_types.insert(*dest, "i64");
                            if bool_vids.contains(left) || bool_vids.contains(right) {
                                bool_vids.insert(*dest);
                            }
                            out.push_str(&format!(
                                "  %v{} = or i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "^" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = xor i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "<<" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = shl i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        ">>" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = ashr i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        _ => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = add i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                    }
                }
            }
            Inst::UnOp {
                dest,
                op,
                operand,
                ty,
            } => {
                if ty == "Float" {
                    value_types.insert(*dest, "double");
                    out.push_str(&format!("  %v{} = fneg double %v{}\n", dest.0, operand.0));
                } else if op == "!" {
                    value_types.insert(*dest, "i64");
                    bool_vids.insert(*dest);
                    // Logical NOT: any nonzero value is truthy, so compare
                    // against zero instead of xor 1 (wrong for non-canonical
                    // bools, e.g. 2 -> 3, still truthy).
                    let cmp_temp = format!("%not_c_{}", dest.0);
                    out.push_str(&format!(
                        "  {} = icmp eq i64 %v{}, 0\n",
                        cmp_temp, operand.0
                    ));
                    out.push_str(&format!("  %v{} = zext i1 {} to i64\n", dest.0, cmp_temp));
                } else if op == "copy" || op == "await" {
                    let oty = value_types.get(operand).copied().unwrap_or("i64");
                    value_types.insert(*dest, oty);
                    if bool_vids.contains(operand) {
                        bool_vids.insert(*dest);
                    }
                    if oty == "double" {
                        out.push_str(&format!(
                            "  %v{} = fadd double %v{}, 0.0\n",
                            dest.0, operand.0
                        ));
                    } else if oty == "ptr" {
                        out.push_str(&format!(
                            "  %v{} = getelementptr inbounds i8, ptr %v{}, i64 0\n",
                            dest.0, operand.0
                        ));
                    } else {
                        out.push_str(&format!("  %v{} = or i64 %v{}, 0\n", dest.0, operand.0));
                    }
                } else {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!("  %v{} = sub i64 0, %v{}\n", dest.0, operand.0));
                }
            }
            Inst::Call {
                dest,
                func,
                args,
                ty,
            } => {
                let ret_ty = self.dmir_type_to_llvm(ty);
                value_types.insert(*dest, ret_ty);
                if ty == "Bool"
                    || module
                        .functions
                        .get(func)
                        .map_or(false, |f| f.return_type == "Bool")
                {
                    bool_vids.insert(*dest);
                }

                if (func == "math_ctz" || func == "ctz") && args.len() == 1 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = call i64 @llvm.cttz.i64(i64 %v{}, i1 false)\n",
                        dest.0, args[0].0
                    ));
                    return;
                }
                if (func == "math_shr" || func == "shr") && args.len() == 2 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = lshr i64 %v{}, %v{}\n",
                        dest.0, args[0].0, args[1].0
                    ));
                    return;
                }
                if (func == "math_shl" || func == "shl") && args.len() == 2 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = shl i64 %v{}, %v{}\n",
                        dest.0, args[0].0, args[1].0
                    ));
                    return;
                }
                if (func == "math_xor" || func == "xor") && args.len() == 2 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = xor i64 %v{}, %v{}\n",
                        dest.0, args[0].0, args[1].0
                    ));
                    return;
                }
                if (func == "math_and" || func == "and") && args.len() == 2 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = and i64 %v{}, %v{}\n",
                        dest.0, args[0].0, args[1].0
                    ));
                    return;
                }
                if (func == "math_or" || func == "or") && args.len() == 2 {
                    value_types.insert(*dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = or i64 %v{}, %v{}\n",
                        dest.0, args[0].0, args[1].0
                    ));
                    return;
                }

                // First-Class Hardware SIMD Inlining (<4 x float>)
                if (func == "float4" || func == "datara_rt_float4") && args.len() == 4 {
                    value_types.insert(*dest, "<4 x float>");
                    let mut cur = "poison".to_string();
                    for (i, arg) in args.iter().enumerate() {
                        let arg_ty = value_types.get(arg).copied().unwrap_or("double");
                        let f_val = if arg_ty == "double" {
                            let tmp = format!("%trunc_{}_{}", dest.0, i);
                            out.push_str(&format!(
                                "  {} = fptrunc double %v{} to float\n",
                                tmp, arg.0
                            ));
                            tmp
                        } else if arg_ty == "i64" {
                            let tmp = format!("%sitofp_{}_{}", dest.0, i);
                            out.push_str(&format!("  {} = sitofp i64 %v{} to float\n", tmp, arg.0));
                            tmp
                        } else {
                            format!("%v{}", arg.0)
                        };
                        let next = format!("%v{}_ins_{}", dest.0, i);
                        out.push_str(&format!(
                            "  {} = insertelement <4 x float> {}, float {}, i32 {}\n",
                            next, cur, f_val, i
                        ));
                        cur = next;
                    }
                    out.push_str(&format!(
                        "  %v{} = bitcast <4 x float> {} to <4 x float>\n",
                        dest.0, cur
                    ));
                    return;
                }

                // First-Class SIMD: int4 packs four i32 lanes into <4 x i32>
                if (func == "int4" || func == "datara_rt_int4") && args.len() == 4 {
                    value_types.insert(*dest, "<4 x i32>");
                    let mut cur = "poison".to_string();
                    for (i, arg) in args.iter().enumerate() {
                        let arg_ty = value_types.get(arg).copied().unwrap_or("i64");
                        let i_val = if arg_ty == "i64" {
                            let tmp = format!("%trunci_{}_{}", dest.0, i);
                            out.push_str(&format!("  {} = trunc i64 %v{} to i32\n", tmp, arg.0));
                            tmp
                        } else if arg_ty == "double" {
                            let tmp = format!("%fptosi_{}_{}", dest.0, i);
                            out.push_str(&format!(
                                "  {} = fptosi double %v{} to i32\n",
                                tmp, arg.0
                            ));
                            tmp
                        } else {
                            format!("%v{}", arg.0)
                        };
                        let next = if i == 3 {
                            format!("%v{}", dest.0)
                        } else {
                            format!("%v{}_ins_{}", dest.0, i)
                        };
                        out.push_str(&format!(
                            "  {} = insertelement <4 x i32> {}, i32 {}, i32 {}\n",
                            next, cur, i_val, i
                        ));
                        cur = next;
                    }
                    return;
                }

                // min4 / max4: lane-wise float min/max via LLVM vector
                // intrinsics (declared above with the runtime decls).
                if (func == "min4" || func == "max4")
                    && args.len() == 2
                    && value_types.get(&args[0]).copied() == Some("<4 x float>")
                    && value_types.get(&args[1]).copied() == Some("<4 x float>")
                {
                    value_types.insert(*dest, "<4 x float>");
                    let op = if func == "min4" {
                        "llvm.minnum.v4f32"
                    } else {
                        "llvm.maxnum.v4f32"
                    };
                    out.push_str(&format!(
                        "  %v{} = call <4 x float> @{}(<4 x float> %v{}, <4 x float> %v{})\n",
                        dest.0, op, args[0].0, args[1].0
                    ));
                    return;
                }

                if (func == "dot" || func == "datara_rt_float4_dot") && args.len() == 2 {
                    // Integer vector dot: widen lanes to i64, multiply, sum.
                    if value_types.get(&args[0]).copied() == Some("<4 x i32>")
                        && value_types.get(&args[1]).copied() == Some("<4 x i32>")
                    {
                        value_types.insert(*dest, "double");
                        let mut acc = String::from("0");
                        for lane in 0..4 {
                            let a_e = format!("%dot_a{}_{}", dest.0, lane);
                            let b_e = format!("%dot_b{}_{}", dest.0, lane);
                            let a_w = format!("%dot_aw{}_{}", dest.0, lane);
                            let b_w = format!("%dot_bw{}_{}", dest.0, lane);
                            let m = format!("%dot_m{}_{}", dest.0, lane);
                            let s = format!("%dot_s{}_{}", dest.0, lane);
                            out.push_str(&format!(
                                "  {} = extractelement <4 x i32> %v{}, i32 {}\n",
                                a_e, args[0].0, lane
                            ));
                            out.push_str(&format!(
                                "  {} = extractelement <4 x i32> %v{}, i32 {}\n",
                                b_e, args[1].0, lane
                            ));
                            out.push_str(&format!("  {} = sext i32 {} to i64\n", a_w, a_e));
                            out.push_str(&format!("  {} = sext i32 {} to i64\n", b_w, b_e));
                            out.push_str(&format!("  {} = mul i64 {}, {}\n", m, a_w, b_w));
                            out.push_str(&format!("  {} = add i64 {}, {}\n", s, acc, m));
                            acc = s;
                        }
                        out.push_str(&format!("  %v{} = sitofp i64 {} to double\n", dest.0, acc));
                        return;
                    }
                    // Float vector dot: only when both operands are tracked
                    // as <4 x float>; otherwise fall through to the generic
                    // call path so the IR error names the real cause.
                    if value_types.get(&args[0]).copied() == Some("<4 x float>")
                        && value_types.get(&args[1]).copied() == Some("<4 x float>")
                    {
                        value_types.insert(*dest, "double");
                        let mul_vec = format!("%vmul_{}", dest.0);
                        out.push_str(&format!(
                            "  {} = fmul <4 x float> %v{}, %v{}\n",
                            mul_vec, args[0].0, args[1].0
                        ));
                        let red_val = format!("%vred_{}", dest.0);
                        out.push_str(&format!(
                            "  {} = call fast float @llvm.vector.reduce.fadd.v4f32(float -0.0, <4 x float> {})\n",
                            red_val, mul_vec
                        ));
                        out.push_str(&format!(
                            "  %v{} = fpext float {} to double\n",
                            dest.0, red_val
                        ));
                        return;
                    }
                }

                // Map standard runtime names to datara_rt equivalents if needed
                let actual_func = match func.as_str() {
                    "math_sqrt" => "datara_rt_math_sqrt",
                    "math_pow" => "datara_rt_math_pow",
                    "math_abs" => "datara_rt_math_abs",
                    "math_sin" => "datara_rt_math_sin",
                    "math_cos" => "datara_rt_math_cos",
                    "math_tan" => "datara_rt_math_tan",
                    "math_floor" => "datara_rt_math_floor",
                    "math_ceil" => "datara_rt_math_ceil",
                    "math_round" => "datara_rt_math_round",
                    "math_min" => "datara_rt_math_min",
                    "math_max" => "datara_rt_math_max",
                    "math_clamp" => "datara_rt_math_clamp",
                    "math_hypot" => "datara_rt_math_hypot",
                    "math_log" => "datara_rt_math_log",
                    "math_exp" => "datara_rt_math_exp",
                    "math_min_int" => "datara_rt_math_min_int",
                    "math_max_int" => "datara_rt_math_max_int",
                    "math_clamp_int" => "datara_rt_math_clamp_int",
                    "math_abs_int" => "datara_rt_math_abs_int",
                    "sleep" => "datara_rt_sleep",
                    "now" => "datara_rt_now_ms",
                    "now_ms" => "datara_rt_now_ms",
                    "now_ns" => "datara_rt_now_ns",
                    "now_precise_ms" => "datara_rt_now_precise_ms",
                    "path_join" => "datara_rt_path_join",
                    "file_write" => "datara_rt_file_write",
                    "file_read" => "datara_rt_file_read",
                    "file_append" => "datara_rt_file_append",
                    "file_exists" => "datara_rt_file_exists",
                    "str_len" => "datara_rt_str_len",
                    "byte_len" => "datara_rt_str_len",
                    "str_chars" => "datara_rt_str_chars",
                    "char_len" => "datara_rt_str_chars",
                    "validate_utf8" => "datara_rt_validate_utf8",
                    "str_sanitize_utf8" => "datara_rt_str_sanitize_utf8",
                    "str_scalar_at" => "datara_rt_str_scalar_at",
                    "str_next_offset" => "datara_rt_str_next_offset",
                    "str_char_at" => "datara_rt_str_char_at",
                    "str_byte_at" => "datara_rt_str_byte_at",
                    "byte_at" => "datara_rt_str_byte_at",
                    "str_trim" => "datara_rt_str_trim",
                    "str_to_int" => "datara_rt_str_to_int",
                    "int_to_str" => "datara_rt_int_to_str",
                    "float_to_str" => "datara_rt_float_to_str",
                    "str_contains" => "datara_rt_str_contains",
                    "str_starts_with" => "datara_rt_str_starts_with",
                    "str_ends_with" => "datara_rt_str_ends_with",
                    "str_index_of" => "datara_rt_str_index_of",
                    "own_acquire" => "datara_rt_own_acquire",
                    "own_release" => "datara_rt_own_release",
                    other => other,
                };

                let is_str_concat = actual_func.starts_with("datara_rt_str_concat");
                let mut converted_args = Vec::new();
                // http_get historically had a zero-arg builtin signature;
                // keep `http_get()` calls valid by padding a null URL.
                if actual_func.ends_with("http_get") && args.is_empty() {
                    converted_args.push("ptr null".to_string());
                }
                for (idx, a) in args.iter().enumerate() {
                    let aty = value_types.get(a).copied().unwrap_or("i64");
                    if is_str_concat && aty != "ptr" {
                        let tmp = format!("%sc_arg_{}_{}", dest.0, idx);
                        out.push_str(&format!(
                            "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                            tmp, a.0
                        ));
                        converted_args.push(format!("ptr {}", tmp));
                    } else {
                        converted_args.push(format!("{} %v{}", aty, a.0));
                    }
                }
                let args_str = converted_args.join(", ");

                let is_internal = actual_func != "main"
                    && module.functions.contains_key(actual_func)
                    && !module.extern_functions.contains_key(actual_func)
                    && !address_taken.contains(actual_func);
                let call_prefix = if is_internal { "call fastcc" } else { "call" };

                if ret_ty == "void" {
                    out.push_str(&format!(
                        "  {} void @{}({})\n",
                        call_prefix, actual_func, args_str
                    ));
                } else {
                    out.push_str(&format!(
                        "  %v{} = {} {} @{}({})\n",
                        dest.0, call_prefix, ret_ty, actual_func, args_str
                    ));
                }
                if (actual_func == "datara_rt_list_get"
                    || actual_func == "datara_rt_list_get_unchecked")
                    && args.len() >= 2
                {
                    out.push_str(&format!(
                        "  %fvrp_bce_min_{} = icmp sge i64 %v{}, 0\n",
                        dest.0, args[1].0
                    ));
                    out.push_str(&format!(
                        "  call void @llvm.assume(i1 %fvrp_bce_min_{})\n",
                        dest.0
                    ));
                }
            }
            Inst::MethodCall {
                dest,
                object,
                method,
                args,
                ty,
            } => {
                let mut ret_ty = self.dmir_type_to_llvm(ty);

                let actual_func = match method.as_str() {
                    "len" | "count" | "length" => {
                        ret_ty = "i64";
                        "datara_rt_list_len".to_string()
                    }
                    "byte_len" => {
                        ret_ty = "i64";
                        "datara_rt_str_len".to_string()
                    }
                    "char_len" => {
                        ret_ty = "i64";
                        "datara_rt_str_chars".to_string()
                    }
                    "str_byte_at" | "byte_at" => {
                        ret_ty = "i64";
                        "datara_rt_str_byte_at".to_string()
                    }
                    "char_at" => {
                        ret_ty = "ptr";
                        "datara_rt_str_char_at".to_string()
                    }
                    "append" | "push" => {
                        ret_ty = "ptr";
                        "datara_rt_list_append".to_string()
                    }
                    "get" | "at" => "datara_rt_list_get".to_string(),
                    "set" => "datara_rt_list_set".to_string(),
                    "insert" => "datara_rt_map_insert".to_string(),
                    _ if module.functions.contains_key(method) => method.clone(),
                    _ => {
                        // Collect suffix matches and pick the smallest name
                        // deterministically; HashMap iteration order previously
                        // made the dispatch target vary run to run.
                        let mut candidates: Vec<&String> = module
                            .functions
                            .keys()
                            .filter(|k| {
                                k.len() > method.len() + 1
                                    && k.ends_with(method.as_str())
                                    && k.as_bytes()[k.len() - method.len() - 1] == b'_'
                            })
                            .collect();
                        candidates.sort();
                        candidates
                            .first()
                            .map(|k| (*k).clone())
                            .unwrap_or_else(|| method.clone())
                    }
                };

                value_types.insert(*dest, ret_ty);

                let obj_ty = value_types.get(object).copied().unwrap_or("ptr");
                let obj_arg = if obj_ty == "i64" {
                    let tmp = format!("%mcast_{}_{}", object.0, dest.0);
                    out.push_str(&format!("  {} = inttoptr i64 %v{} to ptr\n", tmp, object.0));
                    format!("ptr {}", tmp)
                } else {
                    format!("ptr %v{}", object.0)
                };

                let mut all_args = vec![obj_arg];
                for a in args {
                    let aty = value_types.get(a).copied().unwrap_or("i64");
                    all_args.push(format!("{} %v{}", aty, a.0));
                }
                let args_str = all_args.join(", ");

                let is_internal = actual_func != "main"
                    && module.functions.contains_key(&actual_func)
                    && !module.extern_functions.contains_key(&actual_func)
                    && !address_taken.contains(&actual_func);
                let call_prefix = if is_internal { "call fastcc" } else { "call" };

                if ret_ty == "void" {
                    out.push_str(&format!(
                        "  {} void @{}({})\n",
                        call_prefix, actual_func, args_str
                    ));
                } else {
                    out.push_str(&format!(
                        "  %v{} = {} {} @{}({})\n",
                        dest.0, call_prefix, ret_ty, actual_func, args_str
                    ));
                }
            }
            Inst::StructInit {
                dest,
                class_name,
                fields,
            } => {
                value_types.insert(*dest, "ptr");
                value_classes.insert(*dest, class_name.clone());
                for (idx, (_, val_id)) in fields.iter().enumerate() {
                    let f_ty = value_types.get(val_id).copied().unwrap_or("i64");
                    let gep_reg = format!("%gep_{}_{}", dest.0, idx);
                    out.push_str(&format!(
                        "  {} = getelementptr inbounds i8, ptr %v{}, i64 {}\n",
                        gep_reg,
                        dest.0,
                        idx.saturating_mul(8)
                    ));
                    out.push_str(&format!(
                        "  store {} %v{}, ptr {}, align 8\n",
                        f_ty, val_id.0, gep_reg
                    ));
                }
            }
            Inst::GetField {
                dest,
                object,
                field,
                ty,
            } => {
                let f_ty = self.dmir_type_to_llvm(ty);
                value_types.insert(*dest, f_ty);

                let offset = self.find_field_offset(
                    module,
                    value_classes.get(object).map(|c| c.as_str()),
                    field,
                );
                let gep_reg = format!("%fgep_{}", dest.0);
                out.push_str(&format!(
                    "  {} = getelementptr inbounds i8, ptr %v{}, i64 {}\n",
                    gep_reg, object.0, offset
                ));
                out.push_str(&format!(
                    "  %v{} = load {}, ptr {}, align 8\n",
                    dest.0, f_ty, gep_reg
                ));
            }
            Inst::SetField {
                object,
                field,
                value,
            } => {
                let f_ty = value_types.get(value).copied().unwrap_or("i64");
                let offset = self.find_field_offset(
                    module,
                    value_classes.get(object).map(|c| c.as_str()),
                    field,
                );
                let gep_reg = format!("%fgep_s_{}_{}", object.0, value.0);
                out.push_str(&format!(
                    "  {} = getelementptr inbounds i8, ptr %v{}, i64 {}\n",
                    gep_reg, object.0, offset
                ));
                out.push_str(&format!(
                    "  store {} %v{}, ptr {}, align 8\n",
                    f_ty, value.0, gep_reg
                ));
            }
            Inst::Out { value } => {
                let val_ty = value_types.get(value).copied().unwrap_or("i64");
                if bool_vids.contains(value) {
                    out.push_str(&format!(
                        "  call void @datara_rt_out_bool(i64 %v{})\n",
                        value.0
                    ));
                } else {
                    match val_ty {
                        "double" => {
                            out.push_str(&format!(
                                "  call void @datara_rt_out_float(double %v{})\n",
                                value.0
                            ));
                        }
                        "ptr" => {
                            out.push_str(&format!(
                                "  call void @datara_rt_out_str(ptr %v{})\n",
                                value.0
                            ));
                        }
                        _ => {
                            out.push_str(&format!(
                                "  call void @datara_rt_out_int(i64 %v{})\n",
                                value.0
                            ));
                        }
                    }
                }
            }
            Inst::Err { value } => {
                out.push_str(&format!("  call void @datara_rt_err(ptr %v{})\n", value.0));
            }
            Inst::FormatStr {
                dest,
                parts,
                values,
            } => {
                value_types.insert(*dest, "ptr");
                let empty_id = strings.get("").copied().unwrap_or(0);
                let mut pieces: Vec<String> = Vec::new();

                for (idx, p) in parts.iter().enumerate() {
                    if !p.is_empty() || (idx == 0 && values.is_empty()) {
                        let pid = strings.get(p.as_str()).copied().unwrap_or(empty_id);
                        let p_ptr = format!("%fmt_p_{}_{}", dest.0, idx);
                        out.push_str(&format!(
                            "  {} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                            p_ptr, pid
                        ));
                        pieces.push(p_ptr);
                    }
                    if idx < values.len() {
                        let val_id = &values[idx];
                        let val_ty = value_types.get(val_id).copied().unwrap_or("i64");
                        let s_val = format!("%fmt_v_{}_{}", dest.0, idx);
                        if val_ty == "ptr" {
                            out.push_str(&format!(
                                "  {} = getelementptr inbounds i8, ptr %v{}, i64 0\n",
                                s_val, val_id.0
                            ));
                        } else if val_ty == "double" {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_float_to_str(double %v{})\n",
                                s_val, val_id.0
                            ));
                        } else if bool_vids.contains(val_id) {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_bool_to_str(i64 %v{})\n",
                                s_val, val_id.0
                            ));
                        } else {
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                                s_val, val_id.0
                            ));
                        }
                        pieces.push(s_val);
                    }
                }

                match pieces.len() {
                    0 => {
                        out.push_str(&format!(
                            "  %v{} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                            dest.0, empty_id
                        ));
                    }
                    1 => {
                        out.push_str(&format!(
                            "  %v{} = getelementptr inbounds i8, ptr {}, i64 0\n",
                            dest.0, pieces[0]
                        ));
                    }
                    2 => {
                        out.push_str(&format!(
                            "  %v{} = call ptr @datara_rt_str_concat(ptr {}, ptr {})\n",
                            dest.0, pieces[0], pieces[1]
                        ));
                    }
                    3 => {
                        out.push_str(&format!(
                            "  %v{} = call ptr @datara_rt_str_concat_3(ptr {}, ptr {}, ptr {})\n",
                            dest.0, pieces[0], pieces[1], pieces[2]
                        ));
                    }
                    4 => {
                        out.push_str(&format!(
                            "  %v{} = call ptr @datara_rt_str_concat_4(ptr {}, ptr {}, ptr {}, ptr {})\n",
                            dest.0, pieces[0], pieces[1], pieces[2], pieces[3]
                        ));
                    }
                    5 => {
                        out.push_str(&format!(
                            "  %v{} = call ptr @datara_rt_str_concat_5(ptr {}, ptr {}, ptr {}, ptr {}, ptr {})\n",
                            dest.0, pieces[0], pieces[1], pieces[2], pieces[3], pieces[4]
                        ));
                    }
                    _ => {
                        let mut curr = pieces[0].clone();
                        for (i, piece) in pieces[1..].iter().enumerate() {
                            let target = if i + 2 == pieces.len() {
                                format!("%v{}", dest.0)
                            } else {
                                format!("%fmt_cn_{}_{}", dest.0, i)
                            };
                            out.push_str(&format!(
                                "  {} = call ptr @datara_rt_str_concat(ptr {}, ptr {})\n",
                                target, curr, piece
                            ));
                            curr = target;
                        }
                    }
                }
            }
            Inst::GetFuncAddr { dest, func_name } => {
                value_types.insert(*dest, "i64");
                let ptr_temp = format!("%fptr_{}", dest.0);
                out.push_str(&format!(
                    "  {} = bitcast ptr @{} to ptr\n",
                    ptr_temp, func_name
                ));
                out.push_str(&format!(
                    "  %v{} = ptrtoint ptr {} to i64\n",
                    dest.0, ptr_temp
                ));
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ty,
            } => {
                let t_ty = value_types.get(then_val).copied();
                let e_ty = value_types.get(else_val).copied();
                let known_ty = match (t_ty, e_ty) {
                    (Some(a), Some(b)) if a == b => Some(a),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    _ => None,
                };
                let vty = match known_ty {
                    // Prefer the actual tracked operand types: the optimizer's
                    // if-conversion can mislabel Float selects as "Int".
                    Some(v) => v,
                    None => {
                        if ty == "Float" {
                            "double"
                        } else if ty == "String" || ty == "Str" {
                            "ptr"
                        } else {
                            "i64"
                        }
                    }
                };
                value_types.insert(*dest, vty);
                let cmp_temp = format!("%sel_c_{}", dest.0);
                out.push_str(&format!("  {} = icmp ne i64 %v{}, 0\n", cmp_temp, cond.0));
                out.push_str(&format!(
                    "  %v{} = select i1 {}, {} %v{}, {} %v{}\n",
                    dest.0, cmp_temp, vty, then_val.0, vty, else_val.0
                ));
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ty,
            } => {
                let vty = if ty == "Float" {
                    "double"
                } else if ty == "String" || ty == "Str" {
                    "ptr"
                } else {
                    "i64"
                };
                value_types.insert(*dest, vty);
                // No else arm: fall back to a well-defined constant instead
                // of referencing an undefined %v0.
                let mut curr_val_str = match else_val {
                    Some(v) => format!("%v{}", v.0),
                    None => match vty {
                        "double" => String::from("0.0"),
                        "ptr" => String::from("null"),
                        _ => String::from("0"),
                    },
                };
                for (idx, (cond, val)) in arms.iter().enumerate().rev() {
                    let cmp_temp = format!("%dec_c_{}_{}", dest.0, idx);
                    let sel_temp = if idx == 0 {
                        format!("%v{}", dest.0)
                    } else {
                        format!("%dec_s_{}_{}", dest.0, idx)
                    };
                    out.push_str(&format!("  {} = icmp ne i64 %v{}, 0\n", cmp_temp, cond.0));
                    out.push_str(&format!(
                        "  {} = select i1 {}, {} %v{}, {} {}\n",
                        sel_temp, cmp_temp, vty, val.0, vty, curr_val_str
                    ));
                    curr_val_str = sel_temp;
                }
            }
            Inst::InlineAsm {
                template,
                outputs,
                inputs,
                clobbers,
                options,
            } => {
                let is_pure = options.iter().any(|o| o == "pure");
                let sideeffect_kw = if is_pure { "" } else { "sideeffect " };

                let mut constraint_parts = Vec::new();
                for (constraint, _) in outputs {
                    if constraint.is_empty() {
                        constraint_parts.push("=r".to_string());
                    } else if constraint.starts_with('=') {
                        constraint_parts.push(constraint.clone());
                    } else {
                        constraint_parts.push(format!("={}", constraint));
                    }
                }
                for (constraint, _) in inputs {
                    if constraint.is_empty() {
                        constraint_parts.push("r".to_string());
                    } else {
                        constraint_parts.push(constraint.clone());
                    }
                }
                for clobber in clobbers {
                    let clob = clobber.trim();
                    if !clob.is_empty() {
                        if clob.starts_with('~') {
                            constraint_parts.push(clob.to_string());
                        } else if clob.starts_with('{') {
                            constraint_parts.push(format!("~{}", clob));
                        } else {
                            constraint_parts.push(format!("~{{{}}}", clob));
                        }
                    }
                }
                if clobbers.is_empty() {
                    constraint_parts.push("~{dirflag}".to_string());
                    constraint_parts.push("~{fpsr}".to_string());
                    constraint_parts.push("~{flags}".to_string());
                }
                let constraints = constraint_parts.join(",");

                let arg_strs: Vec<String> = inputs
                    .iter()
                    .map(|(_, arg)| {
                        let ty = value_types.get(arg).copied().unwrap_or("i64");
                        format!("{} %v{}", ty, arg.0)
                    })
                    .collect();
                let args_joined = arg_strs.join(", ");
                let escaped_template = template.replace('\\', "\\\\").replace('"', "\\\"");

                if outputs.is_empty() {
                    out.push_str(&format!(
                        "  call void asm {}\"{}\", \"{}\"({})\n",
                        sideeffect_kw, escaped_template, constraints, args_joined
                    ));
                } else if outputs.len() == 1 {
                    let dest = outputs[0].1;
                    value_types.insert(dest, "i64");
                    out.push_str(&format!(
                        "  %v{} = call i64 asm {}\"{}\", \"{}\"({})\n",
                        dest.0, sideeffect_kw, escaped_template, constraints, args_joined
                    ));
                } else {
                    let ret_types = vec!["i64"; outputs.len()].join(", ");
                    let struct_ty = format!("{{ {} }}", ret_types);
                    let tmp = format!("%asm_out_{}", outputs[0].1.0);
                    out.push_str(&format!(
                        "  {} = call {} asm {}\"{}\", \"{}\"({})\n",
                        tmp, struct_ty, sideeffect_kw, escaped_template, constraints, args_joined
                    ));
                    for (idx, (_, dest)) in outputs.iter().enumerate() {
                        value_types.insert(*dest, "i64");
                        out.push_str(&format!(
                            "  %v{} = extractvalue {} {}, {}\n",
                            dest.0, struct_ty, tmp, idx
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}
