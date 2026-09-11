use super::{DataraType, TypeChecker};
use crate::resolver::Resolver;
use std::collections::HashMap;

impl<'a> TypeChecker<'a> {
    pub fn new(resolver: &'a Resolver) -> Self {
        let mut class_fields = HashMap::new();
        let mut class_methods = HashMap::new();
        let mut generic_templates = HashMap::new();
        let mut function_signatures = HashMap::new();
        let fn_symbol_types = HashMap::new();

        // Built-in prelude function signatures
        // Capabilities & Safe OS Resources prelude
        let mut sys_caps_fields = HashMap::new();
        sys_caps_fields.insert(
            "files".to_string(),
            DataraType::Class("FileCapabilityProvider".into()),
        );
        sys_caps_fields.insert(
            "net".to_string(),
            DataraType::Class("NetCapabilityProvider".into()),
        );
        sys_caps_fields.insert(
            "proc".to_string(),
            DataraType::Class("ProcessCapabilityProvider".into()),
        );
        class_fields.insert("SystemCapabilities".to_string(), sys_caps_fields);

        let mut file_prov_methods = HashMap::new();
        file_prov_methods.insert(
            "grant_readonly".to_string(),
            DataraType::GenericInstance {
                name: "Capability".into(),
                args: vec![DataraType::Class("FileRead".into())],
            },
        );
        file_prov_methods.insert(
            "grant_readwrite".to_string(),
            DataraType::GenericInstance {
                name: "Capability".into(),
                args: vec![DataraType::Class("FileWrite".into())],
            },
        );
        class_methods.insert("FileCapabilityProvider".to_string(), file_prov_methods);

        let mut net_prov_methods = HashMap::new();
        net_prov_methods.insert(
            "grant_connect".to_string(),
            DataraType::GenericInstance {
                name: "Capability".into(),
                args: vec![DataraType::Class("NetworkConnect".into())],
            },
        );
        net_prov_methods.insert(
            "grant_listen".to_string(),
            DataraType::GenericInstance {
                name: "Capability".into(),
                args: vec![DataraType::Class("NetworkListen".into())],
            },
        );
        class_methods.insert("NetCapabilityProvider".to_string(), net_prov_methods);

        let mut proc_prov_methods = HashMap::new();
        proc_prov_methods.insert(
            "grant_exec".to_string(),
            DataraType::GenericInstance {
                name: "Capability".into(),
                args: vec![DataraType::Class("ProcessExec".into())],
            },
        );
        class_methods.insert("ProcessCapabilityProvider".to_string(), proc_prov_methods);

        let mut file_handle_methods = HashMap::new();
        file_handle_methods.insert("read_all".to_string(), DataraType::String);
        file_handle_methods.insert("read_line".to_string(), DataraType::String);
        file_handle_methods.insert("close".to_string(), DataraType::Unit);
        class_methods.insert("FileHandle".to_string(), file_handle_methods);

        let mut file_write_handle_methods = HashMap::new();
        file_write_handle_methods.insert("write".to_string(), DataraType::Int);
        file_write_handle_methods.insert("write_all".to_string(), DataraType::Int);
        file_write_handle_methods.insert("close".to_string(), DataraType::Unit);
        class_methods.insert("FileWriteHandle".to_string(), file_write_handle_methods);

        function_signatures.insert(
            "fs_open".to_string(),
            (
                vec![DataraType::String],
                DataraType::Class("FileHandle".into()),
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "fs_read".to_string(),
            (
                vec![DataraType::Class("FileHandle".into())],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "fs_write".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "net_connect".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "net_listen".to_string(),
            (vec![DataraType::Int], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "proc_spawn".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );

        function_signatures.insert(
            "println".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "print".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "eprintln".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "panic".to_string(),
            (vec![DataraType::String], DataraType::Never, Vec::new()),
        );
        function_signatures.insert(
            "assert".to_string(),
            (
                vec![DataraType::Bool, DataraType::String],
                DataraType::Unit,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "length".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert("now".to_string(), (vec![], DataraType::Int, Vec::new()));
        function_signatures.insert("now_ms".to_string(), (vec![], DataraType::Int, Vec::new()));
        function_signatures.insert("now_ns".to_string(), (vec![], DataraType::Int, Vec::new()));
        function_signatures.insert(
            "datara_rt_now_ns".to_string(),
            (vec![], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "now_precise_ms".to_string(),
            (vec![], DataraType::Int, Vec::new()),
        );
        for name in &[
            "time_precise_ms",
            "datara_rt_time_precise_ms",
            "time_delta_ms",
            "datara_rt_time_delta_ms",
        ] {
            function_signatures.insert(name.to_string(), (vec![], DataraType::Float, Vec::new()));
        }
        for name in &["path_join", "datara_rt_path_join"] {
            function_signatures.insert(
                name.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for name in &["math_ctz", "ctz"] {
            function_signatures.insert(
                name.to_string(),
                (vec![DataraType::Int], DataraType::Int, Vec::new()),
            );
        }
        for name in &[
            "math_shr", "shr", "math_shl", "shl", "math_xor", "xor", "math_and", "and", "math_or",
            "or",
        ] {
            function_signatures.insert(
                name.to_string(),
                (
                    vec![DataraType::Int, DataraType::Int],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        for name in &["wrapping", "saturating"] {
            function_signatures.insert(
                name.to_string(),
                (vec![DataraType::Int], DataraType::Int, Vec::new()),
            );
        }
        for name in &[
            "wrapping_add",
            "wrapping_sub",
            "wrapping_mul",
            "saturating_add",
            "saturating_sub",
            "saturating_mul",
        ] {
            function_signatures.insert(
                name.to_string(),
                (
                    vec![DataraType::Int, DataraType::Int],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        function_signatures.insert(
            "exit".to_string(),
            (vec![DataraType::Int], DataraType::Never, Vec::new()),
        );
        function_signatures.insert(
            "input".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "sleep".to_string(),
            (vec![DataraType::Int], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "file_read".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "read_file".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "file_write".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "write_file".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "file_append".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "file_exists".to_string(),
            (vec![DataraType::String], DataraType::Bool, Vec::new()),
        );
        function_signatures.insert(
            "env_get".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "args_count".to_string(),
            (vec![], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "args_get".to_string(),
            (vec![DataraType::Int], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "str_contains".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_starts_with".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_ends_with".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_index_of".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "byte_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_chars".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "char_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_byte_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "byte_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_sanitize_utf8".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "validate_utf8".to_string(),
            (vec![DataraType::String], DataraType::Bool, Vec::new()),
        );
        function_signatures.insert(
            "str_scalar_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_next_offset".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_substring".to_string(),
            (
                vec![DataraType::String, DataraType::Int, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_char_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_trim".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "str_to_int".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_to_float".to_string(),
            (vec![DataraType::String], DataraType::Float, Vec::new()),
        );
        for f in &["str_repeat", "repeat", "datara_rt_str_repeat"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::Int],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "str_pad_left",
            "pad_left",
            "datara_rt_str_pad_left",
            "str_pad_right",
            "pad_right",
            "datara_rt_str_pad_right",
        ] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::Int, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["str_replace", "replace", "datara_rt_str_replace"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "str_to_upper",
            "to_upper",
            "datara_rt_str_to_upper",
            "str_to_lower",
            "to_lower",
            "datara_rt_str_to_lower",
        ] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::String, Vec::new()),
            );
        }
        for f in &["str_split", "split", "datara_rt_str_split"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::List(Box::new(DataraType::String)),
                    Vec::new(),
                ),
            );
        }
        for f in &["str_join", "join", "datara_rt_str_join"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![
                        DataraType::List(Box::new(DataraType::String)),
                        DataraType::String,
                    ],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["format_percent", "datara_rt_format_percent"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::Float, DataraType::Int],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["format_int_with_commas", "datara_rt_format_int_with_commas"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::Int], DataraType::String, Vec::new()),
            );
        }
        for f in &["js_eval", "datara_js_eval"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::String, Vec::new()),
            );
        }
        for f in &["js_eval_int", "datara_js_eval_int"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Int, Vec::new()),
            );
        }
        for f in &["js_eval_float", "datara_js_eval_float"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Float, Vec::new()),
            );
        }
        for f in &["js_require", "datara_js_require"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Int, Vec::new()),
            );
        }
        for f in &["js_call", "datara_js_call"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["js_call_0", "datara_js_call_0"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::String, Vec::new()),
            );
        }
        for f in &["js_call_1", "datara_js_call_1"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["js_call_2", "datara_js_call_2"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["js_set_global", "datara_js_set_global"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        for f in &["js_get_global", "datara_js_get_global"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::String, Vec::new()),
            );
        }
        for f in &[
            "py_eval",
            "datara_py_eval",
            "py_eval_safe",
            "datara_py_eval_safe",
        ] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::String, Vec::new()),
            );
        }
        for f in &["py_eval_int", "datara_py_eval_int"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Int, Vec::new()),
            );
        }
        for f in &["py_eval_float", "datara_py_eval_float"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Float, Vec::new()),
            );
        }
        for f in &["py_call", "datara_py_call"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["py_call_1_str", "datara_py_call_1_str"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::String],
                    DataraType::String,
                    Vec::new(),
                ),
            );
        }
        for f in &["py_call_1_float", "datara_py_call_1_float"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::String, DataraType::Float],
                    DataraType::Float,
                    Vec::new(),
                ),
            );
        }
        for f in &["py_import", "datara_py_import"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Int, Vec::new()),
            );
        }
        for f in &["py_exec", "datara_py_exec"] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::String], DataraType::Int, Vec::new()),
            );
        }
        for f in &["py_last_error", "datara_py_last_error"] {
            function_signatures.insert(f.to_string(), (vec![], DataraType::String, Vec::new()));
        }
        for f in &["py_clear_error", "datara_py_clear_error"] {
            function_signatures.insert(f.to_string(), (vec![], DataraType::Unit, Vec::new()));
        }
        for f in &[
            "datara_py_export_list_f64",
            "datara_py_assert_same_ptr",
            "datara_js_export_list_f64",
            "datara_js_assert_same_ptr",
        ] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![
                        DataraType::String,
                        DataraType::List(Box::new(DataraType::Float)),
                    ],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "math_sqrt",
            "datara_rt_math_sqrt",
            "math_abs",
            "datara_rt_math_abs",
            "math_sin",
            "datara_rt_math_sin",
            "math_cos",
            "datara_rt_math_cos",
            "math_tan",
            "datara_rt_math_tan",
            "math_floor",
            "datara_rt_math_floor",
            "math_ceil",
            "datara_rt_math_ceil",
            "math_round",
            "datara_rt_math_round",
            "math_log",
            "datara_rt_math_log",
            "math_exp",
            "datara_rt_math_exp",
        ] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::Float], DataraType::Float, Vec::new()),
            );
        }
        for f in &["math_clamp", "datara_rt_math_clamp", "clamp"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::Float, DataraType::Float, DataraType::Float],
                    DataraType::Float,
                    Vec::new(),
                ),
            );
        }
        for f in &["math_clamp_int", "datara_rt_math_clamp_int", "clamp_int"] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::Int, DataraType::Int, DataraType::Int],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "math_pow",
            "datara_rt_math_pow",
            "math_min",
            "datara_rt_math_min",
            "math_max",
            "datara_rt_math_max",
            "math_hypot",
            "datara_rt_math_hypot",
        ] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::Float, DataraType::Float],
                    DataraType::Float,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "math_min_int",
            "datara_rt_math_min_int",
            "math_max_int",
            "datara_rt_math_max_int",
            "math_shr",
            "datara_rt_math_shr",
            "shr",
            "math_shl",
            "datara_rt_math_shl",
            "shl",
            "math_xor",
            "datara_rt_math_xor",
            "xor",
            "math_and",
            "datara_rt_math_and",
            "and",
            "math_or",
            "datara_rt_math_or",
            "or",
            "math_bitwise_and",
            "math_bitwise_or",
            "math_bitwise_xor",
        ] {
            function_signatures.insert(
                f.to_string(),
                (
                    vec![DataraType::Int, DataraType::Int],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        for f in &[
            "math_abs_int",
            "datara_rt_math_abs_int",
            "math_ctz",
            "datara_rt_math_ctz",
            "ctz",
            "math_clz",
            "clz",
            "math_popcnt",
            "popcnt",
            "math_bitwise_not",
        ] {
            function_signatures.insert(
                f.to_string(),
                (vec![DataraType::Int], DataraType::Int, Vec::new()),
            );
        }
        function_signatures.insert(
            "read_line".to_string(),
            (vec![], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "socket_create".to_string(),
            (vec![DataraType::Int], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "socket_bind".to_string(),
            (
                vec![DataraType::Int, DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_listen".to_string(),
            (
                vec![DataraType::Int, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_accept".to_string(),
            (vec![DataraType::Int], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "socket_connect".to_string(),
            (
                vec![DataraType::Int, DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_send".to_string(),
            (
                vec![DataraType::Int, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_recv".to_string(),
            (
                vec![DataraType::Int, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_close".to_string(),
            (vec![DataraType::Int], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "sha256".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "base64_encode".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "base64_decode".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "str_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_str_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "byte_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_byte_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_chars".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_str_chars".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "char_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_char_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_byte_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "byte_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "datara_rt_str_byte_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_sanitize_utf8".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_str_sanitize_utf8".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "validate_utf8".to_string(),
            (vec![DataraType::String], DataraType::Bool, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_validate_utf8".to_string(),
            (vec![DataraType::String], DataraType::Bool, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_str_scalar_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "datara_rt_str_next_offset".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "int_to_str".to_string(),
            (vec![DataraType::Int], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_int_to_str".to_string(),
            (vec![DataraType::Int], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "float_to_str".to_string(),
            (vec![DataraType::Float], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_float_to_str".to_string(),
            (vec![DataraType::Float], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "uuid_v4".to_string(),
            (Vec::new(), DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_uuid_v4".to_string(),
            (Vec::new(), DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "datara_rt_dialog_info".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "datara_rt_dialog_alert".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "datara_rt_dialog_confirm".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "process_run".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "system".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "process_output".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "exec".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );

        for (name, sym) in &resolver.functions {
            let mut p_types = Vec::new();
            if let Some(params) = &sym.type_node {
                p_types.push(Self::resolve_tn(resolver, params));
            }
            let ret = sym
                .return_type
                .as_ref()
                .map(|t| Self::resolve_tn(resolver, t))
                .unwrap_or(DataraType::Unit);
            function_signatures.insert(name.clone(), (p_types, ret, sym.generic_params.clone()));
        }

        for (name, sym) in &resolver.classes {
            let mut fields = HashMap::new();
            // 1. Base class fields (recursive)
            let mut curr_base = sym.base_type.clone();
            let mut visited_bases = Vec::new();
            while let Some(b_name) = curr_base {
                if visited_bases.contains(&b_name) {
                    break;
                }
                visited_bases.push(b_name.clone());
                if let Some(b_sym) = resolver.classes.get(&b_name) {
                    for (f_name, f_sym) in &b_sym.fields {
                        let f_type = f_sym
                            .type_node
                            .as_ref()
                            .map(|t| Self::resolve_tn(resolver, t))
                            .unwrap_or(DataraType::String);
                        fields.insert(f_name.clone(), f_type);
                    }
                    for comp_name in &b_sym.compositions {
                        if let Some(comp_sym) = resolver.components.get(comp_name) {
                            for (f_name, f_sym) in &comp_sym.fields {
                                let f_type = f_sym
                                    .type_node
                                    .as_ref()
                                    .map(|t| Self::resolve_tn(resolver, t))
                                    .unwrap_or(DataraType::String);
                                fields.insert(f_name.clone(), f_type);
                            }
                        }
                    }
                    curr_base = b_sym.base_type.clone();
                } else {
                    break;
                }
            }

            // 2. Composed components
            for comp_name in &sym.compositions {
                if let Some(comp_sym) = resolver.components.get(comp_name) {
                    for (f_name, f_sym) in &comp_sym.fields {
                        let f_type = f_sym
                            .type_node
                            .as_ref()
                            .map(|t| Self::resolve_tn(resolver, t))
                            .unwrap_or(DataraType::String);
                        fields.insert(f_name.clone(), f_type);
                    }
                }
            }

            // 3. Own fields
            for (f_name, f_sym) in &sym.fields {
                let f_type = f_sym
                    .type_node
                    .as_ref()
                    .map(|t| Self::resolve_tn(resolver, t))
                    .unwrap_or(DataraType::String);
                fields.insert(f_name.clone(), f_type);
            }
            class_fields.insert(name.clone(), fields.clone());

            let mut methods = HashMap::new();
            for (m_name, m_sym) in &sym.methods {
                let m_type = m_sym
                    .return_type
                    .as_ref()
                    .map(|t| Self::resolve_tn(resolver, t))
                    .unwrap_or(DataraType::Unit);
                methods.insert(m_name.clone(), m_type);
            }
            class_methods.insert(name.clone(), methods);

            if !sym.generic_params.is_empty() {
                generic_templates.insert(name.clone(), (sym.generic_params.clone(), fields));
            }
        }

        Self {
            resolver,
            symbol_types: HashMap::new(),
            symbol_mutability: HashMap::new(),
            function_signatures,
            class_fields,
            class_methods,
            generic_templates,
            generic_specializations: HashMap::new(),
            current_return_type: None,
            propagation_sites: Vec::new(),
            var_element_types: HashMap::new(),
            last_list_element: None,
            current_fn_name: None,
            fn_symbol_types,
            var_refinements: HashMap::new(),
            function_param_nodes: HashMap::new(),
            var_array_lengths: HashMap::new(),
            traits: HashMap::new(),
            impls: HashMap::new(),
            trait_bounds: HashMap::new(),
            current_target_type: None,
        }
    }
}
