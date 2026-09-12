//! First-Class Native 128-bit Vector SIMD Lowering Engine for Cranelift JIT
//!
//! Replaces scalar stack spills with native vector instructions (addps, subps,
//! mulps, divps, minps, maxps, sqrtps) operating purely in XMM registers.

use crate::dmir::ValueId;
use cranelift_codegen::ir::{InstBuilder, MachMemFlags, Value as ClifValue, types as clif_types};
use cranelift_module::Module as ClifModule;

use super::types::FunctionCompileCtx;

/// Converts a scalar or vector ClifValue to an F32 scalar.
pub fn to_f32<M: ClifModule>(ctx: &mut FunctionCompileCtx<'_, '_, M>, val: ClifValue) -> ClifValue {
    let ty = ctx.builder.func.dfg.value_type(val);
    if ty == clif_types::F32 {
        val
    } else if ty == clif_types::F64 {
        ctx.builder.ins().fdemote(clif_types::F32, val)
    } else if ty == clif_types::I64 || ty == clif_types::I32 {
        ctx.builder.ins().fcvt_from_sint(clif_types::F32, val)
    } else {
        ctx.builder.ins().f32const(0.0)
    }
}

/// Converts a ClifValue to an I32 scalar.
pub fn to_i32<M: ClifModule>(ctx: &mut FunctionCompileCtx<'_, '_, M>, val: ClifValue) -> ClifValue {
    let ty = ctx.builder.func.dfg.value_type(val);
    if ty == clif_types::I32 {
        val
    } else if ty == clif_types::I64 {
        ctx.builder.ins().ireduce(clif_types::I32, val)
    } else if ty == clif_types::F64 || ty == clif_types::F32 {
        ctx.builder.ins().fcvt_to_sint(clif_types::I32, val)
    } else {
        ctx.builder.ins().iconst(clif_types::I32, 0)
    }
}

/// Ensures a value is an F32X4 vector. If it is an I64 pointer, loads from memory.
pub fn ensure_f32x4<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    val: ClifValue,
) -> ClifValue {
    let ty = ctx.builder.func.dfg.value_type(val);
    if ty == clif_types::F32X4 {
        val
    } else if ty == clif_types::I64 {
        let flags = MachMemFlags::new();
        ctx.builder.ins().load(clif_types::F32X4, flags, val, 0)
    } else if ty == clif_types::F32 {
        ctx.builder.ins().splat(clif_types::F32X4, val)
    } else if ty == clif_types::F64 {
        let f32_val = ctx.builder.ins().fdemote(clif_types::F32, val);
        ctx.builder.ins().splat(clif_types::F32X4, f32_val)
    } else {
        let zero = ctx.builder.ins().f32const(0.0);
        ctx.builder.ins().splat(clif_types::F32X4, zero)
    }
}

/// Ensures a value is an I32X4 vector. If it is an I64 pointer, loads from memory.
pub fn ensure_i32x4<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    val: ClifValue,
) -> ClifValue {
    let ty = ctx.builder.func.dfg.value_type(val);
    if ty == clif_types::I32X4 {
        val
    } else if ty == clif_types::I64 {
        let flags = MachMemFlags::new();
        ctx.builder.ins().load(clif_types::I32X4, flags, val, 0)
    } else if ty == clif_types::I32 {
        ctx.builder.ins().splat(clif_types::I32X4, val)
    } else {
        let zero = ctx.builder.ins().iconst(clif_types::I32, 0);
        ctx.builder.ins().splat(clif_types::I32X4, zero)
    }
}

