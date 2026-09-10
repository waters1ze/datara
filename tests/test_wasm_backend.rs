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
fn test_wasm_backend_fibonacci() {
    let source = r#"
fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() -> Int {
    return fib(7)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_fib.dtr")
        .expect("DMIR lowering must succeed");
    println!("DMIR FOR FIB:\n{:?}", dmir);

    let temp_dir = std::env::temp_dir().join("datara_wasm_fib_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("fib.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("fib.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "fib.wasm must pass in-crate binary validation"
    );

    let wat_path = wasm_path.with_extension("wat");
    let wat_str = fs::read_to_string(&wat_path).expect("fib.wat must exist");
    println!("FIB WAT:\n{}", wat_str);
    assert!(wat_str.contains("(func $fib"), "WAT must define $fib");
    assert!(wat_str.contains("(func $main"), "WAT must define $main");

    // Execute under Node.js: fib(7) = 13
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:13");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(&wat_path);
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_backend_list_operations() {
    let source = r#"
fn main() -> Int {
    let numbers = [10, 20, 30]
    let second = numbers[1]
    return second + 22
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_list.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_list_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("list.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("list.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "list.wasm must pass in-crate binary validation"
    );

    // Execute under Node.js: 20 + 22 = 42
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:42");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_backend_while_loop() {
    let source = r#"
fn main() -> Int {
    mut sum = 0
    mut i = 1
    while i <= 10 {
        sum = sum + i
        i = i + 1
    }
    return sum
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_loop.dtr")
        .expect("DMIR lowering must succeed");
    println!("WHILE LOOP DMIR:\n{:?}", dmir);

    let temp_dir = std::env::temp_dir().join("datara_wasm_loop_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("loop.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("loop.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "loop.wasm must pass in-crate binary validation"
    );

    let wat_str = fs::read_to_string(wasm_path.with_extension("wat")).expect("loop.wat must exist");
    println!("WHILE LOOP WAT:\n{}", wat_str);

    // Execute under Node.js: sum(1..=10) = 55
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:55");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}
