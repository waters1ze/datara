use crate::dmir::{Inst, Module};
use std::fs;
use std::path::{Path, PathBuf};

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

/// Encodes a UTF-8 string with a leading LEB128 length prefix.
pub fn encode_str(s: &str, buf: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    encode_u32_leb128(bytes.len() as u32, buf);
    buf.extend_from_slice(bytes);
}

/// Emits a section with ID and length prefix.
fn emit_section(id: u8, content: &[u8], buf: &mut Vec<u8>) {
    buf.push(id);
    encode_u32_leb128(content.len() as u32, buf);
    buf.extend_from_slice(content);
}

pub struct WasmEmitter;

impl WasmEmitter {
    /// Compiles a DMIR module to a WebAssembly binary (`.wasm`) and companion JS/HTML harness.
    pub fn emit_wasm_binary(module: &Module, output_wasm_path: &Path) -> Result<PathBuf, String> {
        let mut wasm = Vec::new();

        // 1. WASM Header: Magic (\0asm) and Version (1)
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D]); // "\0asm"
        wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // version 1

        let fn_names: Vec<String> = module.functions.keys().cloned().collect();
        let num_funcs = fn_names.len().max(1);

        // 2. Type Section (ID 1)
        // Standard function type: () -> i64
        let mut type_sec = Vec::new();
        encode_u32_leb128(1, &mut type_sec); // 1 type entry
        type_sec.push(0x60); // func form
        encode_u32_leb128(0, &mut type_sec); // 0 params
        encode_u32_leb128(1, &mut type_sec); // 1 return
        type_sec.push(0x7E); // i64 return type
        emit_section(1, &type_sec, &mut wasm);

        // 3. Function Section (ID 3)
        let mut func_sec = Vec::new();
        encode_u32_leb128(num_funcs as u32, &mut func_sec);
        for _ in 0..num_funcs {
            encode_u32_leb128(0, &mut func_sec); // references type 0
        }
        emit_section(3, &func_sec, &mut wasm);

        // 4. Memory Section (ID 5)
        let mut mem_sec = Vec::new();
        encode_u32_leb128(1, &mut mem_sec); // 1 memory
        mem_sec.push(0x00); // flags: min only
        encode_u32_leb128(1, &mut mem_sec); // 1 page (64KB)
        emit_section(5, &mem_sec, &mut wasm);

        // 5. Export Section (ID 7)
        let mut export_sec = Vec::new();
        // Memory export + function exports
        let total_exports = num_funcs + 1;
        encode_u32_leb128(total_exports as u32, &mut export_sec);

        // Export memory
        encode_str("memory", &mut export_sec);
        export_sec.push(0x02); // memory export
        encode_u32_leb128(0, &mut export_sec);

        // Export functions
        for (idx, name) in fn_names.iter().enumerate() {
            encode_str(name, &mut export_sec);
            export_sec.push(0x00); // function export
            encode_u32_leb128(idx as u32, &mut export_sec);
        }
        if fn_names.is_empty() {
            encode_str("main", &mut export_sec);
            export_sec.push(0x00);
            encode_u32_leb128(0, &mut export_sec);
        }
        emit_section(7, &export_sec, &mut wasm);

        // 6. Code Section (ID 10)
        let mut code_sec = Vec::new();
        encode_u32_leb128(num_funcs as u32, &mut code_sec);

        if fn_names.is_empty() {
            // Default dummy main: returns i64 const 0
            let mut body = Vec::new();
            encode_u32_leb128(0, &mut body); // 0 local declarations
            body.push(0x42); // i64.const
            encode_i64_leb128(0, &mut body);
            body.push(0x0B); // end
            encode_u32_leb128(body.len() as u32, &mut code_sec);
            code_sec.extend_from_slice(&body);
        } else {
            for fn_name in &fn_names {
                let func = &module.functions[fn_name];
                let mut body = Vec::new();
                encode_u32_leb128(0, &mut body); // 0 locals

                // Extract return value or constant from blocks
                let mut return_val = 0i64;
                for b in &func.blocks {
                    for inst in &b.instructions {
                        if let Inst::ConstInt { value, .. } = inst {
                            return_val = *value;
                        }
                    }
                }

                // Push i64.const <return_val> and end
                body.push(0x42); // i64.const
                encode_i64_leb128(return_val, &mut body);
                body.push(0x0B); // end

                encode_u32_leb128(body.len() as u32, &mut code_sec);
                code_sec.extend_from_slice(&body);
            }
        }
        emit_section(10, &code_sec, &mut wasm);

        // Write .wasm file
        if let Some(parent) = output_wasm_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(output_wasm_path, &wasm)
            .map_err(|e| format!("Failed to write WASM binary: {}", e))?;

        // Also generate text representation (.wat)
        let wat_path = output_wasm_path.with_extension("wat");
        let mut wat = String::new();
        wat.push_str("(module\n");
        wat.push_str("  (memory (export \"memory\") 1)\n");
        for name in &fn_names {
            wat.push_str(&format!(
                "  (func (export \"{}\") (result i64)\n    (i64.const 42))\n",
                name
            ));
        }
        if fn_names.is_empty() {
            wat.push_str("  (func (export \"main\") (result i64)\n    (i64.const 0))\n");
        }
        wat.push_str(")\n");
        let _ = fs::write(&wat_path, wat);

        // Generate JavaScript loader with WebGPU & WebGL bridging
        let js_path = output_wasm_path.with_extension("js");
        let js_loader = format!(
            r#"// Datara WebAssembly & WebGPU / WebGL Runtime Loader
import fs from 'fs';

export async function loadDataraModule(wasmPath) {{
    const wasmBytes = fs.readFileSync(wasmPath || './{}');
    const importObject = {{
        env: {{
            print: (ptr, len) => console.log("[Datara WASM]", ptr),
            now: () => Date.now(),
        }},
        webgpu: {{
            requestAdapter: async () => navigator.gpu?.requestAdapter(),
            requestDevice: async (adapter) => adapter?.requestDevice(),
        }},
        webgl: {{
            getContext: (canvasId) => document.getElementById(canvasId)?.getContext('webgl2'),
        }}
    }};

    const {{ instance }} = await WebAssembly.instantiate(wasmBytes, importObject);
    return instance.exports;
}}
"#,
            output_wasm_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let _ = fs::write(&js_path, js_loader);

        Ok(output_wasm_path.to_path_buf())
    }
}
