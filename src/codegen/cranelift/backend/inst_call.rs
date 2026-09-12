use crate::dmir::ValueId;
use cranelift_codegen::ir::{
    BlockArg, InstBuilder, StackSlotData, StackSlotKind, types as clif_types,
};
use cranelift_module::Module as ClifModule;

use super::types::FunctionCompileCtx;

pub fn compile_call<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    dest: &ValueId,
    func: &str,
    args: &[ValueId],
    ty: &str,
) -> Result<(), String> {
    if func == "destroy" || func == "drop" {
        ctx.val_map
            .insert(*dest, ctx.builder.ins().iconst(clif_types::I64, 0));
        return Ok(());
    }
    // First-Class Hardware SIMD Lowering (float4, int4, dot, min4, max4)
    if (func == "float4"
        || func == "datara_rt_float4"
        || func == "f32x4"
        || func == "datara_rt_f32x4")
        && args.len() == 4
    {
        let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4);
        let slot = ctx.builder.create_sized_stack_slot(slot_data);
        let slot_addr = ctx.builder.ins().stack_addr(clif_types::I64, slot, 0);
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        for (i, a) in args.iter().enumerate() {
            let raw_val = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
            let raw_ty = ctx.builder.func.dfg.value_type(raw_val);
            let f32_val = if raw_ty == clif_types::F64 {
                ctx.builder.ins().fdemote(clif_types::F32, raw_val)
            } else if raw_ty == clif_types::I64 {
                ctx.builder.ins().fcvt_from_sint(clif_types::F32, raw_val)
            } else if raw_ty == clif_types::F32 {
                raw_val
            } else {
                ctx.builder.ins().f32const(0.0)
            };
            ctx.builder
                .ins()
                .store(flags, f32_val, slot_addr, (i * 4) as i32);
        }
        ctx.val_map.insert(*dest, slot_addr);
        return Ok(());
    }

    if (func == "int4" || func == "datara_rt_int4" || func == "i32x4" || func == "datara_rt_i32x4")
        && args.len() == 4
    {
        let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4);
        let slot = ctx.builder.create_sized_stack_slot(slot_data);
        let slot_addr = ctx.builder.ins().stack_addr(clif_types::I64, slot, 0);
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        for (i, a) in args.iter().enumerate() {
            let raw_val = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
            let raw_ty = ctx.builder.func.dfg.value_type(raw_val);
            let i32_val = if raw_ty == clif_types::I64 {
                ctx.builder.ins().ireduce(clif_types::I32, raw_val)
            } else if raw_ty == clif_types::F64 {
                ctx.builder.ins().fcvt_to_sint(clif_types::I32, raw_val)
            } else if raw_ty == clif_types::I32 {
                raw_val
            } else {
                ctx.builder.ins().iconst(clif_types::I32, 0)
            };
            ctx.builder
                .ins()
                .store(flags, i32_val, slot_addr, (i * 4) as i32);
        }
        ctx.val_map.insert(*dest, slot_addr);
        return Ok(());
    }

    if (func == "min4" || func == "max4" || func == "f32x4_min" || func == "f32x4_max")
        && args.len() == 2
    {
        let v1 = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2 = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));

        let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4);
        let slot = ctx.builder.create_sized_stack_slot(slot_data);
        let slot_addr = ctx.builder.ins().stack_addr(clif_types::I64, slot, 0);
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        for i in 0..4i32 {
            let offset = i * 4;
            let l1 = ctx.builder.ins().load(clif_types::F32, flags, v1, offset);
            let l2 = ctx.builder.ins().load(clif_types::F32, flags, v2, offset);
            let r = if func.contains("min") {
                ctx.builder.ins().fmin(l1, l2)
            } else {
                ctx.builder.ins().fmax(l1, l2)
            };
            ctx.builder.ins().store(flags, r, slot_addr, offset);
        }
        ctx.val_map.insert(*dest, slot_addr);
        return Ok(());
    }

    if (func == "f32x4_add"
        || func == "f32x4_sub"
        || func == "f32x4_mul"
        || func == "f32x4_div"
        || func == "vec4_add"
        || func == "add4")
        && args.len() == 2
    {
        let v1 = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2 = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4);
        let slot = ctx.builder.create_sized_stack_slot(slot_data);
        let slot_addr = ctx.builder.ins().stack_addr(clif_types::I64, slot, 0);
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        for i in 0..4i32 {
            let offset = i * 4;
            let l1 = ctx.builder.ins().load(clif_types::F32, flags, v1, offset);
            let l2 = ctx.builder.ins().load(clif_types::F32, flags, v2, offset);
            let r = match func {
                "f32x4_sub" => ctx.builder.ins().fsub(l1, l2),
                "f32x4_mul" => ctx.builder.ins().fmul(l1, l2),
                "f32x4_div" => ctx.builder.ins().fdiv(l1, l2),
                _ => ctx.builder.ins().fadd(l1, l2),
            };
            ctx.builder.ins().store(flags, r, slot_addr, offset);
        }
        ctx.val_map.insert(*dest, slot_addr);
        return Ok(());
    }

    if (func == "f32x4_horizontal_add" || func == "horizontal_add") && args.len() == 1 {
        let v = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        let l0 = ctx.builder.ins().load(clif_types::F32, flags, v, 0);
        let l1 = ctx.builder.ins().load(clif_types::F32, flags, v, 4);
        let l2 = ctx.builder.ins().load(clif_types::F32, flags, v, 8);
        let l3 = ctx.builder.ins().load(clif_types::F32, flags, v, 12);
        let s0 = ctx.builder.ins().fadd(l0, l1);
        let s1 = ctx.builder.ins().fadd(l2, l3);
        let sum = ctx.builder.ins().fadd(s0, s1);
        let res_f64 = ctx.builder.ins().fpromote(clif_types::F64, sum);
        ctx.val_map.insert(*dest, res_f64);
        return Ok(());
    }

    if (func == "dot" || func == "datara_rt_float4_dot" || func == "f32x4_dot") && args.len() == 2 {
        let v1 = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2 = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        let l1_0 = ctx.builder.ins().load(clif_types::F32, flags, v1, 0);
        let l2_0 = ctx.builder.ins().load(clif_types::F32, flags, v2, 0);
        let p0 = ctx.builder.ins().fmul(l1_0, l2_0);

        let l1_1 = ctx.builder.ins().load(clif_types::F32, flags, v1, 4);
        let l2_1 = ctx.builder.ins().load(clif_types::F32, flags, v2, 4);
        let p1 = ctx.builder.ins().fmul(l1_1, l2_1);

        let l1_2 = ctx.builder.ins().load(clif_types::F32, flags, v1, 8);
        let l2_2 = ctx.builder.ins().load(clif_types::F32, flags, v2, 8);
        let p2 = ctx.builder.ins().fmul(l1_2, l2_2);

        let l1_3 = ctx.builder.ins().load(clif_types::F32, flags, v1, 12);
        let l2_3 = ctx.builder.ins().load(clif_types::F32, flags, v2, 12);
        let p3 = ctx.builder.ins().fmul(l1_3, l2_3);

        let s0 = ctx.builder.ins().fadd(p0, p1);
        let s1 = ctx.builder.ins().fadd(p2, p3);
        let sum = ctx.builder.ins().fadd(s0, s1);
        let res_f64 = ctx.builder.ins().fpromote(clif_types::F64, sum);
        ctx.val_map.insert(*dest, res_f64);
        return Ok(());
    }

    if (func == "fma" || func == "datara_rt_fma") && args.len() == 3 {
        let mut f_operands = Vec::with_capacity(3);
        for arg in &args[0..3] {
            let val = ctx
                .val_map
                .get(arg)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
            let ty = ctx.builder.func.dfg.value_type(val);
            let fval = if ty == clif_types::I64
                || ty == clif_types::I32
                || ty == clif_types::I16
                || ty == clif_types::I8
            {
                ctx.builder.ins().fcvt_from_sint(clif_types::F64, val)
            } else if ty == clif_types::F32 {
                ctx.builder.ins().fpromote(clif_types::F64, val)
            } else {
                val
            };
            f_operands.push(fval);
        }
        let prod = ctx.builder.ins().fmul(f_operands[0], f_operands[1]);
        let res = ctx.builder.ins().fadd(prod, f_operands[2]);
        ctx.val_map.insert(*dest, res);
        return Ok(());
    }

    // List literals: the lowering emits
    // datara_rt_list_create_N for the exact literal
    // length, so a fixed set of runtime symbols can
    // never cover every arity. Build the
    // DataraListHeader + [count, e0..eN-1] block
    // inline for any N, mirroring the C runtime
    // layout (append/free read the header).
    if let Some(rest) = func.strip_prefix("datara_rt_list_create_")
        && let Ok(n) = rest.parse::<usize>()
    {
        const LIST_HEADER_SIZE: i64 = 16; // { capacity: i64, magic: i64 }
        const LIST_MAGIC: i64 = 0x4441544C49535430; // DATARA_LIST_MAGIC
        let malloc_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.malloc_id, ctx.builder.func);
        let size_val = ctx.builder.ins().iconst(
            clif_types::I64,
            LIST_HEADER_SIZE.saturating_add((n.saturating_add(1) as i64).saturating_mul(8)),
        );
        let call_inst = ctx.builder.ins().call(malloc_ref, &[size_val]);
        let hdr_addr = ctx.builder.inst_results(call_inst)[0];
        let slot_addr = ctx.builder.ins().iadd_imm_s(hdr_addr, LIST_HEADER_SIZE);
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        let cap_val = ctx.builder.ins().iconst(clif_types::I64, n as i64);
        let magic_val = ctx.builder.ins().iconst(clif_types::I64, LIST_MAGIC);
        ctx.builder.ins().store(flags, cap_val, hdr_addr, 0);
        ctx.builder.ins().store(flags, magic_val, hdr_addr, 8);
        let count_val = ctx.builder.ins().iconst(clif_types::I64, n as i64);
        ctx.builder.ins().store(flags, count_val, slot_addr, 0);
        for (i, a) in args.iter().enumerate() {
            let elem = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
            ctx.builder
                .ins()
                .store(flags, elem, slot_addr, ((i + 1) * 8) as i32);
        }
        ctx.val_map.insert(*dest, slot_addr);
        ctx.list_vids.insert(*dest);
        return Ok(());
    }
    if let Some(rest) = func.strip_prefix("datara_rt_tuple_create_")
        && let Ok(n) = rest.parse::<usize>()
    {
        let malloc_ref = ctx
            .module
            .declare_func_in_func(ctx.runtime.malloc_id, ctx.builder.func);
        let size_val = ctx.builder.ins().iconst(
            clif_types::I64,
            (n.saturating_add(1) as i64).saturating_mul(8),
        );
        let call_inst = ctx.builder.ins().call(malloc_ref, &[size_val]);
        let slot_addr = ctx.builder.inst_results(call_inst)[0];
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        let count_val = ctx.builder.ins().iconst(clif_types::I64, n as i64);
        ctx.builder.ins().store(flags, count_val, slot_addr, 0);
        for (i, a) in args.iter().enumerate() {
            let elem = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
            ctx.builder
                .ins()
                .store(flags, elem, slot_addr, ((i + 1) * 8) as i32);
        }
        ctx.val_map.insert(*dest, slot_addr);
        return Ok(());
    }
    // Direct Hardware Math Intrinsics (sqrt, abs)
    if (func == "math_sqrt" || func == "sqrt" || func == "datara_rt_math_sqrt") && args.len() == 1 {
        let arg = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let arg_ty = ctx.builder.func.dfg.value_type(arg);
        let f64_val = if arg_ty == clif_types::F64 {
            arg
        } else if arg_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, arg)
        } else if arg_ty == clif_types::F32 {
            ctx.builder.ins().fpromote(clif_types::F64, arg)
        } else {
            arg
        };
        let res = ctx.builder.ins().sqrt(f64_val);
        ctx.val_map.insert(*dest, res);
        return Ok(());
    }

    if (func == "math_abs" || func == "abs" || func == "datara_rt_math_abs") && args.len() == 1 {
        let arg = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let arg_ty = ctx.builder.func.dfg.value_type(arg);
        if arg_ty == clif_types::F64 {
            let res = ctx.builder.ins().fabs(arg);
            ctx.val_map.insert(*dest, res);
            return Ok(());
        }
    }

    if (func == "math_floor" || func == "floor" || func == "datara_rt_math_floor")
        && args.len() == 1
    {
        let arg = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let arg_ty = ctx.builder.func.dfg.value_type(arg);
        let f64_val = if arg_ty == clif_types::F64 {
            arg
        } else if arg_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, arg)
        } else if arg_ty == clif_types::F32 {
            ctx.builder.ins().fpromote(clif_types::F64, arg)
        } else {
            arg
        };
        let res = ctx.builder.ins().floor(f64_val);
        ctx.val_map.insert(*dest, res);
        return Ok(());
    }

    if (func == "math_ceil" || func == "ceil" || func == "datara_rt_math_ceil") && args.len() == 1 {
        let arg = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let arg_ty = ctx.builder.func.dfg.value_type(arg);
        let f64_val = if arg_ty == clif_types::F64 {
            arg
        } else if arg_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, arg)
        } else if arg_ty == clif_types::F32 {
            ctx.builder.ins().fpromote(clif_types::F64, arg)
        } else {
            arg
        };
        let res = ctx.builder.ins().ceil(f64_val);
        ctx.val_map.insert(*dest, res);
        return Ok(());
    }

    if (func == "math_round" || func == "round" || func == "datara_rt_math_round")
        && args.len() == 1
    {
        let arg = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let arg_ty = ctx.builder.func.dfg.value_type(arg);
        let f64_val = if arg_ty == clif_types::F64 {
            arg
        } else if arg_ty == clif_types::I64 {
            ctx.builder.ins().fcvt_from_sint(clif_types::F64, arg)
        } else if arg_ty == clif_types::F32 {
            ctx.builder.ins().fpromote(clif_types::F64, arg)
        } else {
            arg
        };
        let res = ctx.builder.ins().nearest(f64_val);
        ctx.val_map.insert(*dest, res);
        return Ok(());
    }

    if (func == "math_min" || func == "min" || func == "datara_rt_math_min") && args.len() == 2 {
        let a = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let b = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let a_ty = ctx.builder.func.dfg.value_type(a);
        let b_ty = ctx.builder.func.dfg.value_type(b);
        if a_ty == clif_types::F64 && b_ty == clif_types::F64 {
            let res = ctx.builder.ins().fmin(a, b);
            ctx.val_map.insert(*dest, res);
            return Ok(());
        }
    }

    if (func == "math_max" || func == "max" || func == "datara_rt_math_max") && args.len() == 2 {
        let a = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let b = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let a_ty = ctx.builder.func.dfg.value_type(a);
        let b_ty = ctx.builder.func.dfg.value_type(b);
        if a_ty == clif_types::F64 && b_ty == clif_types::F64 {
            let res = ctx.builder.ins().fmax(a, b);
            ctx.val_map.insert(*dest, res);
            return Ok(());
        }
    }

    // Direct unchecked list get/set: only DMIR sites proven safe
    // by static analysis (induction variable within bounds or
    // bounds-check-elimination pass, which must prove the trip
    // count) may bypass the runtime bounds check. The checked
    // variants always go through the real runtime call below.
    if (func == "datara_rt_list_get_unchecked" || func == "datara_rt_list_get_f64_unchecked")
        && args.len() == 2
    {
        let list_ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_val = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_scaled = ctx.builder.ins().ishl_imm_u(idx_val, 3);
        let offset = ctx.builder.ins().iadd_imm_s(idx_scaled, 8);
        let addr = ctx.builder.ins().iadd(list_ptr, offset);
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        let is_float = ty == "Float" || ty == "f64" || func == "datara_rt_list_get_f64_unchecked";
        let elem = if is_float {
            ctx.builder.ins().load(clif_types::F64, flags, addr, 0)
        } else {
            ctx.builder.ins().load(clif_types::I64, flags, addr, 0)
        };
        ctx.val_map.insert(*dest, elem);
        if ty == "String" || ty == "Str" {
            ctx.string_vids.insert(*dest);
        } else if ty == "Bool" {
            ctx.bool_vids.insert(*dest);
        } else if ty.starts_with("List") || ty.starts_with('[') {
            ctx.list_vids.insert(*dest);
        } else if ty.starts_with("Map") {
            ctx.map_vids.insert(*dest);
        }
        return Ok(());
    }
    if (func == "datara_rt_list_set_unchecked" || func == "datara_rt_list_set_f64_unchecked")
        && args.len() == 3
    {
        let list_ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_val = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let val = ctx
            .val_map
            .get(&args[2])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_scaled = ctx.builder.ins().ishl_imm_u(idx_val, 3);
        let offset = ctx.builder.ins().iadd_imm_s(idx_scaled, 8);
        let addr = ctx.builder.ins().iadd(list_ptr, offset);
        let flags = cranelift_codegen::ir::MachMemFlags::new();
        let val_ty = ctx.builder.func.dfg.value_type(val);
        if val_ty == clif_types::F64 {
            ctx.builder.ins().store(flags, val, addr, 0);
        } else {
            ctx.builder.ins().store(flags, val, addr, 0);
        }
        ctx.val_map.insert(*dest, list_ptr);
        return Ok(());
    }
    if (func == "datara_rt_list_get" || func == "datara_rt_list_get_f64") && args.len() == 2 {
        let list_ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_val = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        let check_block = ctx.builder.create_block();
        let load_block = ctx.builder.create_block();
        let else_block = ctx.builder.create_block();
        let merge_block = ctx.builder.create_block();

        let is_float = ty == "Float" || ty == "f64" || func == "datara_rt_list_get_f64";
        let res_ty = if is_float {
            clif_types::F64
        } else {
            clif_types::I64
        };
        ctx.builder.append_block_param(merge_block, res_ty);

        let is_non_null = ctx.builder.ins().icmp_imm_u(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            list_ptr,
            0,
        );
        ctx.builder
            .ins()
            .brif(is_non_null, check_block, &[], else_block, &[]);

        ctx.builder.switch_to_block(check_block);
        ctx.builder.seal_block(check_block);
        let list_len = ctx.builder.ins().load(clif_types::I64, flags, list_ptr, 0);
        let in_bounds = ctx.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan,
            idx_val,
            list_len,
        );
        ctx.builder
            .ins()
            .brif(in_bounds, load_block, &[], else_block, &[]);

        ctx.builder.switch_to_block(load_block);
        ctx.builder.seal_block(load_block);
        let idx_scaled = ctx.builder.ins().ishl_imm_u(idx_val, 3);
        let offset = ctx.builder.ins().iadd_imm_s(idx_scaled, 8);
        let addr = ctx.builder.ins().iadd(list_ptr, offset);
        let elem = ctx.builder.ins().load(res_ty, flags, addr, 0);
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(elem)]);

        ctx.builder.switch_to_block(else_block);
        ctx.builder.seal_block(else_block);
        let zero = if is_float {
            ctx.builder.ins().f64const(0.0)
        } else {
            ctx.builder.ins().iconst(clif_types::I64, 0)
        };
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(zero)]);

        ctx.builder.switch_to_block(merge_block);
        ctx.builder.seal_block(merge_block);
        let res = ctx.builder.block_params(merge_block)[0];
        ctx.val_map.insert(*dest, res);
        if ty == "String" || ty == "Str" {
            ctx.string_vids.insert(*dest);
        } else if ty == "Bool" {
            ctx.bool_vids.insert(*dest);
        } else if ty.starts_with("List") || ty.starts_with('[') {
            ctx.list_vids.insert(*dest);
        } else if ty.starts_with("Map") {
            ctx.map_vids.insert(*dest);
        }
        return Ok(());
    }
    if (func == "datara_rt_list_set" || func == "datara_rt_list_set_f64") && args.len() == 3 {
        let list_ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let idx_val = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let val = ctx
            .val_map
            .get(&args[2])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let flags = cranelift_codegen::ir::MachMemFlags::new();

        let check_block = ctx.builder.create_block();
        let store_block = ctx.builder.create_block();
        let merge_block = ctx.builder.create_block();

        let is_non_null = ctx.builder.ins().icmp_imm_u(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            list_ptr,
            0,
        );
        ctx.builder
            .ins()
            .brif(is_non_null, check_block, &[], merge_block, &[]);

        ctx.builder.switch_to_block(check_block);
        ctx.builder.seal_block(check_block);
        let list_len = ctx.builder.ins().load(clif_types::I64, flags, list_ptr, 0);
        let in_bounds = ctx.builder.ins().icmp(
            cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan,
            idx_val,
            list_len,
        );
        ctx.builder
            .ins()
            .brif(in_bounds, store_block, &[], merge_block, &[]);

        ctx.builder.switch_to_block(store_block);
        ctx.builder.seal_block(store_block);
        let idx_scaled = ctx.builder.ins().ishl_imm_u(idx_val, 3);
        let offset = ctx.builder.ins().iadd_imm_s(idx_scaled, 8);
        let addr = ctx.builder.ins().iadd(list_ptr, offset);
        let val_ty = ctx.builder.func.dfg.value_type(val);
        if val_ty == clif_types::F64 {
            ctx.builder.ins().store(flags, val, addr, 0);
        } else {
            ctx.builder.ins().store(flags, val, addr, 0);
        }
        ctx.builder.ins().jump(merge_block, &[]);

        ctx.builder.switch_to_block(merge_block);
        ctx.builder.seal_block(merge_block);
        ctx.val_map.insert(*dest, list_ptr);
        return Ok(());
    }
    let (callee_id, callee_name) = match ctx.func_ids.get(func) {
        Some(v) => (v.0, func.to_string()),
        None => {
            let matched = if let Some(first_arg) = args.first() {
                if let Some(c) = ctx.val_to_class.get(first_arg) {
                    let specialized = format!("{}_{}", c, func);
                    if let Some(target) = ctx.func_ids.get(&specialized) {
                        Some((target.0, specialized))
                    } else {
                        let base_c = c.split('_').next().unwrap_or(c);
                        let base_spec = format!("{}_{}", base_c, func);
                        ctx.func_ids
                            .get(&base_spec)
                            .map(|target| (target.0, base_spec))
                    }
                } else {
                    None
                }
            } else {
                None
            };
            matched
                .or_else(|| {
                    // Collect all suffix matches and pick the
                    // smallest name deterministically; a bare
                    // HashMap-order `find` made the dispatch
                    // target vary run to run.
                    let mut candidates: Vec<&String> = ctx
                        .func_ids
                        .keys()
                        .filter(|k| k.split_once('_').map(|(_, m)| m == func).unwrap_or(false))
                        .collect();
                    candidates.sort();
                    candidates
                        .first()
                        .map(|k| (ctx.func_ids[*k].0, (*k).clone()))
                })
                .unwrap_or({
                    if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                        eprintln!(
                            "[datara-codegen] UNRESOLVED CALL: {} in {}",
                            func, ctx.current_func.name
                        );
                    }
                    (cranelift_module::FuncId::from_u32(0), String::new())
                })
        }
    };
    if callee_name.is_empty() {
        return Err(format!(
            "Code generation failed: unresolved function call '{}' in function '{}'",
            func, ctx.current_func.name
        ));
    }

    let mut resolved_callee_id = callee_id;
    if func.starts_with("datara_rt_print_")
        && args.len() == 1
        && let Some(&first_arg_id) = args.first()
        && let Some(&av) = ctx.val_map.get(&first_arg_id)
    {
        let v_ty = ctx.builder.func.dfg.value_type(av);
        if v_ty == clif_types::F64 {
            if let Some(target) = ctx.func_ids.get("datara_rt_print_float") {
                resolved_callee_id = target.0;
            }
        } else if ctx.string_vids.contains(&first_arg_id) {
            if let Some(target) = ctx.func_ids.get("datara_rt_print_str") {
                resolved_callee_id = target.0;
            }
        } else if ctx.bool_vids.contains(&first_arg_id) {
            if let Some(target) = ctx.func_ids.get("datara_rt_print_bool") {
                resolved_callee_id = target.0;
            }
        } else if ctx.list_vids.contains(&first_arg_id)
            && let Some(target) = ctx.func_ids.get("datara_rt_print_list")
        {
            resolved_callee_id = target.0;
        }
    }

    let callee_ref = ctx
        .module
        .declare_func_in_func(resolved_callee_id, ctx.builder.func);
    ctx.builder.func.dfg.ext_funcs[callee_ref].colocated = true;
    let is_str_concat = callee_name.starts_with("datara_rt_str_concat");
    let conv_ref = ctx
        .module
        .declare_func_in_func(ctx.runtime.rt_int_to_str_id, ctx.builder.func);
    let bool_to_str_ref = ctx
        .module
        .declare_func_in_func(ctx.runtime.rt_bool_to_str_id, ctx.builder.func);
    let flt_to_str_ref = ctx
        .module
        .declare_func_in_func(ctx.runtime.rt_flt_to_str_id, ctx.builder.func);
    let mut arg_vals = Vec::new();
    for a in args {
        // Preserve arity: a missing value must still
        // occupy its argument slot in the signature.
        let av = ctx
            .val_map
            .get(a)
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let final_av = if is_str_concat && !ctx.string_vids.contains(a) {
            let call = if ctx.bool_vids.contains(a) {
                ctx.builder.ins().call(bool_to_str_ref, &[av])
            } else if ctx.builder.func.dfg.value_type(av) == clif_types::F64 {
                ctx.builder.ins().call(flt_to_str_ref, &[av])
            } else {
                ctx.builder.ins().call(conv_ref, &[av])
            };
            ctx.builder.inst_results(call)[0]
        } else {
            av
        };
        arg_vals.push(final_av);
    }
    // http_get historically had a zero-arg builtin
    // signature; keep `http_get()` calls compilable by
    // padding a null URL argument.
    if callee_name.ends_with("http_get") && arg_vals.is_empty() {
        arg_vals.push(ctx.builder.ins().iconst(clif_types::I64, 0));
    }
    // The runtime ABI is all-I64 (collections store
    // floats as their IEEE bit pattern). An F64 value
    // passed against an I64 param fails Cranelift
    // verification, so bitcast per the declared sig.
    if let Some((_, callee_sig)) = ctx.func_ids.get(&callee_name) {
        for (i, av) in arg_vals.iter_mut().enumerate() {
            if ctx.builder.func.dfg.value_type(*av) == clif_types::F64
                && callee_sig
                    .params
                    .get(i)
                    .map(|p| p.value_type == clif_types::I64)
                    .unwrap_or(false)
            {
                *av = ctx.builder.ins().bitcast(
                    clif_types::I64,
                    cranelift_codegen::ir::MemFlagsData::new(),
                    *av,
                );
            }
        }
    }
    let call_inst = ctx.builder.ins().call(callee_ref, &arg_vals);
    let results = ctx.builder.inst_results(call_inst);
    let ret_ty = ctx
        .dmir_module
        .functions
        .get(&callee_name)
        .or_else(|| ctx.dmir_module.functions.get(func))
        .map(|f| f.return_type.as_str())
        .or_else(|| {
            ctx.dmir_module
                .extern_functions
                .get(&callee_name)
                .or_else(|| ctx.dmir_module.extern_functions.get(func))
                .map(|(_, ret)| ret.as_str())
        })
        .unwrap_or("");
    if let Some(&r) = results.first() {
        let mut r_val = r;
        if (ty == "Float" || ret_ty == "Float")
            && ctx.builder.func.dfg.value_type(r) == clif_types::I64
        {
            r_val = ctx.builder.ins().bitcast(
                clif_types::F64,
                cranelift_codegen::ir::MemFlagsData::new(),
                r,
            );
        }
        ctx.val_map.insert(*dest, r_val);
        let first_arg_is_str_class = args
            .first()
            .and_then(|a| ctx.val_to_class.get(a))
            .map(|c| c.ends_with("String") || c.ends_with("Str"))
            .unwrap_or(false);
        if ty == "String"
            || ty.contains("Str")
            || ret_ty == "String"
            || ret_ty == "Str"
            || (ret_ty == "T" && first_arg_is_str_class)
            || func == "datara_rt_range_str"
            || func == "datara_rt_int_to_str"
            || func == "datara_rt_str_concat"
            || func == "datara_rt_str_char_at"
            || func == "str_char_at"
            || func == "char_at"
            || ctx.string_return_funcs.contains(func)
            || ctx.string_return_funcs.contains(&callee_name)
        {
            ctx.string_vids.insert(*dest);
        }
        if ty == "Bool" || ret_ty == "Bool" {
            ctx.bool_vids.insert(*dest);
        }
        if ty.contains("List")
            || ret_ty.contains("List")
            || ty.starts_with('[')
            || ret_ty.starts_with('[')
            || func.starts_with("datara_rt_list_create")
            || func == "datara_rt_list_append"
        {
            ctx.list_vids.insert(*dest);
        }
        if ty.contains("Map") || ret_ty.contains("Map") || func.starts_with("datara_rt_map_create")
        {
            ctx.map_vids.insert(*dest);
        }
        let class_to_record = if !ret_ty.is_empty() { ret_ty } else { ty };
        let stripped = class_to_record.split('<').next().unwrap_or(class_to_record);
        if !stripped.is_empty()
            && stripped != "Int"
            && stripped != "Float"
            && stripped != "Bool"
            && stripped != "Str"
            && stripped != "String"
            && stripped != "List"
            && stripped != "Map"
            && stripped != "Unit"
            && !stripped.starts_with('[')
        {
            ctx.val_to_class.insert(*dest, class_to_record.to_string());
        }
    }

    Ok(())
}

