use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, Function as ClifFunction, InstBuilder, Signature, StackSlotData,
    StackSlotKind, Type as ClifType, Value as ClifValue, types as clif_types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, Linkage, Module as ClifModule, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

use crate::ast::Program;
use crate::codegen::CodegenBackend;
use crate::codegen::target::TargetInfo;
use crate::dmir::{BasicBlockId, Inst, Module, Terminator, ValueId};
use crate::types::TypeChecker;

use crate::codegen::linker::linker_lock;

pub struct RealCraneliftBackend {
    pub target: TargetInfo,
}

impl RealCraneliftBackend {
    pub fn new(target: TargetInfo) -> Self {
        Self { target }
    }

    pub fn for_host() -> Self {
        Self::new(TargetInfo::host())
    }

    fn clif_type(&self, ty_str: &str) -> ClifType {
        match ty_str {
            "Int" | "Int64" | "UInt64" | "i64" | "u64" | "isize" | "usize" | "USize" => {
                clif_types::I64
            }
            "Int32" | "UInt32" | "i32" | "u32" => clif_types::I32,
            "Int16" | "UInt16" | "i16" | "u16" => clif_types::I16,
            "Int8" | "UInt8" | "i8" | "u8" | "Byte" => clif_types::I8,
            "i128" | "u128" => clif_types::I128,
            "Float" | "Float64" | "f64" => clif_types::F64,
            "Float32" | "f32" | "f16" => clif_types::F32,
            "dec64" | "dec128" => clif_types::I64,
            "Bool" => clif_types::I64,
            "String" | "Str" => clif_types::I64,
            "Unit" => clif_types::I64,
            _ => clif_types::I64,
        }
    }

