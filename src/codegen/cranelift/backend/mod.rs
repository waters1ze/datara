pub mod compile_func;
pub mod declare_core;
pub mod declare_ext;
pub mod declare_module;
pub mod inst_binop;
pub mod inst_call;
pub mod link;
pub mod types;

use cranelift_codegen::ir::Type as ClifType;
use cranelift_codegen::isa::{CallConv, TargetFrontendConfig, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_module::{Module as ClifModule, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;
use std::sync::Arc;
use target_lexicon::Triple;

use crate::codegen::target::TargetInfo;
use crate::dmir::Module;

pub use types::*;

#[derive(Debug, Clone, Default)]
pub struct ModuleCompileArtifacts {
    pub main_entry_id: Option<cranelift_module::FuncId>,
    pub main_fn_id: Option<cranelift_module::FuncId>,
    pub func_ids: HashMap<String, cranelift_module::FuncId>,
}

#[derive(Clone)]
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

    pub fn clif_type(&self, ty_str: &str) -> ClifType {
        types::clif_type(ty_str)
    }

    pub fn build_target_isa(
        &self,
        is_jit: bool,
    ) -> Result<(Arc<dyn TargetIsa>, CallConv, TargetFrontendConfig), String> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| e.to_string())?;
        let is_pic = if is_jit || self.target.os == crate::codegen::target::Os::Windows {
            "false"
        } else {
            "true"
        };
        flag_builder
            .set("is_pic", is_pic)
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
        Ok((isa, call_conv, frontend_config))
    }

    pub fn compile_to_object_bytes(&self, dmir_module: &Module) -> Result<Vec<u8>, String> {
        let (isa, call_conv, frontend_config) = self.build_target_isa(false)?;
        let builder = ObjectBuilder::new(
            isa,
            dmir_module.name.as_bytes().to_vec(),
            default_libcall_names(),
        )
        .map_err(|e| e.to_string())?;
        let mut module = ObjectModule::new(builder);
        let artifacts =
            self.compile_into_module(&mut module, dmir_module, frontend_config, call_conv)?;
        let mut product = module.finish();
        crate::codegen::cranelift::dwarf::DwarfLineEmitter::emit_dwarf_to_object(
            &mut product.object,
            dmir_module,
            &artifacts,
            &product.functions,
        )?;
        let obj_bytes = product.emit().map_err(|e| e.to_string())?;
        Ok(obj_bytes)
    }

    pub fn compile_and_run_jit(
        &self,
        dmir_module: &Module,
        args: &[String],
        capture: bool,
    ) -> Result<(String, String, i32, u128), String> {
        let (isa, call_conv, frontend_config) = self.build_target_isa(true)?;
        let mut module = crate::codegen::cranelift::jit::create_jit_module(isa)?;
        let artifacts =
            self.compile_into_module(&mut module, dmir_module, frontend_config, call_conv)?;
        module.finalize_definitions().map_err(|e| e.to_string())?;

        let entry_id = artifacts
            .main_entry_id
            .or(artifacts.main_fn_id)
            .ok_or_else(|| "No entry point found in module (missing @main function)".to_string())?;
        let code_ptr = module.get_finalized_function(entry_id);
        unsafe { crate::codegen::cranelift::jit::run_jit_entry(code_ptr, args, capture) }
    }

    pub fn compile_into_module<M: ClifModule>(
        &self,
        module: &mut M,
        dmir_module: &Module,
        frontend_config: TargetFrontendConfig,
        call_conv: CallConv,
    ) -> Result<ModuleCompileArtifacts, String> {
        self.compile_into_module_opt(module, dmir_module, frontend_config, call_conv, false)
    }

    pub fn compile_into_module_opt<M: ClifModule>(
        &self,
        module: &mut M,
        dmir_module: &Module,
        frontend_config: TargetFrontendConfig,
        call_conv: CallConv,
        export_all: bool,
    ) -> Result<ModuleCompileArtifacts, String> {
        let mut func_ids = HashMap::new();
        let core_ids = declare_core::declare_runtime_core(module, call_conv, &mut func_ids)?;
        let (rt_str_char_at_id, rt_str_eq_id) =
            declare_ext::declare_runtime_ext(module, dmir_module, call_conv, &mut func_ids)?;

        let runtime = RuntimeIds {
            rt_out_int_id: core_ids.rt_out_int_id,
            rt_out_bool_id: core_ids.rt_out_bool_id,
            rt_out_flt_id: core_ids.rt_out_flt_id,
            rt_out_str_id: core_ids.rt_out_str_id,
            rt_err_id: core_ids.rt_err_id,
            rt_concat_id: core_ids.rt_concat_id,
            rt_concat_3_id: core_ids.rt_concat_3_id,
            rt_concat_4_id: core_ids.rt_concat_4_id,
            rt_concat_5_id: core_ids.rt_concat_5_id,
            rt_int_to_str_id: core_ids.rt_int_to_str_id,
            rt_bool_to_str_id: core_ids.rt_bool_to_str_id,
            rt_flt_to_str_id: core_ids.rt_flt_to_str_id,
            malloc_id: core_ids.malloc_id,
            rt_list_get_id: core_ids.rt_list_get_id,
            rt_list_create_id: core_ids.rt_list_create_id,
            rt_list_len_id: core_ids.rt_list_len_id,
            rt_list_set_id: core_ids.rt_list_set_id,
            rt_list_append_id: core_ids.rt_list_append_id,
            rt_map_get_id: core_ids.rt_map_get_id,
            rt_map_insert_id: core_ids.rt_map_insert_id,
            rt_pop_id: core_ids.rt_pop_id,
            rt_str_char_at_id,
            rt_str_eq_id,
            str_byte_at_id: core_ids.str_byte_at_id,
            str_chars_id: core_ids.str_chars_id,
            str_len_id: core_ids.str_len_id,
        };

        let decls = declare_module::declare_module_symbols(
            module,
            dmir_module,
            call_conv,
            export_all,
            &mut func_ids,
        )?;

        compile_func::compile_all_functions(
            module,
            dmir_module,
            frontend_config,
            &func_ids,
            &runtime,
            &decls,
        )?;

        compile_func::define_main_entry(
            module,
            frontend_config,
            &func_ids,
            decls.main_entry_info.clone(),
        )?;

        let main_fn_id = func_ids.get("main").map(|&(id, _)| id);
        let main_entry_id = decls.main_entry_info.map(|(id, _)| id);
        let func_ids_map = func_ids.into_iter().map(|(k, (id, _))| (k, id)).collect();

        Ok(ModuleCompileArtifacts {
            main_entry_id,
            main_fn_id,
            func_ids: func_ids_map,
        })
    }
}
