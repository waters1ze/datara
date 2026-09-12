//! LLVM Vector and std.simd Lowering Engine
//! Emits vectorized instructions (<4 x float>, <8 x float>, <16 x float>, <4 x i32>, etc.)
//! or runtime fallback calls for std.simd intrinsics.

use crate::dmir::ValueId;
use std::collections::HashMap;

/// Emits standard SIMD runtime and vector intrinsic declarations into the LLVM IR header.
pub fn emit_simd_declarations(ir: &mut String) {
    ir.push_str("; --- std.simd Vector Intrinsics & Runtime Declarations ---\n");
    ir.push_str("declare <8 x float> @llvm.minnum.v8f32(<8 x float>, <8 x float>)\n");
    ir.push_str("declare <8 x float> @llvm.maxnum.v8f32(<8 x float>, <8 x float>)\n");
    ir.push_str("declare float @llvm.vector.reduce.fadd.v8f32(float, <8 x float>)\n");
    ir.push_str("declare float @llvm.vector.reduce.fadd.v16f32(float, <16 x float>)\n");
    ir.push_str("declare double @llvm.vector.reduce.fadd.v2f64(double, <2 x double>)\n");
    ir.push_str("declare double @llvm.vector.reduce.fadd.v4f64(double, <4 x double>)\n");
    ir.push_str("declare double @datara_rt_dot_f32_array(ptr, ptr, i64)\n");
    ir.push_str("declare double @datara_rt_ray_sphere_intersect_simd(<4 x float>, <4 x float>, <4 x float>, double)\n");
    ir.push_str("declare double @datara_rt_ray_sphere_intersect_scalar(double, double, double, double, double, double, double, double, double, double)\n");
    ir.push_str("declare <4 x float> @datara_rt_f32x4_cross(<4 x float>, <4 x float>)\n");
    ir.push_str("declare <4 x float> @datara_rt_f32x4_normalize(<4 x float>)\n");
    ir.push_str("declare <8 x float> @datara_rt_f32x8_normalize(<8 x float>)\n");
    ir.push_str("declare <16 x float> @datara_rt_f32x16_normalize(<16 x float>)\n");
    ir.push_str("declare <2 x double> @datara_rt_f64x2_normalize(<2 x double>)\n");
    ir.push_str("declare <4 x double> @datara_rt_f64x4_normalize(<4 x double>)\n");
    ir.push_str("declare double @llvm.fma.f64(double, double, double)\n");
    ir.push_str("declare float @llvm.fma.f32(float, float, float)\n");
    ir.push_str("declare double @datara_rt_fma(double, double, double)\n");
    ir.push_str("declare float @datara_rt_fmaf(float, float, float)\n\n");
}

