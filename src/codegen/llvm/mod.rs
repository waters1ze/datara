use crate::ast::Program;
use crate::codegen::CodegenBackend;
use crate::codegen::linker::{compile_with_clang, find_clang};
use crate::codegen::target::{Arch, CallingConvention, Os, TargetInfo};
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use crate::types::{DataraType, TypeChecker};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Dedicated LLVM IR code emitter for Datara / Forgen.
pub struct LlvmEmitter<'a> {
    pub target: &'a TargetInfo,
    pub emit_debug_info: bool,
}

fn get_range_for_var(
    fn_name: &str,
    var_name: &str,
    ty_hint: Option<&str>,
    types: &TypeChecker,
) -> Option<(i64, i64)> {
    if let Some(DataraType::Range { min, max, .. }) = types
        .fn_symbol_types
        .get(&(fn_name.to_string(), var_name.to_string()))
        .or_else(|| types.symbol_types.get(var_name))
    {
        return Some((*min as i64, *max as i64));
    }
    if let Some(th) = ty_hint
        && th.starts_with("Int<")
        && th.ends_with('>')
    {
        let inner = &th[4..th.len() - 1];
        if let Some((s_min, s_max)) = inner.split_once("..")
            && let (Ok(min), Ok(max)) = (s_min.trim().parse::<i64>(), s_max.trim().parse::<i64>())
        {
            return Some((min, max));
        }
    }
    None
}

fn collect_address_taken(module: &Module) -> HashSet<String> {
    let mut address_taken = HashSet::new();
    for f in module.functions.values() {
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::GetFuncAddr { func_name, .. } = inst {
                    address_taken.insert(func_name.clone());
                }
            }
        }
    }
    address_taken
}

impl<'a> LlvmEmitter<'a> {
    pub fn new(target: &'a TargetInfo) -> Self {
        Self {
            target,
            emit_debug_info: false,
        }
    }

    pub fn with_debug(mut self, debug_info: bool) -> Self {
        self.emit_debug_info = debug_info;
        self
    }