    pub fn compile_to_object_bytes(&self, dmir_module: &Module) -> Result<Vec<u8>, String> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| e.to_string())?;
        flag_builder
            .set("is_pic", "true")
            .map_err(|e| e.to_string())?;
        let _ = flag_builder.set("preserve_frame_pointers", "false");

        let triple_str = self.target.triple_string();
        let triple: Triple = triple_str
            .parse()
            .map_err(|e: target_lexicon::ParseError| e.to_string())?;
        let mut isa_builder = cranelift_codegen::isa::lookup(triple).map_err(|e| e.to_string())?;

        // Enable hardware CPU acceleration features strictly respecting target specification
        if matches!(self.target.arch, crate::codegen::target::Arch::X86_64) {
            let allow_sse3 = self.target.cpu_features.contains("sse3")
                || self.target.cpu_features.contains("avx2");
            let allow_sse4 = self.target.cpu_features.contains("sse4_2")
                || self.target.cpu_features.contains("avx2");
            let allow_avx = self
                .target
                .vector_support
                .contains(&crate::codegen::target::VectorExtension::Avx)
                || self
                    .target
                    .vector_support
                    .contains(&crate::codegen::target::VectorExtension::Avx2);
            let allow_avx2 = self
                .target
                .vector_support
                .contains(&crate::codegen::target::VectorExtension::Avx2);

            if allow_sse3 {
                let _ = isa_builder.set("has_sse3", "true");
                let _ = isa_builder.set("has_ssse3", "true");
            }
            if allow_sse4 {
                let _ = isa_builder.set("has_sse41", "true");
                let _ = isa_builder.set("has_sse42", "true");
                let _ = isa_builder.set("has_popcnt", "true");
            }
            if allow_avx {
                let _ = isa_builder.set("has_avx", "true");
            }
            if allow_avx2 {
                let _ = isa_builder.set("has_avx2", "true");
                let _ = isa_builder.set("has_fma", "true");
                let _ = isa_builder.set("has_bmi1", "true");
                let _ = isa_builder.set("has_bmi2", "true");
                let _ = isa_builder.set("has_lzcnt", "true");
            }
        }

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| e.to_string())?;

        let call_conv = isa.default_call_conv();
        let frontend_config = isa.frontend_config();
        let builder = ObjectBuilder::new(
            isa,
            dmir_module.name.as_bytes().to_vec(),
            default_libcall_names(),
        )
        .map_err(|e| e.to_string())?;
        let mut module = ObjectModule::new(builder);

        // 1. Declare native Datara runtime functions (datara_runtime.obj)
        let mut rt_out_int_sig = Signature::new(call_conv);
        rt_out_int_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_out_int_id = module
            .declare_function("datara_rt_out_int", Linkage::Import, &rt_out_int_sig)
            .map_err(|e| e.to_string())?;

        let rt_out_bool_id = module
            .declare_function("datara_rt_out_bool", Linkage::Import, &rt_out_int_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_out_flt_sig = Signature::new(call_conv);
        rt_out_flt_sig.params.push(AbiParam::new(clif_types::F64));
        let rt_out_flt_id = module
            .declare_function("datara_rt_out_float", Linkage::Import, &rt_out_flt_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_out_str_sig = Signature::new(call_conv);
        rt_out_str_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_out_str_id = module
            .declare_function("datara_rt_out_str", Linkage::Import, &rt_out_str_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_err_sig = Signature::new(call_conv);
        rt_err_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_err_id = module
            .declare_function("datara_rt_err", Linkage::Import, &rt_err_sig)
            .map_err(|e| e.to_string())?;

        let mut exit_sig = Signature::new(call_conv);
        exit_sig.params.push(AbiParam::new(clif_types::I32));
        let _exit_id = module
            .declare_function("datara_rt_exit", Linkage::Import, &exit_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_concat_sig = Signature::new(call_conv);
        rt_concat_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_concat_id = module
            .declare_function("datara_rt_str_concat", Linkage::Import, &rt_concat_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_concat_3_sig = Signature::new(call_conv);
        rt_concat_3_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_3_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_3_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_3_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_concat_3_id = module
            .declare_function("datara_rt_str_concat_3", Linkage::Import, &rt_concat_3_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_concat_4_sig = Signature::new(call_conv);
        rt_concat_4_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_4_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_4_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_4_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_4_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_concat_4_id = module
            .declare_function("datara_rt_str_concat_4", Linkage::Import, &rt_concat_4_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_concat_5_sig = Signature::new(call_conv);
        rt_concat_5_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_5_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_5_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_5_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_5_sig.params.push(AbiParam::new(clif_types::I64));
        rt_concat_5_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_concat_5_id = module
            .declare_function("datara_rt_str_concat_5", Linkage::Import, &rt_concat_5_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_int_to_str_sig = Signature::new(call_conv);
        rt_int_to_str_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_int_to_str_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_int_to_str_id = module
            .declare_function("datara_rt_int_to_str", Linkage::Import, &rt_int_to_str_sig)
            .map_err(|e| e.to_string())?;

        let rt_bool_to_str_id = module
            .declare_function("datara_rt_bool_to_str", Linkage::Import, &rt_int_to_str_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_flt_to_str_sig = Signature::new(call_conv);
        rt_flt_to_str_sig
            .params
            .push(AbiParam::new(clif_types::F64));
        rt_flt_to_str_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_flt_to_str_id = module
            .declare_function(
                "datara_rt_float_to_str",
                Linkage::Import,
                &rt_flt_to_str_sig,
            )
            .map_err(|e| e.to_string())?;

        let mut malloc_sig = Signature::new(call_conv);
        malloc_sig.params.push(AbiParam::new(clif_types::I64));
        malloc_sig.returns.push(AbiParam::new(clif_types::I64));
        let malloc_id = module
            .declare_function("malloc", Linkage::Import, &malloc_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_list_get_sig = Signature::new(call_conv);
        rt_list_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_list_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_list_get_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_list_get_id = module
            .declare_function("datara_rt_list_get", Linkage::Import, &rt_list_get_sig)
            .map_err(|e| e.to_string())?;

        // datara_rt_list_len(ptr) -> I64 shares the two-arg I64 signature;
        // declaring it with the single-param list_get shape would misalign
        // the ABI, so give it its own exact signature.
        let mut rt_list_len_sig = Signature::new(call_conv);
        rt_list_len_sig.params.push(AbiParam::new(clif_types::I64));
        rt_list_len_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_list_len_id = module
            .declare_function("datara_rt_list_len", Linkage::Import, &rt_list_len_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_list_set_sig = Signature::new(call_conv);
        for _ in 0..3 {
            rt_list_set_sig.params.push(AbiParam::new(clif_types::I64));
        }
        rt_list_set_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_list_set_id = module
            .declare_function("datara_rt_list_set", Linkage::Import, &rt_list_set_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_list_append_sig = Signature::new(call_conv);
        for _ in 0..2 {
            rt_list_append_sig
                .params
                .push(AbiParam::new(clif_types::I64));
        }
        rt_list_append_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_list_append_id = module
            .declare_function(
                "datara_rt_list_append",
                Linkage::Import,
                &rt_list_append_sig,
            )
            .map_err(|e| e.to_string())?;

        let mut rt_list_repeat_sig = Signature::new(call_conv);
        rt_list_repeat_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_list_repeat_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_list_repeat_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_list_repeat_id = module
            .declare_function(
                "datara_rt_list_create_repeat",
                Linkage::Import,
                &rt_list_repeat_sig,
            )
            .map_err(|e| e.to_string())?;

        let mut rt_map_create_2_sig = Signature::new(call_conv);
        for _ in 0..4 {
            rt_map_create_2_sig
                .params
                .push(AbiParam::new(clif_types::I64));
        }
        rt_map_create_2_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_map_create_2_id = module
            .declare_function(
                "datara_rt_map_create_2",
                Linkage::Import,
                &rt_map_create_2_sig,
            )
            .map_err(|e| e.to_string())?;

        let mut rt_map_get_sig = Signature::new(call_conv);
        rt_map_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_map_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_map_get_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_map_get_id = module
            .declare_function("datara_rt_map_get", Linkage::Import, &rt_map_get_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_map_insert_sig = Signature::new(call_conv);
        rt_map_insert_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_map_insert_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_map_insert_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_map_insert_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_map_insert_id = module
            .declare_function("datara_rt_map_insert", Linkage::Import, &rt_map_insert_sig)
            .map_err(|e| e.to_string())?;

        let mut rt_range_str_sig = Signature::new(call_conv);
        rt_range_str_sig.params.push(AbiParam::new(clif_types::I64));
        rt_range_str_sig.params.push(AbiParam::new(clif_types::I64));
        rt_range_str_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_range_str_id = module
            .declare_function("datara_rt_range_str", Linkage::Import, &rt_range_str_sig)
            .map_err(|e| e.to_string())?;

        // 2. Declare all module functions
        let mut func_ids = HashMap::new();
        for (name, f) in &dmir_module.functions {
            let mut sig = Signature::new(call_conv);
            for (_, p_type, _) in &f.params {
                sig.params.push(AbiParam::new(self.clif_type(p_type)));
            }
            if f.return_type != "Unit" {
                sig.returns
                    .push(AbiParam::new(self.clif_type(&f.return_type)));
            }

            let symbol_name = if name == "main" {
                "datara_main"
            } else {
                name.as_str()
            };
            let func_id = module
                .declare_function(symbol_name, Linkage::Export, &sig)
                .map_err(|e| e.to_string())?;
            func_ids.insert(name.clone(), (func_id, sig));
        }

        func_ids.insert(
            "datara_rt_list_get".into(),
            (rt_list_get_id, rt_list_get_sig),
        );
        func_ids.insert(
            "datara_rt_list_len".into(),
            (rt_list_len_id, rt_list_len_sig),
        );
        func_ids.insert(
            "datara_rt_list_set".into(),
            (rt_list_set_id, rt_list_set_sig),
        );
        func_ids.insert(
            "datara_rt_list_append".into(),
            (rt_list_append_id, rt_list_append_sig),
        );
        func_ids.insert(
            "datara_rt_list_create_repeat".into(),
            (rt_list_repeat_id, rt_list_repeat_sig),
        );
        func_ids.insert(
            "datara_rt_str_concat".into(),
            (rt_concat_id, rt_concat_sig.clone()),
        );
        func_ids.insert(
            "datara_rt_str_concat_3".into(),
            (rt_concat_3_id, rt_concat_3_sig),
        );
        func_ids.insert(
            "datara_rt_str_concat_4".into(),
            (rt_concat_4_id, rt_concat_4_sig),
        );
        func_ids.insert(
            "datara_rt_str_concat_5".into(),
            (rt_concat_5_id, rt_concat_5_sig),
        );
        let mut rt_format_sisi_sig = Signature::new(call_conv);
        rt_format_sisi_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_format_sisi_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_format_sisi_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_format_sisi_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_format_sisi_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_format_sisi_id = module
            .declare_function(
                "datara_rt_format_str_i64_str_i64",
                Linkage::Import,
                &rt_format_sisi_sig,
            )
            .expect("declare datara_rt_format_str_i64_str_i64");
        func_ids.insert(
            "datara_rt_format_str_i64_str_i64".into(),
            (rt_format_sisi_id, rt_format_sisi_sig),
        );
        func_ids.insert(
            "datara_rt_map_create_2".into(),
            (rt_map_create_2_id, rt_map_create_2_sig),
        );
        func_ids.insert("datara_rt_map_get".into(), (rt_map_get_id, rt_map_get_sig));
        func_ids.insert(
            "datara_rt_map_insert".into(),
            (rt_map_insert_id, rt_map_insert_sig),
        );
        func_ids.insert(
            "datara_rt_range_str".into(),
            (rt_range_str_id, rt_range_str_sig),
        );

        let mut now_ms_sig = Signature::new(call_conv);
        now_ms_sig.returns.push(AbiParam::new(clif_types::I64));
        let now_ms_id = module
            .declare_function("now_ms", Linkage::Import, &now_ms_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("now_ms".into(), (now_ms_id, now_ms_sig.clone()));
        func_ids.insert("now".into(), (now_ms_id, now_ms_sig.clone()));
        func_ids.insert("datara_rt_now_ms".into(), (now_ms_id, now_ms_sig.clone()));

        let mut now_precise_sig = Signature::new(call_conv);
        now_precise_sig.returns.push(AbiParam::new(clif_types::I64));
        let now_precise_id = module
            .declare_function(
                "datara_rt_now_precise_ms",
                Linkage::Import,
                &now_precise_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert("now_precise_ms".into(), (now_precise_id, now_precise_sig));

        let mut sleep_sig = Signature::new(call_conv);
        sleep_sig.params.push(AbiParam::new(clif_types::I64));
        let sleep_id = module
            .declare_function("datara_rt_sleep", Linkage::Import, &sleep_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("sleep".into(), (sleep_id, sleep_sig));

        let mut str_len_sig = Signature::new(call_conv);
        str_len_sig.params.push(AbiParam::new(clif_types::I64));
        str_len_sig.returns.push(AbiParam::new(clif_types::I64));
        let str_len_id = module
            .declare_function("datara_rt_str_len", Linkage::Import, &str_len_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("str_len".into(), (str_len_id, str_len_sig.clone()));
        func_ids.insert("len".into(), (str_len_id, str_len_sig.clone()));
        func_ids.insert(
            "datara_rt_str_len".into(),
            (str_len_id, str_len_sig.clone()),
        );
        func_ids.insert("datara_rt_len".into(), (str_len_id, str_len_sig));

        let mut rt_println_sig = Signature::new(call_conv);
        rt_println_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_println_id = module
            .declare_function("datara_rt_println", Linkage::Import, &rt_println_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_println".into(),
            (rt_println_id, rt_println_sig.clone()),
        );
        func_ids.insert("println".into(), (rt_println_id, rt_println_sig));

        let mut rt_print_sig = Signature::new(call_conv);
        rt_print_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_print_id = module
            .declare_function("datara_rt_print", Linkage::Import, &rt_print_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_print".into(),
            (rt_print_id, rt_print_sig.clone()),
        );
        func_ids.insert("print".into(), (rt_print_id, rt_print_sig));

        let mut rt_eprintln_sig = Signature::new(call_conv);
        rt_eprintln_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_eprintln_id = module
            .declare_function("datara_rt_eprintln", Linkage::Import, &rt_eprintln_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_eprintln".into(),
            (rt_eprintln_id, rt_eprintln_sig.clone()),
        );
        func_ids.insert("eprintln".into(), (rt_eprintln_id, rt_eprintln_sig));

        let mut rt_panic_sig = Signature::new(call_conv);
        rt_panic_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_panic_id = module
            .declare_function("datara_rt_panic", Linkage::Import, &rt_panic_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_panic".into(),
            (rt_panic_id, rt_panic_sig.clone()),
        );
        func_ids.insert("panic".into(), (rt_panic_id, rt_panic_sig));

        let mut rt_assert_sig = Signature::new(call_conv);
        rt_assert_sig.params.push(AbiParam::new(clif_types::I64));
        rt_assert_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_assert_id = module
            .declare_function("datara_rt_assert", Linkage::Import, &rt_assert_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_assert".into(),
            (rt_assert_id, rt_assert_sig.clone()),
        );
        func_ids.insert("assert".into(), (rt_assert_id, rt_assert_sig));

        let mut rt_input_sig = Signature::new(call_conv);
        rt_input_sig.params.push(AbiParam::new(clif_types::I64));
        rt_input_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_input_id = module
            .declare_function("datara_rt_input", Linkage::Import, &rt_input_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_input".into(),
            (rt_input_id, rt_input_sig.clone()),
        );
        func_ids.insert("input".into(), (rt_input_id, rt_input_sig.clone()));
        func_ids.insert("read_line".into(), (rt_input_id, rt_input_sig));

        let mut rt_pop_sig = Signature::new(call_conv);
        rt_pop_sig.params.push(AbiParam::new(clif_types::I64));
        rt_pop_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_pop_id = module
            .declare_function("datara_rt_list_pop", Linkage::Import, &rt_pop_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_list_pop".into(), (rt_pop_id, rt_pop_sig));

        let mut rt_slice_sig = Signature::new(call_conv);
        rt_slice_sig.params.push(AbiParam::new(clif_types::I64));
        rt_slice_sig.params.push(AbiParam::new(clif_types::I64));
        rt_slice_sig.params.push(AbiParam::new(clif_types::I64));
        rt_slice_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_slice_id = module
            .declare_function("datara_rt_slice", Linkage::Import, &rt_slice_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_slice".into(), (rt_slice_id, rt_slice_sig));

        let mut rt_out_dec_sig = Signature::new(call_conv);
        rt_out_dec_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_out_dec_id = module
            .declare_function("datara_rt_out_dec64", Linkage::Import, &rt_out_dec_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_out_dec64".into(),
            (rt_out_dec_id, rt_out_dec_sig),
        );

        let mut rt_out_val_sig = Signature::new(call_conv);
        rt_out_val_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_out_val_id = module
            .declare_function("datara_rt_out_val", Linkage::Import, &rt_out_val_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_out_val".into(), (rt_out_val_id, rt_out_val_sig));

        // File I/O: write
        let mut rt_file_write_sig = Signature::new(call_conv);
        rt_file_write_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_file_write_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_file_write_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_file_write_id = module
            .declare_function("datara_rt_file_write", Linkage::Import, &rt_file_write_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_file_write".into(),
            (rt_file_write_id, rt_file_write_sig.clone()),
        );
        func_ids.insert(
            "file_write".into(),
            (rt_file_write_id, rt_file_write_sig.clone()),
        );
        func_ids.insert("write".into(), (rt_file_write_id, rt_file_write_sig));

        // File I/O: append
        let mut rt_file_append_sig = Signature::new(call_conv);
        rt_file_append_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_file_append_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_file_append_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_file_append_id = module
            .declare_function(
                "datara_rt_file_append",
                Linkage::Import,
                &rt_file_append_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_file_append".into(),
            (rt_file_append_id, rt_file_append_sig.clone()),
        );
        func_ids.insert(
            "file_append".into(),
            (rt_file_append_id, rt_file_append_sig),
        );

        // File I/O: read
        let mut rt_file_read_sig = Signature::new(call_conv);
        rt_file_read_sig.params.push(AbiParam::new(clif_types::I64));
        rt_file_read_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_file_read_id = module
            .declare_function("datara_rt_file_read", Linkage::Import, &rt_file_read_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_file_read".into(),
            (rt_file_read_id, rt_file_read_sig.clone()),
        );
        func_ids.insert(
            "file_read".into(),
            (rt_file_read_id, rt_file_read_sig.clone()),
        );
        func_ids.insert("read".into(), (rt_file_read_id, rt_file_read_sig));

        // File I/O: exists
        let mut rt_file_exists_sig = Signature::new(call_conv);
        rt_file_exists_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_file_exists_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_file_exists_id = module
            .declare_function(
                "datara_rt_file_exists",
                Linkage::Import,
                &rt_file_exists_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_file_exists".into(),
            (rt_file_exists_id, rt_file_exists_sig.clone()),
        );
        func_ids.insert(
            "file_exists".into(),
            (rt_file_exists_id, rt_file_exists_sig),
        );

        // Timing: sleep
        let mut rt_sleep_sig = Signature::new(call_conv);
        rt_sleep_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_sleep_id = module
            .declare_function("datara_rt_sleep", Linkage::Import, &rt_sleep_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_sleep".into(),
            (rt_sleep_id, rt_sleep_sig.clone()),
        );
        func_ids.insert("sleep".into(), (rt_sleep_id, rt_sleep_sig));

        // Environment: env_get
        let mut rt_env_get_sig = Signature::new(call_conv);
        rt_env_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_env_get_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_env_get_id = module
            .declare_function("datara_rt_env_get", Linkage::Import, &rt_env_get_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_env_get".into(),
            (rt_env_get_id, rt_env_get_sig.clone()),
        );
        func_ids.insert("env_get".into(), (rt_env_get_id, rt_env_get_sig));

        // CLI Args: args_count & args_get
        let mut rt_args_count_sig = Signature::new(call_conv);
        rt_args_count_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_args_count_id = module
            .declare_function("datara_rt_args_count", Linkage::Import, &rt_args_count_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_args_count".into(),
            (rt_args_count_id, rt_args_count_sig.clone()),
        );
        func_ids.insert("args_count".into(), (rt_args_count_id, rt_args_count_sig));

        let mut rt_args_get_sig = Signature::new(call_conv);
        rt_args_get_sig.params.push(AbiParam::new(clif_types::I64));
        rt_args_get_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_args_get_id = module
            .declare_function("datara_rt_args_get", Linkage::Import, &rt_args_get_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_args_get".into(),
            (rt_args_get_id, rt_args_get_sig.clone()),
        );
        func_ids.insert("args_get".into(), (rt_args_get_id, rt_args_get_sig));

        let mut rt_set_args_sig = Signature::new(call_conv);
        rt_set_args_sig.params.push(AbiParam::new(clif_types::I32));
        rt_set_args_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_set_args_id = module
            .declare_function("datara_rt_set_args", Linkage::Import, &rt_set_args_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_set_args".into(),
            (rt_set_args_id, rt_set_args_sig),
        );

        // String helpers
        let mut rt_str_contains_sig = Signature::new(call_conv);
        rt_str_contains_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_contains_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_contains_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_contains_id = module
            .declare_function(
                "datara_rt_str_contains",
                Linkage::Import,
                &rt_str_contains_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_contains".into(),
            (rt_str_contains_id, rt_str_contains_sig.clone()),
        );
        func_ids.insert(
            "str_contains".into(),
            (rt_str_contains_id, rt_str_contains_sig),
        );

        let mut rt_str_starts_with_sig = Signature::new(call_conv);
        rt_str_starts_with_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_starts_with_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_starts_with_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_starts_with_id = module
            .declare_function(
                "datara_rt_str_starts_with",
                Linkage::Import,
                &rt_str_starts_with_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_starts_with".into(),
            (rt_str_starts_with_id, rt_str_starts_with_sig.clone()),
        );
        func_ids.insert(
            "str_starts_with".into(),
            (rt_str_starts_with_id, rt_str_starts_with_sig),
        );

        let mut rt_str_ends_with_sig = Signature::new(call_conv);
        rt_str_ends_with_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_ends_with_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_ends_with_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_ends_with_id = module
            .declare_function(
                "datara_rt_str_ends_with",
                Linkage::Import,
                &rt_str_ends_with_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_ends_with".into(),
            (rt_str_ends_with_id, rt_str_ends_with_sig.clone()),
        );
        func_ids.insert(
            "str_ends_with".into(),
            (rt_str_ends_with_id, rt_str_ends_with_sig),
        );

        let mut rt_str_index_of_sig = Signature::new(call_conv);
        rt_str_index_of_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_index_of_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_index_of_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_index_of_id = module
            .declare_function(
                "datara_rt_str_index_of",
                Linkage::Import,
                &rt_str_index_of_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_index_of".into(),
            (rt_str_index_of_id, rt_str_index_of_sig.clone()),
        );
        func_ids.insert(
            "str_index_of".into(),
            (rt_str_index_of_id, rt_str_index_of_sig),
        );

        let mut rt_str_trim_sig = Signature::new(call_conv);
        rt_str_trim_sig.params.push(AbiParam::new(clif_types::I64));
        rt_str_trim_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_str_trim_id = module
            .declare_function("datara_rt_str_trim", Linkage::Import, &rt_str_trim_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_trim".into(),
            (rt_str_trim_id, rt_str_trim_sig.clone()),
        );
        func_ids.insert("str_trim".into(), (rt_str_trim_id, rt_str_trim_sig));

        let mut rt_str_to_int_sig = Signature::new(call_conv);
        rt_str_to_int_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_to_int_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_to_int_id = module
            .declare_function("datara_rt_str_to_int", Linkage::Import, &rt_str_to_int_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_to_int".into(),
            (rt_str_to_int_id, rt_str_to_int_sig.clone()),
        );
        func_ids.insert("str_to_int".into(), (rt_str_to_int_id, rt_str_to_int_sig));

        let mut rt_str_to_float_sig = Signature::new(call_conv);
        rt_str_to_float_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_to_float_sig
            .returns
            .push(AbiParam::new(clif_types::F64));
        let rt_str_to_float_id = module
            .declare_function(
                "datara_rt_str_to_float",
                Linkage::Import,
                &rt_str_to_float_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_to_float".into(),
            (rt_str_to_float_id, rt_str_to_float_sig.clone()),
        );
        func_ids.insert(
            "str_to_float".into(),
            (rt_str_to_float_id, rt_str_to_float_sig),
        );

        let mut rt_str_substring_sig = Signature::new(call_conv);
        rt_str_substring_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_substring_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_substring_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_substring_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_substring_id = module
            .declare_function(
                "datara_rt_str_substring",
                Linkage::Import,
                &rt_str_substring_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_substring".into(),
            (rt_str_substring_id, rt_str_substring_sig.clone()),
        );
        func_ids.insert(
            "str_substring".into(),
            (rt_str_substring_id, rt_str_substring_sig.clone()),
        );
        func_ids.insert(
            "substring".into(),
            (rt_str_substring_id, rt_str_substring_sig),
        );

        let mut rt_str_char_at_sig = Signature::new(call_conv);
        rt_str_char_at_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_char_at_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_str_char_at_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_str_char_at_id = module
            .declare_function(
                "datara_rt_str_char_at",
                Linkage::Import,
                &rt_str_char_at_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_char_at".into(),
            (rt_str_char_at_id, rt_str_char_at_sig.clone()),
        );
        func_ids.insert(
            "str_char_at".into(),
            (rt_str_char_at_id, rt_str_char_at_sig.clone()),
        );
        func_ids.insert("char_at".into(), (rt_str_char_at_id, rt_str_char_at_sig));

        // String equality: datara_rt_str_eq
        let mut rt_str_eq_sig = Signature::new(call_conv);
        rt_str_eq_sig.params.push(AbiParam::new(clif_types::I64));
        rt_str_eq_sig.params.push(AbiParam::new(clif_types::I64));
        rt_str_eq_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_str_eq_id = module
            .declare_function("datara_rt_str_eq", Linkage::Import, &rt_str_eq_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_eq".into(),
            (rt_str_eq_id, rt_str_eq_sig.clone()),
        );
        func_ids.insert("str_eq".into(), (rt_str_eq_id, rt_str_eq_sig));

        // Network: socket_create
        let mut rt_sock_create_sig = Signature::new(call_conv);
        rt_sock_create_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_create_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_create_id = module
            .declare_function(
                "datara_rt_socket_create",
                Linkage::Import,
                &rt_sock_create_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_create".into(),
            (rt_sock_create_id, rt_sock_create_sig.clone()),
        );
        func_ids.insert(
            "socket_create".into(),
            (rt_sock_create_id, rt_sock_create_sig),
        );

        // Network: socket_bind
        let mut rt_sock_bind_sig = Signature::new(call_conv);
        rt_sock_bind_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_bind_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_bind_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_bind_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_bind_id = module
            .declare_function("datara_rt_socket_bind", Linkage::Import, &rt_sock_bind_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_bind".into(),
            (rt_sock_bind_id, rt_sock_bind_sig.clone()),
        );
        func_ids.insert("socket_bind".into(), (rt_sock_bind_id, rt_sock_bind_sig));

        // Network: socket_listen
        let mut rt_sock_listen_sig = Signature::new(call_conv);
        rt_sock_listen_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_listen_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_listen_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_listen_id = module
            .declare_function(
                "datara_rt_socket_listen",
                Linkage::Import,
                &rt_sock_listen_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_listen".into(),
            (rt_sock_listen_id, rt_sock_listen_sig.clone()),
        );
        func_ids.insert(
            "socket_listen".into(),
            (rt_sock_listen_id, rt_sock_listen_sig),
        );

        // Network: socket_accept
        let mut rt_sock_accept_sig = Signature::new(call_conv);
        rt_sock_accept_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_accept_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_accept_id = module
            .declare_function(
                "datara_rt_socket_accept",
                Linkage::Import,
                &rt_sock_accept_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_accept".into(),
            (rt_sock_accept_id, rt_sock_accept_sig.clone()),
        );
        func_ids.insert(
            "socket_accept".into(),
            (rt_sock_accept_id, rt_sock_accept_sig),
        );

        // Network: socket_connect
        let mut rt_sock_connect_sig = Signature::new(call_conv);
        rt_sock_connect_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_connect_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_connect_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        rt_sock_connect_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_connect_id = module
            .declare_function(
                "datara_rt_socket_connect",
                Linkage::Import,
                &rt_sock_connect_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_connect".into(),
            (rt_sock_connect_id, rt_sock_connect_sig.clone()),
        );
        func_ids.insert(
            "socket_connect".into(),
            (rt_sock_connect_id, rt_sock_connect_sig),
        );

        // Network: socket_send
        let mut rt_sock_send_sig = Signature::new(call_conv);
        rt_sock_send_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_send_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_send_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_send_id = module
            .declare_function("datara_rt_socket_send", Linkage::Import, &rt_sock_send_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_send".into(),
            (rt_sock_send_id, rt_sock_send_sig.clone()),
        );
        func_ids.insert("socket_send".into(), (rt_sock_send_id, rt_sock_send_sig));

        // Network: socket_recv
        let mut rt_sock_recv_sig = Signature::new(call_conv);
        rt_sock_recv_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_recv_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sock_recv_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_recv_id = module
            .declare_function("datara_rt_socket_recv", Linkage::Import, &rt_sock_recv_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_recv".into(),
            (rt_sock_recv_id, rt_sock_recv_sig.clone()),
        );
        func_ids.insert("socket_recv".into(), (rt_sock_recv_id, rt_sock_recv_sig));

        // Network: socket_close
        let mut rt_sock_close_sig = Signature::new(call_conv);
        rt_sock_close_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        let rt_sock_close_id = module
            .declare_function(
                "datara_rt_socket_close",
                Linkage::Import,
                &rt_sock_close_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_socket_close".into(),
            (rt_sock_close_id, rt_sock_close_sig.clone()),
        );
        func_ids.insert("socket_close".into(), (rt_sock_close_id, rt_sock_close_sig));

        // Network: http_get
        let mut rt_http_get_sig = Signature::new(call_conv);
        rt_http_get_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_http_get_id = module
            .declare_function("datara_rt_http_get", Linkage::Import, &rt_http_get_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_http_get".into(),
            (rt_http_get_id, rt_http_get_sig.clone()),
        );
        func_ids.insert("http_get".into(), (rt_http_get_id, rt_http_get_sig));

        // Crypto: sha256
        let mut rt_sha256_sig = Signature::new(call_conv);
        rt_sha256_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sha256_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_sha256_id = module
            .declare_function("datara_rt_sha256", Linkage::Import, &rt_sha256_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_sha256".into(),
            (rt_sha256_id, rt_sha256_sig.clone()),
        );
        func_ids.insert("sha256".into(), (rt_sha256_id, rt_sha256_sig));

        // Crypto: base64_encode
        let mut rt_b64e_sig = Signature::new(call_conv);
        rt_b64e_sig.params.push(AbiParam::new(clif_types::I64));
        rt_b64e_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_b64e_id = module
            .declare_function("datara_rt_base64_encode", Linkage::Import, &rt_b64e_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_base64_encode".into(),
            (rt_b64e_id, rt_b64e_sig.clone()),
        );
        func_ids.insert("base64_encode".into(), (rt_b64e_id, rt_b64e_sig));

        // Crypto: base64_decode
        let mut rt_b64d_sig = Signature::new(call_conv);
        rt_b64d_sig.params.push(AbiParam::new(clif_types::I64));
        rt_b64d_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_b64d_id = module
            .declare_function("datara_rt_base64_decode", Linkage::Import, &rt_b64d_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_base64_decode".into(),
            (rt_b64d_id, rt_b64d_sig.clone()),
        );
        func_ids.insert("base64_decode".into(), (rt_b64d_id, rt_b64d_sig));

        // System: process_run / datara_rt_system
        let mut rt_sys_sig = Signature::new(call_conv);
        rt_sys_sig.params.push(AbiParam::new(clif_types::I64));
        rt_sys_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_sys_id = module
            .declare_function("datara_rt_system", Linkage::Import, &rt_sys_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_system".into(), (rt_sys_id, rt_sys_sig.clone()));
        func_ids.insert("system".into(), (rt_sys_id, rt_sys_sig.clone()));
        func_ids.insert("process_run".into(), (rt_sys_id, rt_sys_sig));

        // System: process_output / datara_rt_exec
        let mut rt_exec_sig = Signature::new(call_conv);
        rt_exec_sig.params.push(AbiParam::new(clif_types::I64));
        rt_exec_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_exec_id = module
            .declare_function("datara_rt_exec", Linkage::Import, &rt_exec_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_exec".into(), (rt_exec_id, rt_exec_sig.clone()));
        func_ids.insert("exec".into(), (rt_exec_id, rt_exec_sig.clone()));
        func_ids.insert("process_output".into(), (rt_exec_id, rt_exec_sig));

        // Multithreading runtime: parallel_for, parallel_invoke, num_workers
        let mut rt_par_for_sig = Signature::new(call_conv);
        rt_par_for_sig.params.push(AbiParam::new(clif_types::I64)); // start
        rt_par_for_sig.params.push(AbiParam::new(clif_types::I64)); // end
        rt_par_for_sig.params.push(AbiParam::new(clif_types::I64)); // worker_fn
        rt_par_for_sig.params.push(AbiParam::new(clif_types::I64)); // ctx
        let rt_par_for_id = module
            .declare_function("datara_rt_parallel_for", Linkage::Import, &rt_par_for_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_parallel_for".into(),
            (rt_par_for_id, rt_par_for_sig.clone()),
        );
        func_ids.insert("parallel_for".into(), (rt_par_for_id, rt_par_for_sig));

        let mut rt_par_invoke_sig = Signature::new(call_conv);
        rt_par_invoke_sig
            .params
            .push(AbiParam::new(clif_types::I64)); // fn1
        rt_par_invoke_sig
            .params
            .push(AbiParam::new(clif_types::I64)); // ctx1
        rt_par_invoke_sig
            .params
            .push(AbiParam::new(clif_types::I64)); // fn2
        rt_par_invoke_sig
            .params
            .push(AbiParam::new(clif_types::I64)); // ctx2
        let rt_par_invoke_id = module
            .declare_function(
                "datara_rt_parallel_invoke",
                Linkage::Import,
                &rt_par_invoke_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_parallel_invoke".into(),
            (rt_par_invoke_id, rt_par_invoke_sig.clone()),
        );
        func_ids.insert(
            "parallel_invoke".into(),
            (rt_par_invoke_id, rt_par_invoke_sig),
        );

        let mut rt_num_workers_sig = Signature::new(call_conv);
        rt_num_workers_sig
            .returns
            .push(AbiParam::new(clif_types::I64));
        let rt_num_workers_id = module
            .declare_function(
                "datara_rt_num_workers",
                Linkage::Import,
                &rt_num_workers_sig,
            )
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_num_workers".into(),
            (rt_num_workers_id, rt_num_workers_sig.clone()),
        );
        func_ids.insert(
            "num_workers".into(),
            (rt_num_workers_id, rt_num_workers_sig),
        );

        // Fast Math: 1-arg double -> double (sqrt, abs, sin, cos, tan, floor, ceil, round)
        let f64_1_funcs = [
            ("datara_rt_math_sqrt", "math_sqrt"),
            ("datara_rt_math_abs", "math_abs"),
            ("datara_rt_math_sin", "math_sin"),
            ("datara_rt_math_cos", "math_cos"),
            ("datara_rt_math_tan", "math_tan"),
            ("datara_rt_math_floor", "math_floor"),
            ("datara_rt_math_ceil", "math_ceil"),
            ("datara_rt_math_round", "math_round"),
        ];
        for (rt_name, alias) in &f64_1_funcs {
            let mut sig = Signature::new(call_conv);
            sig.params.push(AbiParam::new(clif_types::F64));
            sig.returns.push(AbiParam::new(clif_types::F64));
            let id = module
                .declare_function(rt_name, Linkage::Import, &sig)
                .map_err(|e| e.to_string())?;
            func_ids.insert(rt_name.to_string(), (id, sig.clone()));
            func_ids.insert(alias.to_string(), (id, sig));
        }

        // Fast Math: 2-arg double -> double (pow, min, max, hypot)
        let f64_2_funcs = [
            ("datara_rt_math_pow", "math_pow"),
            ("datara_rt_math_min", "math_min"),
            ("datara_rt_math_max", "math_max"),
            ("datara_rt_math_hypot", "math_hypot"),
        ];
        for (rt_name, alias) in &f64_2_funcs {
            let mut sig = Signature::new(call_conv);
            sig.params.push(AbiParam::new(clif_types::F64));
            sig.params.push(AbiParam::new(clif_types::F64));
            sig.returns.push(AbiParam::new(clif_types::F64));
            let id = module
                .declare_function(rt_name, Linkage::Import, &sig)
                .map_err(|e| e.to_string())?;
            func_ids.insert(rt_name.to_string(), (id, sig.clone()));
            func_ids.insert(alias.to_string(), (id, sig));
        }

        // Fast Math: int functions
        let mut i64_2_sig = Signature::new(call_conv);
        i64_2_sig.params.push(AbiParam::new(clif_types::I64));
        i64_2_sig.params.push(AbiParam::new(clif_types::I64));
        i64_2_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_min_int_id = module
            .declare_function("datara_rt_math_min_int", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_math_min_int".into(),
            (rt_min_int_id, i64_2_sig.clone()),
        );
        func_ids.insert("math_min_int".into(), (rt_min_int_id, i64_2_sig.clone()));
        let rt_max_int_id = module
            .declare_function("datara_rt_math_max_int", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_math_max_int".into(),
            (rt_max_int_id, i64_2_sig.clone()),
        );
        func_ids.insert("math_max_int".into(), (rt_max_int_id, i64_2_sig.clone()));

        let rt_shr_id = module
            .declare_function("datara_rt_math_shr", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_shr".into(), (rt_shr_id, i64_2_sig.clone()));
        func_ids.insert("math_shr".into(), (rt_shr_id, i64_2_sig.clone()));
        func_ids.insert("shr".into(), (rt_shr_id, i64_2_sig.clone()));

        let rt_shl_id = module
            .declare_function("datara_rt_math_shl", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_shl".into(), (rt_shl_id, i64_2_sig.clone()));
        func_ids.insert("math_shl".into(), (rt_shl_id, i64_2_sig.clone()));
        func_ids.insert("shl".into(), (rt_shl_id, i64_2_sig.clone()));

        let mut i64_1_sig = Signature::new(call_conv);
        i64_1_sig.params.push(AbiParam::new(clif_types::I64));
        i64_1_sig.returns.push(AbiParam::new(clif_types::I64));
        let rt_abs_int_id = module
            .declare_function("datara_rt_math_abs_int", Linkage::Import, &i64_1_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_math_abs_int".into(),
            (rt_abs_int_id, i64_1_sig.clone()),
        );
        func_ids.insert("math_abs_int".into(), (rt_abs_int_id, i64_1_sig.clone()));

        let rt_ctz_id = module
            .declare_function("datara_rt_math_ctz", Linkage::Import, &i64_1_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_ctz".into(), (rt_ctz_id, i64_1_sig.clone()));
        func_ids.insert("math_ctz".into(), (rt_ctz_id, i64_1_sig.clone()));
        func_ids.insert("ctz".into(), (rt_ctz_id, i64_1_sig));

        let rt_xor_id = module
            .declare_function("datara_rt_math_xor", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_xor".into(), (rt_xor_id, i64_2_sig.clone()));
        func_ids.insert("math_xor".into(), (rt_xor_id, i64_2_sig.clone()));
        func_ids.insert("xor".into(), (rt_xor_id, i64_2_sig.clone()));

        let rt_and_id = module
            .declare_function("datara_rt_math_and", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_and".into(), (rt_and_id, i64_2_sig.clone()));
        func_ids.insert("math_and".into(), (rt_and_id, i64_2_sig.clone()));
        func_ids.insert("and".into(), (rt_and_id, i64_2_sig.clone()));

        let rt_or_id = module
            .declare_function("datara_rt_math_or", Linkage::Import, &i64_2_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_math_or".into(), (rt_or_id, i64_2_sig.clone()));
        func_ids.insert("math_or".into(), (rt_or_id, i64_2_sig.clone()));
        func_ids.insert("or".into(), (rt_or_id, i64_2_sig));

        // SIMD vector builtins (float4/int4/dot/min4/max4) have no scalar
        // Cranelift lowering: the C runtime returns/accepts 16-byte structs
        // whose ABI cannot be expressed with scalar signatures. They are
        // rejected with a clear diagnostic in the Call lowering below and
        // fully supported by the LLVM backend (`--llvm`).

        // Memory deallocation runtime: free, str_free, list_free
        let mut rt_free_sig = Signature::new(call_conv);
        rt_free_sig.params.push(AbiParam::new(clif_types::I64));
        let rt_free_id = module
            .declare_function("datara_rt_free", Linkage::Import, &rt_free_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_free".into(), (rt_free_id, rt_free_sig.clone()));
        func_ids.insert("free".into(), (rt_free_id, rt_free_sig.clone()));

        let rt_str_free_id = module
            .declare_function("datara_rt_str_free", Linkage::Import, &rt_free_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_str_free".into(),
            (rt_str_free_id, rt_free_sig.clone()),
        );

        let rt_list_free_id = module
            .declare_function("datara_rt_list_free", Linkage::Import, &rt_free_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_rt_list_free".into(),
            (rt_list_free_id, rt_free_sig.clone()),
        );

        let rt_map_free_id = module
            .declare_function("datara_rt_map_free", Linkage::Import, &rt_free_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_map_free".into(), (rt_map_free_id, rt_free_sig));

        // Declare native extern "C" functions
        for (ef_name, (ef_params, ef_ret)) in &dmir_module.extern_functions {
            let mut sig = Signature::new(call_conv);
            for p_ty in ef_params {
                sig.params.push(AbiParam::new(self.clif_type(p_ty)));
            }
            if ef_ret != "Unit" && ef_ret != "Never" {
                sig.returns.push(AbiParam::new(self.clif_type(ef_ret)));
            }
            if let Ok(fid) = module.declare_function(ef_name, Linkage::Import, &sig) {
                func_ids.insert(ef_name.clone(), (fid, sig));
            }
        }

        let mut string_return_funcs: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        string_return_funcs.insert("datara_rt_range_str".into());
        string_return_funcs.insert("datara_rt_int_to_str".into());
        string_return_funcs.insert("datara_rt_str_concat".into());
        string_return_funcs.insert("datara_rt_str_concat_3".into());
        string_return_funcs.insert("datara_rt_str_concat_4".into());
        string_return_funcs.insert("datara_rt_str_concat_5".into());
        string_return_funcs.insert("datara_rt_input".into());
        string_return_funcs.insert("input".into());
        string_return_funcs.insert("read_line".into());
        string_return_funcs.insert("datara_rt_file_read".into());
        string_return_funcs.insert("file_read".into());
        string_return_funcs.insert("read".into());
        string_return_funcs.insert("datara_rt_env_get".into());
        string_return_funcs.insert("env_get".into());
        string_return_funcs.insert("datara_rt_args_get".into());
        string_return_funcs.insert("args_get".into());
        string_return_funcs.insert("datara_rt_str_trim".into());
        string_return_funcs.insert("str_trim".into());
        string_return_funcs.insert("datara_rt_socket_recv".into());
        string_return_funcs.insert("socket_recv".into());
        string_return_funcs.insert("datara_rt_sha256".into());
        string_return_funcs.insert("sha256".into());
        string_return_funcs.insert("datara_rt_base64_encode".into());
        string_return_funcs.insert("base64_encode".into());
        string_return_funcs.insert("datara_rt_base64_decode".into());
        string_return_funcs.insert("base64_decode".into());
        string_return_funcs.insert("datara_rt_exec".into());
        string_return_funcs.insert("process_output".into());
        string_return_funcs.insert("exec".into());

        for (fn_name, func) in &dmir_module.functions {
            if func.return_type == "String" || func.return_type == "Str" {
                string_return_funcs.insert(fn_name.clone());
                if let Some(short) = fn_name.split('_').next_back() {
                    string_return_funcs.insert(short.to_string());
                }
            }
        }

        // Also declare standard main wrapper only if 'main' exists
        let has_main = func_ids.contains_key("main");
        let main_entry_info = if has_main {
            let mut main_entry_sig = Signature::new(call_conv);
            main_entry_sig.params.push(AbiParam::new(clif_types::I32));
            main_entry_sig.params.push(AbiParam::new(clif_types::I64));
            main_entry_sig.returns.push(AbiParam::new(clif_types::I32));
            let id = module
                .declare_function("main", Linkage::Export, &main_entry_sig)
                .map_err(|e| e.to_string())?;
            Some((id, main_entry_sig))
        } else {
            None
        };

        let mut class_field_offsets: HashMap<String, HashMap<String, i32>> = HashMap::new();
        let mut field_default_offsets: HashMap<String, i32> = HashMap::new();
        let mut string_fields: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (cls_name, fields) in &dmir_module.class_fields {
            let m = class_field_offsets.entry(cls_name.clone()).or_default();
            for (idx, fname) in fields.iter().enumerate() {
                m.insert(fname.clone(), (idx * 8) as i32);
                // First-wins: two classes may lay out a same-named field at
                // different offsets; the last one would silently overwrite
                // this table in HashMap iteration order, making codegen
                // nondeterministic between compiler runs.
                field_default_offsets
                    .entry(fname.clone())
                    .or_insert((idx * 8) as i32);
            }
        }

        for func in dmir_module.functions.values() {
            for b in &func.blocks {
                for inst in &b.instructions {
                    if let Inst::StructInit {
                        class_name, fields, ..
                    } = inst
                    {
                        let m = class_field_offsets.entry(class_name.clone()).or_default();
                        for (idx, (fname, _)) in fields.iter().enumerate() {
                            m.entry(fname.clone()).or_insert((idx * 8) as i32);
                            field_default_offsets
                                .entry(fname.clone())
                                .or_insert((idx * 8) as i32);
                        }
                    }
                }
            }
        }

        // String fields are decided by their DECLARED type, never by name
        // substrings: an Int field named "valid" or "width" must not be
        // routed to out_str as a raw integer (that is a segfault).
        for (key, ty) in &dmir_module.class_field_types {
            if ty.contains("Str")
                && let Some((_, fname)) = key.rsplit_once('.')
            {
                string_fields.insert(fname.to_string());
            }
        }

        // Pre-define all string literals in the module
        let mut string_literal_map: HashMap<String, cranelift_module::DataId> = HashMap::new();
        let add_str_literal =
            |s: &str, m: &mut ObjectModule| -> Result<cranelift_module::DataId, String> {
                let mut data_ctx = DataDescription::new();
                let mut bytes = s.as_bytes().to_vec();
                bytes.push(0); // null terminator
                data_ctx.define(bytes.into_boxed_slice());
                let data_id = m
                    .declare_anonymous_data(true, false)
                    .map_err(|e| e.to_string())?;
                m.define_data(data_id, &data_ctx)
                    .map_err(|e| e.to_string())?;
                Ok(data_id)
            };

        // Always include empty string and colon
        string_literal_map.insert("".to_string(), add_str_literal("", &mut module)?);
        string_literal_map.insert(":".to_string(), add_str_literal(":", &mut module)?);

        for func in dmir_module.functions.values() {
            for b in &func.blocks {
                for inst in &b.instructions {
                    match inst {
                        Inst::ConstStr { value, .. } => {
                            if !string_literal_map.contains_key(value) {
                                let id = add_str_literal(value, &mut module)?;
                                string_literal_map.insert(value.clone(), id);
                            }
                        }
                        Inst::FormatStr { parts, .. } => {
                            for p in parts {
                                if !string_literal_map.contains_key(p) {
                                    let id = add_str_literal(p, &mut module)?;
                                    string_literal_map.insert(p.clone(), id);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // 3. Compile functions
        let mut fn_builder_ctx = FunctionBuilderContext::new();
        for (name, f) in &dmir_module.functions {
            let &(func_id, ref sig) = func_ids.get(name).unwrap();
            let mut clif_fn = ClifFunction::with_name_signature(
                cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
                sig.clone(),
            );

            let mut builder = FunctionBuilder::new(&mut clif_fn, &mut fn_builder_ctx);

            let mut val_map: HashMap<ValueId, ClifValue> = HashMap::new();
            let mut const_int_map: HashMap<ValueId, i64> = HashMap::new();
            // Values known to be Bool (0/1). Unlike strings, Bools share the
            // I64 representation, so this set is the only thing separating
            // `print(is_adult)` from `print(1)`.
            let mut bool_vids: std::collections::HashSet<ValueId> =
                std::collections::HashSet::new();

            let mut block_map: HashMap<BasicBlockId, Block> = HashMap::new();
            for b in &f.blocks {
                let clif_block = builder.create_block();
                block_map.insert(b.id, clif_block);
            }

            // Real SSA: non-entry blocks receive their block parameters
            // (phis). Values are registered in `val_map` immediately so any
            // later instruction or terminator operand resolves to them. Bool
            // parameters join `bool_vids` so `print(flag)` keeps boolean
            // semantics through the promotion. The entry block is skipped:
            // its parameter list is the function signature.
            for b in &f.blocks {
                if b.params.is_empty() || b.id == f.entry_block {
                    continue;
                }
                let clif_block = *block_map.get(&b.id).unwrap();
                for p in &b.params {
                    builder.append_block_param(clif_block, self.clif_type(&p.ty));
                }
                for (idx, p) in b.params.iter().enumerate() {
                    let v = builder.block_params(clif_block)[idx];
                    val_map.insert(p.val, v);
                    if p.ty == "Bool" {
                        bool_vids.insert(p.val);
                    }
                }
            }

            let entry_clif_block = *block_map.get(&f.entry_block).unwrap();
            builder.append_block_params_for_function_params(entry_clif_block);
            builder.switch_to_block(entry_clif_block);

            let mut var_map: HashMap<String, Variable> = HashMap::new();
            let mut string_vars: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut string_vids: std::collections::HashSet<ValueId> =
                std::collections::HashSet::new();
            let mut bool_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
            // Values holding a list pointer (`[count, e0..eN]` heap array).
            let mut list_vids: std::collections::HashSet<ValueId> =
                std::collections::HashSet::new();
            let mut list_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut var_to_class: HashMap<String, String> = HashMap::new();
            let mut val_to_class: HashMap<ValueId, String> = HashMap::new();

            for (idx, (p_name, p_type, p_val)) in f.params.iter().enumerate() {
                let p_clif_val = builder.block_params(entry_clif_block)[idx];
                val_map.insert(*p_val, p_clif_val);
                if p_type == "String" || p_type == "Str" {
                    string_vars.insert(p_name.clone());
                    string_vids.insert(*p_val);
                }
                if p_type == "Bool" {
                    bool_vars.insert(p_name.clone());
                    bool_vids.insert(*p_val);
                }
                if !p_type.is_empty()
                    && p_type != "Int"
                    && p_type != "Float"
                    && p_type != "Bool"
                    && p_type != "String"
                    && p_type != "Str"
                    && p_type != "Unit"
                {
                    var_to_class.insert(p_name.clone(), p_type.clone());
                    val_to_class.insert(*p_val, p_type.clone());
                }

                let var = builder.declare_var(self.clif_type(p_type));
                builder.def_var(var, p_clif_val);
                var_map.insert(p_name.clone(), var);
            }

            for b in &f.blocks {
                let current_clif_block = *block_map.get(&b.id).unwrap();
                builder.switch_to_block(current_clif_block);

                for inst in &b.instructions {
                    match inst {
                        Inst::ConstInt { dest, value } => {
                            let v = builder.ins().iconst(clif_types::I64, *value);
                            val_map.insert(*dest, v);
                            const_int_map.insert(*dest, *value);
                        }
                        Inst::ConstFloat { dest, value } => {
                            let v = builder.ins().f64const(*value);
                            val_map.insert(*dest, v);
                        }
                        Inst::ConstBool { dest, value } => {
                            let v = builder
                                .ins()
                                .iconst(clif_types::I64, if *value { 1 } else { 0 });
                            val_map.insert(*dest, v);
                            bool_vids.insert(*dest);
                        }
                        Inst::ConstStr { dest, value } => {
                            if let Some(&str_data_id) = string_literal_map.get(value) {
                                let global_val =
                                    module.declare_data_in_func(str_data_id, builder.func);
                                let v = builder.ins().symbol_value(clif_types::I64, global_val);
                                val_map.insert(*dest, v);
                                string_vids.insert(*dest);
                            }
                        }
                        Inst::GetFuncAddr { dest, func_name } => {
                            if let Some(&(fid, _)) = func_ids.get(func_name) {
                                let fref = module.declare_func_in_func(fid, builder.func);
                                let addr = builder.ins().func_addr(clif_types::I64, fref);
                                val_map.insert(*dest, addr);
                            } else {
                                return Err(format!(
                                    "Code generation failed: unresolved function address for '{}' in function '{}'",
                                    func_name, f.name
                                ));
                            }
                        }
                        Inst::LoadVar { dest, name } => {
                            if let Some(&var) = var_map.get(name) {
                                let v = builder.use_var(var);
                                val_map.insert(*dest, v);
                            } else {
                                let v = builder.ins().iconst(clif_types::I64, 0);
                                val_map.insert(*dest, v);
                            }
                            if string_vars.contains(name) {
                                string_vids.insert(*dest);
                            }
                            if bool_vars.contains(name) {
                                bool_vids.insert(*dest);
                            }
                            if list_vars.contains(name) {
                                list_vids.insert(*dest);
                            }
                            if let Some(c) = var_to_class.get(name) {
                                val_to_class.insert(*dest, c.clone());
                            }
                        }
                        Inst::AssignVar { name, value } => {
                            let v = val_map
                                .get(value)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            if string_vids.contains(value) {
                                string_vars.insert(name.clone());
                            }
                            if bool_vids.contains(value) {
                                bool_vars.insert(name.clone());
                            }
                            if list_vids.contains(value) {
                                list_vars.insert(name.clone());
                            }
                            if let Some(c) = val_to_class.get(value) {
                                var_to_class.insert(name.clone(), c.clone());
                            }
                            if let Some(&var) = var_map.get(name) {
                                builder.def_var(var, v);
                            } else {
                                let v_ty = builder.func.dfg.value_type(v);
                                let var = builder.declare_var(v_ty);
                                builder.def_var(var, v);
                                var_map.insert(name.clone(), var);
                            }
                        }
                        Inst::BinOp {
                            dest,
                            op,
                            left,
                            right,
                            ty,
                        } => {
                            let raw_lv = val_map
                                .get(left)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            let raw_rv = val_map
                                .get(right)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            let lv_ty = builder.func.dfg.value_type(raw_lv);
                            let rv_ty = builder.func.dfg.value_type(raw_rv);
                            let left_is_str = string_vids.contains(left);
                            let right_is_str = string_vids.contains(right);
                            let is_string = op == "+"
                                && (left_is_str
                                    || right_is_str
                                    || ty == "String"
                                    || ty.contains("Str"));

                            if is_string {
                                let conv_ref =
                                    module.declare_func_in_func(rt_int_to_str_id, builder.func);
                                let l_str = if left_is_str || (ty == "String" && !right_is_str) {
                                    raw_lv
                                } else {
                                    let call = builder.ins().call(conv_ref, &[raw_lv]);
                                    builder.inst_results(call)[0]
                                };
                                let r_str = if right_is_str {
                                    raw_rv
                                } else {
                                    let call = builder.ins().call(conv_ref, &[raw_rv]);
                                    builder.inst_results(call)[0]
                                };
                                let fn_ref =
                                    module.declare_func_in_func(rt_concat_id, builder.func);
                                let call_inst = builder.ins().call(fn_ref, &[l_str, r_str]);
                                let res = builder.inst_results(call_inst)[0];
                                val_map.insert(*dest, res);
                                string_vids.insert(*dest);
                                continue;
                            }

                            let is_float = lv_ty == clif_types::F64 || rv_ty == clif_types::F64;

                            let (lv, rv) = if is_float {
                                let flv = if lv_ty == clif_types::I64 {
                                    builder.ins().fcvt_from_sint(clif_types::F64, raw_lv)
                                } else {
                                    raw_lv
                                };
                                let frv = if rv_ty == clif_types::I64 {
                                    builder.ins().fcvt_from_sint(clif_types::F64, raw_rv)
                                } else {
                                    raw_rv
                                };
                                (flv, frv)
                            } else {
                                let ilv = if lv_ty == clif_types::I8 || lv_ty == clif_types::I32 {
                                    builder.ins().sextend(clif_types::I64, raw_lv)
                                } else {
                                    raw_lv
                                };
                                let irv = if rv_ty == clif_types::I8 || rv_ty == clif_types::I32 {
                                    builder.ins().sextend(clif_types::I64, raw_rv)
                                } else {
                                    raw_rv
                                };
                                (ilv, irv)
                            };

                            let res = if is_float {
                                match op.as_str() {
                                    "+" => builder.ins().fadd(lv, rv),
                                    "-" => builder.ins().fsub(lv, rv),
                                    "*" => builder.ins().fmul(lv, rv),
                                    "/" => builder.ins().fdiv(lv, rv),
                                    "<" => {
                                        let c = builder.ins().fcmp(
                                            cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                                            lv,
                                            rv,
                                        );
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    "<=" => {
                                        let c = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual, lv, rv);
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    ">" => {
                                        let c = builder.ins().fcmp(
                                            cranelift_codegen::ir::condcodes::FloatCC::GreaterThan,
                                            lv,
                                            rv,
                                        );
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    ">=" => {
                                        let c = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual, lv, rv);
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    "==" => {
                                        let c = builder.ins().fcmp(
                                            cranelift_codegen::ir::condcodes::FloatCC::Equal,
                                            lv,
                                            rv,
                                        );
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    "!=" => {
                                        let c = builder.ins().fcmp(
                                            cranelift_codegen::ir::condcodes::FloatCC::NotEqual,
                                            lv,
                                            rv,
                                        );
                                        builder.ins().uextend(clif_types::I64, c)
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
                                match op.as_str() {
                                    "+" => builder.ins().iadd(lv, rv),
                                    "-" => builder.ins().isub(lv, rv),
                                    "*" => {
                                        let const_mult = const_int_map
                                            .get(right)
                                            .copied()
                                            .or_else(|| const_int_map.get(left).copied());
                                        let var_val = if const_int_map.contains_key(right) {
                                            lv
                                        } else {
                                            rv
                                        };
                                        if let Some(c) = const_mult {
                                            if c == 0 {
                                                builder.ins().iconst(clif_types::I64, 0)
                                            } else if c == 1 {
                                                var_val
                                            } else if c == 2 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 1);
                                                builder.ins().ishl(var_val, shift)
                                            } else if c == 3 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 1);
                                                let shifted = builder.ins().ishl(var_val, shift);
                                                builder.ins().iadd(shifted, var_val)
                                            } else if c == 4 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 2);
                                                builder.ins().ishl(var_val, shift)
                                            } else if c == 5 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 2);
                                                let shifted = builder.ins().ishl(var_val, shift);
                                                builder.ins().iadd(shifted, var_val)
                                            } else if c == 8 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 3);
                                                builder.ins().ishl(var_val, shift)
                                            } else if c == 9 {
                                                let shift =
                                                    builder.ins().iconst(clif_types::I64, 3);
                                                let shifted = builder.ins().ishl(var_val, shift);
                                                builder.ins().iadd(shifted, var_val)
                                            } else if c > 0 && (c & (c - 1)) == 0 {
                                                let shift = builder.ins().iconst(
                                                    clif_types::I64,
                                                    c.trailing_zeros() as i64,
                                                );
                                                builder.ins().ishl(var_val, shift)
                                            } else {
                                                builder.ins().imul(lv, rv)
                                            }
                                        } else {
                                            builder.ins().imul(lv, rv)
                                        }
                                    }
                                    "/" => {
                                        if let Some(&c) = const_int_map.get(right) {
                                            if c > 1 && (c & (c - 1)) == 0 && c <= (1 << 62) {
                                                // Signed truncating division by 2^k:
                                                // q = (x + ((x >>s 63) >>> (64-k))) >>s k
                                                // (branchless; correct for negative dividends,
                                                // unlike a bare arithmetic shift).
                                                let k = c.trailing_zeros();
                                                let sign = builder.ins().sshr_imm_s(lv, 63);
                                                let bias =
                                                    builder.ins().ushr_imm_u(sign, (64 - k) as i64);
                                                let sum = builder.ins().iadd(lv, bias);
                                                builder.ins().sshr_imm_s(sum, k as i64)
                                            } else {
                                                builder.ins().sdiv(lv, rv)
                                            }
                                        } else {
                                            builder.ins().sdiv(lv, rv)
                                        }
                                    }
                                    "%" => {
                                        if let Some(&c) = const_int_map.get(right) {
                                            if c > 1 && (c & (c - 1)) == 0 && c <= (1 << 62) {
                                                // r = x - (q * 2^k); keeps the sign of the
                                                // dividend like srem, valid for negative x.
                                                let k = c.trailing_zeros();
                                                let sign = builder.ins().sshr_imm_s(lv, 63);
                                                let bias =
                                                    builder.ins().ushr_imm_u(sign, (64 - k) as i64);
                                                let sum = builder.ins().iadd(lv, bias);
                                                let q = builder.ins().sshr_imm_s(sum, k as i64);
                                                let scaled = builder.ins().ishl_imm_s(q, k as i64);
                                                builder.ins().isub(lv, scaled)
                                            } else {
                                                builder.ins().srem(lv, rv)
                                            }
                                        } else {
                                            builder.ins().srem(lv, rv)
                                        }
                                    }
                                    "<" => {
                                        let c = builder.ins().icmp(
                                            cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
                                            lv,
                                            rv,
                                        );
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    "<=" => {
                                        let c = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual, lv, rv);
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    ">" => {
                                        let c = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan, lv, rv);
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    ">=" => {
                                        let c = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual, lv, rv);
                                        builder.ins().uextend(clif_types::I64, c)
                                    }
                                    "==" => {
                                        if string_vids.contains(left) || string_vids.contains(right)
                                        {
                                            let (eq_id, _) =
                                                func_ids.get("datara_rt_str_eq").unwrap();
                                            let eq_ref =
                                                module.declare_func_in_func(*eq_id, builder.func);
                                            let call_inst = builder.ins().call(eq_ref, &[lv, rv]);
                                            builder.inst_results(call_inst)[0]
                                        } else {
                                            let c = builder.ins().icmp(
                                                cranelift_codegen::ir::condcodes::IntCC::Equal,
                                                lv,
                                                rv,
                                            );
                                            builder.ins().uextend(clif_types::I64, c)
                                        }
                                    }
                                    "!=" => {
                                        if string_vids.contains(left) || string_vids.contains(right)
                                        {
                                            let (eq_id, _) =
                                                func_ids.get("datara_rt_str_eq").unwrap();
                                            let eq_ref =
                                                module.declare_func_in_func(*eq_id, builder.func);
                                            let call_inst = builder.ins().call(eq_ref, &[lv, rv]);
                                            let res = builder.inst_results(call_inst)[0];
                                            let one = builder.ins().iconst(clif_types::I64, 1);
                                            builder.ins().bxor(res, one)
                                        } else {
                                            let c = builder.ins().icmp(
                                                cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                                lv,
                                                rv,
                                            );
                                            builder.ins().uextend(clif_types::I64, c)
                                        }
                                    }
                                    "&" | "&&" => builder.ins().band(lv, rv),
                                    "|" | "||" => builder.ins().bor(lv, rv),
                                    "^" => builder.ins().bxor(lv, rv),
                                    "<<" => builder.ins().ishl(lv, rv),
                                    ">>" => builder.ins().sshr(lv, rv),
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
                            val_map.insert(*dest, res);
                            // Comparisons and logical operators always produce
                            // a Bool; the lowering labels them `ty: "Int"`
                            // because that is their machine representation.
                            if matches!(
                                op.as_str(),
                                "<" | "<=" | ">" | ">=" | "==" | "!=" | "&&" | "||"
                            ) || ty == "Bool"
                            {
                                bool_vids.insert(*dest);
                            }
                        }
                        Inst::UnOp {
                            dest,
                            op,
                            operand,
                            ty,
                            ..
                        } => {
                            let raw_v = val_map
                                .get(operand)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            let v_ty = builder.func.dfg.value_type(raw_v);
                            let res = if v_ty == clif_types::F64 {
                                if op == "-" {
                                    builder.ins().fneg(raw_v)
                                } else {
                                    raw_v
                                }
                            } else {
                                let v = raw_v;
                                if op == "-" {
                                    builder.ins().ineg(v)
                                } else if op == "!" {
                                    if bool_vids.contains(operand) {
                                        // Logical NOT: `!true` must be `false`,
                                        // not bnot(1) = -2 (still truthy).
                                        let is_zero = builder.ins().icmp_imm_s(
                                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                                            v,
                                            0,
                                        );
                                        builder.ins().uextend(clif_types::I64, is_zero)
                                    } else {
                                        builder.ins().bnot(v)
                                    }
                                } else {
                                    v
                                }
                            };
                            val_map.insert(*dest, res);
                            if string_vids.contains(operand) {
                                string_vids.insert(*dest);
                            }
                            if op == "!" {
                                bool_vids.insert(*dest);
                            }
                            // SROA field forwarding emits `copy` for
                            // `struct.field` reads. A copied Bool must stay a
                            // Bool or `out m.is_some` prints "1" instead of
                            // "true" (the UnOp result loses the flag the
                            // original GetField carried in its `ty`).
                            if op == "copy" && ty == "Bool" {
                                bool_vids.insert(*dest);
                            }
                            if let Some(c) = val_to_class.get(operand) {
                                val_to_class.insert(*dest, c.clone());
                            }
                        }
                        Inst::Call {
                            dest,
                            func,
                            args,
                            ty,
                        } => {
                            // SIMD vector builtins have no scalar Cranelift
                            // lowering (the C runtime passes/returns 16-byte
                            // structs). Reject them with a clear diagnostic
                            // instead of silently emitting garbage; the LLVM
                            // backend (`--llvm`) supports them natively.
                            if matches!(
                                func.as_str(),
                                "float4"
                                    | "int4"
                                    | "dot"
                                    | "min4"
                                    | "max4"
                                    | "datara_rt_float4"
                                    | "datara_rt_int4"
                                    | "datara_rt_float4_dot"
                            ) {
                                return Err(format!(
                                    "Code generation failed: SIMD function '{}' requires the LLVM backend (build with --llvm) in function '{}'",
                                    func, f.name
                                ));
                            }
                            // List literals: the lowering emits
                            // datara_rt_list_create_N for the exact literal
                            // length, so a fixed set of runtime symbols can
                            // never cover every arity. Build the
                            // [count, e0..eN-1] block inline for any N.
                            if let Some(rest) = func.strip_prefix("datara_rt_list_create_")
                                && let Ok(n) = rest.parse::<usize>()
                            {
                                let malloc_ref =
                                    module.declare_func_in_func(malloc_id, builder.func);
                                let size_val =
                                    builder.ins().iconst(clif_types::I64, ((n + 1) * 8) as i64);
                                let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                                let slot_addr = builder.inst_results(call_inst)[0];
                                let flags = cranelift_codegen::ir::MachMemFlags::new();
                                let count_val = builder.ins().iconst(clif_types::I64, n as i64);
                                builder.ins().store(flags, count_val, slot_addr, 0);
                                for (i, a) in args.iter().enumerate() {
                                    let elem = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    builder.ins().store(
                                        flags,
                                        elem,
                                        slot_addr,
                                        ((i + 1) * 8) as i32,
                                    );
                                }
                                val_map.insert(*dest, slot_addr);
                                list_vids.insert(*dest);
                                continue;
                            }
                            if let Some(rest) = func.strip_prefix("datara_rt_tuple_create_")
                                && let Ok(n) = rest.parse::<usize>()
                            {
                                let malloc_ref =
                                    module.declare_func_in_func(malloc_id, builder.func);
                                let size_val =
                                    builder.ins().iconst(clif_types::I64, ((n + 1) * 8) as i64);
                                let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                                let slot_addr = builder.inst_results(call_inst)[0];
                                let flags = cranelift_codegen::ir::MachMemFlags::new();
                                let count_val = builder.ins().iconst(clif_types::I64, n as i64);
                                builder.ins().store(flags, count_val, slot_addr, 0);
                                for (i, a) in args.iter().enumerate() {
                                    let elem = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    builder.ins().store(
                                        flags,
                                        elem,
                                        slot_addr,
                                        ((i + 1) * 8) as i32,
                                    );
                                }
                                val_map.insert(*dest, slot_addr);
                                continue;
                            }
                            // Only the _unchecked variants (emitted exclusively by the
                            // bounds-check-elimination pass, which must prove the trip
                            // count) may bypass the runtime bounds check. The checked
                            // variants always go through the real runtime call below.
                            if (func == "datara_rt_list_get_unchecked") && args.len() == 2 {
                                let list_ptr = val_map
                                    .get(&args[0])
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                let idx_val = val_map
                                    .get(&args[1])
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                let idx_scaled = builder.ins().ishl_imm_u(idx_val, 3);
                                let offset = builder.ins().iadd_imm_s(idx_scaled, 8);
                                let addr = builder.ins().iadd(list_ptr, offset);
                                let flags = cranelift_codegen::ir::MachMemFlags::new();
                                let elem = builder.ins().load(clif_types::I64, flags, addr, 0);
                                val_map.insert(*dest, elem);
                                continue;
                            }
                            if (func == "datara_rt_list_set_unchecked") && args.len() == 3 {
                                let list_ptr = val_map
                                    .get(&args[0])
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                let idx_val = val_map
                                    .get(&args[1])
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                let val = val_map
                                    .get(&args[2])
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                let idx_scaled = builder.ins().ishl_imm_u(idx_val, 3);
                                let offset = builder.ins().iadd_imm_s(idx_scaled, 8);
                                let addr = builder.ins().iadd(list_ptr, offset);
                                let flags = cranelift_codegen::ir::MachMemFlags::new();
                                builder.ins().store(flags, val, addr, 0);
                                val_map.insert(*dest, list_ptr);
                                continue;
                            }
                            let (callee_id, callee_name) = match func_ids.get(func) {
                                Some(v) => (v.0, func.clone()),
                                None => {
                                    let matched = if let Some(first_arg) = args.first() {
                                        if let Some(c) = val_to_class.get(first_arg) {
                                            let specialized = format!("{}_{}", c, func);
                                            if let Some(target) = func_ids.get(&specialized) {
                                                Some((target.0, specialized))
                                            } else {
                                                let base_c = c.split('_').next().unwrap_or(c);
                                                let base_spec = format!("{}_{}", base_c, func);
                                                func_ids
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
                                            func_ids
                                                .iter()
                                                .find(|(k, _)| {
                                                    k.split_once('_')
                                                        .map(|(_, m)| m == func)
                                                        .unwrap_or(false)
                                                })
                                                .map(|(k, v)| (v.0, k.clone()))
                                        })
                                        .unwrap_or({
                                            if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                                                eprintln!(
                                                    "[datara-codegen] UNRESOLVED CALL: {} in {}",
                                                    func, f.name
                                                );
                                            }
                                            (cranelift_module::FuncId::from_u32(0), String::new())
                                        })
                                }
                            };
                            if callee_name.is_empty() {
                                return Err(format!(
                                    "Code generation failed: unresolved function call '{}' in function '{}'",
                                    func, f.name
                                ));
                            }
                            let callee_ref = module.declare_func_in_func(callee_id, builder.func);
                            let is_str_concat = callee_name.starts_with("datara_rt_str_concat");
                            let mut arg_vals = Vec::new();
                            for a in args {
                                if let Some(&av) = val_map.get(a) {
                                    let final_av = if is_str_concat && !string_vids.contains(a) {
                                        let conv_ref = module
                                            .declare_func_in_func(rt_int_to_str_id, builder.func);
                                        let call = builder.ins().call(conv_ref, &[av]);
                                        builder.inst_results(call)[0]
                                    } else {
                                        av
                                    };
                                    arg_vals.push(final_av);
                                }
                            }
                            let call_inst = builder.ins().call(callee_ref, &arg_vals);
                            let results = builder.inst_results(call_inst);
                            if let Some(&r) = results.first() {
                                val_map.insert(*dest, r);
                                let ret_ty = dmir_module
                                    .functions
                                    .get(&callee_name)
                                    .map(|f| f.return_type.as_str())
                                    .unwrap_or("");
                                let first_arg_is_str_class = args
                                    .first()
                                    .and_then(|a| val_to_class.get(a))
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
                                    || string_return_funcs.contains(func)
                                    || string_return_funcs.contains(&callee_name)
                                {
                                    string_vids.insert(*dest);
                                }
                                if ty == "Bool" || ret_ty == "Bool" {
                                    bool_vids.insert(*dest);
                                }
                                if ty == "List"
                                    || func.starts_with("datara_rt_list_create")
                                    || func == "datara_rt_list_append"
                                {
                                    list_vids.insert(*dest);
                                }
                            }
                        }
                        Inst::MethodCall {
                            dest,
                            object,
                            method,
                            args,
                            ty,
                        } => {
                            let mut all_args = vec![
                                val_map
                                    .get(object)
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0)),
                            ];
                            for a in args {
                                if let Some(&av) = val_map.get(a) {
                                    all_args.push(av);
                                }
                            }
                            // List and String protocol methods: dispatch on the object's
                            // runtime shape, not the class method table.
                            let list_special = if list_vids.contains(object) {
                                match method.as_str() {
                                    "length" | "count" | "len" => Some(rt_list_len_id),
                                    "get" | "at" => Some(rt_list_get_id),
                                    "set" => Some(rt_list_set_id),
                                    "push" | "append" | "add" => Some(rt_list_append_id),
                                    "pop" => Some(rt_pop_id),
                                    _ => None,
                                }
                            } else if string_vids.contains(object) {
                                match method.as_str() {
                                    "length" | "count" | "len" => Some(str_len_id),
                                    _ => None,
                                }
                            } else {
                                None
                            };
                            if let Some(special_id) = list_special {
                                let callee_ref =
                                    module.declare_func_in_func(special_id, builder.func);
                                let call_inst = builder.ins().call(callee_ref, &all_args);
                                let results = builder.inst_results(call_inst);
                                if let Some(&r) = results.first() {
                                    val_map.insert(*dest, r);
                                    // Only set/push/append return the (possibly
                                    // reallocated) list itself; length/get
                                    // return plain ints.
                                    if matches!(method.as_str(), "set" | "push" | "append" | "add")
                                    {
                                        list_vids.insert(*dest);
                                    }
                                }
                            } else {
                                let (callee_id, callee_name) = {
                                    let class_matched = if let Some(c) = val_to_class.get(object) {
                                        let specialized = format!("{}_{}", c, method);
                                        if let Some(target) = func_ids.get(&specialized) {
                                            Some((target.0, specialized))
                                        } else {
                                            let base_c = c.split('_').next().unwrap_or(c);
                                            let base_spec = format!("{}_{}", base_c, method);
                                            func_ids
                                                .get(&base_spec)
                                                .map(|target| (target.0, base_spec))
                                        }
                                    } else {
                                        None
                                    };

                                    let dbg_obj_class = val_to_class.get(object).cloned();
                                    class_matched
                                        .or_else(|| {
                                            let cands: Vec<String> = func_ids
                                                .keys()
                                                .filter(|k| {
                                                    k.split_once('_')
                                                        .map(|(_, m)| m == method)
                                                        .unwrap_or(false)
                                                })
                                                .cloned()
                                                .collect();
                                            if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                                                eprintln!("[dispatch] fn={} method={:?} obj_class={:?} cands={:?}", f.name, method, dbg_obj_class, cands);
                                            }
                                            let mut cands_sorted = cands.clone();
                                            cands_sorted.sort();
                                            cands_sorted
                                                .first()
                                                .and_then(|k| func_ids.get(k).map(|v| (v.0, k.clone())))
                                        })
                                        .or_else(|| {
                                            func_ids.get(method).map(|v| (v.0, method.clone()))
                                        })
                                        .unwrap_or({
                                            if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                                                eprintln!("[datara-codegen] UNRESOLVED METHOD: {} in {}", method, f.name);
                                            }
                                            (
                                                cranelift_module::FuncId::from_u32(0),
                                                String::new(),
                                            )
                                        })
                                };
                                if callee_name.is_empty() {
                                    return Err(format!(
                                        "Code generation failed: unresolved method call '{}' on object with class '{:?}' in function '{}'",
                                        method,
                                        val_to_class.get(object),
                                        f.name
                                    ));
                                }
                                let callee_ref =
                                    module.declare_func_in_func(callee_id, builder.func);
                                let call_inst = builder.ins().call(callee_ref, &all_args);
                                let results = builder.inst_results(call_inst);
                                if let Some(&r) = results.first() {
                                    val_map.insert(*dest, r);
                                    let ret_ty = dmir_module
                                        .functions
                                        .get(&callee_name)
                                        .map(|f| f.return_type.as_str())
                                        .unwrap_or("");
                                    let obj_is_str_class = val_to_class
                                        .get(object)
                                        .map(|c| c.ends_with("String") || c.ends_with("Str"))
                                        .unwrap_or(false);
                                    if ty == "String"
                                        || ty.contains("Str")
                                        || ret_ty == "String"
                                        || ret_ty == "Str"
                                        || (ret_ty == "T" && obj_is_str_class)
                                    {
                                        string_vids.insert(*dest);
                                    }
                                    if ty == "Bool" || ret_ty == "Bool" {
                                        bool_vids.insert(*dest);
                                    }
                                }
                            }
                        }
                        Inst::StructInit {
                            dest,
                            class_name,
                            fields,
                        } => {
                            let escapes = f.return_type == *class_name
                                || f.blocks.iter().any(|b| match &b.terminator {
                                    Terminator::Return {
                                        value: Some(ret_val),
                                    } => ret_val == dest,
                                    _ => false,
                                });
                            let byte_size = ((fields.len() * 8).max(16)) as u32;
                            let slot_addr = if !escapes {
                                let slot_data =
                                    StackSlotData::new(StackSlotKind::ExplicitSlot, byte_size, 3);
                                let slot = builder.create_sized_stack_slot(slot_data);
                                builder.ins().stack_addr(clif_types::I64, slot, 0)
                            } else {
                                let malloc_ref =
                                    module.declare_func_in_func(malloc_id, builder.func);
                                let size_val =
                                    builder.ins().iconst(clif_types::I64, byte_size as i64);
                                let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                                builder.inst_results(call_inst)[0]
                            };
                            let flags = cranelift_codegen::ir::MachMemFlags::new();
                            for (idx, (fname, fval)) in fields.iter().enumerate() {
                                if let Some(&v) = val_map.get(fval) {
                                    // Store at the DECLARED layout offset for this
                                    // class/field pair; the literal's field order
                                    // can differ from the resolved class layout
                                    // (composition merges fields), and GetField
                                    // always reads by declared offset.
                                    let off = class_field_offsets
                                        .get(class_name)
                                        .and_then(|m| m.get(fname).copied())
                                        .unwrap_or((idx * 8) as i32);
                                    let v_ty = builder.func.dfg.value_type(v);
                                    let val_to_store =
                                        if v_ty == clif_types::I8 || v_ty == clif_types::I32 {
                                            builder.ins().sextend(clif_types::I64, v)
                                        } else {
                                            v
                                        };
                                    builder.ins().store(flags, val_to_store, slot_addr, off);
                                }
                            }
                            val_map.insert(*dest, slot_addr);
                            val_to_class.insert(*dest, class_name.clone());
                        }
                        Inst::GetField {
                            dest,
                            object,
                            field,
                            ty,
                        } => {
                            let obj_val = val_map.get(object).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: object %{} not found for GetField in function '{}'",
                                    object, f.name
                                )
                            })?;
                            let current_class_name = val_to_class
                                .get(object)
                                .map(|s| s.as_str())
                                .or_else(|| f.params.first().map(|p| p.1.as_str()))
                                .unwrap_or("");
                            let offset = class_field_offsets
                                .get(current_class_name)
                                .and_then(|m| m.get(field).copied())
                                .or_else(|| field_default_offsets.get(field).copied())
                                .unwrap_or(0);
                            if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                                eprintln!(
                                    "[getfield] fn={} class={:?} field={} offset={} declared_ty={:?} inst_ty={}",
                                    f.name,
                                    current_class_name,
                                    field,
                                    offset,
                                    dmir_module
                                        .class_field_types
                                        .get(&format!("{}.{}", current_class_name, field)),
                                    ty
                                );
                            }
                            let field_key = format!("{}.{}", current_class_name, field);
                            let field_type_declared = dmir_module
                                .class_field_types
                                .get(&field_key)
                                .map(|s| s.as_str())
                                .unwrap_or(ty.as_str());
                            let is_float = field_type_declared == "Float"
                                || ty == "Float"
                                || field_type_declared == "Float64"
                                || field_type_declared == "Float32";
                            let flags = cranelift_codegen::ir::MachMemFlags::new();
                            if is_float {
                                let loaded =
                                    builder.ins().load(clif_types::F64, flags, obj_val, offset);
                                val_map.insert(*dest, loaded);
                            } else {
                                let loaded =
                                    builder.ins().load(clif_types::I64, flags, obj_val, offset);
                                val_map.insert(*dest, loaded);
                                if field_type_declared.contains("Str")
                                    || ty.contains("Str")
                                    || string_fields.contains(field)
                                {
                                    string_vids.insert(*dest);
                                }
                                if field_type_declared == "Bool" || ty == "Bool" {
                                    bool_vids.insert(*dest);
                                }
                            }
                        }
                        Inst::SetField {
                            object,
                            field,
                            value,
                        } => {
                            let obj_val = val_map.get(object).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: object %{} not found for SetField in function '{}'",
                                    object, f.name
                                )
                            })?;
                            let val = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for SetField in function '{}'",
                                    value, f.name
                                )
                            })?;
                            let current_class_name = val_to_class
                                .get(object)
                                .map(|s| s.as_str())
                                .or_else(|| f.params.first().map(|p| p.1.as_str()))
                                .unwrap_or("");
                            let offset = class_field_offsets
                                .get(current_class_name)
                                .and_then(|m| m.get(field).copied())
                                .or_else(|| field_default_offsets.get(field).copied())
                                .unwrap_or(0);
                            let flags = cranelift_codegen::ir::MachMemFlags::new();
                            let val_ty = builder.func.dfg.value_type(val);
                            let val_to_store =
                                if val_ty == clif_types::I8 || val_ty == clif_types::I32 {
                                    builder.ins().sextend(clif_types::I64, val)
                                } else {
                                    val
                                };
                            builder.ins().store(flags, val_to_store, obj_val, offset);
                        }
                        Inst::Out { value } => {
                            let v = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for Out in function '{}'",
                                    value, f.name
                                )
                            })?;
                            let v_ty = builder.func.dfg.value_type(v);
                            if string_vids.contains(value) {
                                let fn_ref =
                                    module.declare_func_in_func(rt_out_str_id, builder.func);
                                builder.ins().call(fn_ref, &[v]);
                            } else if v_ty == clif_types::F64 {
                                let fn_ref =
                                    module.declare_func_in_func(rt_out_flt_id, builder.func);
                                builder.ins().call(fn_ref, &[v]);
                            } else if bool_vids.contains(value) {
                                let fn_ref =
                                    module.declare_func_in_func(rt_out_bool_id, builder.func);
                                builder.ins().call(fn_ref, &[v]);
                            } else {
                                let fn_ref =
                                    module.declare_func_in_func(rt_out_int_id, builder.func);
                                builder.ins().call(fn_ref, &[v]);
                            }
                        }
                        Inst::Err { value } => {
                            let v = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for Err in function '{}'",
                                    value, f.name
                                )
                            })?;
                            let fn_ref = module.declare_func_in_func(rt_err_id, builder.func);
                            builder.ins().call(fn_ref, &[v]);
                        }
                        Inst::FormatStr {
                            dest,
                            parts,
                            values,
                        } => {
                            let concat_ref =
                                module.declare_func_in_func(rt_concat_id, builder.func);
                            let concat_3_ref =
                                module.declare_func_in_func(rt_concat_3_id, builder.func);
                            let concat_4_ref =
                                module.declare_func_in_func(rt_concat_4_id, builder.func);
                            let concat_5_ref =
                                module.declare_func_in_func(rt_concat_5_id, builder.func);
                            let int_to_str_ref =
                                module.declare_func_in_func(rt_int_to_str_id, builder.func);
                            let bool_to_str_ref =
                                module.declare_func_in_func(rt_bool_to_str_id, builder.func);
                            let flt_to_str_ref =
                                module.declare_func_in_func(rt_flt_to_str_id, builder.func);

                            let empty_id = *string_literal_map.get("").unwrap();
                            let mut pieces: Vec<ClifValue> = Vec::new();

                            for (idx, p) in parts.iter().enumerate() {
                                if !p.is_empty() || (idx == 0 && values.is_empty()) {
                                    let pid = string_literal_map
                                        .get(p.as_str())
                                        .copied()
                                        .unwrap_or(empty_id);
                                    let pgv = module.declare_data_in_func(pid, builder.func);
                                    let p_val = builder.ins().symbol_value(clif_types::I64, pgv);
                                    pieces.push(p_val);
                                }
                                if idx < values.len() {
                                    let val_id = &values[idx];
                                    let raw_val = val_map.get(val_id).copied().ok_or_else(|| {
                                        format!(
                                            "Code generation failed: interpolated value %{} not found in function '{}'",
                                            val_id, f.name
                                        )
                                    })?;
                                    let val_is_str = string_vids.contains(val_id);
                                    let v_ty = builder.func.dfg.value_type(raw_val);
                                    let s_val = if val_is_str {
                                        raw_val
                                    } else if bool_vids.contains(val_id) {
                                        let call = builder.ins().call(bool_to_str_ref, &[raw_val]);
                                        builder.inst_results(call)[0]
                                    } else if v_ty == clif_types::F64 {
                                        let call = builder.ins().call(flt_to_str_ref, &[raw_val]);
                                        builder.inst_results(call)[0]
                                    } else {
                                        let call = builder.ins().call(int_to_str_ref, &[raw_val]);
                                        builder.inst_results(call)[0]
                                    };
                                    pieces.push(s_val);
                                }
                            }

                            let res_str = match pieces.len() {
                                0 => {
                                    let pgv = module.declare_data_in_func(empty_id, builder.func);
                                    builder.ins().symbol_value(clif_types::I64, pgv)
                                }
                                1 => pieces[0],
                                2 => {
                                    let call =
                                        builder.ins().call(concat_ref, &[pieces[0], pieces[1]]);
                                    builder.inst_results(call)[0]
                                }
                                3 => {
                                    let call = builder
                                        .ins()
                                        .call(concat_3_ref, &[pieces[0], pieces[1], pieces[2]]);
                                    builder.inst_results(call)[0]
                                }
                                4 => {
                                    let call = builder.ins().call(
                                        concat_4_ref,
                                        &[pieces[0], pieces[1], pieces[2], pieces[3]],
                                    );
                                    builder.inst_results(call)[0]
                                }
                                5 => {
                                    let call = builder.ins().call(
                                        concat_5_ref,
                                        &[pieces[0], pieces[1], pieces[2], pieces[3], pieces[4]],
                                    );
                                    builder.inst_results(call)[0]
                                }
                                _ => {
                                    let mut curr = pieces[0];
                                    for piece in &pieces[1..] {
                                        let call = builder.ins().call(concat_ref, &[curr, *piece]);
                                        curr = builder.inst_results(call)[0];
                                    }
                                    curr
                                }
                            };

                            val_map.insert(*dest, res_str);
                            string_vids.insert(*dest);
                        }
                        Inst::Decide {
                            dest,
                            arms,
                            else_val,
                            ty,
                        } => {
                            let mut current_val = else_val
                                .and_then(|ev| val_map.get(&ev).copied())
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));

                            for (cond, val) in arms.iter().rev() {
                                let raw_cond = val_map
                                    .get(cond)
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                                let cond_ty = builder.func.dfg.value_type(raw_cond);
                                let cv = if cond_ty == clif_types::I8 {
                                    raw_cond
                                } else {
                                    builder.ins().icmp_imm_s(
                                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                        raw_cond,
                                        0,
                                    )
                                };
                                let vv = val_map
                                    .get(val)
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                current_val = builder.ins().select(cv, vv, current_val);
                            }
                            val_map.insert(*dest, current_val);
                            if ty == "String"
                                || ty.contains("Str")
                                || arms.iter().any(|(_, v)| string_vids.contains(v))
                                || else_val
                                    .map(|ev| string_vids.contains(&ev))
                                    .unwrap_or(false)
                            {
                                string_vids.insert(*dest);
                            }
                            if ty == "Bool" {
                                bool_vids.insert(*dest);
                            }
                        }
                        Inst::Select {
                            dest,
                            cond,
                            then_val,
                            else_val,
                            ty,
                        } => {
                            let raw_cond = val_map
                                .get(cond)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                            let cond_ty = builder.func.dfg.value_type(raw_cond);
                            let cv = if cond_ty == clif_types::I8 {
                                raw_cond
                            } else {
                                builder.ins().icmp_imm_s(
                                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                    raw_cond,
                                    0,
                                )
                            };
                            let tv = val_map
                                .get(then_val)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            let ev = val_map
                                .get(else_val)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            let sel = builder.ins().select(cv, tv, ev);
                            val_map.insert(*dest, sel);
                            if string_vids.contains(then_val) || string_vids.contains(else_val) {
                                string_vids.insert(*dest);
                            }
                            if bool_vids.contains(then_val)
                                || bool_vids.contains(else_val)
                                || ty == "Bool"
                            {
                                bool_vids.insert(*dest);
                            }
                        }
                        Inst::WhileLoop { .. } | Inst::TryCatch { .. } | Inst::Return { .. } => {}
                    }
                }

                // Handle terminator
                match &b.terminator {
                    Terminator::Branch { target, args } => {
                        if let Some(&target_block) = block_map.get(target) {
                            // SSA block arguments: every branch edge must
                            // supply one value per target block parameter.
                            let arg_vals: Vec<BlockArg> = args
                                .iter()
                                .map(|a| {
                                    let v = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    BlockArg::Value(v)
                                })
                                .collect();
                            builder.ins().jump(target_block, &arg_vals);
                        }
                    }
                    Terminator::CondBranch {
                        cond,
                        then_block,
                        then_args,
                        else_block,
                        else_args,
                    } => {
                        let cond_val = val_map
                            .get(cond)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                        let cond_ty = builder.func.dfg.value_type(cond_val);
                        let cond_bool = if cond_ty == clif_types::I8 {
                            cond_val
                        } else {
                            builder.ins().icmp_imm_s(
                                cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                cond_val,
                                0,
                            )
                        };
                        if let (Some(&tb), Some(&eb)) =
                            (block_map.get(then_block), block_map.get(else_block))
                        {
                            let then_vals: Vec<BlockArg> = then_args
                                .iter()
                                .map(|a| {
                                    let v = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    BlockArg::Value(v)
                                })
                                .collect();
                            let else_vals: Vec<BlockArg> = else_args
                                .iter()
                                .map(|a| {
                                    let v = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    BlockArg::Value(v)
                                })
                                .collect();
                            builder
                                .ins()
                                .brif(cond_bool, tb, &then_vals, eb, &else_vals);
                        }
                    }
                    Terminator::Return { value } => {
                        if f.return_type == "Unit" {
                            builder.ins().return_(&[]);
                        } else if f.return_type == "Float" {
                            if let Some(v_id) = value {
                                if let Some(&v) = val_map.get(v_id) {
                                    let v_ty = builder.func.dfg.value_type(v);
                                    let ret_v = if v_ty == clif_types::F64 {
                                        v
                                    } else if v_ty == clif_types::I64 {
                                        builder.ins().fcvt_from_sint(clif_types::F64, v)
                                    } else {
                                        v
                                    };
                                    builder.ins().return_(&[ret_v]);
                                } else {
                                    let zero = builder.ins().f64const(0.0);
                                    builder.ins().return_(&[zero]);
                                }
                            } else {
                                let zero = builder.ins().f64const(0.0);
                                builder.ins().return_(&[zero]);
                            }
                        } else {
                            if let Some(v_id) = value {
                                if let Some(&v) = val_map.get(v_id) {
                                    let v_ty = builder.func.dfg.value_type(v);
                                    let ret_v = if v_ty == clif_types::I64 {
                                        v
                                    } else if v_ty == clif_types::F64 {
                                        builder.ins().fcvt_to_sint(clif_types::I64, v)
                                    } else {
                                        v
                                    };
                                    builder.ins().return_(&[ret_v]);
                                } else {
                                    let zero = builder.ins().iconst(clif_types::I64, 0);
                                    builder.ins().return_(&[zero]);
                                }
                            } else {
                                let zero = builder.ins().iconst(clif_types::I64, 0);
                                builder.ins().return_(&[zero]);
                            }
                        }
                    }
                    Terminator::Unreachable => {
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
                    }
                }
            }

            builder.seal_all_blocks();
            builder.finalize(frontend_config);

            let mut ctx = cranelift_codegen::Context::for_function(clif_fn.clone());
            if let Err(e) = module.define_function(func_id, &mut ctx) {
                return Err(format!(
                    "Error in {}:\nCLIF:\n{}\nError:\n{}",
                    name,
                    clif_fn.display(),
                    e
                ));
            }
        }

        // 4. Define main entry function that calls @main() and exits with 0
        if let Some((main_entry_id, main_entry_sig)) = main_entry_info
            && let Some(&(main_fn_id, _)) = func_ids.get("main")
        {
            let mut main_clif_fn = ClifFunction::with_name_signature(
                cranelift_codegen::ir::UserFuncName::user(0, main_entry_id.as_u32()),
                main_entry_sig,
            );
            let mut builder = FunctionBuilder::new(&mut main_clif_fn, &mut fn_builder_ctx);
            let entry_block = builder.create_block();
            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);

            let argc_val = builder.block_params(entry_block)[0];
            let argv_val = builder.block_params(entry_block)[1];

            if let Some(&(set_args_id, _)) = func_ids.get("datara_rt_set_args") {
                let set_args_ref = module.declare_func_in_func(set_args_id, builder.func);
                builder.ins().call(set_args_ref, &[argc_val, argv_val]);
            }

            let main_fn_ref = module.declare_func_in_func(main_fn_id, builder.func);
            builder.ins().call(main_fn_ref, &[]);

            let zero = builder.ins().iconst(clif_types::I32, 0);
            builder.ins().return_(&[zero]);

            builder.seal_all_blocks();
            builder.finalize(frontend_config);

            let mut ctx = cranelift_codegen::Context::for_function(main_clif_fn);
            module
                .define_function(main_entry_id, &mut ctx)
                .map_err(|e| e.to_string())?;
        }

        let product = module.finish();
        let obj_bytes = product.emit().map_err(|e| e.to_string())?;
        Ok(obj_bytes)
    }

    pub fn link_object_to_executable(
        &self,
        obj_bytes: &[u8],
        output_exe: &Path,
        exports: &[String],
    ) -> Result<PathBuf, String> {
        let abs_out = if output_exe.is_absolute() {
            output_exe.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(output_exe)
        };

        if let Some(parent) = abs_out.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let obj_path = abs_out.with_extension("obj");
        fs::write(&obj_path, obj_bytes)
            .map_err(|e| format!("Failed to write object file: {}", e))?;

        // Locate the toolchain and the Datara runtime at run time. Nothing here
        // may depend on this machine: `linker::discover` resolves MSVC through
        // `vswhere` (falling back to `PATH`), and the runtime archive path is
        // baked in by `build.rs` from `OUT_DIR`, so it is correct in any
        // checkout and can never be a stale artifact.
        let spec = crate::codegen::linker::discover();
        let runtime_lib = crate::runtime::runtime_lib_path();
        if !runtime_lib.exists() {
            return Err(format!(
                "Datara runtime library is missing at '{}'. Rebuild the compiler \
                 (`cargo build`) so build.rs can regenerate it.",
                runtime_lib.display()
            ));
        }

        let args =
            crate::codegen::linker::link_args(&spec, &obj_path, &runtime_lib, &abs_out, exports);

        let output = {
            let _guard = linker_lock().lock().unwrap_or_else(|e| e.into_inner());
            Command::new(&spec.program)
                .args(&args)
                .output()
                .map_err(|e| {
                    format!(
                        "Failed to invoke linker '{}': {}\n{}",
                        spec.program.display(),
                        e,
                        crate::codegen::linker::describe(&spec)
                    )
                })?
        };

        if output.status.success() && abs_out.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&abs_out) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&abs_out, perms);
                }
            }
            return Ok(abs_out);
        }

        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "Linking failed: status={:?}\n{}\nargv: {} {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            crate::codegen::linker::describe(&spec),
            spec.program.display(),
            args.join(" "),
            out,
            err
        ))
    }
}

impl CodegenBackend for RealCraneliftBackend {
    fn target_info(&self) -> TargetInfo {
        self.target.clone()
    }

    fn emit(&self, module: &Module, program: &Program, types: &TypeChecker) -> String {
        let clif_emitter = crate::codegen::cranelift::ClifEmitter::new(&self.target);
        clif_emitter.emit_module(module, program, types)
    }

    fn compile_to_executable(&self, source: &str, output_path: &Path) -> Result<PathBuf, String> {
        let clif_path = output_path.with_extension("clif");
        let _ = fs::write(&clif_path, source);
        Ok(output_path.to_path_buf())
    }

    fn run_executable(
        &self,
        exe_path: &Path,
        args: &[String],
    ) -> Result<(String, String, i32, u128), String> {
        let abs_exe = if exe_path.is_absolute() {
            exe_path.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(exe_path)
        } else {
            exe_path.to_path_buf()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&abs_exe) {
                let mut perms = metadata.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(perms.mode() | 0o755);
                    let _ = fs::set_permissions(&abs_exe, perms);
                }
            }
        }
        let start = Instant::now();
        let output = Command::new(&abs_exe).args(args).output().map_err(|e| {
            format!(
                "Failed to run native executable '{}': {}",
                abs_exe.display(),
                e
            )
        })?;
        let duration = start.elapsed().as_millis();

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, code, duration))
    }
}