/// Attempts to compile a SIMD intrinsic call in Cranelift. Returns Ok(true) if handled.
pub fn try_compile_simd_call<M: ClifModule>(
    ctx: &mut FunctionCompileCtx<'_, '_, M>,
    dest: &ValueId,
    func: &str,
    args: &[ValueId],
    _ty: &str,
) -> Result<bool, String> {
    // 1. float4 / f32x4 constructor (4 scalars -> F32X4 vector register)
    if (func == "float4"
        || func == "datara_rt_float4"
        || func == "f32x4"
        || func == "datara_rt_f32x4")
        && args.len() == 4
    {
        let zero = ctx.builder.ins().f32const(0.0);
        let mut vec = ctx.builder.ins().splat(clif_types::F32X4, zero);
        for (i, a) in args.iter().enumerate() {
            let raw_val = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
            let s_val = to_f32(ctx, raw_val);
            vec = ctx.builder.ins().insertlane(vec, s_val, i as u8);
        }
        ctx.val_map.insert(*dest, vec);
        return Ok(true);
    }

    // 2. int4 / i32x4 constructor (4 scalars -> I32X4 vector register)
    if (func == "int4" || func == "datara_rt_int4" || func == "i32x4" || func == "datara_rt_i32x4")
        && args.len() == 4
    {
        let zero = ctx.builder.ins().iconst(clif_types::I32, 0);
        let mut vec = ctx.builder.ins().splat(clif_types::I32X4, zero);
        for (i, a) in args.iter().enumerate() {
            let raw_val = ctx
                .val_map
                .get(a)
                .copied()
                .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
            let s_val = to_i32(ctx, raw_val);
            vec = ctx.builder.ins().insertlane(vec, s_val, i as u8);
        }
        ctx.val_map.insert(*dest, vec);
        return Ok(true);
    }

    // 3. f32x4_splat (1 scalar -> F32X4 broadcast)
    if (func == "f32x4_splat" || func == "datara_rt_f32x4_splat") && args.len() == 1 {
        let raw_val = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let s_val = to_f32(ctx, raw_val);
        let vec = ctx.builder.ins().splat(clif_types::F32X4, s_val);
        ctx.val_map.insert(*dest, vec);
        return Ok(true);
    }

    // 4. Float Vector Arithmetic: add, sub, mul, div
    if (func == "f32x4_add"
        || func == "f32x4_sub"
        || func == "f32x4_mul"
        || func == "f32x4_div"
        || func == "vec4_add"
        || func == "add4")
        && args.len() == 2
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v1 = ensure_f32x4(ctx, v1_raw);
        let v2 = ensure_f32x4(ctx, v2_raw);

        let res = match func {
            "f32x4_sub" => ctx.builder.ins().fsub(v1, v2),
            "f32x4_mul" => ctx.builder.ins().fmul(v1, v2),
            "f32x4_div" => {
                let e0_a = ctx.builder.ins().extractlane(v1, 0);
                let e0_b = ctx.builder.ins().extractlane(v2, 0);
                let d0 = ctx.builder.ins().fdiv(e0_a, e0_b);

                let e1_a = ctx.builder.ins().extractlane(v1, 1);
                let e1_b = ctx.builder.ins().extractlane(v2, 1);
                let d1 = ctx.builder.ins().fdiv(e1_a, e1_b);

                let e2_a = ctx.builder.ins().extractlane(v1, 2);
                let e2_b = ctx.builder.ins().extractlane(v2, 2);
                let d2 = ctx.builder.ins().fdiv(e2_a, e2_b);

                let e3_a = ctx.builder.ins().extractlane(v1, 3);
                let e3_b = ctx.builder.ins().extractlane(v2, 3);
                let d3 = ctx.builder.ins().fdiv(e3_a, e3_b);

                let mut v = ctx.builder.ins().splat(clif_types::F32X4, d0);
                v = ctx.builder.ins().insertlane(v, d1, 1);
                v = ctx.builder.ins().insertlane(v, d2, 2);
                ctx.builder.ins().insertlane(v, d3, 3)
            }
            _ => ctx.builder.ins().fadd(v1, v2),
        };
        ctx.val_map.insert(*dest, res);
        return Ok(true);
    }

    // 5. Float Vector Min/Max
    if (func == "f32x4_min" || func == "min4" || func == "f32x4_max" || func == "max4")
        && args.len() == 2
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v1 = ensure_f32x4(ctx, v1_raw);
        let v2 = ensure_f32x4(ctx, v2_raw);

        let res = if func.contains("min") {
            ctx.builder.ins().fmin(v1, v2)
        } else {
            ctx.builder.ins().fmax(v1, v2)
        };
        ctx.val_map.insert(*dest, res);
        return Ok(true);
    }

    // 6. Sqrt, Neg, Abs
    if (func == "f32x4_sqrt" || func == "f32x4_neg" || func == "f32x4_abs") && args.len() == 1 {
        let v_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v = ensure_f32x4(ctx, v_raw);
        let res = match func {
            "f32x4_sqrt" => ctx.builder.ins().sqrt(v),
            "f32x4_neg" => ctx.builder.ins().fneg(v),
            _ => ctx.builder.ins().fabs(v),
        };
        ctx.val_map.insert(*dest, res);
        return Ok(true);
    }

    // 7. Horizontal Add
    if (func == "f32x4_horizontal_add" || func == "horizontal_add") && args.len() == 1 {
        let v_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v = ensure_f32x4(ctx, v_raw);
        let e0 = ctx.builder.ins().extractlane(v, 0);
        let e1 = ctx.builder.ins().extractlane(v, 1);
        let e2 = ctx.builder.ins().extractlane(v, 2);
        let e3 = ctx.builder.ins().extractlane(v, 3);
        let s0 = ctx.builder.ins().fadd(e0, e1);
        let s1 = ctx.builder.ins().fadd(e2, e3);
        let sum = ctx.builder.ins().fadd(s0, s1);
        let res_f64 = ctx.builder.ins().fpromote(clif_types::F64, sum);
        ctx.val_map.insert(*dest, res_f64);
        return Ok(true);
    }

    // 8. Dot Product (v1 . v2) -> scalar Float64
    if (func == "dot" || func == "datara_rt_float4_dot" || func == "f32x4_dot" || func == "dot4")
        && args.len() == 2
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v1 = ensure_f32x4(ctx, v1_raw);
        let v2 = ensure_f32x4(ctx, v2_raw);

        let prod = ctx.builder.ins().fmul(v1, v2);
        let e0 = ctx.builder.ins().extractlane(prod, 0);
        let e1 = ctx.builder.ins().extractlane(prod, 1);
        let e2 = ctx.builder.ins().extractlane(prod, 2);
        let e3 = ctx.builder.ins().extractlane(prod, 3);
        let s0 = ctx.builder.ins().fadd(e0, e1);
        let s1 = ctx.builder.ins().fadd(e2, e3);
        let sum = ctx.builder.ins().fadd(s0, s1);
        let res_f64 = ctx.builder.ins().fpromote(clif_types::F64, sum);
        ctx.val_map.insert(*dest, res_f64);
        return Ok(true);
    }

    // 9. 3D Cross Product (a x b)
    if (func == "f32x4_cross" || func == "cross3" || func == "datara_rt_f32x4_cross")
        && args.len() == 2
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let a = ensure_f32x4(ctx, v1_raw);
        let b = ensure_f32x4(ctx, v2_raw);

        let ax = ctx.builder.ins().extractlane(a, 0);
        let ay = ctx.builder.ins().extractlane(a, 1);
        let az = ctx.builder.ins().extractlane(a, 2);

        let bx = ctx.builder.ins().extractlane(b, 0);
        let by = ctx.builder.ins().extractlane(b, 1);
        let bz = ctx.builder.ins().extractlane(b, 2);

        // cx = ay * bz - az * by
        let ay_bz = ctx.builder.ins().fmul(ay, bz);
        let az_by = ctx.builder.ins().fmul(az, by);
        let cx = ctx.builder.ins().fsub(ay_bz, az_by);

        // cy = az * bx - ax * bz
        let az_bx = ctx.builder.ins().fmul(az, bx);
        let ax_bz = ctx.builder.ins().fmul(ax, bz);
        let cy = ctx.builder.ins().fsub(az_bx, ax_bz);

        // cz = ax * by - ay * bx
        let ax_by = ctx.builder.ins().fmul(ax, by);
        let ay_bx = ctx.builder.ins().fmul(ay, bx);
        let cz = ctx.builder.ins().fsub(ax_by, ay_bx);

        let zero = ctx.builder.ins().f32const(0.0);
        let mut cross_v = ctx.builder.ins().splat(clif_types::F32X4, zero);
        cross_v = ctx.builder.ins().insertlane(cross_v, cx, 0);
        cross_v = ctx.builder.ins().insertlane(cross_v, cy, 1);
        cross_v = ctx.builder.ins().insertlane(cross_v, cz, 2);

        ctx.val_map.insert(*dest, cross_v);
        return Ok(true);
    }

    // 10. Vector Normalize: v / sqrt(dot(v, v))
    if (func == "f32x4_normalize" || func == "datara_rt_f32x4_normalize") && args.len() == 1 {
        let v_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v = ensure_f32x4(ctx, v_raw);

        let prod = ctx.builder.ins().fmul(v, v);
        let e0 = ctx.builder.ins().extractlane(prod, 0);
        let e1 = ctx.builder.ins().extractlane(prod, 1);
        let e2 = ctx.builder.ins().extractlane(prod, 2);
        let e3 = ctx.builder.ins().extractlane(prod, 3);
        let s0 = ctx.builder.ins().fadd(e0, e1);
        let s1 = ctx.builder.ins().fadd(e2, e3);
        let len_sq = ctx.builder.ins().fadd(s0, s1);
        let len = ctx.builder.ins().sqrt(len_sq);
        let one = ctx.builder.ins().f32const(1.0);
        let inv_len = ctx.builder.ins().fdiv(one, len);
        let inv_splat = ctx.builder.ins().splat(clif_types::F32X4, inv_len);
        let normalized = ctx.builder.ins().fmul(v, inv_splat);

        ctx.val_map.insert(*dest, normalized);
        return Ok(true);
    }

    // 11. Vector Linear Interpolation: a + (b - a) * t
    if (func == "f32x4_lerp" || func == "lerp4" || func == "datara_rt_f32x4_lerp")
        && args.len() == 3
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let t_raw = ctx
            .val_map
            .get(&args[2])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().f64const(0.0));
        let a = ensure_f32x4(ctx, v1_raw);
        let b = ensure_f32x4(ctx, v2_raw);
        let t_scalar = to_f32(ctx, t_raw);
        let t_splat = ctx.builder.ins().splat(clif_types::F32X4, t_scalar);

        let diff = ctx.builder.ins().fsub(b, a);
        let scaled = ctx.builder.ins().fmul(diff, t_splat);
        let res = ctx.builder.ins().fadd(a, scaled);

        ctx.val_map.insert(*dest, res);
        return Ok(true);
    }

    // 12. Vector Distance: sqrt(dot(a - b, a - b))
    if (func == "f32x4_distance" || func == "datara_rt_f32x4_distance") && args.len() == 2 {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let a = ensure_f32x4(ctx, v1_raw);
        let b = ensure_f32x4(ctx, v2_raw);

        let diff = ctx.builder.ins().fsub(a, b);
        let prod = ctx.builder.ins().fmul(diff, diff);
        let e0 = ctx.builder.ins().extractlane(prod, 0);
        let e1 = ctx.builder.ins().extractlane(prod, 1);
        let e2 = ctx.builder.ins().extractlane(prod, 2);
        let e3 = ctx.builder.ins().extractlane(prod, 3);
        let s0 = ctx.builder.ins().fadd(e0, e1);
        let s1 = ctx.builder.ins().fadd(e2, e3);
        let dist_sq = ctx.builder.ins().fadd(s0, s1);
        let dist_f32 = ctx.builder.ins().sqrt(dist_sq);
        let dist_f64 = ctx.builder.ins().fpromote(clif_types::F64, dist_f32);

        ctx.val_map.insert(*dest, dist_f64);
        return Ok(true);
    }

    // 13. f32x4 Load / Store (Direct 128-bit memory instructions)
    if func == "f32x4_load" && args.len() == 1 {
        let ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let flags = MachMemFlags::new();
        let loaded = ctx.builder.ins().load(clif_types::F32X4, flags, ptr, 0);
        ctx.val_map.insert(*dest, loaded);
        return Ok(true);
    }

    if func == "f32x4_store" && args.len() == 2 {
        let ptr = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let val = ensure_f32x4(ctx, v_raw);
        let flags = MachMemFlags::new();
        ctx.builder.ins().store(flags, val, ptr, 0);
        ctx.val_map
            .insert(*dest, ctx.builder.ins().iconst(clif_types::I64, 0));
        return Ok(true);
    }

    // 14. Integer Vector Arithmetic (i32x4)
    if (func == "i32x4_add"
        || func == "i32x4_sub"
        || func == "i32x4_mul"
        || func == "i32x4_and"
        || func == "i32x4_or"
        || func == "i32x4_xor")
        && args.len() == 2
    {
        let v1_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v2_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let v1 = ensure_i32x4(ctx, v1_raw);
        let v2 = ensure_i32x4(ctx, v2_raw);

        let res = match func {
            "i32x4_sub" => ctx.builder.ins().isub(v1, v2),
            "i32x4_mul" => ctx.builder.ins().imul(v1, v2),
            "i32x4_and" => ctx.builder.ins().band(v1, v2),
            "i32x4_or" => ctx.builder.ins().bor(v1, v2),
            "i32x4_xor" => ctx.builder.ins().bxor(v1, v2),
            _ => ctx.builder.ins().iadd(v1, v2),
        };
        ctx.val_map.insert(*dest, res);
        return Ok(true);
    }

    // 15. AABB Intersection Check (min_a, max_a, min_b, max_b) -> Int (1 or 0)
    if (func == "aabb_intersects" || func == "datara_rt_aabb_intersects") && args.len() == 4 {
        let min_a_raw = ctx
            .val_map
            .get(&args[0])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let max_a_raw = ctx
            .val_map
            .get(&args[1])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let min_b_raw = ctx
            .val_map
            .get(&args[2])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));
        let max_b_raw = ctx
            .val_map
            .get(&args[3])
            .copied()
            .unwrap_or_else(|| ctx.builder.ins().iconst(clif_types::I64, 0));

        let min_a = ensure_f32x4(ctx, min_a_raw);
        let max_a = ensure_f32x4(ctx, max_a_raw);
        let min_b = ensure_f32x4(ctx, min_b_raw);
        let max_b = ensure_f32x4(ctx, max_b_raw);

        // Check for overlap: min_a <= max_b and max_a >= min_b on X, Y, Z
        let min_a_x = ctx.builder.ins().extractlane(min_a, 0);
        let min_a_y = ctx.builder.ins().extractlane(min_a, 1);
        let min_a_z = ctx.builder.ins().extractlane(min_a, 2);

        let max_a_x = ctx.builder.ins().extractlane(max_a, 0);
        let max_a_y = ctx.builder.ins().extractlane(max_a, 1);
        let max_a_z = ctx.builder.ins().extractlane(max_a, 2);

        let min_b_x = ctx.builder.ins().extractlane(min_b, 0);
        let min_b_y = ctx.builder.ins().extractlane(min_b, 1);
        let min_b_z = ctx.builder.ins().extractlane(min_b, 2);

        let max_b_x = ctx.builder.ins().extractlane(max_b, 0);
        let max_b_y = ctx.builder.ins().extractlane(max_b, 1);
        let max_b_z = ctx.builder.ins().extractlane(max_b, 2);

        use cranelift_codegen::ir::condcodes::FloatCC;
        let c1_x = ctx
            .builder
            .ins()
            .fcmp(FloatCC::LessThanOrEqual, min_a_x, max_b_x);
        let c2_x = ctx
            .builder
            .ins()
            .fcmp(FloatCC::GreaterThanOrEqual, max_a_x, min_b_x);
        let ov_x = ctx.builder.ins().band(c1_x, c2_x);

        let c1_y = ctx
            .builder
            .ins()
            .fcmp(FloatCC::LessThanOrEqual, min_a_y, max_b_y);
        let c2_y = ctx
            .builder
            .ins()
            .fcmp(FloatCC::GreaterThanOrEqual, max_a_y, min_b_y);
        let ov_y = ctx.builder.ins().band(c1_y, c2_y);

        let c1_z = ctx
            .builder
            .ins()
            .fcmp(FloatCC::LessThanOrEqual, min_a_z, max_b_z);
        let c2_z = ctx
            .builder
            .ins()
            .fcmp(FloatCC::GreaterThanOrEqual, max_a_z, min_b_z);
        let ov_z = ctx.builder.ins().band(c1_z, c2_z);

        let ov_xy = ctx.builder.ins().band(ov_x, ov_y);
        let ov_xyz = ctx.builder.ins().band(ov_xy, ov_z);
        let res_i64 = ctx.builder.ins().uextend(clif_types::I64, ov_xyz);

        ctx.val_map.insert(*dest, res_i64);
        return Ok(true);
    }

    Ok(false)
}
