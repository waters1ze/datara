use crate::dmir::ValueId;
use cranelift_codegen::ir::{InstBuilder, types as clif_types};
use cranelift_module::Module as ClifModule;

use super::types::FunctionCompileCtx;

pub fn compile_binop<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    dest: &ValueId,
    op: &str,
    left: &ValueId,
    right: &ValueId,
    ty: &str,
) -> Result<(), String> {
    let raw_lv = ctx
        .val_map
        .get(left)
        .copied()
        .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
    let raw_rv = ctx
        .val_map
        .get(right)
        .copied()
        .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
    let lv_ty = ctx.builder.func.dfg.value_type(raw_lv);
    let rv_ty = ctx.builder.func.dfg.value_type(raw_rv);
    let left_is_str = ctx.string_vids.contains(left);
    let right_is_str = ctx.string_vids.contains(right);
    let is_string =
        op == "+" && (left_is_str || right_is_str || ty == "String" || ty.contains("Str"));

    if is_string {
        let conv_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.rt_int_to_str_id, ctx.builder.func);
        let bool_to_str_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.rt_bool_to_str_id, ctx.builder.func);
        let flt_to_str_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.rt_flt_to_str_id, ctx.builder.func);
        let l_str = if left_is_str {
            raw_lv
        } else if ctx.bool_vids.contains(left) {
            let call = ctx.builder.ins().call(bool_to_str_ref, &[raw_lv]);
            ctx.builder.inst_results(call)[0]
        } else if lv_ty == clif_types::F64 {
            let call = ctx.builder.ins().call(flt_to_str_ref, &[raw_lv]);
            ctx.builder.inst_results(call)[0]
        } else {
            let call = ctx.builder.ins().call(conv_ref, &[raw_lv]);
            ctx.builder.inst_results(call)[0]
        };
        let r_str = if right_is_str {
            raw_rv
        } else if ctx.bool_vids.contains(right) {
            let call = ctx.builder.ins().call(bool_to_str_ref, &[raw_rv]);
            ctx.builder.inst_results(call)[0]
        } else if rv_ty == clif_types::F64 {
            let call = ctx.builder.ins().call(flt_to_str_ref, &[raw_rv]);
            ctx.builder.inst_results(call)[0]
        } else {
            let call = ctx.builder.ins().call(conv_ref, &[raw_rv]);
            ctx.builder.inst_results(call)[0]
        };
        let fn_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.rt_concat_id, ctx.builder.func);
        let call_inst = ctx.builder.ins().call(fn_ref, &[l_str, r_str]);
        let res = ctx.builder.inst_results(call_inst)[0];
        ctx.val_map.insert(*dest, res);
        ctx.string_vids.insert(*dest);
        return Ok(());
    }

    let is_float = lv_ty == clif_types::F64 || rv_ty == clif_types::F64;

    let (lv, rv) = if is_float {
        let flv = if lv_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, raw_lv)
        } else {
            raw_lv
        };
        let frv = if rv_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, raw_rv)
        } else {
            raw_rv
        };
        (flv, frv)
    } else {
        let ilv = if lv_ty == clif_types::I8 || lv_ty == clif_types::I32 {
            ctx.builder.ins().sextend(clif_types::I64, raw_lv)
        } else {
            raw_lv
        };
        let irv = if rv_ty == clif_types::I8 || rv_ty == clif_types::I32 {
            ctx.builder.ins().sextend(clif_types::I64, raw_rv)
        } else {
            raw_rv
        };
        (ilv, irv)
    };

    let res = if is_float {
        match op {
            "+" => ctx.builder.ins().fadd(lv, rv),
            "-" => ctx.builder.ins().fsub(lv, rv),
            "*" => ctx.builder.ins().fmul(lv, rv),
            "/" => ctx.builder.ins().fdiv(lv, rv),
            "<" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            "<=" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            ">" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::GreaterThan,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            ">=" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            "==" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::Equal,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            "!=" => {
                let c = ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::NotEqual,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            // Used to be a silent `fadd` fallback, which
            // turned any unrecognised operator into
            // addition and produced wrong answers with
            // no diagnostic. Fail loudly instead.
            other => {
                return Err(format!(
                    "No Cranelift lowering for float operator '{}'. \
                                         Refusing to fall back to 'fadd' and silently \
                                         compute the wrong result.",
                    other
                ));
            }
        }
    } else {
        match op {
            "+" => {
                let (res, ovf) = ctx.builder.ins().sadd_overflow(lv, rv);
                ctx.builder
                    .ins()
                    .trapnz(ovf, cranelift_codegen::ir::TrapCode::INTEGER_OVERFLOW);
                res
            }
            "-" => {
                let (res, ovf) = ctx.builder.ins().ssub_overflow(lv, rv);
                ctx.builder
                    .ins()
                    .trapnz(ovf, cranelift_codegen::ir::TrapCode::INTEGER_OVERFLOW);
                res
            }
            "*" => {
                let (res, ovf) = ctx.builder.ins().smul_overflow(lv, rv);
                ctx.builder
                    .ins()
                    .trapnz(ovf, cranelift_codegen::ir::TrapCode::INTEGER_OVERFLOW);
                res
            }
            "wrapping_+" => ctx.builder.ins().iadd(lv, rv),
            "wrapping_-" => ctx.builder.ins().isub(lv, rv),
            "wrapping_*" => {
                let const_mult = ctx
                    .const_int_map
                    .get(right)
                    .copied()
                    .or_else(|| ctx.const_int_map.get(left).copied());
                let var_val = if ctx.const_int_map.contains_key(right) {
                    lv
                } else {
                    rv
                };
                if let Some(c) = const_mult {
                    if c == 0 {
                        ctx.builder.ins().iconst(clif_types::I64, 0)
                    } else if c == 1 {
                        var_val
                    } else if c == 2 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 1);
                        ctx.builder.ins().ishl(var_val, shift)
                    } else if c == 3 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 1);
                        let shifted = ctx.builder.ins().ishl(var_val, shift);
                        ctx.builder.ins().iadd(shifted, var_val)
                    } else if c == 4 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 2);
                        ctx.builder.ins().ishl(var_val, shift)
                    } else if c == 5 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 2);
                        let shifted = ctx.builder.ins().ishl(var_val, shift);
                        ctx.builder.ins().iadd(shifted, var_val)
                    } else if c == 8 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 3);
                        ctx.builder.ins().ishl(var_val, shift)
                    } else if c == 9 {
                        let shift = ctx.builder.ins().iconst(clif_types::I64, 3);
                        let shifted = ctx.builder.ins().ishl(var_val, shift);
                        ctx.builder.ins().iadd(shifted, var_val)
                    } else if c > 0 && (c & (c - 1)) == 0 {
                        let shift = ctx
                            .builder
                            .ins()
                            .iconst(clif_types::I64, c.trailing_zeros() as i64);
                        ctx.builder.ins().ishl(var_val, shift)
                    } else {
                        ctx.builder.ins().imul(lv, rv)
                    }
                } else {
                    ctx.builder.ins().imul(lv, rv)
                }
            }
            "saturating_+" => {
                let (res, ovf) = ctx.builder.ins().sadd_overflow(lv, rv);
                let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
                let is_pos = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
                    lv,
                    zero,
                );
                let max_val = ctx.builder.ins().iconst(clif_types::I64, i64::MAX);
                let min_val = ctx.builder.ins().iconst(clif_types::I64, i64::MIN);
                let sat_val = ctx.builder.ins().select(is_pos, max_val, min_val);
                ctx.builder.ins().select(ovf, sat_val, res)
            }
            "saturating_-" => {
                let (res, ovf) = ctx.builder.ins().ssub_overflow(lv, rv);
                let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
                let is_pos = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
                    lv,
                    zero,
                );
                let max_val = ctx.builder.ins().iconst(clif_types::I64, i64::MAX);
                let min_val = ctx.builder.ins().iconst(clif_types::I64, i64::MIN);
                let sat_val = ctx.builder.ins().select(is_pos, max_val, min_val);
                ctx.builder.ins().select(ovf, sat_val, res)
            }
            "saturating_*" => {
                let (res, ovf) = ctx.builder.ins().smul_overflow(lv, rv);
                let xor_val = ctx.builder.ins().bxor(lv, rv);
                let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
                let is_pos = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
                    xor_val,
                    zero,
                );
                let max_val = ctx.builder.ins().iconst(clif_types::I64, i64::MAX);
                let min_val = ctx.builder.ins().iconst(clif_types::I64, i64::MIN);
                let sat_val = ctx.builder.ins().select(is_pos, max_val, min_val);
                ctx.builder.ins().select(ovf, sat_val, res)
            }
            "/" => {
                let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
                let is_zero = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    rv,
                    zero,
                );
                ctx.builder.ins().trapnz(
                    is_zero,
                    cranelift_codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO,
                );

                let min_val = ctx.builder.ins().iconst(clif_types::I64, i64::MIN);
                let is_min = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    lv,
                    min_val,
                );
                let neg_one = ctx.builder.ins().iconst(clif_types::I64, -1);
                let is_neg_one = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    rv,
                    neg_one,
                );
                let is_ovf = ctx.builder.ins().band(is_min, is_neg_one);
                ctx.builder
                    .ins()
                    .trapnz(is_ovf, cranelift_codegen::ir::TrapCode::INTEGER_OVERFLOW);

                if let Some(&c) = ctx.const_int_map.get(right) {
                    if c > 1 && (c & (c - 1)) == 0 && c <= (1 << 62) {
                        // Signed truncating division by 2^k:
                        let k = c.trailing_zeros();
                        let sign = ctx.builder.ins().sshr_imm_s(lv, 63);
                        let bias = ctx.builder.ins().ushr_imm_u(sign, (64 - k) as i64);
                        let sum = ctx.builder.ins().iadd(lv, bias);
                        ctx.builder.ins().sshr_imm_s(sum, k as i64)
                    } else {
                        ctx.builder.ins().sdiv(lv, rv)
                    }
                } else {
                    ctx.builder.ins().sdiv(lv, rv)
                }
            }
            "%" => {
                let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
                let is_zero = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    rv,
                    zero,
                );
                ctx.builder.ins().trapnz(
                    is_zero,
                    cranelift_codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO,
                );

                let min_val = ctx.builder.ins().iconst(clif_types::I64, i64::MIN);
                let is_min = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    lv,
                    min_val,
                );
                let neg_one = ctx.builder.ins().iconst(clif_types::I64, -1);
                let is_neg_one = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    rv,
                    neg_one,
                );
                let is_ovf = ctx.builder.ins().band(is_min, is_neg_one);
                ctx.builder
                    .ins()
                    .trapnz(is_ovf, cranelift_codegen::ir::TrapCode::INTEGER_OVERFLOW);

                if let Some(&c) = ctx.const_int_map.get(right) {
                    if c > 1 && (c & (c - 1)) == 0 && c <= (1 << 62) {
                        let k = c.trailing_zeros();
                        let sign = ctx.builder.ins().sshr_imm_s(lv, 63);
                        let bias = ctx.builder.ins().ushr_imm_u(sign, (64 - k) as i64);
                        let sum = ctx.builder.ins().iadd(lv, bias);
                        let q = ctx.builder.ins().sshr_imm_s(sum, k as i64);
                        let scaled = ctx.builder.ins().ishl_imm_s(q, k as i64);
                        ctx.builder.ins().isub(lv, scaled)
                    } else {
                        ctx.builder.ins().srem(lv, rv)
                    }
                } else {
                    ctx.builder.ins().srem(lv, rv)
                }
            }
            "<" => {
                let c = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            "<=" => {
                let c = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            ">" => {
                let c = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            ">=" => {
                let c = ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
                    lv,
                    rv,
                );
                ctx.builder.ins().uextend(clif_types::I64, c)
            }
            "==" => {
                if ctx.string_vids.contains(left)
                    || ctx.string_vids.contains(right)
                    || ty == "String"
                    || ty.contains("Str")
                {
                    let eq_ref = ctx
                        .module
                        .declare_func_in_func(ctx.runtime.rt_str_eq_id, ctx.builder.func);
                    let call_inst = ctx.builder.ins().call(eq_ref, &[lv, rv]);
                    ctx.builder.inst_results(call_inst)[0]
                } else {
                    let c = ctx.builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal,
                        lv,
                        rv,
                    );
                    ctx.builder.ins().uextend(clif_types::I64, c)
                }
            }
            "!=" => {
                if ctx.string_vids.contains(left)
                    || ctx.string_vids.contains(right)
                    || ty == "String"
                    || ty.contains("Str")
                {
                    let eq_ref = ctx
                        .module
                        .declare_func_in_func(ctx.runtime.rt_str_eq_id, ctx.builder.func);
                    let call_inst = ctx.builder.ins().call(eq_ref, &[lv, rv]);
                    let res = ctx.builder.inst_results(call_inst)[0];
                    let one = ctx.builder.ins().iconst(clif_types::I64, 1);
                    ctx.builder.ins().bxor(res, one)
                } else {
                    let c = ctx.builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                        lv,
                        rv,
                    );
                    ctx.builder.ins().uextend(clif_types::I64, c)
                }
            }
            "&" | "&&" => ctx.builder.ins().band(lv, rv),
            "|" | "||" => ctx.builder.ins().bor(lv, rv),
            "^" => ctx.builder.ins().bxor(lv, rv),
            "<<" => ctx.builder.ins().ishl(lv, rv),
            ">>" => ctx.builder.ins().sshr(lv, rv),
            // Used to be a silent `iadd` fallback. That is
            // how `a && b` compiled to `a + b`: the operator
            // had no arm here, so it silently became
            // addition. Fail loudly instead.
            other => {
                return Err(format!(
                    "No Cranelift lowering for integer operator '{}'. \
                                         Refusing to fall back to 'iadd' and silently \
                                         compute the wrong result.",
                    other
                ));
            }
        }
    };
    ctx.val_map.insert(*dest, res);
    // Comparisons and logical operators always produce
    // a Bool; the lowering labels them `ty: "Int"`
    // because that is their machine representation.
    if matches!(op, "<" | "<=" | ">" | ">=" | "==" | "!=" | "&&" | "||") || ty == "Bool" {
        ctx.bool_vids.insert(*dest);
    }

    Ok(())
}