pub fn compile_method_call<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    dest: &ValueId,
    object: &ValueId,
    method: &str,
    args: &[ValueId],
    ty: &str,
) -> Result<(), String> {
    let mut all_args = vec![
        ctx.val_map
            .get(object)
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0)),
    ];
    for a in args {
        // Preserve arity: a missing value must still
        // occupy its argument slot in the signature.
        let av = ctx
            .val_map
            .get(a)
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        all_args.push(av);
    }
    if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
        eprintln!(
            "[method_call] fn={} method={} obj={:?} class={:?} is_list={}",
            ctx.current_func.name,
            method,
            object,
            ctx.val_to_class.get(object),
            ctx.list_vids.contains(object)
        );
    }
    // List and String protocol methods: dispatch on the object's
    // runtime shape, not the class method table.
    let list_special = if ctx.map_vids.contains(object) {
        // Map protocol: without this branch a `map.get(k)`
        // falls into the name-suffix fallback, which can
        // dispatch to an unrelated runtime function whose
        // signature mismatch then fails Cranelift
        // verification.
        match method {
            "get" | "at" => Some(ctx.runtime.rt_map_get_id),
            "insert" | "set" | "add" | "put" => Some(ctx.runtime.rt_map_insert_id),
            _ => None,
        }
    } else if ctx.list_vids.contains(object)
        || ctx
            .val_to_class
            .get(object)
            .map(|c| c.starts_with("List"))
            .unwrap_or(false)
        || (ctx.val_to_class.get(object).is_none()
            && !ctx.string_vids.contains(object)
            && matches!(
                method,
                "push" | "append" | "pop" | "set" | "get" | "at" | "len" | "length" | "count"
            ))
    {
        match method {
            "length" | "count" | "len" => Some(ctx.runtime.rt_list_len_id),
            "get" | "at" => Some(ctx.runtime.rt_list_get_id),
            "set" => Some(ctx.runtime.rt_list_set_id),
            "push" | "append" | "add" => Some(ctx.runtime.rt_list_append_id),
            "pop" => Some(ctx.runtime.rt_pop_id),
            _ => None,
        }
    } else if ctx.string_vids.contains(object) {
        match method {
            "length" | "count" | "len" | "byte_len" => Some(ctx.runtime.str_len_id),
            "char_len" | "chars" => Some(ctx.runtime.str_chars_id),
            "byte_at" => Some(ctx.runtime.str_byte_at_id),
            "char_at" => Some(ctx.runtime.rt_str_char_at_id),
            _ => None,
        }
    } else {
        None
    };
    if let Some(special_id) = list_special {
        for a in &mut all_args {
            if ctx.builder.func.dfg.value_type(*a) == clif_types::F64 {
                *a = ctx.builder.ins().bitcast(
                    clif_types::I64,
                    cranelift_codegen::ir::MemFlagsData::new(),
                    *a,
                );
            }
        }
        let callee_ref = ctx
            .module
            .declare_func_in_func(special_id, ctx.builder.func);
        let call_inst = ctx.builder.ins().call(callee_ref, &all_args);
        let results = ctx.builder.inst_results(call_inst);
        if let Some(&r) = results.first() {
            let mut r_val = r;
            if ty == "Float" && ctx.builder.func.dfg.value_type(r) == clif_types::I64 {
                r_val = ctx.builder.ins().bitcast(
                    clif_types::F64,
                    cranelift_codegen::ir::MemFlagsData::new(),
                    r,
                );
            }
            ctx.val_map.insert(*dest, r_val);
            // Only set/push/append return the (possibly
            // reallocated) list itself; length/get
            // return plain ints.
            if matches!(method, "set" | "push" | "append" | "add") {
                ctx.list_vids.insert(*dest);
            }
            if ty.contains("List") || ty.starts_with('[') {
                ctx.list_vids.insert(*dest);
            }
            if ty.contains("Map") {
                ctx.map_vids.insert(*dest);
            }
        }
    } else {
        let (callee_id, callee_name) = {
            let class_matched = if let Some(c) = ctx.val_to_class.get(object) {
                let specialized = format!("{}_{}", c, method);
                if let Some(target) = ctx.func_ids.get(&specialized) {
                    Some((target.0, specialized))
                } else {
                    let base_c = c
                        .split('<')
                        .next()
                        .unwrap_or(c)
                        .split('_')
                        .next()
                        .unwrap_or(c);
                    let base_spec = format!("{}_{}", base_c, method);
                    ctx.func_ids
                        .get(&base_spec)
                        .map(|target| (target.0, base_spec))
                }
            } else {
                None
            };

            let dbg_obj_class = ctx.val_to_class.get(object).cloned();
            class_matched
                .or_else(|| {
                    let cands: Vec<String> = ctx
                        .func_ids
                        .keys()
                        .filter(|k| k.split_once('_').map(|(_, m)| m == method).unwrap_or(false))
                        .cloned()
                        .collect();
                    if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                        eprintln!(
                            "[dispatch] fn={} method={:?} obj_class={:?} cands={:?}",
                            ctx.current_func.name, method, dbg_obj_class, cands
                        );
                    }
                    let mut cands_sorted = cands.clone();
                    cands_sorted.sort();
                    cands_sorted
                        .first()
                        .and_then(|k| ctx.func_ids.get(k).map(|v| (v.0, k.clone())))
                })
                .or_else(|| ctx.func_ids.get(method).map(|v| (v.0, method.to_string())))
                .unwrap_or_else(|| {
                    if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                        eprintln!(
                            "[datara-codegen] UNRESOLVED METHOD: {} in {}",
                            method, ctx.current_func.name
                        );
                    }
                    (cranelift_module::FuncId::from_u32(0), String::new())
                })
        };
        if callee_name.is_empty() {
            return Err(format!(
                "Code generation failed: unresolved method call '{}' on object with class '{:?}' in function '{}'",
                method,
                ctx.val_to_class.get(object),
                ctx.current_func.name
            ));
        }
        let callee_ref = ctx.module.declare_func_in_func(callee_id, ctx.builder.func);
        let call_inst = ctx.builder.ins().call(callee_ref, &all_args);
        let results = ctx.builder.inst_results(call_inst);
        if let Some(&r) = results.first() {
            let ret_ty = ctx
                .dmir_module
                .functions
                .get(&callee_name)
                .map(|f| f.return_type.as_str())
                .unwrap_or("");
            let obj_class = ctx.val_to_class.get(object).cloned();
            let is_float = ty == "Float"
                || ret_ty == "Float"
                || (ret_ty == "T"
                    && obj_class
                        .as_ref()
                        .map(|c| {
                            c.contains("<Float>")
                                || c.contains("<f64>")
                                || c.ends_with("_Float")
                                || c.ends_with("_Float64")
                        })
                        .unwrap_or(false));
            let mut r_val = r;
            if is_float && ctx.builder.func.dfg.value_type(r) == clif_types::I64 {
                r_val = ctx.builder.ins().bitcast(
                    clif_types::F64,
                    cranelift_codegen::ir::MemFlagsData::new(),
                    r,
                );
            }
            ctx.val_map.insert(*dest, r_val);

            let obj_is_str_class = obj_class
                .as_ref()
                .map(|c| {
                    c.ends_with("String")
                        || c.ends_with("Str")
                        || c.contains("<Str>")
                        || c.contains("<String>")
                        || c.ends_with("_Str")
                        || c.ends_with("_String")
                })
                .unwrap_or(false);
            if ty == "String"
                || ty.contains("Str")
                || ret_ty == "String"
                || ret_ty == "Str"
                || (ret_ty == "T" && obj_is_str_class)
            {
                ctx.string_vids.insert(*dest);
            }
            if ty == "Bool"
                || ret_ty == "Bool"
                || (ret_ty == "T"
                    && obj_class
                        .as_ref()
                        .map(|c| c.contains("<Bool>") || c.ends_with("_Bool"))
                        .unwrap_or(false))
            {
                ctx.bool_vids.insert(*dest);
            }

            if ty.contains("List")
                || ret_ty.contains("List")
                || ty.starts_with('[')
                || ret_ty.starts_with('[')
            {
                ctx.list_vids.insert(*dest);
            }
            if ty.contains("Map") || ret_ty.contains("Map") {
                ctx.map_vids.insert(*dest);
            }

            let effective_class = if ret_ty == "T" {
                obj_class
                    .as_ref()
                    .and_then(|c| {
                        if let (Some(start), Some(end)) = (c.find('<'), c.rfind('>')) {
                            if start < end {
                                Some(c[start + 1..end].trim().to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            } else if !ret_ty.is_empty() {
                ret_ty.to_string()
            } else {
                ty.to_string()
            };
            if effective_class.starts_with("List") || effective_class.starts_with('[') {
                ctx.list_vids.insert(*dest);
            }
            if effective_class.starts_with("Map") {
                ctx.map_vids.insert(*dest);
            }
            let stripped_class = effective_class
                .split('<')
                .next()
                .unwrap_or(&effective_class);
            if !stripped_class.is_empty()
                && stripped_class != "Int"
                && stripped_class != "Float"
                && stripped_class != "Bool"
                && stripped_class != "Str"
                && stripped_class != "String"
                && stripped_class != "List"
                && stripped_class != "Map"
                && stripped_class != "Unit"
                && !stripped_class.starts_with('[')
            {
                ctx.val_to_class.insert(*dest, effective_class.to_string());
            }
        }
    }

    Ok(())
}
