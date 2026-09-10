use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;

fn run_wasm_with_node(wasm_path: &Path, expected_output: &str) {
    let script = format!(
        r#"
const fs = require('fs');
const wasmBytes = fs.readFileSync('{}');

const listStorage = new Map();
let nextListHandle = 10000n;

const importObject = {{
    "datara:rt": {{
        alloc: (sz) => 65536n,
        print: (v) => console.log(v),
        err: (v) => console.error(v),
        list_create: (cap) => {{
            const h = nextListHandle++;
            listStorage.set(h, []);
            return h;
        }},
        list_create_1: (a) => {{ const h = nextListHandle++; listStorage.set(h, [a]); return h; }},
        list_create_2: (a, b) => {{ const h = nextListHandle++; listStorage.set(h, [a, b]); return h; }},
        list_create_3: (a, b, c) => {{ const h = nextListHandle++; listStorage.set(h, [a, b, c]); return h; }},
        list_get: (listPtr, idx) => {{
            const l = listStorage.get(listPtr);
            return (l && Number(idx) < l.length) ? l[Number(idx)] : 0n;
        }},
        list_append: (listPtr, val) => {{
            let l = listStorage.get(listPtr);
            if (!l) {{ l = []; listStorage.set(listPtr, l); }}
            l.push(val);
            return listPtr;
        }},
        list_len: (listPtr) => {{
            const l = listStorage.get(listPtr);
            return BigInt(l ? l.length : 0);
        }}
    }},
    env: {{
        now: () => BigInt(Date.now())
    }}
}};

WebAssembly.instantiate(wasmBytes, importObject).then(res => {{
    const val = res.instance.exports.main();
    console.log('EVAL_RESULT:' + val);
}}).catch(err => {{
    console.error('EXEC_ERROR:', err);
    process.exit(1);
}});
"#,
        wasm_path.to_string_lossy().replace('\\', "/")
    );

    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(&script)
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "Node execution failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            stdout,
            stderr
        );
        assert!(
            stdout.contains(expected_output),
            "Expected output '{}', got stdout: {}",
            expected_output,
            stdout
        );
    }
}

#[test]
fn test_wasm_simd_float4_dot() {
    let source = r#"
fn main() -> Float {
    let a = float4(1.0, 2.0, 3.0, 4.0)
    let b = float4(4.0, 3.0, 2.0, 1.0)
    return dot(a, b)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_simd_dot.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_simd_dot_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("simd_dot.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("simd_dot.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "simd_dot.wasm must pass in-crate binary validation"
    );

    let wat_path = wasm_path.with_extension("wat");
    let wat_str = fs::read_to_string(&wat_path).expect("simd_dot.wat must exist");
    assert!(wat_str.contains("v128"), "WAT must declare v128 types");
    assert!(
        wat_str.contains("f32x4.dot"),
        "WAT must document SIMD dot product"
    );

    // Execute under Node.js: 1*4 + 2*3 + 3*2 + 4*1 = 20.0
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:20");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_simd_min4_max4() {
    let source = r#"
fn main() -> Float {
    let a = float4(10.0, 20.0, 30.0, 40.0)
    let b = float4(5.0, 25.0, 15.0, 45.0)
    let m = min4(a, b)
    let x = max4(a, b)
    let m_val = lane0(m)
    let x_val = lane0(x)
    return m_val + x_val
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_simd_min_max.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_simd_min_max_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("simd_min_max.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("simd_min_max.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "simd_min_max.wasm must pass in-crate binary validation"
    );

    let wat_path = wasm_path.with_extension("wat");
    let wat_str = fs::read_to_string(&wat_path).expect("simd_min_max.wat must exist");
    assert!(
        wat_str.contains("f32x4.pmin"),
        "WAT must contain f32x4.pmin"
    );
    assert!(
        wat_str.contains("f32x4.pmax"),
        "WAT must contain f32x4.pmax"
    );

    // min4(10, 5) -> 5.0, max4(10, 5) -> 10.0, sum = 15.0
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:15");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_simd_int4_vector_ops() {
    let source = r#"
fn main() -> Int {
    let a = int4(10, 20, 30, 40)
    let first = int4_x(a)
    return first + 32
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_simd_int4.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_simd_int4_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("simd_int4.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("simd_int4.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "simd_int4.wasm must pass in-crate binary validation"
    );

    // 10 + 32 = 42
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:42");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}