pub fn compile_unop<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    dest: &ValueId,
    op: &str,
    operand: &ValueId,
    ty: &str,
) -> Result<(), String> {
    let raw_v = ctx
        .val_map
        .get(operand)
        .copied()
        .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
    let v_ty = ctx.builder.func.dfg.value_type(raw_v);
    let res = if v_ty == clif_types::F64 {
        if op == "-" {
            ctx.builder.ins().fneg(raw_v)
        } else {
            raw_v
        }
    } else {
        let v = raw_v;
        if op == "-" {
            ctx.builder.ins().ineg(v)
        } else if op == "!" {
            if ctx.bool_vids.contains(operand) {
                // Logical NOT: `!true` must be `false`,
                // not bnot(1) = -2 (still truthy).
                let is_zero = ctx.builder.ins().icmp_imm_s(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    v,
                    0,
                );
                ctx.builder.ins().uextend(clif_types::I64, is_zero)
            } else {
                ctx.builder.ins().bnot(v)
            }
        } else {
            v
        }
    };
    ctx.val_map.insert(*dest, res);
    if ctx.string_vids.contains(operand) {
        ctx.string_vids.insert(*dest);
    }
    if op == "!" {
        ctx.bool_vids.insert(*dest);
    }
    // SROA field forwarding emits `copy` for
    // `struct.field` reads. A copied Bool must stay a
    // Bool or `out m.is_some` prints "1" instead of
    // "true" (the UnOp result loses the flag the
    // original GetField carried in its `ty`).
    if (op == "copy" || op == "await") && ty == "Bool" {
        ctx.bool_vids.insert(*dest);
    }
    if let Some(c) = ctx.val_to_class.get(operand) {
        ctx.val_to_class.insert(*dest, c.clone());
    }

    Ok(())
}
