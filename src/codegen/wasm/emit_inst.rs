use super::WasmEmitter;
use super::types::*;
use crate::dmir::{Inst, Module, ValueId};
use std::collections::HashMap;

impl WasmEmitter {
    /// Compiles a sequence of DMIR instructions to Wasm binary and WAT text.
    pub(crate) fn compile_instructions(
        instructions: &[Inst],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        var_map: &HashMap<String, u32>,
        value_types: &HashMap<ValueId, WasmValType>,
        import_fn_indices: &HashMap<String, u32>,
        defined_fn_indices: &HashMap<String, u32>,
        string_table: &HashMap<String, u32>,
        module: &Module,
        scratch_v128: u32,
    ) -> Result<(), String> {
        for inst in instructions {
            match inst {
                Inst::ConstInt { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    body.push(0x42); // i64.const
                    encode_i64_leb128(*value, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {}))\n",
                        dest.0, value
                    ));
                }
                Inst::ConstFloat { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    body.push(0x44); // f64.const
                    encode_f64(*value, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (f64.const {}))\n",
                        dest.0, value
                    ));
                }
                Inst::ConstBool { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let val = if *value { 1i64 } else { 0i64 };
                    body.push(0x42); // i64.const
                    encode_i64_leb128(val, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {}))\n",
                        dest.0, val
                    ));
                }
                Inst::ConstStr { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let offset = string_table.get(value).copied().unwrap_or(1024) as i64;
                    body.push(0x42); // i64.const
                    encode_i64_leb128(offset, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {})) ;; str: {:?}\n",
                        dest.0, offset, value
                    ));
                }
                Inst::FormatStr { dest, parts, .. } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let first_part = parts.first().map(|s| s.as_str()).unwrap_or("");
                    let offset = string_table.get(first_part).copied().unwrap_or(1024) as i64;
                    body.push(0x42); // i64.const
                    encode_i64_leb128(offset, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {})) ;; format_str\n",
                        dest.0, offset
                    ));
                }
                Inst::LoadVar { dest, name } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let var_loc = var_map.get(name).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(var_loc, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (local.get $var_{}))\n",
                        dest.0, name
                    ));
                }
                Inst::AssignVar { name, value } => {
                    let var_loc = var_map.get(name).copied().unwrap_or(0);
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(val_loc, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(var_loc, body);
                    wat.push_str(&format!(
                        "    (local.set $var_{} (local.get $v{}))\n",
                        name, value.0
                    ));
                }
                Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let left_loc = local_map.get(left).copied().unwrap_or(0);
                    let right_loc = local_map.get(right).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";
                    let is_simd_f32x4 = ty == "Float4" || ty == "Vector4";
                    let is_simd_i32x4 = ty == "Int4";

                    body.push(0x20); // local.get
                    encode_u32_leb128(left_loc, body);
                    body.push(0x20); // local.get
                    encode_u32_leb128(right_loc, body);

                    if is_simd_f32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0xFD);
                                encode_u32_leb128(228, body); // f32x4.add
                            }
                            "-" => {
                                body.push(0xFD);
                                encode_u32_leb128(229, body); // f32x4.sub
                            }
                            "*" => {
                                body.push(0xFD);
                                encode_u32_leb128(230, body); // f32x4.mul
                            }
                            "/" => {
                                body.push(0xFD);
                                encode_u32_leb128(231, body); // f32x4.div
                            }
                            _ => {
                                body.push(0xFD);
                                encode_u32_leb128(228, body);
                            }
                        }
                    } else if is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0xFD);
                                encode_u32_leb128(174, body); // i32x4.add
                            }
                            "-" => {
                                body.push(0xFD);
                                encode_u32_leb128(177, body); // i32x4.sub
                            }
                            "*" => {
                                body.push(0xFD);
                                encode_u32_leb128(181, body); // i32x4.mul
                            }
                            _ => {
                                body.push(0xFD);
                                encode_u32_leb128(174, body);
                            }
                        }
                    } else if is_float {
                        match op.as_str() {
                            "+" => body.push(0xA0), // f64.add
                            "-" => body.push(0xA1), // f64.sub
                            "*" => body.push(0xA2), // f64.mul
                            "/" => body.push(0xA3), // f64.div
                            "==" => {
                                body.push(0x61); // f64.eq
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "!=" => {
                                body.push(0x62); // f64.ne
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "<" => {
                                body.push(0x63); // f64.lt
                                body.push(0xAD);
                            }
                            "<=" => {
                                body.push(0x65); // f64.le
                                body.push(0xAD);
                            }
                            ">" => {
                                body.push(0x64); // f64.gt
                                body.push(0xAD);
                            }
                            ">=" => {
                                body.push(0x66); // f64.ge
                                body.push(0xAD);
                            }
                            _ => body.push(0xA0),
                        }
                    } else {
                        match op.as_str() {
                            "+" | "wrapping_+" | "saturating_+" => body.push(0x7C), // i64.add
                            "-" | "wrapping_-" | "saturating_-" => body.push(0x7D), // i64.sub
                            "*" | "wrapping_*" | "saturating_*" => body.push(0x7E), // i64.mul
                            "/" => body.push(0x7F),                                 // i64.div_s
                            "%" => body.push(0x81),                                 // i64.rem_s
                            "&" | "&&" => body.push(0x83),                          // i64.and
                            "|" | "||" => body.push(0x84),                          // i64.or
                            "^" => body.push(0x85),                                 // i64.xor
                            "<<" => body.push(0x86),                                // i64.shl
                            ">>" => body.push(0x87),                                // i64.shr_s
                            "==" => {
                                body.push(0x51); // i64.eq
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "!=" => {
                                body.push(0x52); // i64.ne
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "<" => {
                                body.push(0x53); // i64.lt_s
                                body.push(0xAD);
                            }
                            "<=" => {
                                body.push(0x57); // i64.le_s
                                body.push(0xAD);
                            }
                            ">" => {
                                body.push(0x55); // i64.gt_s
                                body.push(0xAD);
                            }
                            ">=" => {
                                body.push(0x59); // i64.ge_s
                                body.push(0xAD);
                            }
                            _ => body.push(0x7C),
                        }
                    }

                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);

                    if !is_float && !is_simd_f32x4 && !is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                            }
                            "-" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                            }
                            "*" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x7F); // i64.div_s
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                            }
                            "saturating_+" => {
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body); // local.set dest
                                body.push(0x0B); // end
                            }
                            "saturating_-" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body); // local.set dest
                                body.push(0x0B); // end
                            }
                            "saturating_*" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(-1, body);
                                body.push(0x51); // i64.eq
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x51); // i64.eq
                                body.push(0x83); // i64.and
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(-1, body);
                                body.push(0x51); // i64.eq
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x51); // i64.eq
                                body.push(0x83); // i64.and
                                body.push(0x84); // i64.or
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x05); // else
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x7F); // i64.div_s
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x42);
                                body.push(0x00);
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                            }
                            _ => {}
                        }
                    }

                    let op_str = if is_simd_f32x4 {
                        match op.as_str() {
                            "+" => "f32x4.add",
                            "-" => "f32x4.sub",
                            "*" => "f32x4.mul",
                            "/" => "f32x4.div",
                            _ => "f32x4.add",
                        }
                    } else if is_simd_i32x4 {
                        match op.as_str() {
                            "+" => "i32x4.add",
                            "-" => "i32x4.sub",
                            "*" => "i32x4.mul",
                            _ => "i32x4.add",
                        }
                    } else if is_float {
                        match op.as_str() {
                            "+" => "f64.add",
                            "-" => "f64.sub",
                            "*" => "f64.mul",
                            "/" => "f64.div",
                            "==" => "f64.eq",
                            "!=" => "f64.ne",
                            "<" => "f64.lt",
                            "<=" => "f64.le",
                            ">" => "f64.gt",
                            ">=" => "f64.ge",
                            _ => "f64.add",
                        }
                    } else {
                        match op.as_str() {
                            "+" | "wrapping_+" | "saturating_+" => "i64.add",
                            "-" | "wrapping_-" | "saturating_-" => "i64.sub",
                            "*" | "wrapping_*" | "saturating_*" => "i64.mul",
                            "/" => "i64.div_s",
                            "%" => "i64.rem_s",
                            "&" | "&&" => "i64.and",
                            "|" | "||" => "i64.or",
                            "^" => "i64.xor",
                            "<<" => "i64.shl",
                            ">>" => "i64.shr_s",
                            "==" => "i64.eq",
                            "!=" => "i64.ne",
                            "<" => "i64.lt_s",
                            "<=" => "i64.le_s",
                            ">" => "i64.gt_s",
                            ">=" => "i64.ge_s",
                            _ => "i64.add",
                        }
                    };

                    let is_relational =
                        matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
                    if is_relational {
                        wat.push_str(&format!(
                            "    (local.set $v{} (i64.extend_i32_u ({} (local.get $v{}) (local.get $v{}))))\n",
                            dest.0, op_str, left.0, right.0
                        ));
                    } else {
                        wat.push_str(&format!(
                            "    (local.set $v{} ({} (local.get $v{}) (local.get $v{})))\n",
                            dest.0, op_str, left.0, right.0
                        ));
                    }

                    if !is_float && !is_simd_f32x4 && !is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (unreachable)))\n",
                                    dest.0, left.0, dest.0, right.0
                                ));
                            }
                            "-" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (unreachable)))\n",
                                    left.0, right.0, dest.0, left.0
                                ));
                            }
                            "*" => {
                                wat.push_str(&format!(
                                    "    (if (i64.ne (local.get $v{}) (i64.const 0)) (then (if (i64.ne (i64.div_s (local.get $v{}) (local.get $v{})) (local.get $v{})) (then (unreachable)))))\n",
                                    left.0, dest.0, left.0, right.0
                                ));
                            }
                            "saturating_+" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (local.get $v{}) (i64.const 0))))))\n",
                                    dest.0, left.0, dest.0, right.0, dest.0, left.0
                                ));
                            }
                            "saturating_-" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (local.get $v{}) (i64.const 0))))))\n",
                                    left.0, right.0, dest.0, left.0, dest.0, left.0
                                ));
                            }
                            "saturating_*" => {
                                wat.push_str(&format!(
                                    "    (if (i64.or (i64.and (i64.eq (local.get $v{}) (i64.const -1)) (i64.eq (local.get $v{}) (i64.const -9223372036854775808))) (i64.and (i64.eq (local.get $v{}) (i64.const -1)) (i64.eq (local.get $v{}) (i64.const -9223372036854775808)))) (then (local.set $v{} (i64.const 9223372036854775807))) (else (if (i64.ne (local.get $v{}) (i64.const 0)) (then (if (i64.ne (i64.div_s (local.get $v{}) (local.get $v{})) (local.get $v{})) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (i64.xor (local.get $v{}) (local.get $v{})) (i64.const 0))))))))))\n",
                                    left.0, right.0, right.0, left.0, dest.0, left.0, dest.0, left.0, right.0, dest.0, left.0, right.0
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                Inst::UnOp {
                    dest,
                    op,
                    operand,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let op_loc = local_map.get(operand).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";

                    match op.as_str() {
                        "-" if is_float => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x9A); // f64.neg
                        }
                        "-" => {
                            body.push(0x42); // i64.const 0
                            encode_i64_leb128(0, body);
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x7D); // i64.sub
                        }
                        "!" => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x50); // i64.eqz
                            body.push(0xAD); // i64.extend_i32_u
                        }
                        "~" => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x42); // i64.const -1
                            encode_i64_leb128(-1, body);
                            body.push(0x85); // i64.xor
                        }
                        _ => {
                            body.push(0x20);
                            encode_u32_leb128(op_loc, body);
                        }
                    }
                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);
                    if op == "copy" || op == "await" {
                        wat.push_str(&format!(
                            "    (local.set $v{} (local.get $v{}))\n",
                            dest.0, operand.0
                        ));
                    } else {
                        wat.push_str(&format!(
                            "    (local.set $v{} (unop_{} (local.get $v{})))\n",
                            dest.0, op, operand.0
                        ));
                    }
                }
                Inst::Call {
                    dest, func, args, ..
                } => {
                    Self::compile_call(
                        *dest,
                        func,
                        args,
                        body,
                        wat,
                        local_map,
                        value_types,
                        import_fn_indices,
                        defined_fn_indices,
                        scratch_v128,
                    )?;
                }
                Inst::MethodCall {
                    dest,
                    object,
                    method,
                    args,
                    ..
                } => {
                    let mut full_args = vec![*object];
                    full_args.extend(args.iter().copied());
                    let runtime_fn = match method.as_str() {
                        "push" | "append" => "datara_rt_list_append",
                        "get" => "datara_rt_list_get",
                        "set" => "datara_rt_list_set",
                        "len" | "count" => "datara_rt_list_len",
                        "byte_len" => "datara_rt_str_len",
                        "char_len" => "datara_rt_str_chars",
                        "byte_at" => "datara_rt_str_byte_at",
                        "char_at" => "datara_rt_str_char_at",
                        "insert" => "datara_rt_map_insert",
                        _ => method.as_str(),
                    };
                    Self::compile_call(
                        *dest,
                        runtime_fn,
                        &full_args,
                        body,
                        wat,
                        local_map,
                        value_types,
                        import_fn_indices,
                        defined_fn_indices,
                        scratch_v128,
                    )?;
                }
                Inst::StructInit {
                    dest,
                    class_name,
                    fields,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    // Allocate fields.len() * 8 bytes via datara:rt/alloc
                    if let Some(&alloc_idx) = import_fn_indices.get("datara:rt/alloc") {
                        let size = (fields.len().max(1).saturating_mul(8)) as i64;
                        body.push(0x42); // i64.const
                        encode_i64_leb128(size, body);
                        body.push(0x10); // call
                        encode_u32_leb128(alloc_idx, body);
                        body.push(0x21); // local.set dest
                        encode_u32_leb128(dest_loc, body);

                        // Store fields into linear memory at offset i * 8
                        for (i, (_, val_id)) in fields.iter().enumerate() {
                            let val_loc = local_map.get(val_id).copied().unwrap_or(0);
                            let byte_offset = (i.saturating_mul(8)) as u32;
                            body.push(0x20); // local.get dest
                            encode_u32_leb128(dest_loc, body);
                            body.push(0xA7); // i32.wrap_i64 (memory addr)
                            body.push(0x20); // local.get val
                            encode_u32_leb128(val_loc, body);
                            body.push(0x37); // i64.store
                            encode_u32_leb128(3, body); // alignment 2^3 = 8
                            encode_u32_leb128(byte_offset, body); // offset
                        }
                    } else {
                        // Fallback stub if alloc not imported: set to zero
                        body.push(0x42);
                        encode_i64_leb128(0, body);
                        body.push(0x21);
                        encode_u32_leb128(dest_loc, body);
                    }
                    wat.push_str(&format!(
                        "    (local.set $v{} (struct_init ${}))\n",
                        dest.0, class_name
                    ));
                }
                Inst::GetField {
                    dest,
                    object,
                    field,
                    ..
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let obj_loc = local_map.get(object).copied().unwrap_or(0);

                    // Compute field offset deterministically
                    let mut sorted_classes: Vec<&String> = module.class_fields.keys().collect();
                    sorted_classes.sort();
                    let field_idx = sorted_classes
                        .iter()
                        .find_map(|cls| module.class_fields[*cls].iter().position(|f| f == field))
                        .unwrap_or(0);
                    let byte_offset = (field_idx.saturating_mul(8)) as u32;

                    body.push(0x20); // local.get obj
                    encode_u32_leb128(obj_loc, body);
                    body.push(0xA7); // i32.wrap_i64
                    body.push(0x29); // i64.load
                    encode_u32_leb128(3, body); // align 8
                    encode_u32_leb128(byte_offset, body); // offset
                    body.push(0x21); // local.set dest
                    encode_u32_leb128(dest_loc, body);

                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.load offset={} (i32.wrap_i64 (local.get $v{}))))\n",
                        dest.0,
                        byte_offset,
                        object.0
                    ));
                }
                Inst::SetField {
                    object,
                    field,
                    value,
                } => {
                    let obj_loc = local_map.get(object).copied().unwrap_or(0);
                    let val_loc = local_map.get(value).copied().unwrap_or(0);

                    let mut sorted_classes: Vec<&String> = module.class_fields.keys().collect();
                    sorted_classes.sort();
                    let field_idx = sorted_classes
                        .iter()
                        .find_map(|cls| module.class_fields[*cls].iter().position(|f| f == field))
                        .unwrap_or(0);
                    let byte_offset = (field_idx.saturating_mul(8)) as u32;

                    body.push(0x20); // local.get obj
                    encode_u32_leb128(obj_loc, body);
                    body.push(0xA7); // i32.wrap_i64
                    body.push(0x20); // local.get val
                    encode_u32_leb128(val_loc, body);
                    body.push(0x37); // i64.store
                    encode_u32_leb128(3, body);
                    encode_u32_leb128(byte_offset, body);

                    wat.push_str(&format!(
                        "    (i64.store offset={} (i32.wrap_i64 (local.get $v{})) (local.get $v{}))\n",
                        byte_offset,
                        object.0,
                        value.0
                    ));
                }
                Inst::Out { value } => {
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    if let Some(&print_idx) = import_fn_indices.get("datara:rt/print") {
                        body.push(0x20); // local.get val
                        encode_u32_leb128(val_loc, body);
                        body.push(0x10); // call
                        encode_u32_leb128(print_idx, body);
                    }
                    wat.push_str(&format!("    (call $print (local.get $v{}))\n", value.0));
                }
                Inst::Err { value } => {
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    if let Some(&err_idx) = import_fn_indices.get("datara:rt/err") {
                        body.push(0x20); // local.get val
                        encode_u32_leb128(val_loc, body);
                        body.push(0x10); // call
                        encode_u32_leb128(err_idx, body);
                    }
                    wat.push_str(&format!("    (call $err (local.get $v{}))\n", value.0));
                }
                Inst::Select {
                    dest,
                    cond,
                    then_val,
                    else_val,
                    ..
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let cond_loc = local_map.get(cond).copied().unwrap_or(0);
                    let then_loc = local_map.get(then_val).copied().unwrap_or(0);
                    let else_loc = local_map.get(else_val).copied().unwrap_or(0);

                    body.push(0x20); // local.get then_val
                    encode_u32_leb128(then_loc, body);
                    body.push(0x20); // local.get else_val
                    encode_u32_leb128(else_loc, body);
                    body.push(0x20); // local.get cond
                    encode_u32_leb128(cond_loc, body);
                    body.push(0x50); // i64.eqz (returns i32: 1 if cond == 0, 0 if cond != 0)
                    body.push(0x45); // i32.eqz (returns i32: 1 if cond != 0, 0 if cond == 0)
                    body.push(0x1B); // select
                    body.push(0x21); // local.set dest
                    encode_u32_leb128(dest_loc, body);

                    wat.push_str(&format!(
                        "    (local.set $v{} (select (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                        dest.0, then_val.0, else_val.0, cond.0
                    ));
                }
                Inst::Decide {
                    dest,
                    arms,
                    else_val,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";

                    // 1. Initialize dest with else_val or 0
                    if let Some(e_val) = else_val {
                        let e_loc = local_map.get(e_val).copied().unwrap_or(0);
                        body.push(0x20); // local.get e_loc
                        encode_u32_leb128(e_loc, body);
                    } else if is_float {
                        body.push(0x44); // f64.const 0.0
                        body.extend_from_slice(&0.0f64.to_le_bytes());
                    } else {
                        body.push(0x42); // i64.const 0
                        encode_i64_leb128(0, body);
                    }
                    body.push(0x21); // local.set dest_loc
                    encode_u32_leb128(dest_loc, body);

                    // 2. Fold arms in reverse with select
                    for (arm_cond, arm_val) in arms.iter().rev() {
                        let arm_val_loc = local_map.get(arm_val).copied().unwrap_or(0);
                        let arm_cond_loc = local_map.get(arm_cond).copied().unwrap_or(0);

                        body.push(0x20); // local.get arm_val
                        encode_u32_leb128(arm_val_loc, body);
                        body.push(0x20); // local.get dest (current fallback)
                        encode_u32_leb128(dest_loc, body);
                        body.push(0x20); // local.get arm_cond
                        encode_u32_leb128(arm_cond_loc, body);
                        body.push(0x50); // i64.eqz
                        body.push(0x45); // i32.eqz (1 if cond != 0, 0 if cond == 0)
                        body.push(0x1B); // select
                        body.push(0x21); // local.set dest
                        encode_u32_leb128(dest_loc, body);
                    }

                    wat.push_str(&format!("    (local.set $v{} (decide ...))\n", dest.0));
                }
                Inst::InlineAsm { .. } => {
                    return Err(
                        "Code generation failed: [E0902] inline assembly is not supported on WASM backend; use --llvm backend instead".to_string(),
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }
}
