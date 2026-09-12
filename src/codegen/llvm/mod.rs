pub(crate) mod attributes;
pub(crate) mod emit_inst;
pub(crate) mod simd;

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
    pub profile: Option<&'a crate::pgo::ProfileData>,
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
            profile: None,
        }
    }

    pub fn with_debug(mut self, debug_info: bool) -> Self {
        self.emit_debug_info = debug_info;
        self
    }

    pub fn with_profile(mut self, profile: Option<&'a crate::pgo::ProfileData>) -> Self {
        self.profile = profile;
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
        ir.push_str("define internal i64 @datara_rt_list_get_unchecked(ptr %list, i64 %idx) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %off = add i64 %idx, 1\n");
        ir.push_str("  %ptr = getelementptr inbounds i64, ptr %list, i64 %off\n");
        ir.push_str("  %val = load i64, ptr %ptr, align 8\n");
        ir.push_str("  ret i64 %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal ptr @datara_rt_list_set_unchecked(ptr %list, i64 %idx, i64 %val) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %off = add i64 %idx, 1\n");
        ir.push_str("  %ptr = getelementptr inbounds i64, ptr %list, i64 %off\n");
        ir.push_str("  store i64 %val, ptr %ptr, align 8\n");
        ir.push_str("  ret ptr %list\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal double @datara_rt_list_get_f64_unchecked(ptr %list, i64 %idx) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %off = add i64 %idx, 1\n");
        ir.push_str("  %ptr = getelementptr inbounds double, ptr %list, i64 %off\n");
        ir.push_str("  %val = load double, ptr %ptr, align 8\n");
        ir.push_str("  ret double %val\n");
        ir.push_str("}\n\n");
        ir.push_str("define internal ptr @datara_rt_list_set_f64_unchecked(ptr %list, i64 %idx, double %val) alwaysinline {\n");
        ir.push_str("entry:\n");
        ir.push_str("  %off = add i64 %idx, 1\n");
        ir.push_str("  %ptr = getelementptr inbounds double, ptr %list, i64 %off\n");
        ir.push_str("  store double %val, ptr %ptr, align 8\n");
        ir.push_str("  ret ptr %list\n");
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
        ir.push_str("declare ptr @datara_rt_list_set(ptr, i64, i64)\n");
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
        ir.push_str("declare i64 @datara_rt_map_contains(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_map_len(ptr)\n");
        ir.push_str("declare void @datara_rt_map_free(ptr)\n");
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
        ir.push_str("declare float @llvm.vector.reduce.fadd.v4f32(float, <4 x float>)\n");
        ir.push_str("declare ptr @datara_rt_sha256(ptr)\n");
        ir.push_str("declare ptr @datara_rt_base64_encode(ptr)\n");
        ir.push_str("declare ptr @datara_rt_base64_decode(ptr)\n");
        ir.push_str("declare ptr @datara_rt_uuid_v4()\n");
        ir.push_str("declare i64 @datara_rt_random_bytes(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_dialog_info(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_dialog_alert(ptr, ptr)\n");
        ir.push_str("declare i64 @datara_rt_dialog_confirm(ptr, ptr)\n");
        ir.push_str("declare void @datara_rt_parallel_for(i64, i64, i64, i64)\n");
        ir.push_str("declare void @datara_rt_parallel_invoke(i64, i64, i64, i64)\n");
        ir.push_str("declare ptr @datara_rt_pool_alloc(i64)\n");
        ir.push_str("declare void @datara_rt_pool_free(ptr, i64)\n");
        ir.push_str("declare ptr @datara_rt_box_alloc(i64)\n");
        ir.push_str("declare i64 @datara_rt_box_get(ptr)\n");
        ir.push_str("declare void @datara_rt_box_free(ptr)\n");
        ir.push_str("declare ptr @datara_rt_str_sso(ptr)\n");
        ir.push_str("declare i64 @datara_rt_str_is_sso(ptr)\n");
        ir.push_str("declare i64 @datara_rt_heap_alloc_count()\n");
        ir.push_str("declare void @datara_rt_reset_heap_alloc_count()\n");
        ir.push_str("declare ptr @datara_rt_list_init_stack(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_list_is_small_vec(ptr)\n");
        ir.push_str("declare void @datara_rt_pgo_hit_func(ptr)\n");
        ir.push_str("declare void @datara_rt_pgo_hit_branch(ptr, i64)\n");
        ir.push_str("declare void @datara_rt_pgo_hit_loop(ptr, i64)\n");
        ir.push_str("declare void @datara_rt_pgo_set_output_file(ptr)\n");
        ir.push_str("declare void @datara_rt_pgo_flush(ptr)\n");
        ir.push_str("declare void @datara_rt_pgo_reset()\n");

        ir.push_str("declare ptr @datara_rt_chase_lev_create(i64)\n");
        ir.push_str("declare void @datara_rt_chase_lev_destroy(ptr)\n");
        ir.push_str("declare void @datara_rt_chase_lev_push(ptr, i64)\n");
        ir.push_str("declare i64 @datara_rt_chase_lev_pop(ptr)\n");
        ir.push_str("declare i64 @datara_rt_chase_lev_steal(ptr)\n");
        ir.push_str("declare i64 @datara_rt_chase_lev_size(ptr)\n");

        ir.push_str("declare ptr @datara_rt_fast_memcpy(ptr, ptr, i64)\n");
        ir.push_str("declare ptr @datara_rt_fast_memset(ptr, i32, i64)\n");
        ir.push_str("declare i32 @datara_rt_fast_strncmp(ptr, ptr, i64)\n");
        ir.push_str("declare i32 @datara_rt_fast_memcmp(ptr, ptr, i64)\n");

        ir.push_str("declare i32 @datara_rt_pin_thread(i64)\n");
        ir.push_str("declare i64 @datara_rt_get_current_core()\n");
        ir.push_str("declare void @datara_rt_pin_worker_threads()\n");

        ir.push_str("declare void @datara_rt_cap_set_mask(i64)\n");
        ir.push_str("declare i64 @datara_rt_cap_get_mask()\n");
        ir.push_str("declare void @datara_rt_cap_revoke(i64)\n");
        ir.push_str("declare void @datara_rt_cap_grant(i64)\n");
        ir.push_str("declare void @datara_rt_cap_require(i64, ptr)\n\n");
        simd::emit_simd_declarations(&mut ir);

        // 2b. Declare user-declared extern "C" functions (FFI), mirroring the
        // Cranelift backend so FFI programs compile on both backends.
        let mut sorted_externs: Vec<_> = module.extern_functions.iter().collect();
        sorted_externs.sort_by_key(|(name, _)| *name);
        for (ef_name, (ef_params, ef_ret)) in sorted_externs {
            if ef_name.starts_with("datara_rt_") {
                continue;
            }
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

        // 3. Emit all functions with ownership and effect attributes
        let address_taken = collect_address_taken(module);
        let signatures = attributes::index_program_signatures(_program);
        let mut effect_analyzer = crate::effects::EffectAnalyzer::new();
        effect_analyzer.analyze_program(_program);

        let mut range_metadata_map: HashMap<(i64, i64), usize> = HashMap::new();
        let mut branch_weights_map: HashMap<(u32, u32), usize> = HashMap::new();
        let mut entry_count_map: HashMap<usize, usize> = HashMap::new();
        let mut loop_metadata_map: HashMap<usize, usize> = HashMap::new();
        branch_weights_map.insert((1, 1048576), 9);
        let mut next_meta_id = 10;
        for fname in &sorted_func_names {
            let f = &module.functions[*fname];
            let (ast_params, is_pure_ast) = signatures.get(*fname).cloned().unwrap_or_default();
            let is_pure_eff = effect_analyzer
                .function_effects
                .get(*fname)
                .map(|e| e.is_pure())
                .unwrap_or(false);
            let is_pure = is_pure_ast || is_pure_eff;
            let (is_cold, is_hot, entry_count) = if *fname == "main" {
                (false, false, None)
            } else if let Some(ref prof) = self.profile {
                if prof.is_runtime_measured() {
                    let count = prof.hot_functions.get(*fname).copied().unwrap_or(0);
                    let cold = count == 0;
                    let hot = count > 50;
                    (cold, hot, if count > 0 { Some(count) } else { None })
                } else {
                    let cold = !f.blocks.is_empty()
                        && f.blocks.iter().all(|b| attributes::is_cold_block(f, b.id));
                    (cold, false, None)
                }
            } else {
                let cold = !f.blocks.is_empty()
                    && f.blocks.iter().all(|b| attributes::is_cold_block(f, b.id));
                (cold, false, None)
            };
            let fn_ctx = attributes::FunctionAttrContext {
                is_pure,
                is_cold,
                is_hot,
                entry_count,
                ast_params,
            };
            ir.push_str(&self.emit_function_with_details(
                f,
                module,
                &string_literal_map,
                types,
                &mut range_metadata_map,
                &mut branch_weights_map,
                &mut entry_count_map,
                &mut loop_metadata_map,
                &mut next_meta_id,
                &address_taken,
                Some(&fn_ctx),
            ));
            ir.push('\n');
        }

        // 4. Emit Loop Vectorization & Unroll Metadata (Honest contract: only enabled when target supports it)
        let vec_enabled = attributes::is_vector_supported(self.target);
        ir.push_str("!0 = distinct !{!0, !1, !3, !4, !5}\n");
        ir.push_str(&format!(
            "!1 = !{{!\"llvm.loop.vectorize.enable\", i1 {}}}\n",
            if vec_enabled { 1 } else { 0 }
        ));
        ir.push_str("!3 = !{!\"llvm.loop.unroll.enable\", i1 1}\n");
        ir.push_str("!4 = !{!\"llvm.loop.vectorize.width\", i32 4}\n");
        ir.push_str("!5 = !{!\"llvm.loop.interleave.count\", i32 4}\n");
        ir.push_str("!9 = !{!\"branch_weights\", i32 1, i32 1048576}\n\n");

        // 5. Emit Profile-Guided and Custom Branch Weights Metadata Nodes
        let mut custom_weights: Vec<((u32, u32), usize)> = branch_weights_map
            .into_iter()
            .filter(|&(_, id)| id != 9)
            .collect();
        if !custom_weights.is_empty() {
            ir.push_str("; --- Profile-Guided Branch Weights ---\n");
            custom_weights.sort_by_key(|&(_, id)| id);
            for ((taken, not_taken), id) in custom_weights {
                ir.push_str(&format!(
                    "!{} = !{{!\"branch_weights\", i32 {}, i32 {}}}\n",
                    id, taken, not_taken
                ));
            }
            ir.push('\n');
        }

        // 5b. Emit Profile-Guided Function Entry Counts
        if !entry_count_map.is_empty() {
            ir.push_str("; --- Profile-Guided Function Entry Counts ---\n");
            let mut sorted_entries: Vec<(usize, usize)> = entry_count_map.into_iter().collect();
            sorted_entries.sort_by_key(|&(id, _)| id);
            for (id, count) in sorted_entries {
                ir.push_str(&format!(
                    "!{} = !{{!\"function_entry_count\", i64 {}}}\n",
                    id, count
                ));
            }
            ir.push('\n');
        }

        // 5c. Emit Profile-Guided Loop Unroll Metadata
        if !loop_metadata_map.is_empty() {
            ir.push_str("; --- Profile-Guided Loop Unroll Metadata ---\n");
            let mut sorted_loops: Vec<(usize, usize)> = loop_metadata_map.into_iter().collect();
            sorted_loops.sort_by_key(|&(id, _)| id);
            for (id, trip_count) in sorted_loops {
                let unroll_id = next_meta_id;
                next_meta_id += 1;
                let count_id = next_meta_id;
                next_meta_id += 1;
                ir.push_str(&format!(
                    "!{} = distinct !{{!{}, !{}, !{}}}\n!{} = !{{!\"llvm.loop.unroll.enable\", i1 1}}\n!{} = !{{!\"llvm.loop.unroll.count\", i32 {}}}\n",
                    id, id, unroll_id, count_id, unroll_id, count_id, trip_count
                ));
            }
            ir.push('\n');
        }

        // 6. Emit Formal Value Range Propagation (FVRP) Metadata Nodes
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

        // 7. Emit Type-Based Alias Analysis (TBAA) Metadata Nodes
        ir.push_str(&attributes::emit_tbaa_metadata());

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
        let mut branch_weights = HashMap::new();
        let mut entry_counts = HashMap::new();
        let mut loop_meta = HashMap::new();
        self.emit_function_with_details(
            f,
            module,
            strings,
            types,
            range_metadata,
            &mut branch_weights,
            &mut entry_counts,
            &mut loop_meta,
            next_meta_id,
            address_taken,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn emit_function_with_details(
        &self,
        f: &Function,
        module: &Module,
        strings: &HashMap<String, usize>,
        types: &TypeChecker,
        range_metadata: &mut HashMap<(i64, i64), usize>,
        branch_weights_map: &mut HashMap<(u32, u32), usize>,
        entry_count_map: &mut HashMap<usize, usize>,
        loop_metadata_map: &mut HashMap<usize, usize>,
        next_meta_id: &mut usize,
        address_taken: &HashSet<String>,
        fn_ctx: Option<&attributes::FunctionAttrContext>,
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

        let is_pure = fn_ctx.map(|c| c.is_pure).unwrap_or(false);
        let ast_params = fn_ctx.map(|c| c.ast_params.as_slice());

        let params_sig = if is_main {
            "".to_string()
        } else {
            f.params
                .iter()
                .enumerate()
                .map(|(idx, (p_name, p_ty, p_val))| {
                    let attrs = attributes::derive_param_attributes(
                        f, idx, p_name, p_ty, *p_val, module, ast_params, is_pure,
                    );
                    format!("{}{} %v{}", self.dmir_type_to_llvm(p_ty), attrs, p_val.0)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let linkage_and_cc = if is_internal {
            "define internal fastcc"
        } else {
            "define"
        };

        let is_cold = fn_ctx.map(|c| c.is_cold).unwrap_or(false);
        let is_hot = fn_ctx.map(|c| c.is_hot).unwrap_or(false);
        let fn_attrs = attributes::derive_fn_attributes(f, is_pure, is_cold, is_hot);
        let fn_attrs_str = if fn_attrs.is_empty() {
            "".to_string()
        } else {
            format!(" {}", fn_attrs.join(" "))
        };
        let entry_meta = attributes::derive_fn_entry_count_metadata(
            fn_ctx.and_then(|c| c.entry_count),
            entry_count_map,
            next_meta_id,
        );

        out.push_str(&format!(
            "{} {} @{}({}){}{} {{\n",
            linkage_and_cc, ret_type, f.name, params_sig, fn_attrs_str, entry_meta
        ));

        // Track local variable types and allocas
        let mut local_vars: BTreeSet<String> = BTreeSet::new();
        let mut all_struct_inits: Vec<(ValueId, usize)> = Vec::new();
        let mut escaping_structs: HashSet<ValueId> = HashSet::new();

        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::AssignVar { name, .. } = inst {
                    local_vars.insert(name.clone());
                } else if let Inst::StructInit { dest, fields, .. } = inst {
                    let byte_size = fields.len().saturating_mul(8).max(8);
                    all_struct_inits.push((*dest, byte_size));
                }
            }
        }

        for b in &f.blocks {
            if let Terminator::Return { value: Some(v) } = &b.terminator {
                escaping_structs.insert(*v);
            }
            for inst in &b.instructions {
                match inst {
                    Inst::Call { args, .. } => {
                        for a in args {
                            escaping_structs.insert(*a);
                        }
                    }
                    Inst::MethodCall { object, args, .. } => {
                        escaping_structs.insert(*object);
                        for a in args {
                            escaping_structs.insert(*a);
                        }
                    }
                    Inst::SetField { value, .. } => {
                        escaping_structs.insert(*value);
                    }
                    Inst::StructInit { fields, .. } => {
                        for (_, fv) in fields {
                            escaping_structs.insert(*fv);
                        }
                    }
                    _ => {}
                }
            }
        }

        if f.blocks.len() > 1 {
            for b in &f.blocks {
                if b.id != f.entry_block {
                    for inst in &b.instructions {
                        if let Inst::StructInit { dest, .. } = inst {
                            escaping_structs.insert(*dest);
                        }
                    }
                }
            }
        }

        let mut stack_structs: HashSet<ValueId> = HashSet::new();
        let mut struct_inits: Vec<(ValueId, usize)> = Vec::new();
        for (s_id, s_size) in all_struct_inits {
            if !escaping_structs.contains(&s_id) {
                stack_structs.insert(s_id);
                struct_inits.push((s_id, s_size));
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
                let align = if *s_size >= 64 {
                    64
                } else if *s_size >= 32 {
                    32
                } else {
                    16
                };
                out.push_str(&format!(
                    "  %v{} = alloca [{} x i8], align {}\n",
                    s_id.0, s_size, align
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
                    &stack_structs,
                );
            }

            // Emit terminator
            match &block.terminator {
                Terminator::Branch { target, .. } => {
                    if target.0 <= block.id.0 {
                        let loop_meta = attributes::derive_loop_metadata(
                            &f.name,
                            block.id,
                            self.profile,
                            loop_metadata_map,
                            next_meta_id,
                        );
                        out.push_str(&format!("  br label %bb{}{}\n", target.0, loop_meta));
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
                    let branch_meta = attributes::derive_branch_metadata(
                        &f.name,
                        block.id,
                        *then_block,
                        *else_block,
                        f,
                        self.profile,
                        branch_weights_map,
                        next_meta_id,
                    );
                    if then_block.0 <= block.id.0 || else_block.0 <= block.id.0 {
                        let loop_meta = attributes::derive_loop_metadata(
                            &f.name,
                            block.id,
                            self.profile,
                            loop_metadata_map,
                            next_meta_id,
                        );
                        out.push_str(&format!(
                            "  br i1 {}, label %bb{}, label %bb{}{}{}\n",
                            cmp_reg, then_block.0, else_block.0, branch_meta, loop_meta
                        ));
                    } else {
                        out.push_str(&format!(
                            "  br i1 {}, label %bb{}, label %bb{}{}\n",
                            cmp_reg, then_block.0, else_block.0, branch_meta
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

    pub(crate) fn find_field_offset(
        &self,
        module: &Module,
        class: Option<&str>,
        field: &str,
    ) -> usize {
        // Exact class match first: when the object's class is known, only
        // that class's layout may be consulted — an unrelated class that
        // happens to declare the same field name must not win.
        if let Some(cls) = class {
            let base_c = cls
                .split('<')
                .next()
                .unwrap_or(cls)
                .split('_')
                .next()
                .unwrap_or(cls);
            if let Some(fields) = module
                .class_fields
                .get(cls)
                .or_else(|| module.class_fields.get(base_c))
                && let Some(pos) = fields.iter().position(|f| f == field)
            {
                return pos.saturating_mul(8);
            }
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
    pub profile: Option<crate::pgo::ProfileData>,
}

impl LlvmBackend {
    pub fn new(target: TargetInfo) -> Self {
        Self {
            target,
            debug_info: false,
            profile: None,
        }
    }

    pub fn with_debug(mut self, debug_info: bool) -> Self {
        self.debug_info = debug_info;
        self
    }

    pub fn with_profile(mut self, profile: Option<crate::pgo::ProfileData>) -> Self {
        self.profile = profile;
        self
    }
}

impl CodegenBackend for LlvmBackend {
    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        let emitter = LlvmEmitter::new(&self.target)
            .with_debug(self.debug_info)
            .with_profile(self.profile.as_ref());
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
