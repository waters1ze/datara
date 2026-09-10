//! Capability-Native WebAssembly Backend for Datara.
//!
//! # Architecture & Capabilities
//!
//! 1. **DMIR Lowering to Wasm 1.0 + SIMD (v128)**:
//!    - Functions map to Wasm functions with standard calling conventions.
//!    - Types: Int/Bool -> i64, Float -> f64, Float4/Int4 -> v128, Str/List/Map -> i64
//!      (pointers into a linear-memory bump allocator provided by the runtime shim).
//!    - Hardware SIMD: `float4`, `int4`, `min4`, `max4`, and `dot` (with horizontal add
//!      via `i8x16.shuffle` and `f32x4.add`).
//!
//! 2. **Direct SSA Block-Param Lowering (No Phi Elimination Pass Needed)**:
//!    - In SSA IR with block parameters (like Datara DMIR), branches pass arguments
//!      directly on jump edges. In WebAssembly, operand-stack parameter passing
//!      allows arguments to be pushed onto the stack before branching, and consumed
//!      into target parameters upon entry.
//!    - **Key Backend Advantage**: Unlike LLVM phi nodes, which require a complex
//!      phi-elimination pass (splitting critical edges, scheduling parallel copies,
//!      and breaking swap cycles with temporary registers), Wasm param-stack lowering
//!      lowers directly from SSA block arguments without requiring any phi elimination pass.
//!      The operand stack naturally provides cycle-free parallel transfers.
//!
//! 3. **Compositional Zero-Trust Capability Imports**:
//!    - For each function, transitive effect sets are analyzed.
//!    - ONLY runtime functions of granted/used capabilities are emitted as imports,
//!      grouped per module: `"datara:fs@1.0"` {read, write}, `"datara:net@1.0"` {connect, http_get},
//!      `"datara:sys@1.0"` {exec, env}, and `"datara:rt"` {list_*, map_*, print, alloc}.
//!    - A program whose effect analysis contains no Network operations physically omits
//!      all `"datara:net@1.0"` imports. The Zero-Trust model is enforced by the host sandbox itself.
//!    - Generates a machine-auditable `<name>.capabilities.json` sidecar listing granted imports.

use crate::dmir::{BasicBlockId, Inst, Module, Terminator, ValueId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// LEB128 & Primitive Encoders
// ---------------------------------------------------------------------------

/// Encodes an unsigned 32-bit integer as unsigned LEB128.
pub fn encode_u32_leb128(mut val: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

/// Encodes a signed 32-bit integer as signed LEB128.
pub fn encode_i32_leb128(mut val: i32, buf: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (val == 0 && !sign_bit) || (val == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        buf.push(byte);
    }
}

/// Encodes a signed 64-bit integer as signed LEB128.
pub fn encode_i64_leb128(mut val: i64, buf: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (val == 0 && !sign_bit) || (val == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        buf.push(byte);
    }
}

/// Encodes a 64-bit float in IEEE-754 little-endian format.
pub fn encode_f64(val: f64, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&val.to_le_bytes());
}

/// Encodes a 32-bit float in IEEE-754 little-endian format.
pub fn encode_f32(val: f32, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&val.to_le_bytes());
}

/// Encodes a UTF-8 string with a leading unsigned LEB128 length prefix.
pub fn encode_str(s: &str, buf: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    encode_u32_leb128(bytes.len() as u32, buf);
    buf.extend_from_slice(bytes);
}

/// Emits a WebAssembly binary section with ID and length prefix.
fn emit_section(id: u8, content: &[u8], buf: &mut Vec<u8>) {
    buf.push(id);
    encode_u32_leb128(content.len() as u32, buf);
    buf.extend_from_slice(content);
}