    /// Map Datara / DMIR type names to LLVM IR types.
    pub fn dmir_type_to_llvm(&self, ty: &str) -> &'static str {
        match ty {
            "Int" | "Int64" | "UInt" | "UInt64" | "i64" | "u64" | "isize" | "usize" | "USize"
            | "Bool" => "i64",
            "Int32" | "UInt32" | "i32" | "u32" => "i32",
            "Int16" | "UInt16" | "i16" | "u16" => "i16",
            "Int8" | "UInt8" | "i8" | "u8" | "Byte" => "i8",
            "i128" | "u128" => "i128",
            "Float" | "Float64" | "f64" => "double",
            "Float32" | "f32" | "f16" => "float",
            "<4 x float>" => "<4 x float>",
            "<4 x i32>" => "<4 x i32>",
            "Str" | "String" => "ptr",
            "Unit" | "void" | "Never" => "void",
            s if s.starts_with("Int<") || s.starts_with("UInt<") => "i64",
            s if s.starts_with("Float<") => "double",
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
    pub fn emit_module(&self, module: &Module, _program: &Program, types: &TypeChecker) -> String {
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
        match (&self.target.arch, &self.target.os) {
            (Arch::Aarch64, Os::MacOS) => {
                ir.push_str("target datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128\"\n");
                ir.push_str("target triple = \"arm64-apple-macosx\"\n\n");
            }
            (Arch::Aarch64, Os::Linux) => {
                ir.push_str(
                    "target datalayout = \"e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128\"\n",
                );
                ir.push_str("target triple = \"aarch64-unknown-linux-gnu\"\n\n");
            }
            (Arch::Aarch64, Os::Windows) => {
                ir.push_str(
                    "target datalayout = \"e-m:w-p:64:64-i32:32-i64:64-i128:128-n32:64-S128\"\n",
                );
                ir.push_str("target triple = \"aarch64-pc-windows-msvc\"\n\n");
            }
            (Arch::X86_64, Os::Windows) => {
                ir.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                ir.push_str("target triple = \"x86_64-pc-windows-msvc\"\n\n");
            }
            (Arch::X86_64, Os::MacOS) => {
                ir.push_str("target datalayout = \"e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                ir.push_str("target triple = \"x86_64-apple-macosx\"\n\n");
            }
            _ => match self.target.calling_convention {
                CallingConvention::WindowsFastcall => {
                    ir.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                    ir.push_str("target triple = \"x86_64-pc-windows-msvc\"\n\n");
                }
                CallingConvention::Aarch64Standard => {
                    ir.push_str("target datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128\"\n");
                    ir.push_str("target triple = \"arm64-apple-macosx\"\n\n");
                }
                CallingConvention::SystemV => {
                    ir.push_str("target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
                    ir.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
                }
                CallingConvention::WasmStandard => {
                    ir.push_str("target datalayout = \"e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-n32:64-S128-ni:1:10:20\"\n");
                    ir.push_str("target triple = \"wasm32-unknown-wasi\"\n\n");
                }
            },
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

        let mut sorted_func_names: Vec<&String> = module.functions.keys().collect();
        sorted_func_names.sort();

        for fname in &sorted_func_names {
            let func = &module.functions[*fname];
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

        ir.push_str("; --- Datara Standard Runtime Declarations ---\n");
        ir.push_str("declare void @llvm.assume(i1)\n");
        ir.push_str("declare void @datara_rt_overflow_panic()\n");
        ir.push_str("declare void @datara_rt_div_zero_panic()\n");
        ir.push_str("declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)\n");
        ir.push_str("declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)\n");
        ir.push_str("declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)\n");
        ir.push_str("declare i64 @llvm.sadd.sat.i64(i64, i64)\n");
        ir.push_str("declare i64 @llvm.ssub.sat.i64(i64, i64)\n\n");
        ir.push_str("; --- Inlined Checked & Fast Arithmetic Builtins ---\n");
        ir.push_str("define internal i64 @datara_rt_checked_add(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %a, i64 %b)\n");
        ir.push_str("  %val = extractvalue { i64, i1 } %res, 0\n");
        ir.push_str("  %ovf = extractvalue { i64, i1 } %res, 1\n");
        ir.push_str("  br i1 %ovf, label %trap, label %ok, !prof !9\n");
        ir.push_str("trap:\n");
        ir.push_str("  call void @datara_rt_overflow_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("ok:\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_checked_sub(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %a, i64 %b)\n");
        ir.push_str("  %val = extractvalue { i64, i1 } %res, 0\n");
        ir.push_str("  %ovf = extractvalue { i64, i1 } %res, 1\n");
        ir.push_str("  br i1 %ovf, label %trap, label %ok, !prof !9\n");
        ir.push_str("trap:\n");
        ir.push_str("  call void @datara_rt_overflow_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("ok:\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_checked_mul(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = call { i64, i1 } @llvm.smul.with.overflow.i64(i64 %a, i64 %b)\n");
        ir.push_str("  %val = extractvalue { i64, i1 } %res, 0\n");
        ir.push_str("  %ovf = extractvalue { i64, i1 } %res, 1\n");
        ir.push_str("  br i1 %ovf, label %trap, label %ok, !prof !9\n");
        ir.push_str("trap:\n");
        ir.push_str("  call void @datara_rt_overflow_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("ok:\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_checked_div(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %zero = icmp eq i64 %b, 0\n");
        ir.push_str("  br i1 %zero, label %trap_zero, label %chk_min, !prof !9\n");
        ir.push_str("trap_zero:\n");
        ir.push_str("  call void @datara_rt_div_zero_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("chk_min:\n");
        ir.push_str("  %is_min = icmp eq i64 %a, -9223372036854775808\n");
        ir.push_str("  %is_neg1 = icmp eq i64 %b, -1\n");
        ir.push_str("  %ovf = and i1 %is_min, %is_neg1\n");
        ir.push_str("  br i1 %ovf, label %trap_ovf, label %ok, !prof !9\n");
        ir.push_str("trap_ovf:\n");
        ir.push_str("  call void @datara_rt_overflow_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("ok:\n");
        ir.push_str("  %val = sdiv i64 %a, %b\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_checked_rem(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %zero = icmp eq i64 %b, 0\n");
        ir.push_str("  br i1 %zero, label %trap_zero, label %chk_min, !prof !9\n");
        ir.push_str("trap_zero:\n");
        ir.push_str("  call void @datara_rt_div_zero_panic()\n");
        ir.push_str("  unreachable\n");
        ir.push_str("chk_min:\n");
        ir.push_str("  %is_min = icmp eq i64 %a, -9223372036854775808\n");
        ir.push_str("  %is_neg1 = icmp eq i64 %b, -1\n");
        ir.push_str("  %ovf = and i1 %is_min, %is_neg1\n");
        ir.push_str("  br i1 %ovf, label %ret_zero, label %ok, !prof !9\n");
        ir.push_str("ret_zero:\n");
        ir.push_str("  ret i64 0\n");
        ir.push_str("ok:\n");
        ir.push_str("  %val = srem i64 %a, %b\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str(
            "define internal i64 @datara_rt_saturating_add(i64 %a, i64 %b) alwaysinline {\n",
        );
        ir.push_str("entry:\n");
        ir.push_str("  %res = call i64 @llvm.sadd.sat.i64(i64 %a, i64 %b)\n");
        ir.push_str("  ret i64 %res\n");
        ir.push_str("}\n\n");
        ir.push_str(
            "define internal i64 @datara_rt_saturating_sub(i64 %a, i64 %b) alwaysinline {\n",
        );
        ir.push_str("entry:\n");
        ir.push_str("  %res = call i64 @llvm.ssub.sat.i64(i64 %a, i64 %b)\n");
        ir.push_str("  ret i64 %res\n");
        ir.push_str("}\n\n");
        ir.push_str("declare i64 @datara_rt_saturating_mul(i64, i64)\n");
        ir.push_str("define internal i64 @datara_rt_wrapping_add(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = add i64 %a, %b\n");
        ir.push_str("  ret i64 %res\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_wrapping_sub(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = sub i64 %a, %b\n");
        ir.push_str("  ret i64 %res\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal i64 @datara_rt_wrapping_mul(i64 %a, i64 %b) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %res = mul i64 %a, %b\n");
        ir.push_str("  ret i64 %res\n");
        ir.push_str("}\n\n");
        ir.push_str("declare i64 @datara_rt_own_acquire(i64)\n");
        ir.push_str("declare void @datara_rt_own_release(i64)\n");
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
        ir.push_str("declare i64 @datara_rt_byte_len(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_chars(ptr)\n");
        ir.push_str("declare i64 @datara_rt_char_len(ptr)\n");
        ir.push_str("declare i32 @datara_rt_validate_utf8(ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_sanitize_utf8(ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_scalar_at(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_str_next_offset(ptr, i64)\n");
        ir.push_str("declare ptr @datara_rt_str_char_at(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_str_byte_at(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_str_eq(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_trim(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_to_int(ptr)\n");
        ir.push_str("declare double @datara_rt_str_to_float(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_contains(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_starts_with(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_ends_with(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_index_of(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_substring(ptr, i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_str_split(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_join(ptr, ptr)\n");
        ir.push_str("declare void @datara_rt_assert(i64, ptr)\n");
        ir.push_str("declare i64 @destroy(i64)\n");
        ir.push_str("declare i64 @datara_rt_file_write(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_file_read(ptr)\n");
        ir.push_str("declare i64 @datara_rt_file_append(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_file_exists(ptr)\n");
        ir.push_str("declare ptr @datara_rt_list_create(i64)\n");
        ir.push_str("declare ptr @datara_rt_list_create_1(i64)\n");
        ir.push_str("declare ptr @datara_rt_list_create_2(i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_list_create_3(i64, i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_list_create_4(i64, i64, i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_list_create_5(i64, i64, i64, i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_list_append(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_get(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_get_unchecked(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_len(ptr)\n");
        ir.push_str(
            "declare ptr @datara_rt_map_create()
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_create_1(ptr, i64)
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_create_2(ptr, i64, ptr, i64)
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_create_3(ptr, i64, ptr, i64, ptr, i64)
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_create_4(ptr, i64, ptr, i64, ptr, i64, ptr, i64)
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_create_5(ptr, i64, ptr, i64, ptr, i64, ptr, i64, ptr, i64)
",
        );
        ir.push_str(
            "declare ptr @datara_rt_map_insert(ptr, ptr, i64)
",
        );
        ir.push_str(
            "declare i64 @datara_rt_map_get(ptr, ptr)
",
        );
        ir.push_str("declare i64 @datara_rt_now_ms()\n");
        ir.push_str("declare i64 @datara_rt_now_ns()\n");
        ir.push_str("declare i64 @datara_rt_now_precise_ms()\n");
        ir.push_str("declare void @datara_rt_sleep(i64)\n");
        ir.push_str("declare ptr @datara_rt_path_join(ptr, ptr)\n");
        ir.push_str("declare ptr @datara_rt_http_get(ptr)\n");
        // Fast Math
        ir.push_str("declare double @datara_rt_math_sqrt(double)\n");
        ir.push_str("declare double @datara_rt_math_log(double)\n");
        ir.push_str("declare double @datara_rt_math_exp(double)\n");
        ir.push_str("declare double @datara_rt_math_clamp(double, double, double)\n");
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
        ir.push_str("declare i64 @datara_rt_math_clamp_int(i64, i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_math_abs_int(i64)\n");
        ir.push_str("declare i64 @datara_rt_math_ctz(i64)\n");
        ir.push_str("declare i64 @datara_rt_math_shr(i64, i64)\n");
        ir.push_str("declare i64 @datara_rt_math_shl(i64, i64)\n");
        ir.push_str("declare i64 @llvm.cttz.i64(i64, i1)\n");
        ir.push_str("declare ptr @malloc(i64)\n");
        ir.push_str("declare void @free(ptr)\n");
        ir.push_str("declare <4 x float> @llvm.minnum.v4f32(<4 x float>, <4 x float>)\n");
        ir.push_str("declare <4 x float> @llvm.maxnum.v4f32(<4 x float>, <4 x float>)\n");
        ir.push_str("declare ptr @datara_rt_sha256(ptr)\n");
        ir.push_str("declare ptr @datara_rt_base64_encode(ptr)\n");
        ir.push_str("declare ptr @datara_rt_base64_decode(ptr)\n");
        ir.push_str("declare ptr @datara_rt_uuid_v4()\n");
        ir.push_str("declare i64 @datara_rt_random_bytes(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_dialog_info(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_dialog_alert(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_dialog_confirm(ptr, ptr)\n");
        ir.push_str("declare void @datara_rt_parallel_for(i64, i64, i64, i64)\n");
        ir.push_str("declare void @datara_rt_parallel_invoke(i64, i64, i64, i64)\n\n");

        // 2b. Declare user-declared extern "C" functions (FFI), mirroring the
        // Cranelift backend so FFI programs compile on both backends.
        let mut sorted_externs: Vec<_> = module.extern_functions.iter().collect();
        sorted_externs.sort_by_key(|(name, _)| *name);
        for (ef_name, (ef_params, ef_ret)) in sorted_externs {
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
        let address_taken = collect_address_taken(module);
        let mut range_metadata_map: HashMap<(i64, i64), usize> = HashMap::new();
        let mut next_meta_id = 10;
        for fname in &sorted_func_names {
            let f = &module.functions[*fname];
            ir.push_str(&self.emit_function_with_address_taken(
                f,
                module,
                &string_literal_map,
                types,
                &mut range_metadata_map,
                &mut next_meta_id,
                &address_taken,
            ));
            ir.push('\n');
        }

        // 4. Emit Loop Vectorization & Unroll Metadata
        ir.push_str("!0 = distinct !{!0, !1, !3}\n");
        ir.push_str("!1 = !{!\"llvm.loop.vectorize.enable\", i1 1}\n");
        ir.push_str("!3 = !{!\"llvm.loop.unroll.enable\", i1 1}\n");
        ir.push_str("!9 = !{!\"branch_weights\", i32 1, i32 1048576}\n\n");

        // 5. Emit Formal Value Range Propagation (FVRP) Metadata Nodes
        if !range_metadata_map.is_empty() {
            ir.push_str("; --- FVRP Range Metadata Nodes ---\n");
            let mut sorted_ranges: Vec<((i64, i64), usize)> =
                range_metadata_map.into_iter().collect();
            sorted_ranges.sort_by_key(|&(_, id)| id);
            for ((min, high), id) in sorted_ranges {
                ir.push_str(&format!("!{} = !{{i64 {}, i64 {}}}\n", id, min, high));
            }
            ir.push('\n');
        }

        ir
    }

    /// Emit a single function to LLVM IR.
    pub fn emit_function(
        &self,
        f: &Function,
        module: &Module,
        strings: &HashMap<String, usize>,
        types: &TypeChecker,
        range_metadata: &mut HashMap<(i64, i64), usize>,
        next_meta_id: &mut usize,
    ) -> String {
        let address_taken = collect_address_taken(module);
        self.emit_function_with_address_taken(
            f,
            module,
            strings,
            types,
            range_metadata,
            next_meta_id,
            &address_taken,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn emit_function_with_address_taken(
        &self,
        f: &Function,
        module: &Module,
        strings: &HashMap<String, usize>,
        types: &TypeChecker,
        range_metadata: &mut HashMap<(i64, i64), usize>,
        next_meta_id: &mut usize,
        address_taken: &HashSet<String>,
    ) -> String {
        let mut out = String::new();
        let is_main = f.name == "main";
        let has_main = module.functions.contains_key("main");
        let is_internal = has_main
            && !is_main
            && module.functions.contains_key(&f.name)
            && !module.extern_functions.contains_key(&f.name)
            && !address_taken.contains(&f.name);

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

        let linkage_and_cc = if is_internal {
            "define internal fastcc"
        } else {
            "define"
        };

        out.push_str(&format!(
            "{} {} @{}({}) {{\n",
            linkage_and_cc, ret_type, f.name, params_sig
        ));

        // Track local variable types and allocas
        let mut local_vars: BTreeSet<String> = BTreeSet::new();
        let mut struct_inits: Vec<(ValueId, usize)> = Vec::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::AssignVar { name, .. } = inst {
                    local_vars.insert(name.clone());
                } else if let Inst::StructInit { dest, fields, .. } = inst {
                    let byte_size = fields.len().saturating_mul(8).max(8);
                    struct_inits.push((*dest, byte_size));
                }
            }
        }

        // Value types tracking within the function
        let mut value_types: HashMap<ValueId, &'static str> = HashMap::new();
        let mut bool_vids: HashSet<ValueId> = HashSet::new();
        let mut bool_vars: HashSet<String> = HashSet::new();
        for (p_name, p_ty, p_val) in &f.params {
            value_types.insert(*p_val, self.dmir_type_to_llvm(p_ty));
            if p_ty == "Bool" {
                bool_vids.insert(*p_val);
                bool_vars.insert(p_name.clone());
            }
        }

        // Track which class each struct-typed value belongs to so field
        // offsets resolve against the right class (exact match) instead of
        // whichever class happens to declare the field name.
        let mut value_classes: HashMap<ValueId, String> = HashMap::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::StructInit {
                    dest, class_name, ..
                } = inst
                {
                    value_classes.insert(*dest, class_name.clone());
                }
            }
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
                if let Some((min, max)) = get_range_for_var(&f.name, pname, Some(pty), types) {
                    out.push_str(&format!(
                        "  %fvrp_pmin_{} = icmp sge i64 %v{}, {}\n",
                        pname, pval.0, min
                    ));
                    out.push_str(&format!(
                        "  call void @llvm.assume(i1 %fvrp_pmin_{})\n",
                        pname
                    ));
                    out.push_str(&format!(
                        "  %fvrp_pmax_{} = icmp sle i64 %v{}, {}\n",
                        pname, pval.0, max
                    ));
                    out.push_str(&format!(
                        "  call void @llvm.assume(i1 %fvrp_pmax_{})\n",
                        pname
                    ));
                }
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
                    &mut value_classes,
                    &mut bool_vids,
                    &mut bool_vars,
                    &mut out,
                    &f.name,
                    types,
                    range_metadata,
                    next_meta_id,
                    &address_taken,
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
                        // Propagate datara_main's Int result as the process
                        // exit code instead of always returning 0.
                        if let (Some(v), "Int" | "Bool") = (value, f.return_type.as_str()) {
                            out.push_str(&format!(
                                "  %main_ret_{} = trunc i64 %v{} to i32\n",
                                v.0, v.0
                            ));
                            out.push_str(&format!("  ret i32 %main_ret_{}\n", v.0));
                        } else {
                            out.push_str("  ret i32 0\n");
                        }
                    } else if f.return_type == "Unit" || f.return_type == "Never" {
                        out.push_str("  ret void\n");
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
            out.push('\n');
        }

        out.push_str("}\n");
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_instruction(
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

    fn find_field_offset(&self, module: &Module, class: Option<&str>, field: &str) -> usize {
        // Exact class match first: when the object's class is known, only
        // that class's layout may be consulted — an unrelated class that
        // happens to declare the same field name must not win.
        if let Some(cls) = class
            && let Some(fields) = module.class_fields.get(cls)
            && let Some(pos) = fields.iter().position(|f| f == field)
        {
            return pos.saturating_mul(8);
        }
        // Class unknown: keep the historical fallback scan so programs whose
        // objects come from call results still resolve offsets.
        // Sort class names to ensure deterministic offset resolution.
        let mut sorted_classes: Vec<&String> = module.class_fields.keys().collect();
        sorted_classes.sort();
        for cls in sorted_classes {
            let fields = &module.class_fields[cls];
            if let Some(pos) = fields.iter().position(|f| f == field) {
                return pos.saturating_mul(8);
            }
        }
        0
    }
}

/// LLVM Backend implementing `CodegenBackend`.
pub struct LlvmBackend {
    pub target: TargetInfo,
    pub debug_info: bool,
}

impl LlvmBackend {
    pub fn new(target: TargetInfo) -> Self {
        Self {
            target,
            debug_info: false,
        }
    }

    pub fn with_debug(mut self, debug_info: bool) -> Self {
        self.debug_info = debug_info;
        self
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
            let rt_source = crate::runtime::runtime_source_path();
            let rt_archive = crate::runtime::runtime_lib_path();
            let rt_opt = if let Some(ref src) = rt_source {
                Some(src.as_path())
            } else if rt_archive.exists() {
                Some(rt_archive.as_path())
            } else {
                None
            };
            compile_with_clang(&ll_path, rt_opt, &exe_path, "3", None, self.debug_info)?;
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
        let mut cmd = Command::new(exe_path);
        cmd.args(args);
        cmd.env_remove("LD_PRELOAD");
        // Set detect_leaks=0 to suppress LeakSanitizer on compiled child binaries.
        if std::env::var("ASAN_OPTIONS").is_ok() {
            cmd.env("ASAN_OPTIONS", "detect_leaks=0");
        } else {
            cmd.env_remove("ASAN_OPTIONS");
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run executable {}: {}", exe_path.display(), e))?;
        let elapsed = start.elapsed().as_nanos();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let code = out.status.code().unwrap_or(-1);
        Ok((stdout, stderr, code, elapsed))
    }
}
