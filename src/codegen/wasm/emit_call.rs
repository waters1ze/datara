use super::WasmEmitter;
use super::capabilities::classify_capability_call;
use super::types::*;
use crate::dmir::{BasicBlockId, Terminator, ValueId};
use std::collections::HashMap;

impl WasmEmitter {
    /// Lowers function calls, hardware SIMD operations, or imported runtime built-ins.
    pub(crate) fn compile_call(
        dest: ValueId,
        func: &str,
        args: &[ValueId],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        value_types: &HashMap<ValueId, WasmValType>,
        import_fn_indices: &HashMap<String, u32>,
        defined_fn_indices: &HashMap<String, u32>,
        scratch_v128: u32,
    ) -> Result<(), String> {
        let dest_loc = local_map.get(&dest).copied().unwrap_or(0);

        // -------------------------------------------------------------------
        // Hardware SIMD Lowering (v128)
        // -------------------------------------------------------------------
        if (func == "float4" || func == "datara_rt_float4") && args.len() == 4 {
            // v128.const (opcode 12 = 0x0C), then replace_lane 0..3 with f32.demote_f64
            body.push(0xFD); // SIMD prefix
            encode_u32_leb128(12, body); // v128.const (opcode 12)
            body.extend_from_slice(&[0u8; 16]); // 16 zero bytes

            for (lane, arg) in args.iter().enumerate() {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                let arg_ty = value_types.get(arg).copied().unwrap_or(WasmValType::F64);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);

                if arg_ty == WasmValType::I64 {
                    body.push(0xB9); // f64.convert_i64_s
                    body.push(0xB6); // f32.demote_f64
                } else if arg_ty == WasmValType::F64 {
                    body.push(0xB6); // f32.demote_f64
                }

                body.push(0xFD); // SIMD prefix
                encode_u32_leb128(32, body); // f32x4.replace_lane
                body.push(lane as u8);
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (v128.float4 (local.get $v{}) (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0, args[2].0, args[3].0
            ));
            return Ok(());
        }

        if (func == "int4" || func == "datara_rt_int4") && args.len() == 4 {
            // v128.const (opcode 12 = 0x0C), then replace_lane 0..3 with i32.wrap_i64
            body.push(0xFD); // SIMD prefix
            encode_u32_leb128(12, body); // v128.const (opcode 12)
            body.extend_from_slice(&[0u8; 16]);

            for (lane, arg) in args.iter().enumerate() {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
                body.push(0xA7); // i32.wrap_i64
                body.push(0xFD); // SIMD prefix
                encode_u32_leb128(28, body); // i32x4.replace_lane
                body.push(lane as u8);
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (v128.int4 (local.get $v{}) (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0, args[2].0, args[3].0
            ));
            return Ok(());
        }

        if (func == "min4"
            || func == "max4"
            || func == "datara_rt_float4_min4"
            || func == "datara_rt_float4_max4")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);

            body.push(0x20); // local.get a
            encode_u32_leb128(a_loc, body);
            body.push(0x20); // local.get b
            encode_u32_leb128(b_loc, body);

            body.push(0xFD); // SIMD prefix
            if func.contains("min4") {
                encode_u32_leb128(234, body); // f32x4.pmin
            } else {
                encode_u32_leb128(235, body); // f32x4.pmax
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.{} (local.get $v{}) (local.get $v{})))\n",
                dest.0,
                if func.contains("min4") {
                    "pmin"
                } else {
                    "pmax"
                },
                args[0].0,
                args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_add" || func == "add4" || func == "datara_rt_float4_add")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.add (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_sub" || func == "sub4" || func == "datara_rt_float4_sub")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(229, body); // f32x4.sub
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.sub (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_mul" || func == "mul4" || func == "datara_rt_float4_mul")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(230, body); // f32x4.mul
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.mul (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "float4_x"
            || func == "lane0"
            || func == "float4_y"
            || func == "lane1"
            || func == "float4_z"
            || func == "lane2"
            || func == "float4_w"
            || func == "lane3")
            && args.len() == 1
        {
            let arg_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let lane_idx: u8 = match func {
                "float4_y" | "lane1" => 1,
                "float4_z" | "lane2" => 2,
                "float4_w" | "lane3" => 3,
                _ => 0,
            };
            body.push(0x20);
            encode_u32_leb128(arg_loc, body);
            body.push(0xFD);
            encode_u32_leb128(31, body); // f32x4.extract_lane
            body.push(lane_idx);
            body.push(0xBB); // f64.promote_f32
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f64.promote_f32 (f32x4.extract_lane {} (local.get $v{}))))\n",
                dest.0, lane_idx, args[0].0
            ));
            return Ok(());
        }

        if (func == "int4_x" || func == "int4_y" || func == "int4_z" || func == "int4_w")
            && args.len() == 1
        {
            let arg_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let lane_idx: u8 = match func {
                "int4_y" => 1,
                "int4_z" => 2,
                "int4_w" => 3,
                _ => 0,
            };
            body.push(0x20);
            encode_u32_leb128(arg_loc, body);
            body.push(0xFD);
            encode_u32_leb128(25, body); // i32x4.extract_lane_s
            body.push(lane_idx);
            body.push(0xAC); // i64.extend_i32_s
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!("    (local.set $v{} (i64.extend_i32_s (i32x4.extract_lane_s {} (local.get $v{}))))\n", dest.0, lane_idx, args[0].0));
            return Ok(());
        }

        if (func == "dot" || func == "datara_rt_float4_dot") && args.len() == 2 {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);

            // Step 1: f32x4.mul(a, b) -> store in scratch_v128
            body.push(0x20); // local.get a
            encode_u32_leb128(a_loc, body);
            body.push(0x20); // local.get b
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(230, body); // f32x4.mul (opcode 230)
            body.push(0x21); // local.set scratch_v128
            encode_u32_leb128(scratch_v128, body);

            // Step 2: Shuffle swap pairs [1, 0, 3, 2] and add
            // We want: f32x4.add(scratch, shuffle(scratch, scratch))
            body.push(0x20); // local.get scratch_v128 (operand for add)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 1 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 2 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0xFD);
            encode_u32_leb128(13, body); // i8x16.shuffle
            body.extend_from_slice(&[4, 5, 6, 7, 0, 1, 2, 3, 12, 13, 14, 15, 8, 9, 10, 11]);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add
            body.push(0x21); // local.set scratch_v128 (store intermediate sum)
            encode_u32_leb128(scratch_v128, body);

            // Step 3: Shuffle swap 64-bit halves [2, 3, 0, 1] and add
            // We want: f32x4.add(scratch, shuffle(scratch, scratch))
            body.push(0x20); // local.get scratch_v128 (operand for add)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 1 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 2 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0xFD);
            encode_u32_leb128(13, body); // i8x16.shuffle
            body.extend_from_slice(&[8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7]);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add

            // Step 4: Extract lane 0 and promote to f64
            body.push(0xFD);
            encode_u32_leb128(31, body); // f32x4.extract_lane
            body.push(0); // lane 0
            body.push(0xBB); // f64.promote_f32

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (f64.promote_f32 (f32x4.extract_lane 0 (f32x4.dot (local.get $v{}) (local.get $v{})))))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        // -------------------------------------------------------------------
        // Direct Function & Import Calls
        // -------------------------------------------------------------------
        let imp_idx = import_fn_indices.get(func).copied().or_else(|| {
            classify_capability_call(func)
                .and_then(|(m, f)| import_fn_indices.get(&format!("{}/{}", m, f)).copied())
        });

        let arg_wat = args
            .iter()
            .map(|a| format!("(local.get $v{})", a.0))
            .collect::<Vec<_>>()
            .join(" ");
        let arg_wat_suffix = if arg_wat.is_empty() {
            String::new()
        } else {
            format!(" {}", arg_wat)
        };

        if let Some(&def_idx) = defined_fn_indices.get(func) {
            for arg in args {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
            }
            body.push(0x10); // call
            encode_u32_leb128(def_idx, body);
            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (call ${}{}))\n",
                dest.0, func, arg_wat_suffix
            ));
        } else if let Some(imp_idx) = imp_idx {
            for arg in args {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
            }
            body.push(0x10); // call
            encode_u32_leb128(imp_idx, body);

            // Void-returning imports (e.g. own_release, print, err) leave no value on operand stack
            let is_void = matches!(
                func,
                "own_release"
                    | "datara_rt_own_release"
                    | "print"
                    | "err"
                    | "datara_rt_print"
                    | "datara_rt_err"
            );
            if !is_void {
                body.push(0x21); // local.set dest
                encode_u32_leb128(dest_loc, body);
            }
            let sanitized = func.replace(':', "_").replace('@', "_").replace('/', "_");
            if is_void {
                wat.push_str(&format!("    (call ${}{})\n", sanitized, arg_wat_suffix));
            } else {
                wat.push_str(&format!(
                    "    (local.set $v{} (call ${}{}))\n",
                    dest.0, sanitized, arg_wat_suffix
                ));
            }
        } else {
            // Fallback / runtime-only call: do NOT push args onto operand stack,
            // as no callee exists to consume them. Just initialize dest to 0.
            body.push(0x42);
            encode_i64_leb128(0, body);
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (i64.const 0)) ;; fallback call: {}\n",
                dest.0, func
            ));
        }

        Ok(())
    }

    /// Lowers SSA block parameters to WebAssembly using standard operand-stack transfer.
    /// Arguments are pushed onto the operand stack and popped into parameter locals in reverse order.
    /// This models SSA φ-functions directly with zero temporary variables and no φ-elimination pass needed.
    pub(crate) fn transfer_block_params(
        target_params: &[crate::dmir::BlockParam],
        args: &[ValueId],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
    ) {
        if target_params.is_empty() || args.is_empty() {
            return;
        }
        // Push all arguments onto Wasm operand stack
        for arg in args {
            let arg_loc = local_map.get(arg).copied().unwrap_or(0);
            body.push(0x20); // local.get
            encode_u32_leb128(arg_loc, body);
        }
        // Pop into target block parameter locals in reverse order (direct SSA lowering)
        for (param, _) in target_params.iter().zip(args.iter()).rev() {
            let param_loc = local_map.get(&param.val).copied().unwrap_or(0);
            body.push(0x21); // local.set
            encode_u32_leb128(param_loc, body);
        }
        for (param, arg) in target_params.iter().zip(args.iter()) {
            wat.push_str(&format!(
                "    (local.set $v{} (local.get $v{})) ;; SSA block param transfer (no phi pass needed)\n",
                param.val.0, arg.0
            ));
        }
    }

    /// Compiles block terminators (Branch, CondBranch, Return, Unreachable).
    pub(crate) fn compile_terminator(
        terminator: &Terminator,
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        pc_local: Option<u32>,
        block_id_to_idx: &HashMap<BasicBlockId, u32>,
        func: &crate::dmir::Function,
    ) -> Result<(), String> {
        match terminator {
            Terminator::Return { value } => {
                if let Some(val_id) = value {
                    let val_loc = local_map.get(val_id).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(val_loc, body);
                }
                body.push(0x0F); // return
                wat.push_str(&format!(
                    "    (return{})\n",
                    value
                        .map(|v| format!(" (local.get $v{})", v.0))
                        .unwrap_or_default()
                ));
            }
            Terminator::Branch { target, args } => {
                let target_idx = block_id_to_idx.get(target).copied().unwrap_or(0);
                if let Some(pc_idx) = pc_local {
                    if let Some(target_block) = func.get_block(*target) {
                        Self::transfer_block_params(
                            &target_block.params,
                            args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41); // i32.const target_idx
                    encode_i32_leb128(target_idx as i32, body);
                    body.push(0x21); // local.set $pc
                    encode_u32_leb128(pc_idx, body);

                    wat.push_str(&format!("    (local.set $pc (i32.const {}))\n", target_idx));
                }
            }
            Terminator::CondBranch {
                cond,
                then_block,
                then_args,
                else_block,
                else_args,
            } => {
                let cond_loc = local_map.get(cond).copied().unwrap_or(0);
                let then_idx = block_id_to_idx.get(then_block).copied().unwrap_or(0);
                let else_idx = block_id_to_idx.get(else_block).copied().unwrap_or(0);

                if let Some(pc_idx) = pc_local {
                    body.push(0x20); // local.get cond
                    encode_u32_leb128(cond_loc, body);
                    body.push(0x50); // i64.eqz
                    body.push(0x04); // if
                    body.push(0x40); // void blocktype

                    // Else block (cond == 0)
                    if let Some(target_block) = func.get_block(*else_block) {
                        Self::transfer_block_params(
                            &target_block.params,
                            else_args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41);
                    encode_i32_leb128(else_idx as i32, body);
                    body.push(0x21);
                    encode_u32_leb128(pc_idx, body);

                    body.push(0x05); // else

                    // Then block (cond != 0)
                    if let Some(target_block) = func.get_block(*then_block) {
                        Self::transfer_block_params(
                            &target_block.params,
                            then_args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41);
                    encode_i32_leb128(then_idx as i32, body);
                    body.push(0x21);
                    encode_u32_leb128(pc_idx, body);

                    body.push(0x0B); // end

                    wat.push_str(&format!(
                        "    (if (i64.ne (local.get $v{}) (i64.const 0))\n      (then (local.set $pc (i32.const {})))\n      (else (local.set $pc (i32.const {}))))\n",
                        cond.0, then_idx, else_idx
                    ));
                }
            }
            Terminator::Unreachable => {
                body.push(0x00); // unreachable
                wat.push_str("    (unreachable)\n");
            }
        }
        Ok(())
    }
}
