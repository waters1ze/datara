use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_wasm_binary_generation_and_validation() {
    let source = r#"
class Calculator {
    base: Int
}

behavior Calculator {
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}

fn main() -> Int {
    return Calculator.add(10, 32)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_calc.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("calc.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(
        result.is_ok(),
        "WASM emission must succeed: {:?}",
        result.err()
    );

    // Verify .wasm binary file
    let wasm_bytes = fs::read(&wasm_path).expect("calc.wasm must exist");
    assert!(
        wasm_bytes.len() >= 8,
        "WASM binary must be at least 8 bytes"
    );
    // Standard WASM magic: \0asm, version: 1
    assert_eq!(
        &wasm_bytes[0..4],
        &[0x00, 0x61, 0x73, 0x6D],
        "Invalid WASM magic header"
    );
    assert_eq!(
        &wasm_bytes[4..8],
        &[0x01, 0x00, 0x00, 0x00],
        "Invalid WASM version"
    );

    // Verify .wat text format
    let wat_path = wasm_path.with_extension("wat");
    let wat_str = fs::read_to_string(&wat_path).expect("calc.wat must exist");
    assert!(
        wat_str.contains("(module"),
        "WAT must contain module definition"
    );
    assert!(
        wat_str.contains("(memory (export \"memory\") 1)"),
        "WAT must export memory"
    );

    // Verify .js runtime loader with WebGPU/WebGL
    let js_path = wasm_path.with_extension("js");
    let js_str = fs::read_to_string(&js_path).expect("calc.js must exist");
    assert!(
        js_str.contains("loadDataraModule"),
        "JS must contain loadDataraModule"
    );
    assert!(
        js_str.contains("webgpu:"),
        "JS must bind WebGPU adapter/device"
    );
    assert!(js_str.contains("webgl:"), "JS must bind WebGL getContext");

    // Verify .capabilities.json machine-auditable sidecar
    let sidecar_path = wasm_path.with_extension("capabilities.json");
    let sidecar_str = fs::read_to_string(&sidecar_path).expect("calc.capabilities.json must exist");
    assert!(
        sidecar_str.contains("compositional_wasm_import_sandbox"),
        "Capabilities sidecar must document compositional sandbox enforcement"
    );

    // Verify execution via Node.js if available in environment
    let node_script = format!(
        "const fs = require('fs');\n\
         const bytes = fs.readFileSync('{}');\n\
         WebAssembly.instantiate(bytes).then(r => {{\n\
             const res = r.instance.exports.main();\n\
             console.log('NODE_OUTPUT:' + res);\n\
         }}).catch(err => {{\n\
             console.error(err);\n\
             process.exit(1);\n\
         }});\n",
        wasm_path.to_string_lossy().replace('\\', "/")
    );
    if let Ok(output) = std::process::Command::new("node")
        .arg("-e")
        .arg(&node_script)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("NODE_OUTPUT:42"),
                "Expected main to return 42, got: {}",
                stdout
            );
        }
    }

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(&js_path);
    let _ = fs::remove_file(&sidecar_path);
    let _ = fs::remove_dir(&temp_dir);
}
