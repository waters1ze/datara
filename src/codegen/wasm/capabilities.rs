use crate::dmir::{Inst, Module};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityImport {
    pub module: String,
    pub function: String,
    pub params: Vec<String>,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySidecar {
    pub module_name: String,
    pub zero_trust_enforcement: String,
    pub audit_guarantee: String,
    pub granted_capabilities: Vec<GrantedCapabilityGroup>,
    pub absent_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantedCapabilityGroup {
    pub module: String,
    pub version: String,
    pub functions: Vec<String>,
    pub capability_token: String,
}

/// Classifies called operations into capability modules.
pub fn classify_capability_call(func: &str) -> Option<(&'static str, &'static str)> {
    match func {
        // Filesystem capability
        "fs_read" | "file_read" | "read_file" | "datara_rt_file_read" => {
            Some(("datara:fs@1.0", "read"))
        }
        "fs_write"
        | "file_write"
        | "write_file"
        | "file_append"
        | "datara_rt_file_write"
        | "datara_rt_file_append" => Some(("datara:fs@1.0", "write")),

        // Network capability
        "socket_connect" | "net_connect" | "datara_rt_socket_connect" => {
            Some(("datara:net@1.0", "connect"))
        }
        "http_get" | "fetch" | "datara_rt_http_get" => Some(("datara:net@1.0", "http_get")),
        "socket_listen" | "socket_bind" | "net_listen" => Some(("datara:net@1.0", "listen")),

        // System capability
        "proc_spawn" | "process_run" | "system" | "exec" | "process_output" => {
            Some(("datara:sys@1.0", "exec"))
        }
        "env_get" | "datara_rt_env_get" => Some(("datara:sys@1.0", "env_get")),
        "clock_now" | "now" | "datara_rt_clock_now" => Some(("datara:sys@1.0", "clock_now")),

        // Standard runtime built-ins
        "list_create"
        | "datara_rt_list_create"
        | "list_append"
        | "list_push"
        | "datara_rt_list_append"
        | "list_get"
        | "datara_rt_list_get"
        | "list_get_unchecked"
        | "datara_rt_list_get_unchecked"
        | "list_set"
        | "datara_rt_list_set"
        | "list_set_unchecked"
        | "datara_rt_list_set_unchecked"
        | "list_len"
        | "datara_rt_list_len"
        | "str_len"
        | "datara_rt_str_len"
        | "byte_len"
        | "datara_rt_byte_len"
        | "str_chars"
        | "datara_rt_str_chars"
        | "char_len"
        | "datara_rt_char_len"
        | "validate_utf8"
        | "datara_rt_validate_utf8"
        | "str_scalar_at"
        | "datara_rt_str_scalar_at"
        | "str_next_offset"
        | "datara_rt_str_next_offset"
        | "str_char_at"
        | "datara_rt_str_char_at"
        | "byte_at"
        | "str_byte_at"
        | "datara_rt_str_byte_at"
        | "str_split"
        | "datara_rt_str_split"
        | "str_join"
        | "datara_rt_str_join"
        | "map_create"
        | "datara_rt_map_create"
        | "map_insert"
        | "map_set"
        | "datara_rt_map_insert"
        | "map_get"
        | "datara_rt_map_get"
        | "map_len"
        | "datara_rt_map_len"
        | "print"
        | "out"
        | "println"
        | "datara_rt_print"
        | "err"
        | "datara_rt_err"
        | "alloc"
        | "datara_rt_alloc" => {
            let normalized_op = match func {
                "out" | "println" | "print" | "datara_rt_print" => "print",
                "list_push" | "list_append" | "datara_rt_list_append" => "list_append",
                "list_create" | "datara_rt_list_create" => "list_create",
                "list_get"
                | "datara_rt_list_get"
                | "list_get_unchecked"
                | "datara_rt_list_get_unchecked" => "list_get",
                "list_set"
                | "datara_rt_list_set"
                | "list_set_unchecked"
                | "datara_rt_list_set_unchecked" => "list_set",
                "list_len" | "datara_rt_list_len" => "list_len",
                "str_len" | "datara_rt_str_len" | "byte_len" | "datara_rt_byte_len" => "str_len",
                "str_chars" | "datara_rt_str_chars" | "char_len" | "datara_rt_char_len" => {
                    "str_chars"
                }
                "validate_utf8" | "datara_rt_validate_utf8" => "validate_utf8",
                "str_scalar_at" | "datara_rt_str_scalar_at" => "str_scalar_at",
                "str_next_offset" | "datara_rt_str_next_offset" => "str_next_offset",
                "str_char_at" | "datara_rt_str_char_at" => "str_char_at",
                "byte_at" | "str_byte_at" | "datara_rt_str_byte_at" => "byte_at",
                "str_split" | "datara_rt_str_split" => "str_split",
                "str_join" | "datara_rt_str_join" => "str_join",
                "map_create" | "datara_rt_map_create" => "map_create",
                "map_insert" | "map_set" | "datara_rt_map_insert" => "map_insert",
                "map_get" | "datara_rt_map_get" => "map_get",
                "map_len" | "datara_rt_map_len" => "map_len",
                "err" | "datara_rt_err" => "err",
                "alloc" | "datara_rt_alloc" => "alloc",
                _ => return None,
            };
            Some(("datara:rt", normalized_op))
        }

        "datara_rt_list_create_1" => Some(("datara:rt", "list_create_1")),
        "datara_rt_list_create_2" => Some(("datara:rt", "list_create_2")),
        "datara_rt_list_create_3" => Some(("datara:rt", "list_create_3")),
        "datara_rt_list_create_4" => Some(("datara:rt", "list_create_4")),
        "datara_rt_list_create_5" => Some(("datara:rt", "list_create_5")),
        "list_create_repeat" | "datara_rt_list_create_repeat" => {
            Some(("datara:rt", "list_create_repeat"))
        }
        _ if func.starts_with("datara_rt_list_create_") => Some(("datara:rt", "list_create")),
        _ if func.starts_with("datara_rt_map_create_") => Some(("datara:rt", "map_create")),

        // Ownership runtime guards
        "own_acquire" | "datara_rt_own_acquire" => Some(("datara:rt", "own_acquire")),
        "own_release" | "datara_rt_own_release" => Some(("datara:rt", "own_release")),

        _ => None,
    }
}

/// Collects transitive function calls across a DMIR module in deterministic order.
pub fn collect_transitive_calls(module: &Module) -> BTreeSet<String> {
    let mut called = BTreeSet::new();

    let mut sorted_func_names: Vec<&String> = module.functions.keys().collect();
    sorted_func_names.sort();

    for fname in &sorted_func_names {
        let func = &module.functions[*fname];
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::Call { func: callee, .. } => {
                        called.insert(callee.clone());
                    }
                    Inst::MethodCall { method, .. } => {
                        let runtime_fn = match method.as_str() {
                            "push" | "append" => "datara_rt_list_append",
                            "get" => "datara_rt_list_get",
                            "set" => "datara_rt_list_set",
                            "len" | "count" => "datara_rt_list_len",
                            "insert" => "datara_rt_map_insert",
                            _ => method.as_str(),
                        };
                        called.insert(runtime_fn.to_string());
                    }
                    Inst::StructInit { .. } => {
                        called.insert("datara_rt_alloc".to_string());
                    }
                    Inst::Out { .. } => {
                        called.insert("datara_rt_print".to_string());
                    }
                    Inst::Err { .. } => {
                        called.insert("datara_rt_err".to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    // Fixed-point transitive closure over module functions
    let mut changed = true;
    while changed {
        changed = false;
        let current: Vec<String> = called.iter().cloned().collect();
        for fn_name in current {
            if let Some(target_fn) = module.functions.get(&fn_name) {
                for block in &target_fn.blocks {
                    for inst in &block.instructions {
                        if let Inst::Call { func: callee, .. } = inst {
                            if called.insert(callee.clone()) {
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }

    called
}

// ---------------------------------------------------------------------------
// Wasm Type & Function Representations
// ---------------------------------------------------------------------------