/// Attempts to lower a std.simd function call. Returns true if lowered.
pub fn try_emit_simd_call(
    func: &str,
    dest: &ValueId,
    args: &[ValueId],
    value_types: &mut HashMap<ValueId, &'static str>,
    out: &mut String,
) -> bool {
    // f32x4 constructor
    if (func == "f32x4"
        || func == "datara_rt_f32x4"
        || func == "float4"
        || func == "datara_rt_float4")
        && args.len() == 4
    {
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
        return true;
    }

    // f32x4 operations: f32x4_add, sub, mul, div
    if (func == "f32x4_add" || func == "f32x4_sub" || func == "f32x4_mul" || func == "f32x4_div")
        && args.len() == 2
    {
        value_types.insert(*dest, "<4 x float>");
        let op = match func {
            "f32x4_add" => "fadd",
            "f32x4_sub" => "fsub",
            "f32x4_mul" => "fmul",
            "f32x4_div" => "fdiv",
            _ => "fadd",
        };
        out.push_str(&format!(
            "  %v{} = {} <4 x float> %v{}, %v{}\n",
            dest.0, op, args[0].0, args[1].0
        ));
        return true;
    }

    // f32x4_dot / dot
    if (func == "f32x4_dot" || func == "dot" || func == "datara_rt_float4_dot") && args.len() == 2 {
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
            return true;
        }
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
            return true;
        }
    }

    // f32x4_horizontal_add
    if (func == "f32x4_horizontal_add" || func == "horizontal_add")
        && args.len() == 1
        && value_types.get(&args[0]).copied() == Some("<4 x float>")
    {
        value_types.insert(*dest, "double");
        let red_val = format!("%vhadd_{}", dest.0);
        out.push_str(&format!(
            "  {} = call fast float @llvm.vector.reduce.fadd.v4f32(float -0.0, <4 x float> %v{})\n",
            red_val, args[0].0
        ));
        out.push_str(&format!(
            "  %v{} = fpext float {} to double\n",
            dest.0, red_val
        ));
        return true;
    }

    // f32x4_min / f32x4_max / min4 / max4
    if (func == "f32x4_min" || func == "f32x4_max" || func == "min4" || func == "max4")
        && args.len() == 2
        && value_types.get(&args[0]).copied() == Some("<4 x float>")
        && value_types.get(&args[1]).copied() == Some("<4 x float>")
    {
        value_types.insert(*dest, "<4 x float>");
        let op = if func == "f32x4_min" || func == "min4" {
            "llvm.minnum.v4f32"
        } else {
            "llvm.maxnum.v4f32"
        };
        out.push_str(&format!(
            "  %v{} = call <4 x float> @{}(<4 x float> %v{}, <4 x float> %v{})\n",
            dest.0, op, args[0].0, args[1].0
        ));
        return true;
    }

    // f32x4_lerp: a + t * (b - a)
    if func == "f32x4_lerp" && args.len() == 3 {
        value_types.insert(*dest, "<4 x float>");
        let diff = format!("%lerp_diff_{}", dest.0);
        out.push_str(&format!(
            "  {} = fsub <4 x float> %v{}, %v{}\n",
            diff, args[1].0, args[0].0
        ));
        let t_f = format!("%lerp_tf_{}", dest.0);
        out.push_str(&format!(
            "  {} = fptrunc double %v{} to float\n",
            t_f, args[2].0
        ));
        let t_splat = format!("%lerp_tsplat_{}", dest.0);
        out.push_str(&format!(
            "  {} = insertelement <4 x float> poison, float {}, i32 0\n",
            t_splat, t_f
        ));
        let t_vec = format!("%lerp_tvec_{}", dest.0);
        out.push_str(&format!(
            "  {} = shufflevector <4 x float> {}, <4 x float> poison, <4 x i32> zeroinitializer\n",
            t_vec, t_splat
        ));
        let scaled = format!("%lerp_scaled_{}", dest.0);
        out.push_str(&format!(
            "  {} = fmul <4 x float> {}, {}\n",
            scaled, diff, t_vec
        ));
        out.push_str(&format!(
            "  %v{} = fadd <4 x float> %v{}, {}\n",
            dest.0, args[0].0, scaled
        ));
        return true;
    }

    // f32x4_cross
    if func == "f32x4_cross" && args.len() == 2 {
        value_types.insert(*dest, "<4 x float>");
        out.push_str(&format!("  %v{} = call <4 x float> @datara_rt_f32x4_cross(<4 x float> %v{}, <4 x float> %v{})\n", dest.0, args[0].0, args[1].0));
        return true;
    }

    // f32x4_normalize
    if func == "f32x4_normalize" && args.len() == 1 {
        value_types.insert(*dest, "<4 x float>");
        out.push_str(&format!(
            "  %v{} = call <4 x float> @datara_rt_f32x4_normalize(<4 x float> %v{})\n",
            dest.0, args[0].0
        ));
        return true;
    }

    // f32x4_distance
    if func == "f32x4_distance" && args.len() == 2 {
        value_types.insert(*dest, "double");
        let diff = format!("%dist_diff_{}", dest.0);
        out.push_str(&format!(
            "  {} = fsub <4 x float> %v{}, %v{}\n",
            diff, args[0].0, args[1].0
        ));
        let mul_v = format!("%dist_mul_{}", dest.0);
        out.push_str(&format!(
            "  {} = fmul <4 x float> {}, {}\n",
            mul_v, diff, diff
        ));
        let red_v = format!("%dist_red_{}", dest.0);
        out.push_str(&format!(
            "  {} = call fast float @llvm.vector.reduce.fadd.v4f32(float -0.0, <4 x float> {})\n",
            red_v, mul_v
        ));
        let red_d = format!("%dist_d_{}", dest.0);
        out.push_str(&format!("  {} = fpext float {} to double\n", red_d, red_v));
        out.push_str(&format!(
            "  %v{} = call double @datara_rt_math_sqrt(double {})\n",
            dest.0, red_d
        ));
        return true;
    }

    // i32x4 / int4 constructor
    if (func == "i32x4" || func == "datara_rt_i32x4" || func == "int4" || func == "datara_rt_int4")
        && args.len() == 4
    {
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
                out.push_str(&format!("  {} = fptosi double %v{} to i32\n", tmp, arg.0));
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
        return true;
    }

    // i32x4 operations: add, sub, mul
    if (func == "i32x4_add" || func == "i32x4_sub" || func == "i32x4_mul") && args.len() == 2 {
        value_types.insert(*dest, "<4 x i32>");
        let op = match func {
            "i32x4_add" => "add",
            "i32x4_sub" => "sub",
            "i32x4_mul" => "mul",
            _ => "add",
        };
        out.push_str(&format!(
            "  %v{} = {} <4 x i32> %v{}, %v{}\n",
            dest.0, op, args[0].0, args[1].0
        ));
        return true;
    }

    // dot_f32_array
    if (func == "dot_f32_array" || func == "datara_rt_dot_f32_array") && args.len() == 3 {
        value_types.insert(*dest, "double");
        out.push_str(&format!(
            "  %v{} = call double @datara_rt_dot_f32_array(ptr %v{}, ptr %v{}, i64 %v{})\n",
            dest.0, args[0].0, args[1].0, args[2].0
        ));
        return true;
    }

    // ray_sphere_intersect_simd
    if (func == "ray_sphere_intersect_simd" || func == "datara_rt_ray_sphere_intersect_simd")
        && args.len() == 4
    {
        value_types.insert(*dest, "double");
        out.push_str(&format!("  %v{} = call double @datara_rt_ray_sphere_intersect_simd(<4 x float> %v{}, <4 x float> %v{}, <4 x float> %v{}, double %v{})\n", dest.0, args[0].0, args[1].0, args[2].0, args[3].0));
        return true;
    }

    // ray_sphere_intersect_scalar
    if (func == "ray_sphere_intersect_scalar" || func == "datara_rt_ray_sphere_intersect_scalar")
        && args.len() == 10
    {
        value_types.insert(*dest, "double");
        out.push_str(&format!("  %v{} = call double @datara_rt_ray_sphere_intersect_scalar(double %v{}, double %v{}, double %v{}, double %v{}, double %v{}, double %v{}, double %v{}, double %v{}, double %v{}, double %v{})\n",
            dest.0, args[0].0, args[1].0, args[2].0, args[3].0, args[4].0, args[5].0, args[6].0, args[7].0, args[8].0, args[9].0));
        return true;
    }

    // FMA (Fused Multiply-Add)
    if (func == "fma" || func == "datara_rt_fma") && args.len() == 3 {
        let mut cast_args = Vec::with_capacity(3);
        for (i, arg) in args.iter().enumerate() {
            let arg_ty = value_types.get(arg).copied().unwrap_or("double");
            if arg_ty == "i64" || arg_ty == "i32" {
                let tmp = format!("%fma_cast_{}_{}", dest.0, i);
                out.push_str(&format!(
                    "  {} = sitofp {} %v{} to double\n",
                    tmp, arg_ty, arg.0
                ));
                cast_args.push(tmp);
            } else if arg_ty == "float" {
                let tmp = format!("%fma_cast_{}_{}", dest.0, i);
                out.push_str(&format!("  {} = fpext float %v{} to double\n", tmp, arg.0));
                cast_args.push(tmp);
            } else {
                cast_args.push(format!("%v{}", arg.0));
            }
        }
        value_types.insert(*dest, "double");
        out.push_str(&format!(
            "  %v{} = call double @llvm.fma.f64(double {}, double {}, double {})\n",
            dest.0, cast_args[0], cast_args[1], cast_args[2]
        ));
        return true;
    }
    if (func == "fmaf" || func == "datara_rt_fmaf") && args.len() == 3 {
        let mut cast_args = Vec::with_capacity(3);
        for (i, arg) in args.iter().enumerate() {
            let arg_ty = value_types.get(arg).copied().unwrap_or("float");
            if arg_ty == "i64" || arg_ty == "i32" {
                let tmp = format!("%fmaf_cast_{}_{}", dest.0, i);
                out.push_str(&format!(
                    "  {} = sitofp {} %v{} to float\n",
                    tmp, arg_ty, arg.0
                ));
                cast_args.push(tmp);
            } else if arg_ty == "double" {
                let tmp = format!("%fmaf_cast_{}_{}", dest.0, i);
                out.push_str(&format!(
                    "  {} = fptrunc double %v{} to float\n",
                    tmp, arg.0
                ));
                cast_args.push(tmp);
            } else {
                cast_args.push(format!("%v{}", arg.0));
            }
        }
        value_types.insert(*dest, "float");
        out.push_str(&format!(
            "  %v{} = call float @llvm.fma.f32(float {}, float {}, float {})\n",
            dest.0, cast_args[0], cast_args[1], cast_args[2]
        ));
        return true;
    }

    false
}
