pub mod capabilities;
pub(crate) mod emit_call;
pub(crate) mod emit_inst;
pub(crate) mod shim;
pub mod types;

pub use capabilities::*;
pub use types::*;

use crate::dmir::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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

        // 0. Verify backend feature compatibility
        for func in module.functions.values() {
            for block in &func.blocks {
                for inst in &block.instructions {
                    let is_await = match inst {
                        Inst::UnOp { op, .. } => op == "await",
                        Inst::MethodCall { method, .. } => method == "await",
                        _ => false,
                    };
                    if is_await {
                        return Err(format!(
                            "[{}] WebAssembly backend does not support async execution (await) without PCS runtime",
                            crate::diagnostics::ErrorCode::AsyncBackendUnsupported.as_str()
                        ));
                    }
                }
            }
        }

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
                    own_funcs.sort();
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
                let mut f_list: Vec<String> = funcs.iter().map(|s| s.to_string()).collect();
                f_list.sort();
                granted_groups.push(GrantedCapabilityGroup {
                    module: mod_name.to_string(),
                    version: "1.0".to_string(),
                    functions: f_list,
                    capability_token: token_name.to_string(),
                });
            } else {
                absent_capabilities.push(mod_name.to_string());
            }
        }

        granted_groups.sort_by(|a, b| a.module.cmp(&b.module));
        absent_capabilities.sort();

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
        // Initialize parameter variables ($var_<param_name>) from function parameters
        for (pname, _, pval) in &func.params {
            if let Some(&var_loc) = var_map.get(pname) {
                let param_loc = local_map.get(pval).copied().unwrap_or(0);
                body.push(0x20); // local.get param_loc
                encode_u32_leb128(param_loc, &mut body);
                body.push(0x21); // local.set var_loc
                encode_u32_leb128(var_loc, &mut body);
                wat_fn.push_str(&format!(
                    "    (local.set $var_{} (local.get $v{}))\n",
                    pname, pval.0
                ));
            }
        }

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
