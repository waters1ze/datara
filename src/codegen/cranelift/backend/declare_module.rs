use crate::dmir::{Inst, Module};
use cranelift_codegen::ir::{AbiParam, Signature, types as clif_types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{DataDescription, FuncId, Linkage, Module as ClifModule};
use std::collections::HashMap;

use super::types::{ModuleDecls, clif_type};

pub fn declare_module_symbols<M: ClifModule>(
    module: &mut M,
    dmir_module: &Module,
    call_conv: CallConv,
    export_all: bool,
    func_ids: &mut HashMap<String, (FuncId, Signature)>,
) -> Result<ModuleDecls, String> {
    // 0. Collect address-taken functions across all functions in the module
    let mut address_taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    for f in dmir_module.functions.values() {
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::GetFuncAddr { func_name, .. } = inst {
                    address_taken.insert(func_name.clone());
                }
            }
        }
    }

    // 2. Declare all module functions
    let mut sorted_func_names: Vec<String> = dmir_module.functions.keys().cloned().collect();
    sorted_func_names.sort();
    let has_main = dmir_module.functions.contains_key("main");
    for name in &sorted_func_names {
        let f = &dmir_module.functions[name];
        let is_internal = !export_all
            && has_main
            && name != "main"
            && !dmir_module.extern_functions.contains_key(name)
            && !address_taken.contains(name);
        let fn_call_conv = if is_internal {
            CallConv::Fast
        } else {
            call_conv
        };
        let linkage = if is_internal {
            Linkage::Local
        } else {
            Linkage::Export
        };

        let mut sig = Signature::new(fn_call_conv);
        for (_, p_type, _) in &f.params {
            sig.params.push(AbiParam::new(clif_type(p_type)));
        }
        if f.return_type != "Unit" {
            sig.returns.push(AbiParam::new(clif_type(&f.return_type)));
        }

        let symbol_name = if name == "main" {
            "datara_main"
        } else {
            name.as_str()
        };
        let func_id = module
            .declare_function(symbol_name, linkage, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(name.clone(), (func_id, sig));
    }

    // Declare native extern "C" functions
    let mut sorted_ef_names: Vec<&String> = dmir_module.extern_functions.keys().collect();
    sorted_ef_names.sort();
    for ef_name in sorted_ef_names {
        let (ef_params, ef_ret) = &dmir_module.extern_functions[ef_name];
        let mut sig = Signature::new(call_conv);
        for p_ty in ef_params {
            sig.params.push(AbiParam::new(clif_type(p_ty)));
        }
        if ef_ret != "Unit" && ef_ret != "Never" {
            sig.returns.push(AbiParam::new(clif_type(ef_ret)));
        }
        if let Ok(fid) = module.declare_function(ef_name, Linkage::Import, &sig) {
            func_ids.insert(ef_name.clone(), (fid, sig));
        }
    }

    // Declare CPython bridge imports dynamically ONLY if used, preserving DCE zero-cost guarantee
    let uses_python_bridge = dmir_module.functions.values().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| match i {
                crate::dmir::Inst::Call { func, .. } => {
                    func.starts_with("datara_py_") || func.starts_with("py_")
                }
                _ => false,
            })
        })
    }) || dmir_module
        .extern_functions
        .keys()
        .any(|k| k.starts_with("datara_py_") || k.starts_with("py_"));

    if uses_python_bridge {
        let mut py_1_str_sig = Signature::new(call_conv);
        py_1_str_sig.params.push(AbiParam::new(clif_types::I64));
        py_1_str_sig.returns.push(AbiParam::new(clif_types::I64));

        let mut py_1_str_to_i64_sig = Signature::new(call_conv);
        py_1_str_to_i64_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        py_1_str_to_i64_sig
            .returns
            .push(AbiParam::new(clif_types::I64));

        let mut py_1_str_to_f64_sig = Signature::new(call_conv);
        py_1_str_to_f64_sig
            .params
            .push(AbiParam::new(clif_types::I64));
        py_1_str_to_f64_sig
            .returns
            .push(AbiParam::new(clif_types::F64));

        let mut py_2_str_sig = Signature::new(call_conv);
        py_2_str_sig.params.push(AbiParam::new(clif_types::I64));
        py_2_str_sig.params.push(AbiParam::new(clif_types::I64));
        py_2_str_sig.returns.push(AbiParam::new(clif_types::I64));

        let mut py_str_f64_sig = Signature::new(call_conv);
        py_str_f64_sig.params.push(AbiParam::new(clif_types::I64));
        py_str_f64_sig.params.push(AbiParam::new(clif_types::F64));
        py_str_f64_sig.returns.push(AbiParam::new(clif_types::F64));

        let mut py_0_arg_str_sig = Signature::new(call_conv);
        py_0_arg_str_sig
            .returns
            .push(AbiParam::new(clif_types::I64));

        let py_0_arg_void_sig = Signature::new(call_conv);

        let mut py_str_list_sig = Signature::new(call_conv);
        py_str_list_sig.params.push(AbiParam::new(clif_types::I64));
        py_str_list_sig.params.push(AbiParam::new(clif_types::I64));
        py_str_list_sig.returns.push(AbiParam::new(clif_types::I64));

        let decls = [
            ("datara_py_eval", "py_eval", &py_1_str_sig),
            ("datara_py_eval_safe", "py_eval_safe", &py_1_str_sig),
            ("datara_py_eval_int", "py_eval_int", &py_1_str_to_i64_sig),
            (
                "datara_py_eval_float",
                "py_eval_float",
                &py_1_str_to_f64_sig,
            ),
            ("datara_py_call", "py_call", &py_2_str_sig),
            ("datara_py_call_1_str", "py_call_1_str", &py_2_str_sig),
            ("datara_py_call_1_float", "py_call_1_float", &py_str_f64_sig),
            ("datara_py_import", "py_import", &py_1_str_to_i64_sig),
            ("datara_py_exec", "py_exec", &py_1_str_to_i64_sig),
            ("datara_py_last_error", "py_last_error", &py_0_arg_str_sig),
            (
                "datara_py_clear_error",
                "py_clear_error",
                &py_0_arg_void_sig,
            ),
            (
                "datara_py_export_list_f64",
                "datara_py_export_list_f64",
                &py_str_list_sig,
            ),
            (
                "datara_py_assert_same_ptr",
                "datara_py_assert_same_ptr",
                &py_str_list_sig,
            ),
        ];

        for (c_fn, alias, sig) in decls {
            if let Ok(id) = module.declare_function(c_fn, Linkage::Import, sig) {
                func_ids.insert(c_fn.into(), (id, sig.clone()));
                func_ids.insert(alias.into(), (id, sig.clone()));
            }
        }
    }

    let mut string_return_funcs: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    string_return_funcs.insert("datara_rt_range_str".into());
    string_return_funcs.insert("datara_rt_int_to_str".into());
    string_return_funcs.insert("datara_rt_float_to_str".into());
    string_return_funcs.insert("float_to_str".into());
    string_return_funcs.insert("datara_rt_str_concat".into());
    string_return_funcs.insert("datara_rt_str_concat_3".into());
    string_return_funcs.insert("datara_rt_str_concat_4".into());
    string_return_funcs.insert("datara_rt_str_concat_5".into());
    string_return_funcs.insert("datara_rt_input".into());
    string_return_funcs.insert("datara_py_eval".into());
    string_return_funcs.insert("datara_py_eval_safe".into());
    string_return_funcs.insert("datara_py_call".into());
    string_return_funcs.insert("datara_py_call_1_str".into());
    string_return_funcs.insert("datara_py_last_error".into());
    string_return_funcs.insert("py_eval".into());
    string_return_funcs.insert("py_eval_safe".into());
    string_return_funcs.insert("py_call".into());
    string_return_funcs.insert("py_call_1_str".into());
    string_return_funcs.insert("py_last_error".into());
    string_return_funcs.insert("input".into());
    string_return_funcs.insert("read_line".into());
    string_return_funcs.insert("datara_rt_file_read".into());
    string_return_funcs.insert("file_read".into());
    string_return_funcs.insert("read".into());
    string_return_funcs.insert("datara_rt_env_get".into());
    string_return_funcs.insert("env_get".into());
    string_return_funcs.insert("datara_rt_args_get".into());
    string_return_funcs.insert("args_get".into());
    string_return_funcs.insert("datara_rt_path_join".into());
    string_return_funcs.insert("path_join".into());
    string_return_funcs.insert("datara_rt_str_trim".into());
    string_return_funcs.insert("str_trim".into());
    string_return_funcs.insert("datara_rt_str_char_at".into());
    string_return_funcs.insert("str_char_at".into());
    string_return_funcs.insert("char_at".into());
    string_return_funcs.insert("datara_rt_socket_recv".into());
    string_return_funcs.insert("socket_recv".into());
    string_return_funcs.insert("datara_rt_sha256".into());
    string_return_funcs.insert("sha256".into());
    string_return_funcs.insert("datara_rt_base64_encode".into());
    string_return_funcs.insert("base64_encode".into());
    string_return_funcs.insert("datara_rt_base64_decode".into());
    string_return_funcs.insert("base64_decode".into());
    string_return_funcs.insert("datara_rt_uuid_v4".into());
    string_return_funcs.insert("uuid_v4".into());
    string_return_funcs.insert("datara_rt_int_to_str".into());
    string_return_funcs.insert("int_to_str".into());
    string_return_funcs.insert("datara_rt_float_to_str".into());
    string_return_funcs.insert("float_to_str".into());
    string_return_funcs.insert("datara_rt_exec".into());
    string_return_funcs.insert("process_output".into());
    string_return_funcs.insert("exec".into());
    string_return_funcs.insert("datara_rt_str_repeat".into());
    string_return_funcs.insert("str_repeat".into());
    string_return_funcs.insert("repeat".into());
    string_return_funcs.insert("datara_rt_str_pad_left".into());
    string_return_funcs.insert("str_pad_left".into());
    string_return_funcs.insert("pad_left".into());
    string_return_funcs.insert("datara_rt_str_pad_right".into());
    string_return_funcs.insert("str_pad_right".into());
    string_return_funcs.insert("pad_right".into());
    string_return_funcs.insert("datara_rt_str_replace".into());
    string_return_funcs.insert("str_replace".into());
    string_return_funcs.insert("replace".into());
    string_return_funcs.insert("datara_rt_str_to_upper".into());
    string_return_funcs.insert("str_to_upper".into());
    string_return_funcs.insert("to_upper".into());
    string_return_funcs.insert("datara_rt_str_to_lower".into());
    string_return_funcs.insert("str_to_lower".into());
    string_return_funcs.insert("to_lower".into());
    string_return_funcs.insert("datara_rt_format_percent".into());
    string_return_funcs.insert("format_percent".into());
    string_return_funcs.insert("datara_rt_format_int_with_commas".into());
    string_return_funcs.insert("format_int_with_commas".into());
    string_return_funcs.insert("datara_js_eval".into());
    string_return_funcs.insert("js_eval".into());
    string_return_funcs.insert("datara_js_require".into());
    string_return_funcs.insert("js_require".into());
    string_return_funcs.insert("datara_js_call".into());
    string_return_funcs.insert("js_call".into());
    string_return_funcs.insert("datara_js_call_0".into());
    string_return_funcs.insert("js_call_0".into());
    string_return_funcs.insert("datara_js_call_1".into());
    string_return_funcs.insert("js_call_1".into());
    string_return_funcs.insert("datara_js_call_2".into());
    string_return_funcs.insert("js_call_2".into());
    string_return_funcs.insert("datara_js_get_global".into());
    string_return_funcs.insert("js_get_global".into());

    string_return_funcs.insert("datara_py_eval".into());
    string_return_funcs.insert("datara_py_eval_safe".into());
    string_return_funcs.insert("datara_py_last_error".into());
    string_return_funcs.insert("datara_py_call".into());
    string_return_funcs.insert("datara_py_call_1_str".into());
    string_return_funcs.insert("py_eval".into());
    string_return_funcs.insert("py_call".into());

    for (fn_name, func) in &dmir_module.functions {
        if func.return_type == "String" || func.return_type == "Str" {
            string_return_funcs.insert(fn_name.clone());
        }
    }
    for (ef_name, (_, ef_ret)) in &dmir_module.extern_functions {
        if ef_ret == "String" || ef_ret == "Str" {
            string_return_funcs.insert(ef_name.clone());
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

    let mut sorted_cls_names: Vec<&String> = dmir_module.class_fields.keys().collect();
    sorted_cls_names.sort();
    for cls_name in sorted_cls_names {
        let fields = &dmir_module.class_fields[cls_name];
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
                    let base_c = class_name
                        .split('<')
                        .next()
                        .unwrap_or(class_name)
                        .split('_')
                        .next()
                        .unwrap_or(class_name);
                    if let Some(base_offsets) = class_field_offsets.get(base_c).cloned() {
                        class_field_offsets
                            .entry(class_name.clone())
                            .or_insert(base_offsets);
                    } else {
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
    let add_str_literal = |s: &str, m: &mut M| -> Result<cranelift_module::DataId, String> {
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
    string_literal_map.insert("".to_string(), add_str_literal("", module)?);
    string_literal_map.insert(":".to_string(), add_str_literal(":", module)?);

    let mut all_literals = std::collections::BTreeSet::new();
    for func in dmir_module.functions.values() {
        for b in &func.blocks {
            for inst in &b.instructions {
                match inst {
                    Inst::ConstStr { value, .. } => {
                        all_literals.insert(value.clone());
                    }
                    Inst::FormatStr { parts, .. } => {
                        for p in parts {
                            all_literals.insert(p.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for s in all_literals {
        if !string_literal_map.contains_key(&s) {
            let id = add_str_literal(&s, module)?;
            string_literal_map.insert(s, id);
        }
    }

    Ok(ModuleDecls {
        class_field_offsets,
        field_default_offsets,
        string_fields,
        string_literal_map,
        main_entry_info,
        sorted_func_names,
        string_return_funcs,
    })
}
