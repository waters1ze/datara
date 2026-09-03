use crate::ast::Program;
use crate::codegen::CodegenBackend;
use crate::codegen::linker::{compile_with_clang, find_clang};
use crate::codegen::target::{CallingConvention, TargetInfo};
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use crate::types::TypeChecker;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Dedicated LLVM IR code emitter for Datara / Forgen.
pub struct LlvmEmitter<'a> {
    pub target: &'a TargetInfo,
}

impl<'a> LlvmEmitter<'a> {
    pub fn new(target: &'a TargetInfo) -> Self {
        Self { target }
    }

    /// Map Datara / DMIR type names to LLVM IR types.
    pub fn dmir_type_to_llvm(&self, ty: &str) -> &'static str {
        match ty {
            "Int" => "i64",
            "Float" => "double",
            "Bool" => "i64",
            "Str" => "ptr",
            "Unit" => "void",
            _ => "ptr",
        }
    }

    /// Escape string content for LLVM IR string literals: `c"...\00"`.
    /// Returns the escaped string and the total byte length including null terminator.
    pub fn escape_llvm_string(s: &str) -> (String, usize) {
        let mut out = String::new();
        let mut bytes_count = 0;
        for b in s.bytes() {
            bytes_count += 1;
            match b {
                b'\\' => out.push_str("\\5C"),
                b'"' => out.push_str("\\22"),
                b'\n' => out.push_str("\\0A"),
                b'\r' => out.push_str("\\0D"),
                b'\t' => out.push_str("\\09"),
                0 => out.push_str("\\00"),
                32..=126 => out.push(b as char),
                _ => out.push_str(&format!("\\{:02X}", b)),
            }
        }
        bytes_count += 1; // null terminator
        out.push_str("\\00");
        (out, bytes_count)
    }

    /// Emit complete LLVM IR module from DMIR Module.
    pub fn emit_module(&self, module: &Module, _program: &Program, _types: &TypeChecker) -> String {
        let mut ir = String::new();

        ir.push_str(
            "; ============================================================================\n",
        );
        ir.push_str("; Auto-generated LLVM IR by Datara Forgen Compiler v1.0\n");
        ir.push_str(&format!(
            "; Target Triple: {}\n",
            self.target.triple_string()
        ));
        ir.push_str("; Architecture: x86_64 / AArch64 Native Backend\n");
        ir.push_str(
            "; ============================================================================\n\n",
        );

        // Target Layout & Triple
        match self.target.calling_convention {
            CallingConvention::WindowsFastcall => {
                ir.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                ir.push_str("target triple = \"x86_64-pc-windows-msvc\"\n\n");
            }
            CallingConvention::SystemV => {
                ir.push_str("target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                ir.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
            }
            CallingConvention::Aarch64Standard => {
                ir.push_str("target datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128\"\n");
                ir.push_str("target triple = \"arm64-apple-macosx\"\n\n");
            }
        }

        // 1. Collect and emit string literals
        let mut string_literal_map: HashMap<String, usize> = HashMap::new();
        let mut str_id = 0;

        // Always register empty string and colon
        let mut register_str = |s: &str| {
            if !string_literal_map.contains_key(s) {
                string_literal_map.insert(s.to_string(), str_id);
                str_id += 1;
            }
        };

        register_str("");
        register_str(":");

        for func in module.functions.values() {
            for b in &func.blocks {
                for inst in &b.instructions {
                    match inst {
                        Inst::ConstStr { value, .. } => register_str(value),
                        Inst::FormatStr { parts, .. } => {
                            for p in parts {
                                register_str(p);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        ir.push_str("; --- Global String Literals ---\n");
        let mut sorted_strings: Vec<(&String, &usize)> = string_literal_map.iter().collect();
        sorted_strings.sort_by_key(|&(_, id)| *id);
        for (content, id) in sorted_strings {
            let (escaped, len) = Self::escape_llvm_string(content);
            ir.push_str(&format!(
                "@.str.{} = private unnamed_addr constant [{} x i8] c\"{}\", align 1\n",
                id, len, escaped
            ));
        }
        ir.push('\n');

        // 2. Declare external Datara runtime functions
        ir.push_str("; --- Datara Standard Runtime Declarations ---\n");
        ir.push_str("declare void @datara_rt_out_int(i64)\n");
        ir.push_str("declare void @datara_rt_out_float(double)\n");
        ir.push_str("declare void @datara_rt_out_bool(i64)\n");
        ir.push_str("declare void @datara_rt_out_str(ptr)\n");
        ir.push_str("declare void @datara_rt_err(ptr)\n");
        ir.push_str("declare void @datara_rt_print_str(ptr)\n");
        ir.push_str("declare void @datara_rt_print_int(i64)\n");
        ir.push_str("declare void @datara_rt_print_float(double)\n");
        ir.push_str("declare void @datara_rt_print_bool(i64)\n");
        ir.push_str("declare void @datara_rt_print_space()\n");
        ir.push_str("declare void @datara_rt_print_newline()\n");
        ir.push_str("declare void @datara_rt_flush()\n");
        ir.push_str("declare void @datara_rt_print_list(ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_concat(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_concat_3(ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_concat_4(ptr, ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_concat_5(ptr, ptr, ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_format_str_i64_str_i64(ptr, i64, ptr, i64)\n");
        ir.push_str("declare ptr @datara_rt_int_to_str(i64)\n");
        ir.push_str("declare ptr @datara_rt_bool_to_str(i64)\n");
        ir.push_str("declare ptr @datara_rt_float_to_str(double)\n");
        ir.push_str("declare i64 @datara_rt_str_len(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_eq(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_trim(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_to_int(ptr)\n");
        ir.push_str("declare double @datara_rt_str_to_float(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_contains(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_starts_with(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_ends_with(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_index_of(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_substring(ptr, i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_file_write(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_file_read(ptr)\n");
        ir.push_str("declare i64 @datara_rt_file_append(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_file_exists(ptr)\n");
        ir.push_str("declare ptr @datara_rt_list_create(i64)\n");
        ir.push_str("declare ptr @datara_rt_list_append(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_get(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_len(ptr)\n");
        ir.push_str("declare i64 @datara_rt_now_ms()\n");
        ir.push_str("declare i64 @datara_rt_now_precise_ms()\n");
        ir.push_str("declare void @datara_rt_sleep(i64)\n");
        ir.push_str("declare ptr @datara_rt_http_get()\n");
        // Fast Math
        ir.push_str("declare double @datara_rt_math_sqrt(double)\n");
        ir.push_str("declare double @datara_rt_math_pow(double, double)\n");
        ir.push_str("declare double @datara_rt_math_abs(double)\n");
        ir.push_str("declare double @datara_rt_math_sin(double)\n");
        ir.push_str("declare double @datara_rt_math_cos(double)\n");
        ir.push_str("declare double @datara_rt_math_tan(double)\n");
        ir.push_str("declare double @datara_rt_math_floor(double)\n");
        ir.push_str("declare double @datara_rt_math_ceil(double)\n");
        ir.push_str("declare double @datara_rt_math_round(double)\n");
        ir.push_str("declare double @datara_rt_math_min(double, double)\n");
        ir.push_str("declare double @datara_rt_math_max(double, double)\n");
        ir.push_str("declare double @datara_rt_math_hypot(double, double)\n");
        ir.push_str("declare i64 @datara_rt_math_min_int(i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_math_max_int(i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_math_abs_int(i64)\n");
        ir.push_str("declare i64 @datara_rt_math_ctz(i64)\n");
        ir.push_str("declare i64 @datara_rt_math_shr(i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_math_shl(i64, i64)\n");
        ir.push_str("declare i64 @llvm.cttz.i64(i64, i1)\n");
        ir.push_str("declare ptr @malloc(i64)\n");
        ir.push_str("declare void @free(ptr)\n");
        ir.push_str("declare <4 x float> @llvm.minnum.v4f32(<4 x float>, <4 x float>)\n");
        ir.push_str("declare <4 x float> @llvm.maxnum.v4f32(<4 x float>, <4 x float>)\n\n");

        // 2b. Declare user-declared extern "C" functions (FFI), mirroring the
        // Cranelift backend so FFI programs compile on both backends.
        for (ef_name, (ef_params, ef_ret)) in &module.extern_functions {
            let ret = if ef_ret == "Unit" || ef_ret == "Never" {
                "void".to_string()
            } else {
                self.dmir_type_to_llvm(ef_ret).to_string()
            };
            let params = ef_params
                .iter()
                .map(|p| self.dmir_type_to_llvm(p))
                .collect::<Vec<_>>()
                .join(", ");
            ir.push_str(&format!("declare {} @{}({})\n", ret, ef_name, params));
        }
        if !module.extern_functions.is_empty() {
            ir.push('\n');
        }

        // 3. Emit all functions
        for f in module.functions.values() {
            ir.push_str(&self.emit_function(f, module, &string_literal_map));
            ir.push('\n');
        }

        // 4. Emit Loop Vectorization & Unroll Metadata
        let vector_width = if self
            .target
            .vector_support
            .contains(&crate::codegen::target::VectorExtension::Avx2)
            || self
                .target
                .vector_support
                .contains(&crate::codegen::target::VectorExtension::Avx512)
        {
            8
        } else {
            4
        };
        ir.push_str("!0 = distinct !{!0, !1, !2, !3}\n");
        ir.push_str("!1 = !{!\"llvm.loop.vectorize.enable\", i1 1}\n");
        ir.push_str(&format!(
            "!2 = !{{!\"llvm.loop.vectorize.width\", i32 {}}}\n",
            vector_width
        ));
        ir.push_str("!3 = !{!\"llvm.loop.unroll.enable\", i1 1}\n\n");

        ir
    }

    /// Emit a single function to LLVM IR.
    pub fn emit_function(
        &self,
        f: &Function,
        module: &Module,
        strings: &HashMap<String, usize>,
    ) -> String {
        let mut out = String::new();
        let is_main = f.name == "main";

        // Signature
        let ret_type = if is_main {
            "i32".to_string()
        } else if f.return_type == "Unit" {
            "void".to_string()
        } else {
            self.dmir_type_to_llvm(&f.return_type).to_string()
        };

        let params_sig = if is_main {
            "".to_string()
        } else {
            f.params
                .iter()
                .map(|(_, p_ty, p_val)| format!("{} %v{}", self.dmir_type_to_llvm(p_ty), p_val.0))
                .collect::<Vec<_>>()
                .join(", ")
        };

        out.push_str(&format!(
            "define {} @{}({}) {{\n",
            ret_type, f.name, params_sig
        ));

        // Track local variable types and allocas
        let mut local_vars: HashSet<String> = HashSet::new();
        let mut struct_inits: Vec<(ValueId, usize)> = Vec::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::AssignVar { name, .. } = inst {
                    local_vars.insert(name.clone());
                } else if let Inst::StructInit { dest, fields, .. } = inst {
                    let byte_size = (fields.len() * 8).max(8);
                    struct_inits.push((*dest, byte_size));
                }
            }
        }

        // Value types tracking within the function
        let mut value_types: HashMap<ValueId, &'static str> = HashMap::new();
        for (_, p_ty, p_val) in &f.params {
            value_types.insert(*p_val, self.dmir_type_to_llvm(p_ty));
        }

        // Collect incoming jump edges for basic block PHI nodes
        let mut incoming_edges: HashMap<BasicBlockId, Vec<(BasicBlockId, Vec<ValueId>)>> =
            HashMap::new();
        for b in &f.blocks {
            match &b.terminator {
                Terminator::Branch { target, args } => {
                    incoming_edges
                        .entry(*target)
                        .or_default()
                        .push((b.id, args.clone()));
                }
                Terminator::CondBranch {
                    then_block,
                    then_args,
                    else_block,
                    else_args,
                    ..
                } => {
                    incoming_edges
                        .entry(*then_block)
                        .or_default()
                        .push((b.id, then_args.clone()));
                    incoming_edges
                        .entry(*else_block)
                        .or_default()
                        .push((b.id, else_args.clone()));
                }
                _ => {}
            }
        }

        // Allocate local variables in entry block if any
        let has_allocas =
            !local_vars.is_empty() || !f.params.is_empty() || !struct_inits.is_empty();
        if has_allocas {
            out.push_str("entry_allocas:\n");
            for (pname, pty, pval) in &f.params {
                let llvm_pty = self.dmir_type_to_llvm(pty);
                out.push_str(&format!(
                    "  %var_{} = alloca {}, align 8\n",
                    pname, llvm_pty
                ));
                out.push_str(&format!(
                    "  store {} %v{}, ptr %var_{}, align 8\n",
                    llvm_pty, pval.0, pname
                ));
            }
            for vname in &local_vars {
                if !f.params.iter().any(|(p, _, _)| p == vname) {
                    out.push_str(&format!("  %var_{} = alloca [16 x i8], align 16\n", vname));
                }
            }
            for (s_id, s_size) in &struct_inits {
                out.push_str(&format!(
                    "  %v{} = alloca [{} x i8], align 8\n",
                    s_id.0, s_size
                ));
            }
            if let Some(first_block) = f.blocks.first() {
                out.push_str(&format!("  br label %bb{}\n\n", first_block.id.0));
            }
        }

        let mut var_types: HashMap<String, &'static str> = HashMap::new();
        for (pname, pty, _) in &f.params {
            var_types.insert(pname.clone(), self.dmir_type_to_llvm(pty));
        }

        // Emit blocks
        for block in &f.blocks {
            out.push_str(&format!("bb{}:\n", block.id.0));

            // Emit PHIs for block parameters if present
            if !block.params.is_empty()
                && block.id != f.entry_block
                && let Some(preds) = incoming_edges.get(&block.id)
            {
                for (param_idx, param) in block.params.iter().enumerate() {
                    let param_ty = self.dmir_type_to_llvm(&param.ty);
                    value_types.insert(param.val, param_ty);

                    let phi_incoming = preds
                        .iter()
                        .filter_map(|(pred_id, args)| {
                            args.get(param_idx)
                                .map(|arg_val| format!("[ %v{}, %bb{} ]", arg_val.0, pred_id.0))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");

                    if !phi_incoming.is_empty() {
                        out.push_str(&format!(
                            "  %v{} = phi {} {}\n",
                            param.val.0, param_ty, phi_incoming
                        ));
                    }
                }
            }

            // Emit instructions
            for inst in &block.instructions {
                self.emit_instruction(
                    inst,
                    module,
                    strings,
                    &mut value_types,
                    &mut var_types,
                    &mut out,
                );
            }

            // Emit terminator
            match &block.terminator {
                Terminator::Branch { target, .. } => {
                    if target.0 <= block.id.0 {
                        out.push_str(&format!("  br label %bb{}, !llvm.loop !0\n", target.0));
                    } else {
                        out.push_str(&format!("  br label %bb{}\n", target.0));
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_block,
                    else_block,
                    ..
                } => {
                    let cmp_reg = format!("%c_{}_{}", block.id.0, cond.0);
                    out.push_str(&format!("  {} = icmp ne i64 %v{}, 0\n", cmp_reg, cond.0));
                    if then_block.0 <= block.id.0 || else_block.0 <= block.id.0 {
                        out.push_str(&format!(
                            "  br i1 {}, label %bb{}, label %bb{}, !llvm.loop !0\n",
                            cmp_reg, then_block.0, else_block.0
                        ));
                    } else {
                        out.push_str(&format!(
                            "  br i1 {}, label %bb{}, label %bb{}\n",
                            cmp_reg, then_block.0, else_block.0
                        ));
                    }
                }
                Terminator::Return { value } => {
                    if is_main {
                        out.push_str("  ret i32 0\n");
                    } else if let Some(v) = value {
                        let rty = value_types.get(v).copied().unwrap_or("i64");
                        out.push_str(&format!("  ret {} %v{}\n", rty, v.0));
                    } else {
                        out.push_str("  ret void\n");
                    }
                }
                Terminator::Unreachable => {
                    out.push_str("  unreachable\n");
                }
            }
        }

        out.push_str("}\n");
        out
    }

    fn emit_instruction(
        &self,
        inst: &Inst,
        module: &Module,
        strings: &HashMap<String, usize>,
        value_types: &mut HashMap<ValueId, &'static str>,
        var_types: &mut HashMap<String, &'static str>,
        out: &mut String,
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
                let align = if vty == "<4 x float>" { 16 } else { 8 };
                out.push_str(&format!(
                    "  %v{} = load {}, ptr %var_{}, align {}\n",
                    dest.0, vty, name, align
                ));
            }
            Inst::AssignVar { name, value } => {
                let vty = value_types.get(value).copied().unwrap_or("i64");
                var_types.insert(name.clone(), vty);
                let align = if vty == "<4 x float>" { 16 } else { 8 };
                out.push_str(&format!(
                    "  store {} %v{}, ptr %var_{}, align {}\n",
                    vty, value.0, name, align
                ));
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
                        out.push_str(&format!(
                            "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                            tmp, left.0
                        ));
                        tmp
                    } else {
                        format!("%v{}", left.0)
                    };
                    let right_s = if r_ty != "ptr" {
                        let tmp = format!("%str_conv_r_{}", dest.0);
                        out.push_str(&format!(
                            "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                            tmp, right.0
                        ));
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
                                "  %v{} = add i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "-" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = sub i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "*" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = mul i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "/" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = sdiv i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "%" => {
                            value_types.insert(*dest, "i64");
                            out.push_str(&format!(
                                "  %v{} = srem i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            value_types.insert(*dest, "i64");
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
                            out.push_str(&format!(
                                "  %v{} = and i64 %v{}, %v{}\n",
                                dest.0, left.0, right.0
                            ));
                        }
                        "|" | "||" => {
                            value_types.insert(*dest, "i64");
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
                    out.push_str(&format!("  %v{} = xor i64 %v{}, 1\n", dest.0, operand.0));
                } else if op == "copy" {
                    let oty = value_types.get(operand).copied().unwrap_or("i64");
                    value_types.insert(*dest, oty);
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
                        let e0 = format!("%e0_{}", dest.0);
                        let e1 = format!("%e1_{}", dest.0);
                        let e2 = format!("%e2_{}", dest.0);
                        let e3 = format!("%e3_{}", dest.0);
                        out.push_str(&format!(
                            "  {} = extractelement <4 x float> {}, i32 0\n",
                            e0, mul_vec
                        ));
                        out.push_str(&format!(
                            "  {} = extractelement <4 x float> {}, i32 1\n",
                            e1, mul_vec
                        ));
                        out.push_str(&format!(
                            "  {} = extractelement <4 x float> {}, i32 2\n",
                            e2, mul_vec
                        ));
                        out.push_str(&format!(
                            "  {} = extractelement <4 x float> {}, i32 3\n",
                            e3, mul_vec
                        ));
                        let s0 = format!("%s0_{}", dest.0);
                        let s1 = format!("%s1_{}", dest.0);
                        let s = format!("%s_{}", dest.0);
                        out.push_str(&format!("  {} = fadd float {}, {}\n", s0, e0, e1));
                        out.push_str(&format!("  {} = fadd float {}, {}\n", s1, e2, e3));
                        out.push_str(&format!("  {} = fadd float {}, {}\n", s, s0, s1));
                        out.push_str(&format!("  %v{} = fpext float {} to double\n", dest.0, s));
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
                    "math_hypot" => "datara_rt_math_hypot",
                    "math_min_int" => "datara_rt_math_min_int",
                    "math_max_int" => "datara_rt_math_max_int",
                    "math_abs_int" => "datara_rt_math_abs_int",
                    "sleep" => "datara_rt_sleep",
                    "now" => "datara_rt_now_ms",
                    "now_ms" => "datara_rt_now_ms",
                    "now_precise_ms" => "datara_rt_now_precise_ms",
                    "file_write" => "datara_rt_file_write",
                    "file_read" => "datara_rt_file_read",
                    "file_append" => "datara_rt_file_append",
                    "file_exists" => "datara_rt_file_exists",
                    "str_len" => "datara_rt_str_len",
                    "str_trim" => "datara_rt_str_trim",
                    "str_to_int" => "datara_rt_str_to_int",
                    "str_contains" => "datara_rt_str_contains",
                    "str_starts_with" => "datara_rt_str_starts_with",
                    "str_ends_with" => "datara_rt_str_ends_with",
                    "str_index_of" => "datara_rt_str_index_of",
                    other => other,
                };

                let is_str_concat = actual_func.starts_with("datara_rt_str_concat");
                let mut converted_args = Vec::new();
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

                if ret_ty == "void" {
                    out.push_str(&format!("  call void @{}({})\n", actual_func, args_str));
                } else {
                    out.push_str(&format!(
                        "  %v{} = call {} @{}({})\n",
                        dest.0, ret_ty, actual_func, args_str
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
                let ret_ty = self.dmir_type_to_llvm(ty);
                value_types.insert(*dest, ret_ty);

                let actual_func = if module.functions.contains_key(method) {
                    method.clone()
                } else if let Some(k) = module
                    .functions
                    .keys()
                    .find(|k| k.ends_with(&format!("_{}", method)))
                {
                    k.clone()
                } else {
                    method.clone()
                };

                let mut all_args = vec![format!("ptr %v{}", object.0)];
                for a in args {
                    let aty = value_types.get(a).copied().unwrap_or("i64");
                    all_args.push(format!("{} %v{}", aty, a.0));
                }
                let args_str = all_args.join(", ");

                if ret_ty == "void" {
                    out.push_str(&format!("  call void @{}({})\n", actual_func, args_str));
                } else {
                    out.push_str(&format!(
                        "  %v{} = call {} @{}({})\n",
                        dest.0, ret_ty, actual_func, args_str
                    ));
                }
            }
            Inst::StructInit {
                dest,
                class_name: _,
                fields,
            } => {
                value_types.insert(*dest, "ptr");
                for (idx, (_, val_id)) in fields.iter().enumerate() {
                    let f_ty = value_types.get(val_id).copied().unwrap_or("i64");
                    let gep_reg = format!("%gep_{}_{}", dest.0, idx);
                    out.push_str(&format!(
                        "  {} = getelementptr inbounds i8, ptr %v{}, i64 {}\n",
                        gep_reg,
                        dest.0,
                        idx * 8
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

                let offset = self.find_field_offset(module, field);
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
                let offset = self.find_field_offset(module, field);
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
            Inst::Err { value } => {
                out.push_str(&format!("  call void @datara_rt_err(ptr %v{})\n", value.0));
            }
            Inst::FormatStr {
                dest,
                parts,
                values,
            } => {
                value_types.insert(*dest, "ptr");
                if parts.len() == 1 && values.is_empty() {
                    let s_id = strings.get(&parts[0]).copied().unwrap_or(0);
                    out.push_str(&format!(
                        "  %v{} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                        dest.0, s_id
                    ));
                } else if parts.len() == 2 && values.len() == 1 {
                    let p0_id = strings.get(&parts[0]).copied().unwrap_or(0);
                    let p1_id = strings.get(&parts[1]).copied().unwrap_or(0);
                    let str_val = format!("%fmt_s_{}", values[0].0);
                    let val_ty = value_types.get(&values[0]).copied().unwrap_or("i64");
                    if val_ty == "ptr" {
                        out.push_str(&format!(
                            "  {} = bitcast ptr %v{} to ptr\n",
                            str_val, values[0].0
                        ));
                    } else if val_ty == "double" {
                        out.push_str(&format!(
                            "  {} = call ptr @datara_rt_float_to_str(double %v{})\n",
                            str_val, values[0].0
                        ));
                    } else {
                        out.push_str(&format!(
                            "  {} = call ptr @datara_rt_int_to_str(i64 %v{})\n",
                            str_val, values[0].0
                        ));
                    }
                    let p0_ptr = format!("%fmt_p0_{}", dest.0);
                    let p1_ptr = format!("%fmt_p1_{}", dest.0);
                    out.push_str(&format!(
                        "  {} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                        p0_ptr, p0_id
                    ));
                    out.push_str(&format!(
                        "  {} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                        p1_ptr, p1_id
                    ));
                    out.push_str(&format!(
                        "  %v{} = call ptr @datara_rt_str_concat_3(ptr {}, ptr {}, ptr {})\n",
                        dest.0, p0_ptr, str_val, p1_ptr
                    ));
                } else {
                    let s_id = strings.get("").copied().unwrap_or(0);
                    out.push_str(&format!(
                        "  %v{} = getelementptr inbounds [0 x i8], ptr @.str.{}, i64 0, i64 0\n",
                        dest.0, s_id
                    ));
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
                let default_val = else_val.unwrap_or(ValueId(0));
                let mut curr_val_str = format!("%v{}", default_val.0);
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
            _ => {}
        }
    }

    fn find_field_offset(&self, module: &Module, field: &str) -> usize {
        for fields in module.class_fields.values() {
            if let Some(pos) = fields.iter().position(|f| f == field) {
                return pos * 8;
            }
        }
        0
    }
}

/// LLVM Backend implementing `CodegenBackend`.
pub struct LlvmBackend {
    pub target: TargetInfo,
}

impl LlvmBackend {
    pub fn new(target: TargetInfo) -> Self {
        Self { target }
    }
}

impl CodegenBackend for LlvmBackend {
    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        let emitter = LlvmEmitter::new(&self.target);
        emitter.emit_module(module, program, types)
    }

    fn compile_to_executable(&self, source: &str, target_path: &Path) -> Result<PathBuf, String> {
        let ll_path = target_path.with_extension("ll");
        std::fs::write(&ll_path, source)
            .map_err(|e| format!("Failed to write LLVM IR to {}: {}", ll_path.display(), e))?;

        let exe_path = if cfg!(windows) {
            target_path.with_extension("exe")
        } else {
            target_path.to_path_buf()
        };

        if find_clang().is_some() {
            let rt_path = PathBuf::from("src/runtime/datara_runtime.c");
            let rt_opt = if rt_path.exists() {
                Some(rt_path.as_path())
            } else {
                None
            };
            compile_with_clang(&ll_path, rt_opt, &exe_path, "3")?;
            Ok(exe_path)
        } else {
            Err(format!(
                "Clang not found. LLVM IR written to {}. Install Clang to compile with --llvm.",
                ll_path.display()
            ))
        }
    }

    fn target_info(&self) -> TargetInfo {
        self.target.clone()
    }

    fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        let start = std::time::Instant::now();
        let out = Command::new(exe_path)
            .args(args)
            .output()
            .map_err(|e| format!("Failed to run executable {}: {}", exe_path.display(), e))?;
        let elapsed = start.elapsed().as_nanos();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let code = out.status.code().unwrap_or(-1);
        Ok((stdout, stderr, code, elapsed))
    }
}
