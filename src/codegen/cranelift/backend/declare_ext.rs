use crate::dmir::Module;
use cranelift_codegen::ir::{AbiParam, Signature, types as clif_types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, Linkage, Module as ClifModule};
use std::collections::HashMap;

pub fn declare_runtime_ext<M: ClifModule>(
    module: &mut M,
    dmir_module: &Module,
    call_conv: CallConv,
    func_ids: &mut HashMap<String, (FuncId, Signature)>,
) -> Result<(FuncId, FuncId), String> {
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

    // str_repeat(s: ptr, count: i64) -> ptr
    let mut rt_str_repeat_sig = Signature::new(call_conv);
    rt_str_repeat_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_str_repeat_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_str_repeat_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_str_repeat_id = module
        .declare_function("datara_rt_str_repeat", Linkage::Import, &rt_str_repeat_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_repeat".into(),
        (rt_str_repeat_id, rt_str_repeat_sig.clone()),
    );
    func_ids.insert(
        "str_repeat".into(),
        (rt_str_repeat_id, rt_str_repeat_sig.clone()),
    );
    func_ids.insert("repeat".into(), (rt_str_repeat_id, rt_str_repeat_sig));

    // str_pad_left(s: ptr, total_len: i64, pad: ptr) -> ptr
    let mut rt_str_pad_sig = Signature::new(call_conv);
    rt_str_pad_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_pad_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_pad_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_pad_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_str_pad_left_id = module
        .declare_function("datara_rt_str_pad_left", Linkage::Import, &rt_str_pad_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_pad_left".into(),
        (rt_str_pad_left_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert(
        "str_pad_left".into(),
        (rt_str_pad_left_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert(
        "pad_left".into(),
        (rt_str_pad_left_id, rt_str_pad_sig.clone()),
    );

    // str_pad_right(s: ptr, total_len: i64, pad: ptr) -> ptr
    let rt_str_pad_right_id = module
        .declare_function("datara_rt_str_pad_right", Linkage::Import, &rt_str_pad_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_pad_right".into(),
        (rt_str_pad_right_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert(
        "str_pad_right".into(),
        (rt_str_pad_right_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert(
        "pad_right".into(),
        (rt_str_pad_right_id, rt_str_pad_sig.clone()),
    );

    // str_replace(s: ptr, target: ptr, replacement: ptr) -> ptr
    let rt_str_replace_id = module
        .declare_function("datara_rt_str_replace", Linkage::Import, &rt_str_pad_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_replace".into(),
        (rt_str_replace_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert(
        "str_replace".into(),
        (rt_str_replace_id, rt_str_pad_sig.clone()),
    );
    func_ids.insert("replace".into(), (rt_str_replace_id, rt_str_pad_sig));

    // str_to_upper(s: ptr) -> ptr
    let mut rt_str_case_sig = Signature::new(call_conv);
    rt_str_case_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_case_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_str_to_upper_id = module
        .declare_function("datara_rt_str_to_upper", Linkage::Import, &rt_str_case_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_to_upper".into(),
        (rt_str_to_upper_id, rt_str_case_sig.clone()),
    );
    func_ids.insert(
        "str_to_upper".into(),
        (rt_str_to_upper_id, rt_str_case_sig.clone()),
    );
    func_ids.insert(
        "to_upper".into(),
        (rt_str_to_upper_id, rt_str_case_sig.clone()),
    );

    // str_to_lower(s: ptr) -> ptr
    let rt_str_to_lower_id = module
        .declare_function("datara_rt_str_to_lower", Linkage::Import, &rt_str_case_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_to_lower".into(),
        (rt_str_to_lower_id, rt_str_case_sig.clone()),
    );
    func_ids.insert(
        "str_to_lower".into(),
        (rt_str_to_lower_id, rt_str_case_sig.clone()),
    );
    func_ids.insert("to_lower".into(), (rt_str_to_lower_id, rt_str_case_sig));

    // str_split(s: ptr, delim: ptr) -> ptr (list)
    let mut rt_str_split_sig = Signature::new(call_conv);
    rt_str_split_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_split_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_split_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_str_split_id = module
        .declare_function("datara_rt_str_split", Linkage::Import, &rt_str_split_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_split".into(),
        (rt_str_split_id, rt_str_split_sig.clone()),
    );
    func_ids.insert(
        "str_split".into(),
        (rt_str_split_id, rt_str_split_sig.clone()),
    );
    func_ids.insert("split".into(), (rt_str_split_id, rt_str_split_sig));

    // str_join(list: ptr, delim: ptr) -> ptr (str)
    let mut rt_str_join_sig = Signature::new(call_conv);
    rt_str_join_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_join_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_join_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_str_join_id = module
        .declare_function("datara_rt_str_join", Linkage::Import, &rt_str_join_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_join".into(),
        (rt_str_join_id, rt_str_join_sig.clone()),
    );
    func_ids.insert("str_join".into(), (rt_str_join_id, rt_str_join_sig.clone()));
    func_ids.insert("join".into(), (rt_str_join_id, rt_str_join_sig));

    // format_percent(val: f64, decimals: i64) -> ptr
    let mut rt_format_pct_sig = Signature::new(call_conv);
    rt_format_pct_sig
        .params
        .push(AbiParam::new(clif_types::F64));
    rt_format_pct_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_format_pct_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_format_pct_id = module
        .declare_function(
            "datara_rt_format_percent",
            Linkage::Import,
            &rt_format_pct_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_format_percent".into(),
        (rt_format_pct_id, rt_format_pct_sig.clone()),
    );
    func_ids.insert(
        "format_percent".into(),
        (rt_format_pct_id, rt_format_pct_sig),
    );

    // format_int_with_commas(n: i64) -> ptr
    let mut rt_format_commas_sig = Signature::new(call_conv);
    rt_format_commas_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_format_commas_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_format_commas_id = module
        .declare_function(
            "datara_rt_format_int_with_commas",
            Linkage::Import,
            &rt_format_commas_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_format_int_with_commas".into(),
        (rt_format_commas_id, rt_format_commas_sig.clone()),
    );
    func_ids.insert(
        "format_int_with_commas".into(),
        (rt_format_commas_id, rt_format_commas_sig),
    );

    // JS Interop: declare dynamically ONLY if used, preserving DCE zero-cost guarantee
    let uses_js_bridge = dmir_module.functions.values().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| match i {
                crate::dmir::Inst::Call { func, .. } => {
                    func.starts_with("datara_js_") || func.starts_with("js_")
                }
                _ => false,
            })
        })
    }) || dmir_module
        .extern_functions
        .keys()
        .any(|k| k.starts_with("datara_js_") || k.starts_with("js_"));

    if uses_js_bridge {
        let mut js_eval_sig = Signature::new(call_conv);
        js_eval_sig.params.push(AbiParam::new(clif_types::I64));
        js_eval_sig.returns.push(AbiParam::new(clif_types::I64));
        let js_eval_id = module
            .declare_function("datara_js_eval", Linkage::Import, &js_eval_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_js_eval".into(), (js_eval_id, js_eval_sig.clone()));
        func_ids.insert("js_eval".into(), (js_eval_id, js_eval_sig.clone()));

        let js_require_id = module
            .declare_function("datara_js_require", Linkage::Import, &js_eval_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_require".into(),
            (js_require_id, js_eval_sig.clone()),
        );
        func_ids.insert("js_require".into(), (js_require_id, js_eval_sig.clone()));

        let js_eval_int_id = module
            .declare_function("datara_js_eval_int", Linkage::Import, &js_eval_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_eval_int".into(),
            (js_eval_int_id, js_eval_sig.clone()),
        );
        func_ids.insert("js_eval_int".into(), (js_eval_int_id, js_eval_sig.clone()));

        let mut js_eval_flt_sig = Signature::new(call_conv);
        js_eval_flt_sig.params.push(AbiParam::new(clif_types::I64));
        js_eval_flt_sig.returns.push(AbiParam::new(clif_types::F64));
        let js_eval_flt_id = module
            .declare_function("datara_js_eval_float", Linkage::Import, &js_eval_flt_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_eval_float".into(),
            (js_eval_flt_id, js_eval_flt_sig.clone()),
        );
        func_ids.insert("js_eval_float".into(), (js_eval_flt_id, js_eval_flt_sig));

        let mut js_2_str_sig = Signature::new(call_conv);
        js_2_str_sig.params.push(AbiParam::new(clif_types::I64));
        js_2_str_sig.params.push(AbiParam::new(clif_types::I64));
        js_2_str_sig.returns.push(AbiParam::new(clif_types::I64));
        let js_call_id = module
            .declare_function("datara_js_call", Linkage::Import, &js_2_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_js_call".into(), (js_call_id, js_2_str_sig.clone()));
        func_ids.insert("js_call".into(), (js_call_id, js_2_str_sig.clone()));

        let js_call_0_id = module
            .declare_function("datara_js_call_0", Linkage::Import, &js_eval_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_call_0".into(),
            (js_call_0_id, js_eval_sig.clone()),
        );
        func_ids.insert("js_call_0".into(), (js_call_0_id, js_eval_sig.clone()));

        let js_call_1_id = module
            .declare_function("datara_js_call_1", Linkage::Import, &js_2_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_call_1".into(),
            (js_call_1_id, js_2_str_sig.clone()),
        );
        func_ids.insert("js_call_1".into(), (js_call_1_id, js_2_str_sig.clone()));

        let mut js_3_str_sig = Signature::new(call_conv);
        js_3_str_sig.params.push(AbiParam::new(clif_types::I64));
        js_3_str_sig.params.push(AbiParam::new(clif_types::I64));
        js_3_str_sig.params.push(AbiParam::new(clif_types::I64));
        js_3_str_sig.returns.push(AbiParam::new(clif_types::I64));
        let js_call_2_id = module
            .declare_function("datara_js_call_2", Linkage::Import, &js_3_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_call_2".into(),
            (js_call_2_id, js_3_str_sig.clone()),
        );
        func_ids.insert("js_call_2".into(), (js_call_2_id, js_3_str_sig));

        let js_set_global_id = module
            .declare_function("datara_js_set_global", Linkage::Import, &js_2_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_set_global".into(),
            (js_set_global_id, js_2_str_sig.clone()),
        );
        func_ids.insert(
            "js_set_global".into(),
            (js_set_global_id, js_2_str_sig.clone()),
        );

        let js_get_global_id = module
            .declare_function("datara_js_get_global", Linkage::Import, &js_eval_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_get_global".into(),
            (js_get_global_id, js_eval_sig.clone()),
        );
        func_ids.insert(
            "js_get_global".into(),
            (js_get_global_id, js_eval_sig.clone()),
        );

        let js_export_list_id = module
            .declare_function("datara_js_export_list_f64", Linkage::Import, &js_2_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_export_list_f64".into(),
            (js_export_list_id, js_2_str_sig.clone()),
        );

        let js_assert_same_ptr_id = module
            .declare_function("datara_js_assert_same_ptr", Linkage::Import, &js_2_str_sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(
            "datara_js_assert_same_ptr".into(),
            (js_assert_same_ptr_id, js_2_str_sig.clone()),
        );
    }

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

    // Network: http_get (takes the URL; a missing argument is tolerated
    // so legacy zero-arg `http_get()` calls still compile through codegen)
    let mut rt_http_get_sig = Signature::new(call_conv);
    rt_http_get_sig.params.push(AbiParam::new(clif_types::I64));
    rt_http_get_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_http_get_id = module
        .declare_function("datara_rt_http_get", Linkage::Import, &rt_http_get_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_http_get".into(),
        (rt_http_get_id, rt_http_get_sig.clone()),
    );
    func_ids.insert("http_get".into(), (rt_http_get_id, rt_http_get_sig));

    // String: str_len
    let mut rt_str_len_sig = Signature::new(call_conv);
    rt_str_len_sig.params.push(AbiParam::new(clif_types::I64));
    rt_str_len_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_str_len_id = module
        .declare_function("datara_rt_str_len", Linkage::Import, &rt_str_len_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_str_len".into(),
        (rt_str_len_id, rt_str_len_sig.clone()),
    );
    func_ids.insert("str_len".into(), (rt_str_len_id, rt_str_len_sig));

    // String: int_to_str
    let mut rt_i2s_sig = Signature::new(call_conv);
    rt_i2s_sig.params.push(AbiParam::new(clif_types::I64));
    rt_i2s_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_i2s_id = module
        .declare_function("datara_rt_int_to_str", Linkage::Import, &rt_i2s_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_int_to_str".into(),
        (rt_i2s_id, rt_i2s_sig.clone()),
    );
    func_ids.insert("int_to_str".into(), (rt_i2s_id, rt_i2s_sig));

    // String: float_to_str
    let mut rt_f2s_sig = Signature::new(call_conv);
    rt_f2s_sig.params.push(AbiParam::new(clif_types::F64));
    rt_f2s_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_f2s_id = module
        .declare_function("datara_rt_float_to_str", Linkage::Import, &rt_f2s_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_float_to_str".into(),
        (rt_f2s_id, rt_f2s_sig.clone()),
    );
    func_ids.insert("float_to_str".into(), (rt_f2s_id, rt_f2s_sig));

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

    // Crypto: uuid_v4
    let mut rt_uuid_sig = Signature::new(call_conv);
    rt_uuid_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_uuid_id = module
        .declare_function("datara_rt_uuid_v4", Linkage::Import, &rt_uuid_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_uuid_v4".into(),
        (rt_uuid_id, rt_uuid_sig.clone()),
    );
    func_ids.insert("uuid_v4".into(), (rt_uuid_id, rt_uuid_sig));

    // Native dialogs
    let mut rt_dialog_sig = Signature::new(call_conv);
    rt_dialog_sig.params.push(AbiParam::new(clif_types::I64));
    rt_dialog_sig.params.push(AbiParam::new(clif_types::I64));
    rt_dialog_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_dlg_info_id = module
        .declare_function("datara_rt_dialog_info", Linkage::Import, &rt_dialog_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_dialog_info".into(),
        (rt_dlg_info_id, rt_dialog_sig.clone()),
    );

    let rt_dlg_alert_id = module
        .declare_function("datara_rt_dialog_alert", Linkage::Import, &rt_dialog_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_dialog_alert".into(),
        (rt_dlg_alert_id, rt_dialog_sig.clone()),
    );

    let rt_dlg_confirm_id = module
        .declare_function("datara_rt_dialog_confirm", Linkage::Import, &rt_dialog_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_dialog_confirm".into(),
        (rt_dlg_confirm_id, rt_dialog_sig),
    );

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

    // Proof-Carrying Scheduler runtime: schedule_run, schedule_cancel
    let mut rt_sched_run_sig = Signature::new(call_conv);
    rt_sched_run_sig.params.push(AbiParam::new(clif_types::I64)); // node_count
    rt_sched_run_sig.params.push(AbiParam::new(clif_types::I64)); // nodes
    rt_sched_run_sig.params.push(AbiParam::new(clif_types::I64)); // region_id
    rt_sched_run_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_sched_run_id = module
        .declare_function("datara_rt_schedule_run", Linkage::Import, &rt_sched_run_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_schedule_run".into(),
        (rt_sched_run_id, rt_sched_run_sig.clone()),
    );
    func_ids.insert("schedule_run".into(), (rt_sched_run_id, rt_sched_run_sig));

    let mut rt_sched_cancel_sig = Signature::new(call_conv);
    rt_sched_cancel_sig
        .params
        .push(AbiParam::new(clif_types::I64)); // region_id
    let rt_sched_cancel_id = module
        .declare_function(
            "datara_rt_schedule_cancel",
            Linkage::Import,
            &rt_sched_cancel_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_schedule_cancel".into(),
        (rt_sched_cancel_id, rt_sched_cancel_sig.clone()),
    );
    func_ids.insert(
        "schedule_cancel".into(),
        (rt_sched_cancel_id, rt_sched_cancel_sig),
    );

    // Fast Math: 1-arg double -> double (sqrt, sin, cos, tan, floor, ceil, round, log, exp)
    // NOTE: plain "abs" is intentionally excluded here. `abs(Float)` is
    // lowered inline via Cranelift's `fabs` instruction in inst_call.rs,
    // and `abs(Int)` is provided by sparks / user code with an i64 signature.
    // Declaring "abs" as F64 here would cause a signature conflict whenever
    // sparks declares abs(i64) -> i64.
    let f64_1_funcs = [
        ("sqrt", "math_sqrt", "datara_rt_math_sqrt"),
        ("datara_rt_math_abs", "math_abs", "datara_rt_math_abs"),
        ("sin", "math_sin", "datara_rt_math_sin"),
        ("cos", "math_cos", "datara_rt_math_cos"),
        ("tan", "math_tan", "datara_rt_math_tan"),
        ("floor", "math_floor", "datara_rt_math_floor"),
        ("ceil", "math_ceil", "datara_rt_math_ceil"),
        ("round", "math_round", "datara_rt_math_round"),
        ("log", "math_log", "datara_rt_math_log"),
        ("exp", "math_exp", "datara_rt_math_exp"),
    ];
    for (crt_name, alias, rt_name) in &f64_1_funcs {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(clif_types::F64));
        sig.returns.push(AbiParam::new(clif_types::F64));
        let id = module
            .declare_function(crt_name, Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(crt_name.to_string(), (id, sig.clone()));
        func_ids.insert(alias.to_string(), (id, sig.clone()));
        func_ids.insert(rt_name.to_string(), (id, sig));
    }

    // Fast Math: 2-arg double -> double (pow, min, max, hypot)
    let f64_2_funcs = [
        ("pow", "math_pow", "datara_rt_math_pow"),
        ("fmin", "math_min", "datara_rt_math_min"),
        ("fmax", "math_max", "datara_rt_math_max"),
        ("hypot", "math_hypot", "datara_rt_math_hypot"),
    ];
    for (crt_name, alias, rt_name) in &f64_2_funcs {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(clif_types::F64));
        sig.params.push(AbiParam::new(clif_types::F64));
        sig.returns.push(AbiParam::new(clif_types::F64));
        let id = module
            .declare_function(crt_name, Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(crt_name.to_string(), (id, sig.clone()));
        func_ids.insert(alias.to_string(), (id, sig.clone()));
        func_ids.insert(rt_name.to_string(), (id, sig));
    }

    // Fast Math: 3-arg double -> double (clamp)
    let mut f64_3_sig = Signature::new(call_conv);
    f64_3_sig.params.push(AbiParam::new(clif_types::F64));
    f64_3_sig.params.push(AbiParam::new(clif_types::F64));
    f64_3_sig.params.push(AbiParam::new(clif_types::F64));
    f64_3_sig.returns.push(AbiParam::new(clif_types::F64));
    let rt_clamp_id = module
        .declare_function("datara_rt_math_clamp", Linkage::Import, &f64_3_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_math_clamp".into(),
        (rt_clamp_id, f64_3_sig.clone()),
    );
    func_ids.insert("math_clamp".into(), (rt_clamp_id, f64_3_sig));

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

    let mut i64_3_sig = Signature::new(call_conv);
    i64_3_sig.params.push(AbiParam::new(clif_types::I64));
    i64_3_sig.params.push(AbiParam::new(clif_types::I64));
    i64_3_sig.params.push(AbiParam::new(clif_types::I64));
    i64_3_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_clamp_int_id = module
        .declare_function("datara_rt_math_clamp_int", Linkage::Import, &i64_3_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_math_clamp_int".into(),
        (rt_clamp_int_id, i64_3_sig.clone()),
    );
    func_ids.insert("math_clamp_int".into(), (rt_clamp_int_id, i64_3_sig));

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

    let mut own_acq_sig = Signature::new(call_conv);
    own_acq_sig.params.push(AbiParam::new(clif_types::I64));
    own_acq_sig.returns.push(AbiParam::new(clif_types::I64));
    let own_acq_id = module
        .declare_function("datara_rt_own_acquire", Linkage::Import, &own_acq_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("datara_rt_own_acquire".into(), (own_acq_id, own_acq_sig));

    let mut own_rel_sig = Signature::new(call_conv);
    own_rel_sig.params.push(AbiParam::new(clif_types::I64));
    let own_rel_id = module
        .declare_function("datara_rt_own_release", Linkage::Import, &own_rel_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("datara_rt_own_release".into(), (own_rel_id, own_rel_sig));

    let mut cap_set_sig = Signature::new(call_conv);
    cap_set_sig.params.push(AbiParam::new(clif_types::I64));
    let cap_set_id = module
        .declare_function("datara_rt_cap_set_mask", Linkage::Import, &cap_set_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_cap_set_mask".into(),
        (cap_set_id, cap_set_sig.clone()),
    );
    func_ids.insert("cap_set_mask".into(), (cap_set_id, cap_set_sig));

    let mut cap_get_sig = Signature::new(call_conv);
    cap_get_sig.returns.push(AbiParam::new(clif_types::I64));
    let cap_get_id = module
        .declare_function("datara_rt_cap_get_mask", Linkage::Import, &cap_get_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_cap_get_mask".into(),
        (cap_get_id, cap_get_sig.clone()),
    );
    func_ids.insert("cap_get_mask".into(), (cap_get_id, cap_get_sig));

    let mut cap_rev_sig = Signature::new(call_conv);
    cap_rev_sig.params.push(AbiParam::new(clif_types::I64));
    let cap_rev_id = module
        .declare_function("datara_rt_cap_revoke", Linkage::Import, &cap_rev_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_cap_revoke".into(),
        (cap_rev_id, cap_rev_sig.clone()),
    );
    func_ids.insert("cap_revoke".into(), (cap_rev_id, cap_rev_sig));

    let mut cap_grant_sig = Signature::new(call_conv);
    cap_grant_sig.params.push(AbiParam::new(clif_types::I64));
    let cap_grant_id = module
        .declare_function("datara_rt_cap_grant", Linkage::Import, &cap_grant_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_cap_grant".into(),
        (cap_grant_id, cap_grant_sig.clone()),
    );
    func_ids.insert("cap_grant".into(), (cap_grant_id, cap_grant_sig));

    let mut cap_req_sig = Signature::new(call_conv);
    cap_req_sig.params.push(AbiParam::new(clif_types::I64));
    cap_req_sig.params.push(AbiParam::new(clif_types::I64));
    let cap_req_id = module
        .declare_function("datara_rt_cap_require", Linkage::Import, &cap_req_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_cap_require".into(),
        (cap_req_id, cap_req_sig.clone()),
    );
    func_ids.insert("cap_require".into(), (cap_req_id, cap_req_sig));

    Ok((rt_str_char_at_id, rt_str_eq_id))
}
