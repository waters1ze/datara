use cranelift_codegen::ir::{AbiParam, Signature, types as clif_types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, Linkage, Module as ClifModule};
use std::collections::HashMap;

use super::types::CoreRuntimeIds;

pub fn declare_runtime_core<M: ClifModule>(
    module: &mut M,
    call_conv: CallConv,
    func_ids: &mut HashMap<String, (FuncId, Signature)>,
) -> Result<CoreRuntimeIds, String> {
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

    let mut pgo_str_sig = Signature::new(call_conv);
    pgo_str_sig.params.push(AbiParam::new(clif_types::I64));
    let _ = module.declare_function("datara_rt_pgo_hit_func", Linkage::Import, &pgo_str_sig);
    let _ = module.declare_function(
        "datara_rt_pgo_set_output_file",
        Linkage::Import,
        &pgo_str_sig,
    );
    let _ = module.declare_function("datara_rt_pgo_flush", Linkage::Import, &pgo_str_sig);
    let _ = module.declare_function(
        "datara_rt_pgo_reset",
        Linkage::Import,
        &Signature::new(call_conv),
    );

    let mut pgo_branch_sig = Signature::new(call_conv);
    pgo_branch_sig.params.push(AbiParam::new(clif_types::I64));
    pgo_branch_sig.params.push(AbiParam::new(clif_types::I64));
    let _ = module.declare_function("datara_rt_pgo_hit_branch", Linkage::Import, &pgo_branch_sig);
    let _ = module.declare_function("datara_rt_pgo_hit_loop", Linkage::Import, &pgo_branch_sig);

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

    // datara_rt_list_create(cap) -> ptr: used by the lowering fallback
    // for list literals larger than the fixed-arity constructors.
    let mut rt_list_create_sig = Signature::new(call_conv);
    rt_list_create_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_list_create_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_list_create_id = module
        .declare_function(
            "datara_rt_list_create",
            Linkage::Import,
            &rt_list_create_sig,
        )
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

    let mut rt_map_get_sig = Signature::new(call_conv);
    rt_map_get_sig.params.push(AbiParam::new(clif_types::I64));
    rt_map_get_sig.params.push(AbiParam::new(clif_types::I64));
    rt_map_get_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_map_get_id = module
        .declare_function("datara_rt_map_get", Linkage::Import, &rt_map_get_sig)
        .map_err(|e| e.to_string())?;

    let mut rt_map_contains_sig = Signature::new(call_conv);
    rt_map_contains_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_map_contains_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    rt_map_contains_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_map_contains_id = module
        .declare_function(
            "datara_rt_map_contains",
            Linkage::Import,
            &rt_map_contains_sig,
        )
        .map_err(|e| e.to_string())?;

    let mut rt_map_len_sig = Signature::new(call_conv);
    rt_map_len_sig.params.push(AbiParam::new(clif_types::I64));
    rt_map_len_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_map_len_id = module
        .declare_function("datara_rt_map_len", Linkage::Import, &rt_map_len_sig)
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

    func_ids.insert(
        "datara_rt_list_get".into(),
        (rt_list_get_id, rt_list_get_sig),
    );
    func_ids.insert(
        "datara_rt_list_create".into(),
        (rt_list_create_id, rt_list_create_sig),
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
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_format_str_i64_str_i64".into(),
        (rt_format_sisi_id, rt_format_sisi_sig),
    );
    // datara_rt_map_create() -> ptr: 0-arg constructor used by the
    // lowering fallback for empty and oversized map literals.
    {
        let mut sig = Signature::new(call_conv);
        sig.returns.push(AbiParam::new(clif_types::I64));
        let id = module
            .declare_function("datara_rt_map_create", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert("datara_rt_map_create".into(), (id, sig));
    }
    // The DMIR lowering emits `datara_rt_map_create_{n}` for an n-entry
    // map literal, so every supported arity (1..=5) must have a declaration.
    for n in 1..=5 {
        let name = format!("datara_rt_map_create_{}", n);
        let mut sig = Signature::new(call_conv);
        for _ in 0..(n * 2) {
            sig.params.push(AbiParam::new(clif_types::I64));
        }
        sig.returns.push(AbiParam::new(clif_types::I64));
        let id = module
            .declare_function(&name, Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.insert(name, (id, sig));
    }
    func_ids.insert("datara_rt_map_get".into(), (rt_map_get_id, rt_map_get_sig));
    func_ids.insert(
        "datara_rt_map_contains".into(),
        (rt_map_contains_id, rt_map_contains_sig),
    );
    func_ids.insert("datara_rt_map_len".into(), (rt_map_len_id, rt_map_len_sig));
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

    let mut now_ns_sig = Signature::new(call_conv);
    now_ns_sig.returns.push(AbiParam::new(clif_types::I64));
    let now_ns_id = module
        .declare_function("datara_rt_now_ns", Linkage::Import, &now_ns_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("now_ns".into(), (now_ns_id, now_ns_sig.clone()));
    func_ids.insert("datara_rt_now_ns".into(), (now_ns_id, now_ns_sig));

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

    let mut time_precise_sig = Signature::new(call_conv);
    time_precise_sig
        .returns
        .push(AbiParam::new(clif_types::F64));
    let time_precise_id = module
        .declare_function(
            "datara_rt_time_precise_ms",
            Linkage::Import,
            &time_precise_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_time_precise_ms".into(),
        (time_precise_id, time_precise_sig.clone()),
    );
    func_ids.insert(
        "time_precise_ms".into(),
        (time_precise_id, time_precise_sig),
    );

    let mut time_delta_sig = Signature::new(call_conv);
    time_delta_sig.returns.push(AbiParam::new(clif_types::F64));
    let time_delta_id = module
        .declare_function("datara_rt_time_delta_ms", Linkage::Import, &time_delta_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_time_delta_ms".into(),
        (time_delta_id, time_delta_sig.clone()),
    );
    func_ids.insert("time_delta_ms".into(), (time_delta_id, time_delta_sig));

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
    func_ids.insert("byte_len".into(), (str_len_id, str_len_sig.clone()));
    func_ids.insert("len".into(), (str_len_id, str_len_sig.clone()));
    func_ids.insert(
        "datara_rt_str_len".into(),
        (str_len_id, str_len_sig.clone()),
    );
    func_ids.insert(
        "datara_rt_byte_len".into(),
        (str_len_id, str_len_sig.clone()),
    );
    func_ids.insert("datara_rt_len".into(), (str_len_id, str_len_sig));

    let mut str_chars_sig = Signature::new(call_conv);
    str_chars_sig.params.push(AbiParam::new(clif_types::I64));
    str_chars_sig.returns.push(AbiParam::new(clif_types::I64));
    let str_chars_id = module
        .declare_function("datara_rt_str_chars", Linkage::Import, &str_chars_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("str_chars".into(), (str_chars_id, str_chars_sig.clone()));
    func_ids.insert("char_len".into(), (str_chars_id, str_chars_sig.clone()));
    func_ids.insert(
        "datara_rt_char_len".into(),
        (str_chars_id, str_chars_sig.clone()),
    );
    func_ids.insert("datara_rt_str_chars".into(), (str_chars_id, str_chars_sig));

    let mut validate_utf8_sig = Signature::new(call_conv);
    validate_utf8_sig
        .params
        .push(AbiParam::new(clif_types::I64));
    validate_utf8_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let validate_utf8_id = module
        .declare_function(
            "datara_rt_validate_utf8",
            Linkage::Import,
            &validate_utf8_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "validate_utf8".into(),
        (validate_utf8_id, validate_utf8_sig.clone()),
    );
    func_ids.insert(
        "datara_rt_validate_utf8".into(),
        (validate_utf8_id, validate_utf8_sig),
    );

    let mut str_offset_sig = Signature::new(call_conv);
    str_offset_sig.params.push(AbiParam::new(clif_types::I64));
    str_offset_sig.params.push(AbiParam::new(clif_types::I64));
    str_offset_sig.returns.push(AbiParam::new(clif_types::I64));
    let str_scalar_at_id = module
        .declare_function("datara_rt_str_scalar_at", Linkage::Import, &str_offset_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "str_scalar_at".into(),
        (str_scalar_at_id, str_offset_sig.clone()),
    );
    func_ids.insert(
        "datara_rt_str_scalar_at".into(),
        (str_scalar_at_id, str_offset_sig.clone()),
    );

    let str_next_offset_id = module
        .declare_function(
            "datara_rt_str_next_offset",
            Linkage::Import,
            &str_offset_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "str_next_offset".into(),
        (str_next_offset_id, str_offset_sig.clone()),
    );
    func_ids.insert(
        "datara_rt_str_next_offset".into(),
        (str_next_offset_id, str_offset_sig.clone()),
    );

    let str_byte_at_id = module
        .declare_function("datara_rt_str_byte_at", Linkage::Import, &str_offset_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "str_byte_at".into(),
        (str_byte_at_id, str_offset_sig.clone()),
    );
    func_ids.insert("byte_at".into(), (str_byte_at_id, str_offset_sig.clone()));
    func_ids.insert(
        "datara_rt_str_byte_at".into(),
        (str_byte_at_id, str_offset_sig),
    );

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

    // Fast streaming print functions
    let mut void_1_i64_sig = Signature::new(call_conv);
    void_1_i64_sig.params.push(AbiParam::new(clif_types::I64));
    let rt_print_str_id = module
        .declare_function("datara_rt_print_str", Linkage::Import, &void_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_str".into(),
        (rt_print_str_id, void_1_i64_sig.clone()),
    );

    let rt_print_int_id = module
        .declare_function("datara_rt_print_int", Linkage::Import, &void_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_int".into(),
        (rt_print_int_id, void_1_i64_sig.clone()),
    );

    let mut void_1_f64_sig = Signature::new(call_conv);
    void_1_f64_sig.params.push(AbiParam::new(clif_types::F64));
    let rt_print_flt_id = module
        .declare_function("datara_rt_print_float", Linkage::Import, &void_1_f64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_float".into(),
        (rt_print_flt_id, void_1_f64_sig),
    );

    let rt_print_bool_id = module
        .declare_function("datara_rt_print_bool", Linkage::Import, &void_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_bool".into(),
        (rt_print_bool_id, void_1_i64_sig.clone()),
    );

    let rt_print_list_id = module
        .declare_function("datara_rt_print_list", Linkage::Import, &void_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_list".into(),
        (rt_print_list_id, void_1_i64_sig),
    );

    let void_0_sig = Signature::new(call_conv);
    let rt_print_sp_id = module
        .declare_function("datara_rt_print_space", Linkage::Import, &void_0_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_space".into(),
        (rt_print_sp_id, void_0_sig.clone()),
    );

    let rt_print_nl_id = module
        .declare_function("datara_rt_print_newline", Linkage::Import, &void_0_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_print_newline".into(),
        (rt_print_nl_id, void_0_sig.clone()),
    );

    let rt_flush_id = module
        .declare_function("datara_rt_flush", Linkage::Import, &void_0_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("datara_rt_flush".into(), (rt_flush_id, void_0_sig));

    // Fast typed input functions
    let mut i64_1_i64_sig = Signature::new(call_conv);
    i64_1_i64_sig.params.push(AbiParam::new(clif_types::I64));
    i64_1_i64_sig.returns.push(AbiParam::new(clif_types::I64));
    let rt_input_int_id = module
        .declare_function("datara_rt_input_int", Linkage::Import, &i64_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_input_int".into(),
        (rt_input_int_id, i64_1_i64_sig.clone()),
    );
    func_ids.insert("input_int".into(), (rt_input_int_id, i64_1_i64_sig));

    let mut f64_1_i64_sig = Signature::new(call_conv);
    f64_1_i64_sig.params.push(AbiParam::new(clif_types::I64));
    f64_1_i64_sig.returns.push(AbiParam::new(clif_types::F64));
    let rt_input_flt_id = module
        .declare_function("datara_rt_input_float", Linkage::Import, &f64_1_i64_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_input_float".into(),
        (rt_input_flt_id, f64_1_i64_sig.clone()),
    );
    func_ids.insert("input_float".into(), (rt_input_flt_id, f64_1_i64_sig));

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

    // Path: path_join
    let mut rt_path_join_sig = Signature::new(call_conv);
    rt_path_join_sig.params.push(AbiParam::new(clif_types::I64));
    rt_path_join_sig.params.push(AbiParam::new(clif_types::I64));
    rt_path_join_sig
        .returns
        .push(AbiParam::new(clif_types::I64));
    let rt_path_join_id = module
        .declare_function("datara_rt_path_join", Linkage::Import, &rt_path_join_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_path_join".into(),
        (rt_path_join_id, rt_path_join_sig.clone()),
    );
    func_ids.insert("path_join".into(), (rt_path_join_id, rt_path_join_sig));

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

    // Phase 17 Chase-Lev deque
    let mut cl_create_sig = Signature::new(call_conv);
    cl_create_sig.params.push(AbiParam::new(clif_types::I64));
    cl_create_sig.returns.push(AbiParam::new(clif_types::I64));
    let cl_create_id = module
        .declare_function(
            "datara_rt_chase_lev_create",
            Linkage::Import,
            &cl_create_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_chase_lev_create".into(),
        (cl_create_id, cl_create_sig),
    );

    let mut cl_destroy_sig = Signature::new(call_conv);
    cl_destroy_sig.params.push(AbiParam::new(clif_types::I64));
    let cl_destroy_id = module
        .declare_function(
            "datara_rt_chase_lev_destroy",
            Linkage::Import,
            &cl_destroy_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_chase_lev_destroy".into(),
        (cl_destroy_id, cl_destroy_sig),
    );

    let mut cl_push_sig = Signature::new(call_conv);
    cl_push_sig.params.push(AbiParam::new(clif_types::I64));
    cl_push_sig.params.push(AbiParam::new(clif_types::I64));
    let cl_push_id = module
        .declare_function("datara_rt_chase_lev_push", Linkage::Import, &cl_push_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("datara_rt_chase_lev_push".into(), (cl_push_id, cl_push_sig));

    let mut cl_pop_sig = Signature::new(call_conv);
    cl_pop_sig.params.push(AbiParam::new(clif_types::I64));
    cl_pop_sig.returns.push(AbiParam::new(clif_types::I64));
    let cl_pop_id = module
        .declare_function("datara_rt_chase_lev_pop", Linkage::Import, &cl_pop_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_chase_lev_pop".into(),
        (cl_pop_id, cl_pop_sig.clone()),
    );

    let cl_steal_id = module
        .declare_function("datara_rt_chase_lev_steal", Linkage::Import, &cl_pop_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_chase_lev_steal".into(),
        (cl_steal_id, cl_pop_sig.clone()),
    );

    let cl_size_id = module
        .declare_function("datara_rt_chase_lev_size", Linkage::Import, &cl_pop_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert("datara_rt_chase_lev_size".into(), (cl_size_id, cl_pop_sig));

    // Phase 17 SIMD fast memory operations
    let mut fast_mem_sig = Signature::new(call_conv);
    fast_mem_sig.params.push(AbiParam::new(clif_types::I64));
    fast_mem_sig.params.push(AbiParam::new(clif_types::I64));
    fast_mem_sig.params.push(AbiParam::new(clif_types::I64));
    fast_mem_sig.returns.push(AbiParam::new(clif_types::I64));
    let fast_memcpy_id = module
        .declare_function("datara_rt_fast_memcpy", Linkage::Import, &fast_mem_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_fast_memcpy".into(),
        (fast_memcpy_id, fast_mem_sig.clone()),
    );

    let mut fast_memset_sig = Signature::new(call_conv);
    fast_memset_sig.params.push(AbiParam::new(clif_types::I64));
    fast_memset_sig.params.push(AbiParam::new(clif_types::I32));
    fast_memset_sig.params.push(AbiParam::new(clif_types::I64));
    fast_memset_sig.returns.push(AbiParam::new(clif_types::I64));
    let fast_memset_id = module
        .declare_function("datara_rt_fast_memset", Linkage::Import, &fast_memset_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_fast_memset".into(),
        (fast_memset_id, fast_memset_sig),
    );

    let mut fast_cmp_sig = Signature::new(call_conv);
    fast_cmp_sig.params.push(AbiParam::new(clif_types::I64));
    fast_cmp_sig.params.push(AbiParam::new(clif_types::I64));
    fast_cmp_sig.params.push(AbiParam::new(clif_types::I64));
    fast_cmp_sig.returns.push(AbiParam::new(clif_types::I32));
    let fast_strncmp_id = module
        .declare_function("datara_rt_fast_strncmp", Linkage::Import, &fast_cmp_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_fast_strncmp".into(),
        (fast_strncmp_id, fast_cmp_sig.clone()),
    );

    let fast_memcmp_id = module
        .declare_function("datara_rt_fast_memcmp", Linkage::Import, &fast_cmp_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_fast_memcmp".into(),
        (fast_memcmp_id, fast_cmp_sig),
    );

    // Phase 17 Thread pinning
    let mut pin_thread_sig = Signature::new(call_conv);
    pin_thread_sig.params.push(AbiParam::new(clif_types::I64));
    pin_thread_sig.returns.push(AbiParam::new(clif_types::I32));
    let pin_thread_id = module
        .declare_function("datara_rt_pin_thread", Linkage::Import, &pin_thread_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_pin_thread".into(),
        (pin_thread_id, pin_thread_sig),
    );

    let mut get_core_sig = Signature::new(call_conv);
    get_core_sig.returns.push(AbiParam::new(clif_types::I64));
    let get_core_id = module
        .declare_function("datara_rt_get_current_core", Linkage::Import, &get_core_sig)
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_get_current_core".into(),
        (get_core_id, get_core_sig),
    );

    let pin_workers_sig = Signature::new(call_conv);
    let pin_workers_id = module
        .declare_function(
            "datara_rt_pin_worker_threads",
            Linkage::Import,
            &pin_workers_sig,
        )
        .map_err(|e| e.to_string())?;
    func_ids.insert(
        "datara_rt_pin_worker_threads".into(),
        (pin_workers_id, pin_workers_sig),
    );

    Ok(CoreRuntimeIds {
        rt_out_int_id,
        rt_out_bool_id,
        rt_out_flt_id,
        rt_out_str_id,
        rt_err_id,
        rt_concat_id,
        rt_concat_3_id,
        rt_concat_4_id,
        rt_concat_5_id,
        rt_int_to_str_id,
        rt_bool_to_str_id,
        rt_flt_to_str_id,
        malloc_id,
        rt_list_get_id,
        rt_list_create_id,
        rt_list_len_id,
        rt_list_set_id,
        rt_list_append_id,
        rt_map_get_id,
        rt_map_insert_id,
        rt_pop_id,
        str_byte_at_id,
        str_chars_id,
        str_len_id,
    })
}