// ---------------------------------------------------------------------------
// Capability Specification & Transitive Effect Analysis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
fn classify_capability_call(func: &str) -> Option<(&'static str, &'static str)> {
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
        | "list_set"
        | "datara_rt_list_set"
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
                "list_get" | "datara_rt_list_get" => "list_get",
                "list_set" | "datara_rt_list_set" => "list_set",
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
fn collect_transitive_calls(module: &Module) -> BTreeSet<String> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmValType {
    I32 = 0x7F,
    I64 = 0x7E,
    F32 = 0x7D,
    F64 = 0x7C,
    V128 = 0x7B,
}

impl WasmValType {
    pub fn wat_name(&self) -> &'static str {
        match self {
            WasmValType::I32 => "i32",
            WasmValType::I64 => "i64",
            WasmValType::F32 => "f32",
            WasmValType::F64 => "f64",
            WasmValType::V128 => "v128",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WasmFuncType {
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

impl WasmFuncType {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        buf.push(0x60); // func form
        encode_u32_leb128(self.params.len() as u32, buf);
        for p in &self.params {
            buf.push(*p as u8);
        }
        encode_u32_leb128(self.results.len() as u32, buf);
        for r in &self.results {
            buf.push(*r as u8);
        }
    }
}

fn map_dmir_type_to_wasm(ty: &str) -> Option<WasmValType> {
    match ty {
        "Int" | "Bool" | "Int64" | "Char" | "Unit" => Some(WasmValType::I64),
        "Float" | "Float64" => Some(WasmValType::F64),
        "Float32" => Some(WasmValType::F32),
        "Int32" => Some(WasmValType::I32),
        "Float4" | "Int4" | "Vector4" => Some(WasmValType::V128),
        "String" | "Str" | "List" | "Map" | "Dynamic" => Some(WasmValType::I64), // linear-memory pointer
        _ if ty.starts_with("List<") || ty.starts_with("Map<") => Some(WasmValType::I64),
        _ => Some(WasmValType::I64),
    }
}

// ---------------------------------------------------------------------------
// Wasm Emitter Implementation
// ---------------------------------------------------------------------------

pub struct WasmEmitter;

impl WasmEmitter {
    /// Compiles a DMIR module to a WebAssembly binary (`.wasm`), companion JS runtime shim,
    /// human-readable `.wat`, and machine-auditable `.capabilities.json` sidecar.
    pub fn emit_wasm_binary(module: &Module, output_wasm_path: &Path) -> Result<PathBuf, String> {
        let (wasm_bytes, wat_text, sidecar, js_shim) = Self::compile_module(module)?;

        // Ensure parent output directory exists
        if let Some(parent) = output_wasm_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // 1. Write WebAssembly binary (.wasm)
        fs::write(output_wasm_path, &wasm_bytes)
            .map_err(|e| format!("Failed to write WASM binary: {}", e))?;

        // 2. Write Text Representation (.wat)
        let wat_path = output_wasm_path.with_extension("wat");
        let _ = fs::write(&wat_path, wat_text);

        // 3. Write Capability Sidecar (.capabilities.json)
        let sidecar_path = output_wasm_path.with_extension("capabilities.json");
        let sidecar_json = serde_json::to_string_pretty(&sidecar)
            .map_err(|e| format!("Failed to serialize capabilities JSON: {}", e))?;
        let _ = fs::write(&sidecar_path, sidecar_json);

        // 4. Write Companion JavaScript Runtime Loader (.js)
        let js_path = output_wasm_path.with_extension("js");
        let _ = fs::write(&js_path, js_shim);

        Ok(output_wasm_path.to_path_buf())
    }

    /// Internal compiler pipeline that produces all 4 artifacts in memory.
    pub fn compile_module(
        module: &Module,
    ) -> Result<(Vec<u8>, String, CapabilitySidecar, String), String> {
        let mut type_pool: Vec<WasmFuncType> = Vec::new();
        let mut get_or_insert_type = |ft: WasmFuncType| -> u32 {
            if let Some(pos) = type_pool.iter().position(|t| t == &ft) {
                pos as u32
            } else {
                type_pool.push(ft);
                (type_pool.len() - 1) as u32
            }
        };

        // 1. Transitive Call Analysis & Capability Module Partitioning
        let transitive_calls = collect_transitive_calls(module);

        // Scan module for granted capability tokens
        let mut granted_capability_tokens: HashSet<String> = HashSet::new();
        for func in module.functions.values() {
            for (_, ty_str, _) in &func.params {
                if ty_str.contains("FileRead") || ty_str.contains("FileCapabilityProvider") {
                    granted_capability_tokens.insert("Capability<FileRead>".to_string());
                }
                if ty_str.contains("FileWrite") {
                    granted_capability_tokens.insert("Capability<FileWrite>".to_string());
                }
                if ty_str.contains("NetworkConnect") || ty_str.contains("NetCapabilityProvider") {
                    granted_capability_tokens.insert("Capability<NetworkConnect>".to_string());
                }
                if ty_str.contains("NetworkListen") {
                    granted_capability_tokens.insert("Capability<NetworkListen>".to_string());
                }
                if ty_str.contains("ProcessExec") || ty_str.contains("ProcessCapabilityProvider") {
                    granted_capability_tokens.insert("Capability<ProcessExec>".to_string());
                }
                if ty_str.contains("SystemClock") {
                    granted_capability_tokens.insert("Capability<SystemClock>".to_string());
                }
                if ty_str.contains("SystemEnv") {
                    granted_capability_tokens.insert("Capability<SystemEnv>".to_string());
                }
                if ty_str.contains("SystemCapabilities") {
                    granted_capability_tokens.insert("Capability<FileRead>".to_string());
                    granted_capability_tokens.insert("Capability<FileWrite>".to_string());
                    granted_capability_tokens.insert("Capability<NetworkConnect>".to_string());
                    granted_capability_tokens.insert("Capability<NetworkListen>".to_string());
                    granted_capability_tokens.insert("Capability<ProcessExec>".to_string());
                    granted_capability_tokens.insert("Capability<SystemClock>".to_string());
                    granted_capability_tokens.insert("Capability<SystemEnv>".to_string());
                }
            }
            for req in &func.requires {
                let s = format!("{:?}", req);
                if s.contains("FileRead") {
                    granted_capability_tokens.insert("Capability<FileRead>".to_string());
                }
                if s.contains("FileWrite") {
                    granted_capability_tokens.insert("Capability<FileWrite>".to_string());
                }
                if s.contains("NetworkConnect") {
                    granted_capability_tokens.insert("Capability<NetworkConnect>".to_string());
                }
                if s.contains("NetworkListen") {
                    granted_capability_tokens.insert("Capability<NetworkListen>".to_string());
                }
                if s.contains("ProcessExec") {
                    granted_capability_tokens.insert("Capability<ProcessExec>".to_string());
                }
                if s.contains("System") {
                    granted_capability_tokens.insert("Capability<SystemClock>".to_string());
                    granted_capability_tokens.insert("Capability<SystemEnv>".to_string());
                }
            }
        }

        let mut granted_by_module: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
        let mut import_entries: Vec<(&'static str, &'static str, WasmFuncType, u32)> = Vec::new();

        for callee in &transitive_calls {
            if let Some((mod_name, func_name)) = classify_capability_call(callee) {
                if mod_name != "datara:rt" {
                    let req_cap = match (mod_name, func_name) {
                        ("datara:fs@1.0", "read") => "Capability<FileRead>",
                        ("datara:fs@1.0", "write") => "Capability<FileWrite>",
                        ("datara:net@1.0", "connect") | ("datara:net@1.0", "http_get") => {
                            "Capability<NetworkConnect>"
                        }
                        ("datara:net@1.0", "listen") => "Capability<NetworkListen>",
                        ("datara:sys@1.0", "exec") => "Capability<ProcessExec>",
                        ("datara:sys@1.0", "clock_now") => "Capability<SystemClock>",
                        ("datara:sys@1.0", "env_get") => "Capability<SystemEnv>",
                        _ => "Capability<System>",
                    };

                    if !granted_capability_tokens.contains(req_cap) {
                        return Err(format!(
                            "E0940: Security violation: Operation '{}' requires '{}', but this capability is not granted to the module. Compositional import generation aborted.",
                            callee, req_cap
                        ));
                    }
                }

                granted_by_module
                    .entry(mod_name)
                    .or_default()
                    .insert(func_name);
            }
        }

        // Build import entries in deterministic order
        let mut import_fn_indices: HashMap<String, u32> = HashMap::new();
        for (&mod_name, funcs) in &granted_by_module {
            for &func_name in funcs {
                let sig = match (mod_name, func_name) {
                    ("datara:fs@1.0", "read") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:fs@1.0", "write") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:net@1.0", "connect") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:net@1.0", "http_get") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:net@1.0", "listen") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:sys@1.0", "exec") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:sys@1.0", "env_get") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:sys@1.0", "clock_now") => WasmFuncType {
                        params: vec![],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "print") | ("datara:rt", "err") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![],
                    },
                    ("datara:rt", "alloc") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_1") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_2") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_3") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_4") => WasmFuncType {
                        params: vec![WasmValType::I64; 4],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_5") => WasmFuncType {
                        params: vec![WasmValType::I64; 5],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_create_repeat") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_append") | ("datara:rt", "list_push") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_get") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_set") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "list_len") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "map_create") => WasmFuncType {
                        params: vec![],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "map_insert") | ("datara:rt", "map_set") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "map_get") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "map_len") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "byte_at") => WasmFuncType {
                        params: vec![WasmValType::I64, WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    // Ownership runtime guards: acquire retains reference and returns val,
                    // release decreases reference and returns void.
                    ("datara:rt", "own_acquire") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                    ("datara:rt", "own_release") => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![],
                    },
                    _ => WasmFuncType {
                        params: vec![WasmValType::I64],
                        results: vec![WasmValType::I64],
                    },
                };

                let type_idx = get_or_insert_type(sig.clone());
                let idx = import_entries.len() as u32;
                import_entries.push((mod_name, func_name, sig, type_idx));
                import_fn_indices.insert(format!("{}/{}", mod_name, func_name), idx);
                // Also map raw DMIR function names to this import index
                for callee in &transitive_calls {
                    if classify_capability_call(callee) == Some((mod_name, func_name)) {
                        import_fn_indices.insert(callee.clone(), idx);
                    }
                }
            }
        }

        // Build capability sidecar report
        let all_known_modules = [
            (
                "datara:fs@1.0",
                "Capability<FileRead> / Capability<FileWrite>",
            ),
            (
                "datara:net@1.0",
                "Capability<NetworkConnect> / Capability<NetworkListen>",
            ),
            (
                "datara:sys@1.0",
                "Capability<ProcessExec> / Capability<SystemClock> / Capability<SystemEnv>",
            ),
            ("datara:rt/own", "Capability<OwnershipGuard>"),
        ];
        let mut granted_groups = Vec::new();
        let mut absent_capabilities = Vec::new();

        for &(mod_name, token_name) in &all_known_modules {
            if mod_name == "datara:rt/own" {
                let mut own_funcs = Vec::new();
                if let Some(rt_funcs) = granted_by_module.get("datara:rt") {
                    for f in ["own_acquire", "own_release"] {
                        if rt_funcs.contains(f) {
                            own_funcs.push(f.to_string());
                        }
                    }
                }
                if !own_funcs.is_empty() {
                    granted_groups.push(GrantedCapabilityGroup {
                        module: mod_name.to_string(),
                        version: "1.0".to_string(),
                        functions: own_funcs,
                        capability_token: token_name.to_string(),
                    });
                } else {
                    absent_capabilities.push(mod_name.to_string());
                }
            } else if let Some(funcs) = granted_by_module.get(mod_name) {
                granted_groups.push(GrantedCapabilityGroup {
                    module: mod_name.to_string(),
                    version: "1.0".to_string(),
                    functions: funcs.iter().map(|s| s.to_string()).collect(),
                    capability_token: token_name.to_string(),
                });
            } else {
                absent_capabilities.push(mod_name.to_string());
            }
        }

        let sidecar = CapabilitySidecar {
            module_name: module.name.clone(),
            zero_trust_enforcement: "compositional_wasm_import_sandbox".to_string(),
            audit_guarantee: "Provably absent capabilities are physically excluded from WebAssembly import section by construction".to_string(),
            granted_capabilities: granted_groups,
            absent_capabilities,
        };

        // 2. Prepare Defined Functions
        let num_imports = import_entries.len() as u32;
        let mut fn_names: Vec<String> = module.functions.keys().cloned().collect();
        fn_names.sort(); // deterministic ordering

        let mut defined_fn_indices: HashMap<String, u32> = HashMap::new();
        for (i, name) in fn_names.iter().enumerate() {
            defined_fn_indices.insert(name.clone(), num_imports + (i as u32));
        }

        // String literals & data segment table
        let mut string_table: HashMap<String, u32> = HashMap::new();
        let mut data_bytes: Vec<u8> = Vec::new();
        let base_memory_offset: u32 = 1024; // offset 0..1023 reserved

        let mut register_str = |val: &str| {
            if !string_table.contains_key(val) {
                let offset = base_memory_offset + (data_bytes.len() as u32);
                let s_bytes = val.as_bytes();
                data_bytes.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
                data_bytes.extend_from_slice(s_bytes);
                data_bytes.push(0); // null terminator
                string_table.insert(val.to_string(), offset);
            }
        };

        for fn_name in &fn_names {
            let func = &module.functions[fn_name];
            for block in &func.blocks {
                for inst in &block.instructions {
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

        // 3. Compile Function Bodies to Wasm Bytecode & WAT Text
        let mut defined_types: Vec<u32> = Vec::new();
        let mut code_bodies: Vec<Vec<u8>> = Vec::new();
        let mut wat_functions: Vec<String> = Vec::new();

        for fn_name in &fn_names {
            let func = &module.functions[fn_name];
            let (type_idx, body, wat_fn) = Self::compile_function(
                func,
                module,
                &import_fn_indices,
                &defined_fn_indices,
                &string_table,
                &mut get_or_insert_type,
            )?;
            defined_types.push(type_idx);
            code_bodies.push(body);
            wat_functions.push(wat_fn);
        }

        if fn_names.is_empty() {
            // Default main function if module has no functions
            let main_sig = WasmFuncType {
                params: vec![],
                results: vec![WasmValType::I64],
            };
            let type_idx = get_or_insert_type(main_sig);
            defined_types.push(type_idx);

            let mut body = Vec::new();
            encode_u32_leb128(0, &mut body); // 0 locals
            body.push(0x42); // i64.const
            encode_i64_leb128(0, &mut body);
            body.push(0x0B); // end
            code_bodies.push(body);
            wat_functions.push(
                "  (func $main (export \"main\") (result i64)\n    (i64.const 0)\n  )".to_string(),
            );
        }

        // 4. Construct Binary WebAssembly (.wasm)
        let mut wasm = Vec::new();
        // Magic header & Version 1
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D]);
        wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

        // Section 1: Type Section
        {
            let mut sec = Vec::new();
            encode_u32_leb128(type_pool.len() as u32, &mut sec);
            for t in &type_pool {
                t.encode(&mut sec);
            }
            emit_section(1, &sec, &mut wasm);
        }

        // Section 2: Import Section
        if !import_entries.is_empty() {
            let mut sec = Vec::new();
            encode_u32_leb128(import_entries.len() as u32, &mut sec);
            for (mod_name, fn_name, _sig, type_idx) in &import_entries {
                encode_str(mod_name, &mut sec);
                encode_str(fn_name, &mut sec);
                sec.push(0x00); // function import
                encode_u32_leb128(*type_idx, &mut sec);
            }
            emit_section(2, &sec, &mut wasm);
        }

        // Section 3: Function Section
        {
            let mut sec = Vec::new();
            encode_u32_leb128(defined_types.len() as u32, &mut sec);
            for &t in &defined_types {
                encode_u32_leb128(t, &mut sec);
            }
            emit_section(3, &sec, &mut wasm);
        }

        // Section 5: Memory Section (1 page min = 64KB)
        {
            let mut sec = Vec::new();
            encode_u32_leb128(1, &mut sec); // 1 memory entry
            sec.push(0x00); // flags: min only
            encode_u32_leb128(1, &mut sec); // 1 page (64KB)
            emit_section(5, &sec, &mut wasm);
        }

        // Section 7: Export Section
        {
            let mut sec = Vec::new();
            let total_exports = if fn_names.is_empty() {
                2
            } else {
                fn_names.len() + 1
            };
            encode_u32_leb128(total_exports as u32, &mut sec);

            // Export memory
            encode_str("memory", &mut sec);
            sec.push(0x02); // memory export
            encode_u32_leb128(0, &mut sec);

            // Export defined functions
            for (i, name) in fn_names.iter().enumerate() {
                encode_str(name, &mut sec);
                sec.push(0x00); // function export
                encode_u32_leb128(num_imports + (i as u32), &mut sec);
            }
            if fn_names.is_empty() {
                encode_str("main", &mut sec);
                sec.push(0x00);
                encode_u32_leb128(num_imports, &mut sec);
            }
            emit_section(7, &sec, &mut wasm);
        }

        // Section 10: Code Section
        {
            let mut sec = Vec::new();
            encode_u32_leb128(code_bodies.len() as u32, &mut sec);
            for b in &code_bodies {
                encode_u32_leb128(b.len() as u32, &mut sec);
                sec.extend_from_slice(b);
            }
            emit_section(10, &sec, &mut wasm);
        }

        // Section 11: Data Section (String literals in linear memory)
        if !data_bytes.is_empty() {
            let mut sec = Vec::new();
            encode_u32_leb128(1, &mut sec); // 1 data segment
            sec.push(0x00); // active segment index 0
            // i32.const <base_memory_offset>
            sec.push(0x41);
            encode_i32_leb128(base_memory_offset as i32, &mut sec);
            sec.push(0x0B); // end instruction
            encode_u32_leb128(data_bytes.len() as u32, &mut sec);
            sec.extend_from_slice(&data_bytes);
            emit_section(11, &sec, &mut wasm);
        }

        // 5. Build Human-Readable .wat Text
        let mut wat = String::new();
        wat.push_str("(module\n");
        wat.push_str("  ;; Datara Capability-Native WebAssembly Backend (Wasm 1.0 + SIMD)\n");
        wat.push_str("  ;; Direct SSA param-stack lowering: no phi elimination required\n");
        wat.push_str("  (memory (export \"memory\") 1)\n\n");

        // Imports
        for (mod_name, fn_name, sig, _) in &import_entries {
            let params_str = sig
                .params
                .iter()
                .map(|p| format!(" {}", p.wat_name()))
                .collect::<Vec<_>>()
                .concat();
            let results_str = sig
                .results
                .iter()
                .map(|r| format!(" (result {})", r.wat_name()))
                .collect::<Vec<_>>()
                .concat();
            wat.push_str(&format!(
                "  (import \"{}\" \"{}\" (func ${}_{} (param{}){}))\n",
                mod_name,
                fn_name,
                mod_name
                    .replace(':', "_")
                    .replace('@', "_")
                    .replace('.', "_"),
                fn_name,
                params_str,
                results_str
            ));
        }
        if !import_entries.is_empty() {
            wat.push('\n');
        }

        // Functions
        for wat_fn in wat_functions {
            wat.push_str(&wat_fn);
            wat.push('\n');
        }
        wat.push_str(")\n");

        // 6. Build Companion JS Runtime Loader
        let js_shim = Self::generate_js_runtime_shim(&module.name, &import_entries);

        Ok((wasm, wat, sidecar, js_shim))
    }

    /// Compiles a single DMIR function into Wasm bytecode and a WAT representation.
    fn compile_function(
        func: &crate::dmir::Function,
        module: &Module,
        import_fn_indices: &HashMap<String, u32>,
        defined_fn_indices: &HashMap<String, u32>,
        string_table: &HashMap<String, u32>,
        get_or_insert_type: &mut impl FnMut(WasmFuncType) -> u32,
    ) -> Result<(u32, Vec<u8>, String), String> {
        // Collect parameter types
        let mut param_types = Vec::new();
        for (_, ty_str, _) in &func.params {
            param_types.push(map_dmir_type_to_wasm(ty_str).unwrap_or(WasmValType::I64));
        }

        // Return type
        let return_type = match func.return_type.as_str() {
            "Unit" | "()" | "" => None,
            other => map_dmir_type_to_wasm(other),
        };
        let results = return_type.into_iter().collect::<Vec<_>>();

        let func_sig = WasmFuncType {
            params: param_types.clone(),
            results: results.clone(),
        };
        let type_idx = get_or_insert_type(func_sig);

        // Assign local indices:
        // Parameters: indices 0..func.params.len() - 1
        let mut local_map: HashMap<ValueId, u32> = HashMap::new();
        let mut var_map: HashMap<String, u32> = HashMap::new();
        let mut value_types: HashMap<ValueId, WasmValType> = HashMap::new();

        for (i, (_, ty_str, val_id)) in func.params.iter().enumerate() {
            local_map.insert(*val_id, i as u32);
            let val_ty = map_dmir_type_to_wasm(ty_str).unwrap_or(WasmValType::I64);
            value_types.insert(*val_id, val_ty);
        }
        let num_params = func.params.len() as u32;

        // Discover non-parameter values and variables needed
        // Discover non-parameter values and variables needed
        let mut non_param_locals: Vec<(ValueId, WasmValType)> = Vec::new();
        let mut var_names: Vec<String> = Vec::new();
        let mut var_types: HashMap<String, WasmValType> = HashMap::new();

        for block in &func.blocks {
            for p in &block.params {
                if !local_map.contains_key(&p.val)
                    && !non_param_locals.iter().any(|(v, _)| *v == p.val)
                {
                    let val_ty = map_dmir_type_to_wasm(&p.ty).unwrap_or(WasmValType::I64);
                    non_param_locals.push((p.val, val_ty));
                    value_types.insert(p.val, val_ty);
                }
            }
            for inst in &block.instructions {
                match inst {
                    Inst::ConstInt { dest, .. }
                    | Inst::ConstStr { dest, .. }
                    | Inst::ConstBool { dest, .. }
                    | Inst::StructInit { dest, .. }
                    | Inst::GetField { dest, .. }
                    | Inst::FormatStr { dest, .. } => {
                        if !local_map.contains_key(dest)
                            && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                        {
                            non_param_locals.push((*dest, WasmValType::I64));
                            value_types.insert(*dest, WasmValType::I64);
                        }
                    }
                    Inst::InlineAsm { outputs, .. } => {
                        for (_, dest) in outputs {
                            if !local_map.contains_key(dest)
                                && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                            {
                                non_param_locals.push((*dest, WasmValType::I64));
                                value_types.insert(*dest, WasmValType::I64);
                            }
                        }
                    }
                    Inst::ConstFloat { dest, .. } => {
                        if !local_map.contains_key(dest)
                            && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                        {
                            non_param_locals.push((*dest, WasmValType::F64));
                            value_types.insert(*dest, WasmValType::F64);
                        }
                    }
                    Inst::BinOp { dest, ty, op, .. } => {
                        if !local_map.contains_key(dest)
                            && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                        {
                            let is_relational =
                                matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
                            let val_ty = if is_relational {
                                WasmValType::I64
                            } else {
                                map_dmir_type_to_wasm(ty).unwrap_or(WasmValType::I64)
                            };
                            non_param_locals.push((*dest, val_ty));
                            value_types.insert(*dest, val_ty);
                        }
                    }
                    Inst::UnOp { dest, ty, .. }
                    | Inst::Call { dest, ty, .. }
                    | Inst::MethodCall { dest, ty, .. }
                    | Inst::Select { dest, ty, .. }
                    | Inst::Decide { dest, ty, .. } => {
                        if !local_map.contains_key(dest)
                            && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                        {
                            let val_ty = map_dmir_type_to_wasm(ty).unwrap_or(WasmValType::I64);
                            non_param_locals.push((*dest, val_ty));
                            value_types.insert(*dest, val_ty);
                        }
                    }
                    Inst::AssignVar { name, value } => {
                        if !var_names.contains(name) {
                            var_names.push(name.clone());
                        }
                        if let Some(&ty) = value_types.get(value) {
                            var_types.insert(name.clone(), ty);
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        if !var_names.contains(name) {
                            var_names.push(name.clone());
                        }
                        let ty = var_types.get(name).copied().unwrap_or(WasmValType::I64);
                        if !local_map.contains_key(dest)
                            && !non_param_locals.iter().any(|(v, _)| *v == *dest)
                        {
                            non_param_locals.push((*dest, ty));
                            value_types.insert(*dest, ty);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Second pass: resolve any variables whose AssignVar appeared later
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::AssignVar { name, value } => {
                        if let Some(&ty) = value_types.get(value) {
                            var_types.insert(name.clone(), ty);
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(&ty) = var_types.get(name) {
                            if let Some(entry) =
                                non_param_locals.iter_mut().find(|(v, _)| *v == *dest)
                            {
                                entry.1 = ty;
                            }
                            value_types.insert(*dest, ty);
                        }
                    }
                    _ => {}
                }
            }
        }

        let has_pc = func.blocks.len() > 1;

        // Group non-parameter locals by type in fixed canonical order:
        // [I64, I32, F64, V128, F32]
        // This ensures the local index sequence 100% matches the declared locals sections!
        let mut cur_local_idx = num_params;

        // I64 group: non_param_locals with I64 + var_names with I64
        let i64_locals: Vec<ValueId> = non_param_locals
            .iter()
            .filter(|(_, ty)| *ty == WasmValType::I64)
            .map(|(v, _)| *v)
            .collect();
        let i64_vars: Vec<String> = var_names
            .iter()
            .filter(|name| {
                var_types.get(*name).copied().unwrap_or(WasmValType::I64) == WasmValType::I64
            })
            .cloned()
            .collect();
        for vid in &i64_locals {
            local_map.insert(*vid, cur_local_idx);
            cur_local_idx += 1;
        }
        for name in &i64_vars {
            var_map.insert(name.clone(), cur_local_idx);
            cur_local_idx += 1;
        }
        let total_i64 = (i64_locals.len() + i64_vars.len()) as u32;

        // I32 group: $pc (if multi-block) + I32 non_param_locals + I32 vars
        let i32_locals: Vec<ValueId> = non_param_locals
            .iter()
            .filter(|(_, ty)| *ty == WasmValType::I32)
            .map(|(v, _)| *v)
            .collect();
        let i32_vars: Vec<String> = var_names
            .iter()
            .filter(|name| var_types.get(*name).copied() == Some(WasmValType::I32))
            .cloned()
            .collect();
        for vid in &i32_locals {
            local_map.insert(*vid, cur_local_idx);
            cur_local_idx += 1;
        }
        for name in &i32_vars {
            var_map.insert(name.clone(), cur_local_idx);
            cur_local_idx += 1;
        }
        let pc_local = if has_pc {
            let l = cur_local_idx;
            cur_local_idx += 1;
            Some(l)
        } else {
            None
        };
        let total_i32 = (i32_locals.len() + i32_vars.len() + if has_pc { 1 } else { 0 }) as u32;

        // F64 group: f64_locals + f64 vars
        let f64_locals: Vec<ValueId> = non_param_locals
            .iter()
            .filter(|(_, ty)| *ty == WasmValType::F64)
            .map(|(v, _)| *v)
            .collect();
        let f64_vars: Vec<String> = var_names
            .iter()
            .filter(|name| var_types.get(*name).copied() == Some(WasmValType::F64))
            .cloned()
            .collect();
        for vid in &f64_locals {
            local_map.insert(*vid, cur_local_idx);
            cur_local_idx += 1;
        }
        for name in &f64_vars {
            var_map.insert(name.clone(), cur_local_idx);
            cur_local_idx += 1;
        }
        let total_f64 = (f64_locals.len() + f64_vars.len()) as u32;

        // V128 group: v128_locals + v128 vars + 1 dedicated scratch_v128 local
        let v128_locals: Vec<ValueId> = non_param_locals
            .iter()
            .filter(|(_, ty)| *ty == WasmValType::V128)
            .map(|(v, _)| *v)
            .collect();
        let v128_vars: Vec<String> = var_names
            .iter()
            .filter(|name| var_types.get(*name).copied() == Some(WasmValType::V128))
            .cloned()
            .collect();
        for vid in &v128_locals {
            local_map.insert(*vid, cur_local_idx);
            cur_local_idx += 1;
        }
        for name in &v128_vars {
            var_map.insert(name.clone(), cur_local_idx);
            cur_local_idx += 1;
        }
        let scratch_v128 = cur_local_idx;
        cur_local_idx += 1;
        let total_v128 = (v128_locals.len() + v128_vars.len() + 1) as u32;

        // F32 group: f32_locals + f32 vars
        let f32_locals: Vec<ValueId> = non_param_locals
            .iter()
            .filter(|(_, ty)| *ty == WasmValType::F32)
            .map(|(v, _)| *v)
            .collect();
        let f32_vars: Vec<String> = var_names
            .iter()
            .filter(|name| var_types.get(*name).copied() == Some(WasmValType::F32))
            .cloned()
            .collect();
        for vid in &f32_locals {
            local_map.insert(*vid, cur_local_idx);
            cur_local_idx += 1;
        }
        for name in &f32_vars {
            var_map.insert(name.clone(), cur_local_idx);
            cur_local_idx += 1;
        }
        let total_f32 = (f32_locals.len() + f32_vars.len()) as u32;

        let mut local_declarations: Vec<(u32, WasmValType)> = Vec::new();
        if total_i64 > 0 {
            local_declarations.push((total_i64, WasmValType::I64));
        }
        if total_i32 > 0 {
            local_declarations.push((total_i32, WasmValType::I32));
        }
        if total_f64 > 0 {
            local_declarations.push((total_f64, WasmValType::F64));
        }
        if total_v128 > 0 {
            local_declarations.push((total_v128, WasmValType::V128));
        }
        if total_f32 > 0 {
            local_declarations.push((total_f32, WasmValType::F32));
        }

        // Emit local declarations
        let mut body = Vec::new();
        encode_u32_leb128(local_declarations.len() as u32, &mut body);
        for (count, ty) in &local_declarations {
            encode_u32_leb128(*count, &mut body);
            body.push(*ty as u8);
        }

        let mut wat_fn = format!("  (func ${} (export \"{}\")", func.name, func.name);
        for (_pname, ty_str, pval) in &func.params {
            let ty = map_dmir_type_to_wasm(ty_str).unwrap_or(WasmValType::I64);
            wat_fn.push_str(&format!(" (param $v{} {})", pval.0, ty.wat_name()));
        }
        for r in &results {
            wat_fn.push_str(&format!(" (result {})", r.wat_name()));
        }
        wat_fn.push('\n');

        // WAT local declarations
        for (vid, ty) in &non_param_locals {
            wat_fn.push_str(&format!("    (local $v{} {})\n", vid.0, ty.wat_name()));
        }
        for name in &var_names {
            let ty = var_types.get(name).copied().unwrap_or(WasmValType::I64);
            wat_fn.push_str(&format!("    (local $var_{} {})\n", name, ty.wat_name()));
        }
        if pc_local.is_some() {
            wat_fn.push_str("    (local $pc i32)\n");
        }
        wat_fn.push_str("    (local $scratch_v128 v128)\n");

        // Lower blocks and control flow
        if func.blocks.len() <= 1 {
            // Straight-line block lowering
            if let Some(block) = func.blocks.first() {
                Self::compile_instructions(
                    &block.instructions,
                    &mut body,
                    &mut wat_fn,
                    &local_map,
                    &var_map,
                    &value_types,
                    import_fn_indices,
                    defined_fn_indices,
                    string_table,
                    module,
                    scratch_v128,
                )?;

                Self::compile_terminator(
                    &block.terminator,
                    &mut body,
                    &mut wat_fn,
                    &local_map,
                    None,
                    &HashMap::new(),
                    func,
                )?;
            } else {
                // Empty block default return 0
                body.push(0x42); // i64.const
                encode_i64_leb128(0, &mut body);
                body.push(0x0F); // return
                body.push(0x0B); // end
                wat_fn.push_str("    (i64.const 0)\n    (return)\n");
            }
        } else {
            // Multi-block CFG lowering via loop with dispatcher
            // Standard param-stack lowering for block params is direct for SSA without phi elimination.
            let pc_idx = pc_local.unwrap();
            let block_id_to_idx: HashMap<BasicBlockId, u32> = func
                .blocks
                .iter()
                .enumerate()
                .map(|(idx, b)| (b.id, idx as u32))
                .collect();

            let entry_idx = block_id_to_idx.get(&func.entry_block).copied().unwrap_or(0);

            // Initialize pc to entry block: local.set $pc (i32.const <entry_idx>)
            body.push(0x41); // i32.const
            encode_i32_leb128(entry_idx as i32, &mut body);
            body.push(0x21); // local.set
            encode_u32_leb128(pc_idx, &mut body);

            wat_fn.push_str(&format!(
                "    (local.set $pc (i32.const {}))\n    (loop $cfg_loop\n",
                entry_idx
            ));

            // Outer loop: opcode 0x03, blocktype 0x40 (void)
            body.push(0x03);
            body.push(0x40);

            for (idx, b) in func.blocks.iter().enumerate() {
                // If pc == idx, execute block
                body.push(0x20); // local.get $pc
                encode_u32_leb128(pc_idx, &mut body);
                body.push(0x41); // i32.const <idx>
                encode_i32_leb128(idx as i32, &mut body);
                body.push(0x46); // i32.eq
                body.push(0x04); // if
                body.push(0x40); // void blocktype

                wat_fn.push_str(&format!(
                    "      (if (i32.eq (local.get $pc) (i32.const {}))\n        (then\n",
                    idx
                ));

                Self::compile_instructions(
                    &b.instructions,
                    &mut body,
                    &mut wat_fn,
                    &local_map,
                    &var_map,
                    &value_types,
                    import_fn_indices,
                    defined_fn_indices,
                    string_table,
                    module,
                    scratch_v128,
                )?;

                Self::compile_terminator(
                    &b.terminator,
                    &mut body,
                    &mut wat_fn,
                    &local_map,
                    Some(pc_idx),
                    &block_id_to_idx,
                    func,
                )?;

                wat_fn.push_str("        )\n");

                if idx + 1 < func.blocks.len() {
                    body.push(0x05); // else
                    wat_fn.push_str("        (else\n");
                }
            }

            // Close all the nested if/else blocks
            for (idx, _) in func.blocks.iter().enumerate() {
                body.push(0x0B); // end
                if idx + 1 < func.blocks.len() {
                    wat_fn.push_str("        )\n      )\n");
                } else {
                    wat_fn.push_str("      )\n");
                }
            }

            // br $cfg_loop (opcode 0x0C, depth 0)
            body.push(0x0C);
            encode_u32_leb128(0, &mut body);

            // end loop (opcode 0x0B)
            body.push(0x0B);

            // Default fallback return 0
            body.push(0x42);
            encode_i64_leb128(0, &mut body);
            body.push(0x0F);

            wat_fn.push_str("      (br $cfg_loop)\n    )\n    (i64.const 0)\n    (return)\n");
        }

        // End function (opcode 0x0B)
        body.push(0x0B);
        wat_fn.push_str("  )");

        Ok((type_idx, body, wat_fn))
    }

    /// Compiles a sequence of DMIR instructions to Wasm binary and WAT text.
    fn compile_instructions(
        instructions: &[Inst],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        var_map: &HashMap<String, u32>,
        value_types: &HashMap<ValueId, WasmValType>,
        import_fn_indices: &HashMap<String, u32>,
        defined_fn_indices: &HashMap<String, u32>,
        string_table: &HashMap<String, u32>,
        module: &Module,
        scratch_v128: u32,
    ) -> Result<(), String> {
        for inst in instructions {
            match inst {
                Inst::ConstInt { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    body.push(0x42); // i64.const
                    encode_i64_leb128(*value, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {}))\n",
                        dest.0, value
                    ));
                }
                Inst::ConstFloat { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    body.push(0x44); // f64.const
                    encode_f64(*value, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (f64.const {}))\n",
                        dest.0, value
                    ));
                }
                Inst::ConstBool { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let val = if *value { 1i64 } else { 0i64 };
                    body.push(0x42); // i64.const
                    encode_i64_leb128(val, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {}))\n",
                        dest.0, val
                    ));
                }
                Inst::ConstStr { dest, value } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let offset = string_table.get(value).copied().unwrap_or(1024) as i64;
                    body.push(0x42); // i64.const
                    encode_i64_leb128(offset, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {})) ;; str: {:?}\n",
                        dest.0, offset, value
                    ));
                }
                Inst::FormatStr { dest, parts, .. } => {
                    let loc = local_map.get(dest).copied().unwrap_or(0);
                    let first_part = parts.first().map(|s| s.as_str()).unwrap_or("");
                    let offset = string_table.get(first_part).copied().unwrap_or(1024) as i64;
                    body.push(0x42); // i64.const
                    encode_i64_leb128(offset, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.const {})) ;; format_str\n",
                        dest.0, offset
                    ));
                }
                Inst::LoadVar { dest, name } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let var_loc = var_map.get(name).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(var_loc, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);
                    wat.push_str(&format!(
                        "    (local.set $v{} (local.get $var_{}))\n",
                        dest.0, name
                    ));
                }
                Inst::AssignVar { name, value } => {
                    let var_loc = var_map.get(name).copied().unwrap_or(0);
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(val_loc, body);
                    body.push(0x21); // local.set
                    encode_u32_leb128(var_loc, body);
                    wat.push_str(&format!(
                        "    (local.set $var_{} (local.get $v{}))\n",
                        name, value.0
                    ));
                }
                Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let left_loc = local_map.get(left).copied().unwrap_or(0);
                    let right_loc = local_map.get(right).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";
                    let is_simd_f32x4 = ty == "Float4" || ty == "Vector4";
                    let is_simd_i32x4 = ty == "Int4";

                    body.push(0x20); // local.get
                    encode_u32_leb128(left_loc, body);
                    body.push(0x20); // local.get
                    encode_u32_leb128(right_loc, body);

                    if is_simd_f32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0xFD);
                                encode_u32_leb128(228, body); // f32x4.add
                            }
                            "-" => {
                                body.push(0xFD);
                                encode_u32_leb128(229, body); // f32x4.sub
                            }
                            "*" => {
                                body.push(0xFD);
                                encode_u32_leb128(230, body); // f32x4.mul
                            }
                            "/" => {
                                body.push(0xFD);
                                encode_u32_leb128(231, body); // f32x4.div
                            }
                            _ => {
                                body.push(0xFD);
                                encode_u32_leb128(228, body);
                            }
                        }
                    } else if is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0xFD);
                                encode_u32_leb128(174, body); // i32x4.add
                            }
                            "-" => {
                                body.push(0xFD);
                                encode_u32_leb128(177, body); // i32x4.sub
                            }
                            "*" => {
                                body.push(0xFD);
                                encode_u32_leb128(181, body); // i32x4.mul
                            }
                            _ => {
                                body.push(0xFD);
                                encode_u32_leb128(174, body);
                            }
                        }
                    } else if is_float {
                        match op.as_str() {
                            "+" => body.push(0xA0), // f64.add
                            "-" => body.push(0xA1), // f64.sub
                            "*" => body.push(0xA2), // f64.mul
                            "/" => body.push(0xA3), // f64.div
                            "==" => {
                                body.push(0x61); // f64.eq
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "!=" => {
                                body.push(0x62); // f64.ne
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "<" => {
                                body.push(0x63); // f64.lt
                                body.push(0xAD);
                            }
                            "<=" => {
                                body.push(0x65); // f64.le
                                body.push(0xAD);
                            }
                            ">" => {
                                body.push(0x64); // f64.gt
                                body.push(0xAD);
                            }
                            ">=" => {
                                body.push(0x66); // f64.ge
                                body.push(0xAD);
                            }
                            _ => body.push(0xA0),
                        }
                    } else {
                        match op.as_str() {
                            "+" | "wrapping_+" | "saturating_+" => body.push(0x7C), // i64.add
                            "-" | "wrapping_-" | "saturating_-" => body.push(0x7D), // i64.sub
                            "*" | "wrapping_*" | "saturating_*" => body.push(0x7E), // i64.mul
                            "/" => body.push(0x7F),                                 // i64.div_s
                            "%" => body.push(0x81),                                 // i64.rem_s
                            "&" | "&&" => body.push(0x83),                          // i64.and
                            "|" | "||" => body.push(0x84),                          // i64.or
                            "^" => body.push(0x85),                                 // i64.xor
                            "<<" => body.push(0x86),                                // i64.shl
                            ">>" => body.push(0x87),                                // i64.shr_s
                            "==" => {
                                body.push(0x51); // i64.eq
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "!=" => {
                                body.push(0x52); // i64.ne
                                body.push(0xAD); // i64.extend_i32_u
                            }
                            "<" => {
                                body.push(0x53); // i64.lt_s
                                body.push(0xAD);
                            }
                            "<=" => {
                                body.push(0x57); // i64.le_s
                                body.push(0xAD);
                            }
                            ">" => {
                                body.push(0x55); // i64.gt_s
                                body.push(0xAD);
                            }
                            ">=" => {
                                body.push(0x59); // i64.ge_s
                                body.push(0xAD);
                            }
                            _ => body.push(0x7C),
                        }
                    }

                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);

                    if !is_float && !is_simd_f32x4 && !is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                            }
                            "-" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                            }
                            "*" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x7F); // i64.div_s
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x00); // unreachable
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                            }
                            "saturating_+" => {
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body); // local.set dest
                                body.push(0x0B); // end
                            }
                            "saturating_-" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x83); // i64.and
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x53); // i64.lt_s
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00); // i64.const 0
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body); // local.set dest
                                body.push(0x0B); // end
                            }
                            "saturating_*" => {
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(-1, body);
                                body.push(0x51); // i64.eq
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x51); // i64.eq
                                body.push(0x83); // i64.and
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(-1, body);
                                body.push(0x51); // i64.eq
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x51); // i64.eq
                                body.push(0x83); // i64.and
                                body.push(0x84); // i64.or
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x05); // else
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x42);
                                body.push(0x00);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x7F); // i64.div_s
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x52); // i64.ne
                                body.push(0x04);
                                body.push(0x40); // if (void)
                                body.push(0x20);
                                encode_u32_leb128(left_loc, body);
                                body.push(0x20);
                                encode_u32_leb128(right_loc, body);
                                body.push(0x85); // i64.xor
                                body.push(0x42);
                                body.push(0x00);
                                body.push(0x59); // i64.ge_s
                                body.push(0x04);
                                body.push(0x7E); // if (i64)
                                body.push(0x42);
                                encode_i64_leb128(i64::MAX, body);
                                body.push(0x05); // else
                                body.push(0x42);
                                encode_i64_leb128(i64::MIN, body);
                                body.push(0x0B); // end
                                body.push(0x21);
                                encode_u32_leb128(dest_loc, body);
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                                body.push(0x0B); // end
                            }
                            _ => {}
                        }
                    }

                    let op_str = if is_simd_f32x4 {
                        match op.as_str() {
                            "+" => "f32x4.add",
                            "-" => "f32x4.sub",
                            "*" => "f32x4.mul",
                            "/" => "f32x4.div",
                            _ => "f32x4.add",
                        }
                    } else if is_simd_i32x4 {
                        match op.as_str() {
                            "+" => "i32x4.add",
                            "-" => "i32x4.sub",
                            "*" => "i32x4.mul",
                            _ => "i32x4.add",
                        }
                    } else if is_float {
                        match op.as_str() {
                            "+" => "f64.add",
                            "-" => "f64.sub",
                            "*" => "f64.mul",
                            "/" => "f64.div",
                            "==" => "f64.eq",
                            "!=" => "f64.ne",
                            "<" => "f64.lt",
                            "<=" => "f64.le",
                            ">" => "f64.gt",
                            ">=" => "f64.ge",
                            _ => "f64.add",
                        }
                    } else {
                        match op.as_str() {
                            "+" | "wrapping_+" | "saturating_+" => "i64.add",
                            "-" | "wrapping_-" | "saturating_-" => "i64.sub",
                            "*" | "wrapping_*" | "saturating_*" => "i64.mul",
                            "/" => "i64.div_s",
                            "%" => "i64.rem_s",
                            "&" | "&&" => "i64.and",
                            "|" | "||" => "i64.or",
                            "^" => "i64.xor",
                            "<<" => "i64.shl",
                            ">>" => "i64.shr_s",
                            "==" => "i64.eq",
                            "!=" => "i64.ne",
                            "<" => "i64.lt_s",
                            "<=" => "i64.le_s",
                            ">" => "i64.gt_s",
                            ">=" => "i64.ge_s",
                            _ => "i64.add",
                        }
                    };

                    let is_relational =
                        matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
                    if is_relational {
                        wat.push_str(&format!(
                            "    (local.set $v{} (i64.extend_i32_u ({} (local.get $v{}) (local.get $v{}))))\n",
                            dest.0, op_str, left.0, right.0
                        ));
                    } else {
                        wat.push_str(&format!(
                            "    (local.set $v{} ({} (local.get $v{}) (local.get $v{})))\n",
                            dest.0, op_str, left.0, right.0
                        ));
                    }

                    if !is_float && !is_simd_f32x4 && !is_simd_i32x4 {
                        match op.as_str() {
                            "+" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (unreachable)))\n",
                                    dest.0, left.0, dest.0, right.0
                                ));
                            }
                            "-" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (unreachable)))\n",
                                    left.0, right.0, dest.0, left.0
                                ));
                            }
                            "*" => {
                                wat.push_str(&format!(
                                    "    (if (i64.ne (local.get $v{}) (i64.const 0)) (then (if (i64.ne (i64.div_s (local.get $v{}) (local.get $v{})) (local.get $v{})) (then (unreachable)))))\n",
                                    left.0, dest.0, left.0, right.0
                                ));
                            }
                            "saturating_+" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (local.get $v{}) (i64.const 0))))))\n",
                                    dest.0, left.0, dest.0, right.0, dest.0, left.0
                                ));
                            }
                            "saturating_-" => {
                                wat.push_str(&format!(
                                    "    (if (i64.lt_s (i64.and (i64.xor (local.get $v{}) (local.get $v{})) (i64.xor (local.get $v{}) (local.get $v{}))) (i64.const 0)) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (local.get $v{}) (i64.const 0))))))\n",
                                    left.0, right.0, dest.0, left.0, dest.0, left.0
                                ));
                            }
                            "saturating_*" => {
                                wat.push_str(&format!(
                                    "    (if (i64.or (i64.and (i64.eq (local.get $v{}) (i64.const -1)) (i64.eq (local.get $v{}) (i64.const -9223372036854775808))) (i64.and (i64.eq (local.get $v{}) (i64.const -1)) (i64.eq (local.get $v{}) (i64.const -9223372036854775808)))) (then (local.set $v{} (i64.const 9223372036854775807))) (else (if (i64.ne (local.get $v{}) (i64.const 0)) (then (if (i64.ne (i64.div_s (local.get $v{}) (local.get $v{})) (local.get $v{})) (then (local.set $v{} (select (i64.const 9223372036854775807) (i64.const -9223372036854775808) (i64.ge_s (i64.xor (local.get $v{}) (local.get $v{})) (i64.const 0))))))))))\n",
                                    left.0, right.0, right.0, left.0, dest.0, left.0, dest.0, left.0, right.0, dest.0, left.0, right.0
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                Inst::UnOp {
                    dest,
                    op,
                    operand,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let op_loc = local_map.get(operand).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";

                    match op.as_str() {
                        "-" if is_float => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x9A); // f64.neg
                        }
                        "-" => {
                            body.push(0x42); // i64.const 0
                            encode_i64_leb128(0, body);
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x7D); // i64.sub
                        }
                        "!" => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x50); // i64.eqz
                            body.push(0xAD); // i64.extend_i32_u
                        }
                        "~" => {
                            body.push(0x20); // local.get
                            encode_u32_leb128(op_loc, body);
                            body.push(0x42); // i64.const -1
                            encode_i64_leb128(-1, body);
                            body.push(0x85); // i64.xor
                        }
                        _ => {
                            body.push(0x20);
                            encode_u32_leb128(op_loc, body);
                        }
                    }
                    body.push(0x21); // local.set
                    encode_u32_leb128(dest_loc, body);
                    if op == "copy" || op == "await" {
                        wat.push_str(&format!(
                            "    (local.set $v{} (local.get $v{}))\n",
                            dest.0, operand.0
                        ));
                    } else {
                        wat.push_str(&format!(
                            "    (local.set $v{} (unop_{} (local.get $v{})))\n",
                            dest.0, op, operand.0
                        ));
                    }
                }
                Inst::Call {
                    dest, func, args, ..
                } => {
                    Self::compile_call(
                        *dest,
                        func,
                        args,
                        body,
                        wat,
                        local_map,
                        value_types,
                        import_fn_indices,
                        defined_fn_indices,
                        scratch_v128,
                    )?;
                }
                Inst::MethodCall {
                    dest,
                    object,
                    method,
                    args,
                    ..
                } => {
                    let mut full_args = vec![*object];
                    full_args.extend(args.iter().copied());
                    let runtime_fn = match method.as_str() {
                        "push" | "append" => "datara_rt_list_append",
                        "get" => "datara_rt_list_get",
                        "set" => "datara_rt_list_set",
                        "len" | "count" => "datara_rt_list_len",
                        "byte_len" => "datara_rt_str_len",
                        "char_len" => "datara_rt_str_chars",
                        "byte_at" => "datara_rt_str_byte_at",
                        "char_at" => "datara_rt_str_char_at",
                        "insert" => "datara_rt_map_insert",
                        _ => method.as_str(),
                    };
                    Self::compile_call(
                        *dest,
                        runtime_fn,
                        &full_args,
                        body,
                        wat,
                        local_map,
                        value_types,
                        import_fn_indices,
                        defined_fn_indices,
                        scratch_v128,
                    )?;
                }
                Inst::StructInit {
                    dest,
                    class_name,
                    fields,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    // Allocate fields.len() * 8 bytes via datara:rt/alloc
                    if let Some(&alloc_idx) = import_fn_indices.get("datara:rt/alloc") {
                        let size = (fields.len().max(1).saturating_mul(8)) as i64;
                        body.push(0x42); // i64.const
                        encode_i64_leb128(size, body);
                        body.push(0x10); // call
                        encode_u32_leb128(alloc_idx, body);
                        body.push(0x21); // local.set dest
                        encode_u32_leb128(dest_loc, body);

                        // Store fields into linear memory at offset i * 8
                        for (i, (_, val_id)) in fields.iter().enumerate() {
                            let val_loc = local_map.get(val_id).copied().unwrap_or(0);
                            let byte_offset = (i.saturating_mul(8)) as u32;
                            body.push(0x20); // local.get dest
                            encode_u32_leb128(dest_loc, body);
                            body.push(0xA7); // i32.wrap_i64 (memory addr)
                            body.push(0x20); // local.get val
                            encode_u32_leb128(val_loc, body);
                            body.push(0x37); // i64.store
                            encode_u32_leb128(3, body); // alignment 2^3 = 8
                            encode_u32_leb128(byte_offset, body); // offset
                        }
                    } else {
                        // Fallback stub if alloc not imported: set to zero
                        body.push(0x42);
                        encode_i64_leb128(0, body);
                        body.push(0x21);
                        encode_u32_leb128(dest_loc, body);
                    }
                    wat.push_str(&format!(
                        "    (local.set $v{} (struct_init ${}))\n",
                        dest.0, class_name
                    ));
                }
                Inst::GetField {
                    dest,
                    object,
                    field,
                    ..
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let obj_loc = local_map.get(object).copied().unwrap_or(0);

                    // Compute field offset deterministically
                    let mut sorted_classes: Vec<&String> = module.class_fields.keys().collect();
                    sorted_classes.sort();
                    let field_idx = sorted_classes
                        .iter()
                        .find_map(|cls| module.class_fields[*cls].iter().position(|f| f == field))
                        .unwrap_or(0);
                    let byte_offset = (field_idx.saturating_mul(8)) as u32;

                    body.push(0x20); // local.get obj
                    encode_u32_leb128(obj_loc, body);
                    body.push(0xA7); // i32.wrap_i64
                    body.push(0x29); // i64.load
                    encode_u32_leb128(3, body); // align 8
                    encode_u32_leb128(byte_offset, body); // offset
                    body.push(0x21); // local.set dest
                    encode_u32_leb128(dest_loc, body);

                    wat.push_str(&format!(
                        "    (local.set $v{} (i64.load offset={} (i32.wrap_i64 (local.get $v{}))))\n",
                        dest.0,
                        byte_offset,
                        object.0
                    ));
                }
                Inst::SetField {
                    object,
                    field,
                    value,
                } => {
                    let obj_loc = local_map.get(object).copied().unwrap_or(0);
                    let val_loc = local_map.get(value).copied().unwrap_or(0);

                    let mut sorted_classes: Vec<&String> = module.class_fields.keys().collect();
                    sorted_classes.sort();
                    let field_idx = sorted_classes
                        .iter()
                        .find_map(|cls| module.class_fields[*cls].iter().position(|f| f == field))
                        .unwrap_or(0);
                    let byte_offset = (field_idx.saturating_mul(8)) as u32;

                    body.push(0x20); // local.get obj
                    encode_u32_leb128(obj_loc, body);
                    body.push(0xA7); // i32.wrap_i64
                    body.push(0x20); // local.get val
                    encode_u32_leb128(val_loc, body);
                    body.push(0x37); // i64.store
                    encode_u32_leb128(3, body);
                    encode_u32_leb128(byte_offset, body);

                    wat.push_str(&format!(
                        "    (i64.store offset={} (i32.wrap_i64 (local.get $v{})) (local.get $v{}))\n",
                        byte_offset,
                        object.0,
                        value.0
                    ));
                }
                Inst::Out { value } => {
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    if let Some(&print_idx) = import_fn_indices.get("datara:rt/print") {
                        body.push(0x20); // local.get val
                        encode_u32_leb128(val_loc, body);
                        body.push(0x10); // call
                        encode_u32_leb128(print_idx, body);
                    }
                    wat.push_str(&format!("    (call $print (local.get $v{}))\n", value.0));
                }
                Inst::Err { value } => {
                    let val_loc = local_map.get(value).copied().unwrap_or(0);
                    if let Some(&err_idx) = import_fn_indices.get("datara:rt/err") {
                        body.push(0x20); // local.get val
                        encode_u32_leb128(val_loc, body);
                        body.push(0x10); // call
                        encode_u32_leb128(err_idx, body);
                    }
                    wat.push_str(&format!("    (call $err (local.get $v{}))\n", value.0));
                }
                Inst::Select {
                    dest,
                    cond,
                    then_val,
                    else_val,
                    ..
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let cond_loc = local_map.get(cond).copied().unwrap_or(0);
                    let then_loc = local_map.get(then_val).copied().unwrap_or(0);
                    let else_loc = local_map.get(else_val).copied().unwrap_or(0);

                    body.push(0x20); // local.get then_val
                    encode_u32_leb128(then_loc, body);
                    body.push(0x20); // local.get else_val
                    encode_u32_leb128(else_loc, body);
                    body.push(0x20); // local.get cond
                    encode_u32_leb128(cond_loc, body);
                    body.push(0x50); // i64.eqz (returns i32: 1 if cond == 0, 0 if cond != 0)
                    body.push(0x45); // i32.eqz (returns i32: 1 if cond != 0, 0 if cond == 0)
                    body.push(0x1B); // select
                    body.push(0x21); // local.set dest
                    encode_u32_leb128(dest_loc, body);

                    wat.push_str(&format!(
                        "    (local.set $v{} (select (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                        dest.0, then_val.0, else_val.0, cond.0
                    ));
                }
                Inst::Decide {
                    dest,
                    arms,
                    else_val,
                    ty,
                } => {
                    let dest_loc = local_map.get(dest).copied().unwrap_or(0);
                    let is_float = ty == "Float" || ty == "Float64";

                    // 1. Initialize dest with else_val or 0
                    if let Some(e_val) = else_val {
                        let e_loc = local_map.get(e_val).copied().unwrap_or(0);
                        body.push(0x20); // local.get e_loc
                        encode_u32_leb128(e_loc, body);
                    } else if is_float {
                        body.push(0x44); // f64.const 0.0
                        body.extend_from_slice(&0.0f64.to_le_bytes());
                    } else {
                        body.push(0x42); // i64.const 0
                        encode_i64_leb128(0, body);
                    }
                    body.push(0x21); // local.set dest_loc
                    encode_u32_leb128(dest_loc, body);

                    // 2. Fold arms in reverse with select
                    for (arm_cond, arm_val) in arms.iter().rev() {
                        let arm_val_loc = local_map.get(arm_val).copied().unwrap_or(0);
                        let arm_cond_loc = local_map.get(arm_cond).copied().unwrap_or(0);

                        body.push(0x20); // local.get arm_val
                        encode_u32_leb128(arm_val_loc, body);
                        body.push(0x20); // local.get dest (current fallback)
                        encode_u32_leb128(dest_loc, body);
                        body.push(0x20); // local.get arm_cond
                        encode_u32_leb128(arm_cond_loc, body);
                        body.push(0x50); // i64.eqz
                        body.push(0x45); // i32.eqz (1 if cond != 0, 0 if cond == 0)
                        body.push(0x1B); // select
                        body.push(0x21); // local.set dest
                        encode_u32_leb128(dest_loc, body);
                    }

                    wat.push_str(&format!("    (local.set $v{} (decide ...))\n", dest.0));
                }
                Inst::InlineAsm { .. } => {
                    return Err(
                        "Code generation failed: [E0902] inline assembly is not supported on WASM backend; use --llvm backend instead".to_string(),
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Lowers function calls, hardware SIMD operations, or imported runtime built-ins.
    fn compile_call(
        dest: ValueId,
        func: &str,
        args: &[ValueId],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        value_types: &HashMap<ValueId, WasmValType>,
        import_fn_indices: &HashMap<String, u32>,
        defined_fn_indices: &HashMap<String, u32>,
        scratch_v128: u32,
    ) -> Result<(), String> {
        let dest_loc = local_map.get(&dest).copied().unwrap_or(0);

        // -------------------------------------------------------------------
        // Hardware SIMD Lowering (v128)
        // -------------------------------------------------------------------
        if (func == "float4" || func == "datara_rt_float4") && args.len() == 4 {
            // v128.const (opcode 12 = 0x0C), then replace_lane 0..3 with f32.demote_f64
            body.push(0xFD); // SIMD prefix
            encode_u32_leb128(12, body); // v128.const (opcode 12)
            body.extend_from_slice(&[0u8; 16]); // 16 zero bytes

            for (lane, arg) in args.iter().enumerate() {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                let arg_ty = value_types.get(arg).copied().unwrap_or(WasmValType::F64);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);

                if arg_ty == WasmValType::I64 {
                    body.push(0xB9); // f64.convert_i64_s
                    body.push(0xB6); // f32.demote_f64
                } else if arg_ty == WasmValType::F64 {
                    body.push(0xB6); // f32.demote_f64
                }

                body.push(0xFD); // SIMD prefix
                encode_u32_leb128(32, body); // f32x4.replace_lane
                body.push(lane as u8);
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (v128.float4 (local.get $v{}) (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0, args[2].0, args[3].0
            ));
            return Ok(());
        }

        if (func == "int4" || func == "datara_rt_int4") && args.len() == 4 {
            // v128.const (opcode 12 = 0x0C), then replace_lane 0..3 with i32.wrap_i64
            body.push(0xFD); // SIMD prefix
            encode_u32_leb128(12, body); // v128.const (opcode 12)
            body.extend_from_slice(&[0u8; 16]);

            for (lane, arg) in args.iter().enumerate() {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
                body.push(0xA7); // i32.wrap_i64
                body.push(0xFD); // SIMD prefix
                encode_u32_leb128(28, body); // i32x4.replace_lane
                body.push(lane as u8);
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (v128.int4 (local.get $v{}) (local.get $v{}) (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0, args[2].0, args[3].0
            ));
            return Ok(());
        }

        if (func == "min4"
            || func == "max4"
            || func == "datara_rt_float4_min4"
            || func == "datara_rt_float4_max4")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);

            body.push(0x20); // local.get a
            encode_u32_leb128(a_loc, body);
            body.push(0x20); // local.get b
            encode_u32_leb128(b_loc, body);

            body.push(0xFD); // SIMD prefix
            if func.contains("min4") {
                encode_u32_leb128(234, body); // f32x4.pmin
            } else {
                encode_u32_leb128(235, body); // f32x4.pmax
            }

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.{} (local.get $v{}) (local.get $v{})))\n",
                dest.0,
                if func.contains("min4") {
                    "pmin"
                } else {
                    "pmax"
                },
                args[0].0,
                args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_add" || func == "add4" || func == "datara_rt_float4_add")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.add (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_sub" || func == "sub4" || func == "datara_rt_float4_sub")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(229, body); // f32x4.sub
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.sub (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "vec4_mul" || func == "mul4" || func == "datara_rt_float4_mul")
            && args.len() == 2
        {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);
            body.push(0x20);
            encode_u32_leb128(a_loc, body);
            body.push(0x20);
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(230, body); // f32x4.mul
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f32x4.mul (local.get $v{}) (local.get $v{})))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        if (func == "float4_x"
            || func == "lane0"
            || func == "float4_y"
            || func == "lane1"
            || func == "float4_z"
            || func == "lane2"
            || func == "float4_w"
            || func == "lane3")
            && args.len() == 1
        {
            let arg_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let lane_idx: u8 = match func {
                "float4_y" | "lane1" => 1,
                "float4_z" | "lane2" => 2,
                "float4_w" | "lane3" => 3,
                _ => 0,
            };
            body.push(0x20);
            encode_u32_leb128(arg_loc, body);
            body.push(0xFD);
            encode_u32_leb128(31, body); // f32x4.extract_lane
            body.push(lane_idx);
            body.push(0xBB); // f64.promote_f32
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (f64.promote_f32 (f32x4.extract_lane {} (local.get $v{}))))\n",
                dest.0, lane_idx, args[0].0
            ));
            return Ok(());
        }

        if (func == "int4_x" || func == "int4_y" || func == "int4_z" || func == "int4_w")
            && args.len() == 1
        {
            let arg_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let lane_idx: u8 = match func {
                "int4_y" => 1,
                "int4_z" => 2,
                "int4_w" => 3,
                _ => 0,
            };
            body.push(0x20);
            encode_u32_leb128(arg_loc, body);
            body.push(0xFD);
            encode_u32_leb128(25, body); // i32x4.extract_lane_s
            body.push(lane_idx);
            body.push(0xAC); // i64.extend_i32_s
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!("    (local.set $v{} (i64.extend_i32_s (i32x4.extract_lane_s {} (local.get $v{}))))\n", dest.0, lane_idx, args[0].0));
            return Ok(());
        }

        if (func == "dot" || func == "datara_rt_float4_dot") && args.len() == 2 {
            let a_loc = local_map.get(&args[0]).copied().unwrap_or(0);
            let b_loc = local_map.get(&args[1]).copied().unwrap_or(0);

            // Step 1: f32x4.mul(a, b) -> store in scratch_v128
            body.push(0x20); // local.get a
            encode_u32_leb128(a_loc, body);
            body.push(0x20); // local.get b
            encode_u32_leb128(b_loc, body);
            body.push(0xFD);
            encode_u32_leb128(230, body); // f32x4.mul (opcode 230)
            body.push(0x21); // local.set scratch_v128
            encode_u32_leb128(scratch_v128, body);

            // Step 2: Shuffle swap pairs [1, 0, 3, 2] and add
            // We want: f32x4.add(scratch, shuffle(scratch, scratch))
            body.push(0x20); // local.get scratch_v128 (operand for add)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 1 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 2 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0xFD);
            encode_u32_leb128(13, body); // i8x16.shuffle
            body.extend_from_slice(&[4, 5, 6, 7, 0, 1, 2, 3, 12, 13, 14, 15, 8, 9, 10, 11]);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add
            body.push(0x21); // local.set scratch_v128 (store intermediate sum)
            encode_u32_leb128(scratch_v128, body);

            // Step 3: Shuffle swap 64-bit halves [2, 3, 0, 1] and add
            // We want: f32x4.add(scratch, shuffle(scratch, scratch))
            body.push(0x20); // local.get scratch_v128 (operand for add)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 1 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0x20); // local.get scratch_v128 (operand 2 for shuffle)
            encode_u32_leb128(scratch_v128, body);
            body.push(0xFD);
            encode_u32_leb128(13, body); // i8x16.shuffle
            body.extend_from_slice(&[8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7]);
            body.push(0xFD);
            encode_u32_leb128(228, body); // f32x4.add

            // Step 4: Extract lane 0 and promote to f64
            body.push(0xFD);
            encode_u32_leb128(31, body); // f32x4.extract_lane
            body.push(0); // lane 0
            body.push(0xBB); // f64.promote_f32

            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);

            wat.push_str(&format!(
                "    (local.set $v{} (f64.promote_f32 (f32x4.extract_lane 0 (f32x4.dot (local.get $v{}) (local.get $v{})))))\n",
                dest.0, args[0].0, args[1].0
            ));
            return Ok(());
        }

        // -------------------------------------------------------------------
        // Direct Function & Import Calls
        // -------------------------------------------------------------------
        let imp_idx = import_fn_indices.get(func).copied().or_else(|| {
            classify_capability_call(func)
                .and_then(|(m, f)| import_fn_indices.get(&format!("{}/{}", m, f)).copied())
        });

        let arg_wat = args
            .iter()
            .map(|a| format!("(local.get $v{})", a.0))
            .collect::<Vec<_>>()
            .join(" ");
        let arg_wat_suffix = if arg_wat.is_empty() {
            String::new()
        } else {
            format!(" {}", arg_wat)
        };

        if let Some(&def_idx) = defined_fn_indices.get(func) {
            for arg in args {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
            }
            body.push(0x10); // call
            encode_u32_leb128(def_idx, body);
            body.push(0x21); // local.set dest
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (call ${}{}))\n",
                dest.0, func, arg_wat_suffix
            ));
        } else if let Some(imp_idx) = imp_idx {
            for arg in args {
                let arg_loc = local_map.get(arg).copied().unwrap_or(0);
                body.push(0x20); // local.get arg
                encode_u32_leb128(arg_loc, body);
            }
            body.push(0x10); // call
            encode_u32_leb128(imp_idx, body);

            // Void-returning imports (e.g. own_release, print, err) leave no value on operand stack
            let is_void = matches!(
                func,
                "own_release"
                    | "datara_rt_own_release"
                    | "print"
                    | "err"
                    | "datara_rt_print"
                    | "datara_rt_err"
            );
            if !is_void {
                body.push(0x21); // local.set dest
                encode_u32_leb128(dest_loc, body);
            }
            let sanitized = func.replace(':', "_").replace('@', "_").replace('/', "_");
            if is_void {
                wat.push_str(&format!("    (call ${}{})\n", sanitized, arg_wat_suffix));
            } else {
                wat.push_str(&format!(
                    "    (local.set $v{} (call ${}{}))\n",
                    dest.0, sanitized, arg_wat_suffix
                ));
            }
        } else {
            // Fallback / runtime-only call: do NOT push args onto operand stack,
            // as no callee exists to consume them. Just initialize dest to 0.
            body.push(0x42);
            encode_i64_leb128(0, body);
            body.push(0x21);
            encode_u32_leb128(dest_loc, body);
            wat.push_str(&format!(
                "    (local.set $v{} (i64.const 0)) ;; fallback call: {}\n",
                dest.0, func
            ));
        }

        Ok(())
    }

    /// Lowers SSA block parameters to WebAssembly using standard operand-stack transfer.
    /// Arguments are pushed onto the operand stack and popped into parameter locals in reverse order.
    /// This models SSA φ-functions directly with zero temporary variables and no φ-elimination pass needed.
    fn transfer_block_params(
        target_params: &[crate::dmir::BlockParam],
        args: &[ValueId],
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
    ) {
        if target_params.is_empty() || args.is_empty() {
            return;
        }
        // Push all arguments onto Wasm operand stack
        for arg in args {
            let arg_loc = local_map.get(arg).copied().unwrap_or(0);
            body.push(0x20); // local.get
            encode_u32_leb128(arg_loc, body);
        }
        // Pop into target block parameter locals in reverse order (direct SSA lowering)
        for (param, _) in target_params.iter().zip(args.iter()).rev() {
            let param_loc = local_map.get(&param.val).copied().unwrap_or(0);
            body.push(0x21); // local.set
            encode_u32_leb128(param_loc, body);
        }
        for (param, arg) in target_params.iter().zip(args.iter()) {
            wat.push_str(&format!(
                "    (local.set $v{} (local.get $v{})) ;; SSA block param transfer (no phi pass needed)\n",
                param.val.0, arg.0
            ));
        }
    }

    /// Compiles block terminators (Branch, CondBranch, Return, Unreachable).
    fn compile_terminator(
        terminator: &Terminator,
        body: &mut Vec<u8>,
        wat: &mut String,
        local_map: &HashMap<ValueId, u32>,
        pc_local: Option<u32>,
        block_id_to_idx: &HashMap<BasicBlockId, u32>,
        func: &crate::dmir::Function,
    ) -> Result<(), String> {
        match terminator {
            Terminator::Return { value } => {
                if let Some(val_id) = value {
                    let val_loc = local_map.get(val_id).copied().unwrap_or(0);
                    body.push(0x20); // local.get
                    encode_u32_leb128(val_loc, body);
                }
                body.push(0x0F); // return
                wat.push_str(&format!(
                    "    (return{})\n",
                    value
                        .map(|v| format!(" (local.get $v{})", v.0))
                        .unwrap_or_default()
                ));
            }
            Terminator::Branch { target, args } => {
                let target_idx = block_id_to_idx.get(target).copied().unwrap_or(0);
                if let Some(pc_idx) = pc_local {
                    if let Some(target_block) = func.get_block(*target) {
                        Self::transfer_block_params(
                            &target_block.params,
                            args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41); // i32.const target_idx
                    encode_i32_leb128(target_idx as i32, body);
                    body.push(0x21); // local.set $pc
                    encode_u32_leb128(pc_idx, body);

                    wat.push_str(&format!("    (local.set $pc (i32.const {}))\n", target_idx));
                }
            }
            Terminator::CondBranch {
                cond,
                then_block,
                then_args,
                else_block,
                else_args,
            } => {
                let cond_loc = local_map.get(cond).copied().unwrap_or(0);
                let then_idx = block_id_to_idx.get(then_block).copied().unwrap_or(0);
                let else_idx = block_id_to_idx.get(else_block).copied().unwrap_or(0);

                if let Some(pc_idx) = pc_local {
                    body.push(0x20); // local.get cond
                    encode_u32_leb128(cond_loc, body);
                    body.push(0x50); // i64.eqz
                    body.push(0x04); // if
                    body.push(0x40); // void blocktype

                    // Else block (cond == 0)
                    if let Some(target_block) = func.get_block(*else_block) {
                        Self::transfer_block_params(
                            &target_block.params,
                            else_args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41);
                    encode_i32_leb128(else_idx as i32, body);
                    body.push(0x21);
                    encode_u32_leb128(pc_idx, body);

                    body.push(0x05); // else

                    // Then block (cond != 0)
                    if let Some(target_block) = func.get_block(*then_block) {
                        Self::transfer_block_params(
                            &target_block.params,
                            then_args,
                            body,
                            wat,
                            local_map,
                        );
                    }
                    body.push(0x41);
                    encode_i32_leb128(then_idx as i32, body);
                    body.push(0x21);
                    encode_u32_leb128(pc_idx, body);

                    body.push(0x0B); // end

                    wat.push_str(&format!(
                        "    (if (i64.ne (local.get $v{}) (i64.const 0))\n      (then (local.set $pc (i32.const {})))\n      (else (local.set $pc (i32.const {}))))\n",
                        cond.0, then_idx, else_idx
                    ));
                }
            }
            Terminator::Unreachable => {
                body.push(0x00); // unreachable
                wat.push_str("    (unreachable)\n");
            }
        }
        Ok(())
    }

    /// Generates the companion JavaScript runtime loader with linear memory bump allocator,
    /// standard collection built-ins, and compositional capability bridging.
    fn generate_js_runtime_shim(
        module_name: &str,
        import_entries: &[(&'static str, &'static str, WasmFuncType, u32)],
    ) -> String {
        let has_fs = import_entries
            .iter()
            .any(|(m, _, _, _)| *m == "datara:fs@1.0");
        let has_net = import_entries
            .iter()
            .any(|(m, _, _, _)| *m == "datara:net@1.0");
        let has_sys = import_entries
            .iter()
            .any(|(m, _, _, _)| *m == "datara:sys@1.0");

        format!(
            r#"// Datara Capability-Native WebAssembly Runtime Loader ({module_name})
import fs from 'fs';

export async function loadDataraModule(wasmPath, customImports = {{}}) {{
    const wasmFile = wasmPath || './{module_name}.wasm';
    const wasmBytes = fs.readFileSync(wasmFile);

    // Linear-memory bump allocator and runtime context
    let memoryInstance = null;
    let bumpPointer = 65536; // start at 64KB boundary

    const readString = (ptr) => {{
        if (!memoryInstance || ptr === 0n || ptr === 0) return "";
        const p = Number(ptr);
        const mem = new Uint8Array(memoryInstance.buffer);
        const view = new DataView(memoryInstance.buffer);
        const len = view.getUint32(p, true);
        const bytes = mem.slice(p + 4, p + 4 + len);
        return new TextDecoder('utf-8').decode(bytes);
    }};

    const allocateMemory = (size) => {{
        const alignedSize = (Number(size) + 7) & ~7;
        const ptr = bumpPointer;
        bumpPointer += alignedSize;
        if (memoryInstance && bumpPointer > memoryInstance.buffer.byteLength) {{
            const neededPages = Math.ceil((bumpPointer - memoryInstance.buffer.byteLength) / 65536);
            memoryInstance.grow(neededPages);
        }}
        return BigInt(ptr);
    }};

    // In-memory collection storage backed by linear memory pointers
    const listStorage = new Map();
    let nextListHandle = 10000n;

    const mapStorage = new Map();
    let nextMapHandle = 20000n;

    // Ownership guard reference-counting storage (per-value Map)
    const ownershipStorage = new Map();

    const importObject = {{
        "datara:rt": {{
            alloc: (size) => allocateMemory(size),
            print: (val) => console.log(typeof val === 'bigint' ? val.toString() : val),
            err: (val) => console.error(typeof val === 'bigint' ? val.toString() : val),
            list_create: (cap) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, []);
                return handle;
            }},
            list_create_1: (a) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, [a]);
                return handle;
            }},
            list_create_2: (a, b) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, [a, b]);
                return handle;
            }},
            list_create_3: (a, b, c) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, [a, b, c]);
                return handle;
            }},
            list_create_4: (a, b, c, d) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, [a, b, c, d]);
                return handle;
            }},
            list_create_5: (a, b, c, d, e) => {{
                const handle = nextListHandle++;
                listStorage.set(handle, [a, b, c, d, e]);
                return handle;
            }},
            list_create_repeat: (elem, count) => {{
                const handle = nextListHandle++;
                const arr = new Array(Number(count)).fill(elem);
                listStorage.set(handle, arr);
                return handle;
            }},
            list_append: (listPtr, val) => {{
                let l = listStorage.get(listPtr);
                if (!l) {{ l = []; listStorage.set(listPtr, l); }}
                l.push(val);
                return listPtr;
            }},
            list_push: (listPtr, val) => {{
                let l = listStorage.get(listPtr);
                if (!l) {{ l = []; listStorage.set(listPtr, l); }}
                l.push(val);
                return listPtr;
            }},
            list_get: (listPtr, idx) => {{
                const l = listStorage.get(listPtr);
                if (!l || Number(idx) >= l.length) return 0n;
                return l[Number(idx)];
            }},
            list_set: (listPtr, idx, val) => {{
                let l = listStorage.get(listPtr);
                if (!l) {{ l = []; listStorage.set(listPtr, l); }}
                l[Number(idx)] = val;
                return val;
            }},
            list_len: (listPtr) => {{
                const l = listStorage.get(listPtr);
                return BigInt(l ? l.length : 0);
            }},
            map_create: () => {{
                const handle = nextMapHandle++;
                mapStorage.set(handle, new Map());
                return handle;
            }},
            map_insert: (mapPtr, key, val) => {{
                let m = mapStorage.get(mapPtr);
                if (!m) {{ m = new Map(); mapStorage.set(mapPtr, m); }}
                m.set(key, val);
                return val;
            }},
            map_set: (mapPtr, key, val) => {{
                let m = mapStorage.get(mapPtr);
                if (!m) {{ m = new Map(); mapStorage.set(mapPtr, m); }}
                m.set(key, val);
                return val;
            }},
            map_get: (mapPtr, key) => {{
                const m = mapStorage.get(mapPtr);
                return (m && m.has(key)) ? m.get(key) : 0n;
            }},
            map_len: (mapPtr) => {{
                const m = mapStorage.get(mapPtr);
                return BigInt(m ? m.size : 0);
            }},
            own_acquire: (val) => {{
                const count = (ownershipStorage.get(val) || 0) + 1;
                ownershipStorage.set(val, count);
                return val;
            }},
            own_release: (val) => {{
                const count = (ownershipStorage.get(val) || 1) - 1;
                if (count <= 0) {{
                    ownershipStorage.delete(val);
                }} else {{
                    ownershipStorage.set(val, count);
                }}
            }},
        }},
        env: {{
            now: () => BigInt(Date.now()),
        }},
        webgpu: {{
            requestAdapter: async () => globalThis.navigator?.gpu?.requestAdapter(),
            requestDevice: async (adapter) => adapter?.requestDevice(),
        }},
        webgl: {{
            getContext: (canvasId) => globalThis.document?.getElementById(canvasId)?.getContext('webgl2'),
        }}
    }};

    // Compositional capability imports (only included if granted and used)
    {fs_binding}
    {net_binding}
    {sys_binding}

    // Merge with any caller overrides
    for (const [k, v] of Object.entries(customImports)) {{
        importObject[k] = Object.assign(importObject[k] || {{}}, v);
    }}

    const {{ instance }} = await WebAssembly.instantiate(wasmBytes, importObject);
    memoryInstance = instance.exports.memory;
    return instance.exports;
}}
"#,
            module_name = module_name,
            fs_binding = if has_fs {
                r#"importObject["datara:fs@1.0"] = {
        read: (pathPtr) => {
            const p = readString(pathPtr);
            try { return BigInt(fs.readFileSync(p).length); } catch (e) { return 0n; }
        },
        write: (pathPtr, contentPtr) => {
            const p = readString(pathPtr);
            return 1n;
        }
    };"#
            } else {
                "// datara:fs@1.0: NOT GRANTED (physically omitted from imports)"
            },
            net_binding = if has_net {
                r#"importObject["datara:net@1.0"] = {
        connect: (hostPtr, port) => 1n,
        listen: (port, backlog) => 1n,
        http_get: (urlPtr) => 200n,
    };"#
            } else {
                "// datara:net@1.0: NOT GRANTED (physically omitted from imports)"
            },
            sys_binding = if has_sys {
                r#"importObject["datara:sys@1.0"] = {
        exec: (cmdPtr) => 0n,
        env: (keyPtr) => 0n,
        env_get: (keyPtr) => 0n,
        clock_now: () => BigInt(Date.now()),
    };"#
            } else {
                "// datara:sys@1.0: NOT GRANTED (physically omitted from imports)"
            }
        )
    }

    /// Validates the binary structure of a WebAssembly module.
    pub fn validate_wasm_binary(bytes: &[u8]) -> Result<(), String> {
        if bytes.len() < 8 {
            return Err("Wasm binary is less than 8 bytes".into());
        }
        if &bytes[0..4] != &[0x00, 0x61, 0x73, 0x6D] {
            return Err("Invalid Wasm magic number header".into());
        }
        if &bytes[4..8] != &[0x01, 0x00, 0x00, 0x00] {
            return Err("Invalid Wasm version (expected 1)".into());
        }

        let mut offset = 8;
        let mut last_section_id = 0u8;

        while offset < bytes.len() {
            let section_id = bytes[offset];
            offset += 1;

            if section_id != 0 && section_id <= last_section_id && last_section_id != 0 {
                return Err(format!(
                    "Section {} out of order after section {}",
                    section_id, last_section_id
                ));
            }
            if section_id != 0 {
                last_section_id = section_id;
            }

            // Read section length (LEB128)
            let mut sec_len = 0u32;
            let mut shift = 0;
            loop {
                if offset >= bytes.len() {
                    return Err("Unexpected EOF in section length".into());
                }
                let byte = bytes[offset];
                offset += 1;
                if shift < 32 {
                    sec_len |= ((byte & 0x7F) as u32) << shift;
                }
                if (byte & 0x80) == 0 {
                    break;
                }
                shift += 7;
                if shift > 35 {
                    return Err("LEB128 integer too large".into());
                }
            }

            match offset.checked_add(sec_len as usize) {
                Some(end) if end <= bytes.len() => {
                    offset = end;
                }
                _ => {
                    return Err(format!(
                        "Section {} length {} exceeds binary size",
                        section_id, sec_len
                    ));
                }
            }
        }

        Ok(())
    }
}
