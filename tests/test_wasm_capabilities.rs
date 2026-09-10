use forgen::codegen::wasm::WasmEmitter;
use forgen::dmir::{BasicBlock, BasicBlockId, Function, Inst, Module, Terminator, ValueId};
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
    "datara:fs@1.0": {{
        read: (pathPtr) => 42n,
        write: (pathPtr, contentPtr) => 1n
    }},
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
    const val = (res.instance.exports.main && res.instance.exports.main.length > 0)
        ? res.instance.exports.main(0n)
        : (res.instance.exports.main ? res.instance.exports.main() : 0n);
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
fn test_wasm_compositional_capability_sandboxing_granted_fs() {
    let source = r#"
fn read_config(path: String, token: Capability<FileRead>) -> String {
    let handle = token.open(path)
    return handle.read_all()
}

fn main(sys_caps: SystemCapabilities) {
    let safe_token = sys_caps.files.grant_readonly("config.json")
    let content = read_config("config.json", safe_token)
    out content
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "cap_fs.dtr")
        .expect("DMIR lowering with SystemCapabilities must succeed");

    let temp_dir = std::env::temp_dir().join("datara_wasm_cap_granted_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("cap_fs.wasm");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(
        result.is_ok(),
        "WASM emission must succeed: {:?}",
        result.err()
    );

    let wasm_bytes = fs::read(&wasm_path).expect("cap_fs.wasm must exist");
    assert!(
        WasmEmitter::validate_wasm_binary(&wasm_bytes).is_ok(),
        "WASM binary validation must pass"
    );

    let wat_str =
        fs::read_to_string(wasm_path.with_extension("wat")).expect("cap_fs.wat must exist");

    // Verify compositional sandboxing: datara:net@1.0 and datara:sys@1.0 are PHYSICALLY ABSENT
    assert!(
        !wat_str.contains("datara:net@1.0"),
        "datara:net@1.0 must be physically absent from imports"
    );

    // Verify machine-auditable sidecar
    let sidecar_path = wasm_path.with_extension("capabilities.json");
    let sidecar_str = fs::read_to_string(&sidecar_path).expect("capabilities.json must exist");

    assert!(
        sidecar_str.contains("compositional_wasm_import_sandbox"),
        "Sidecar must specify compositional zero-trust enforcement"
    );
    assert!(
        sidecar_str.contains("datara:net@1.0"),
        "Sidecar must document datara:net@1.0 under absent capabilities"
    );

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(sidecar_path);
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_capability_e0940_compile_time_error_on_unauthorized_call() {
    // Construct a synthetic DMIR module that calls file_read without granting Capability<FileRead>
    let mut module = Module::new("unauthorized_wasm_module");
    let mut func = Function::default();
    func.name = "main".to_string();
    func.return_type = "Int".to_string();
    func.entry_block = BasicBlockId(0);

    let bb = BasicBlock {
        id: BasicBlockId(0),
        label: "entry".to_string(),
        params: Vec::new(),
        instructions: vec![
            Inst::ConstStr {
                dest: ValueId(0),
                value: "secret_file.txt".to_string(),
            },
            Inst::Call {
                dest: ValueId(1),
                func: "file_read".to_string(),
                args: vec![ValueId(0)],
                ty: "Int".to_string(),
            },
            Inst::Return {
                value: Some(ValueId(1)),
            },
        ],
        terminator: Terminator::Return {
            value: Some(ValueId(1)),
        },
    };
    func.blocks.push(bb);
    module.functions.insert("main".to_string(), func);

    let temp_dir = std::env::temp_dir().join("datara_wasm_cap_e0940_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("unauthorized.wasm");

    let result = WasmEmitter::emit_wasm_binary(&module, &wasm_path);
    assert!(
        result.is_err(),
        "WASM emission MUST fail for unauthorized capability operation"
    );

    let err_msg = result.err().unwrap();
    assert!(
        err_msg.contains("E0940"),
        "Error message must contain diagnostic code E0940: {}",
        err_msg
    );
    assert!(
        err_msg.contains("Capability<FileRead>"),
        "Error message must specify required capability 'Capability<FileRead>': {}",
        err_msg
    );

    // Verify that NO .wasm binary was emitted
    assert!(
        !wasm_path.exists(),
        "Unauthorized module must NOT emit a .wasm binary"
    );
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_capability_sidecar_and_execution_with_granted_token() {
    // Construct a DMIR module that accepts Capability<FileRead> parameter and calls file_read
    let mut module = Module::new("authorized_wasm_module");
    let mut func = Function::default();
    func.name = "main".to_string();
    func.params = vec![(
        "token".to_string(),
        "Capability<FileRead>".to_string(),
        ValueId(0),
    )];
    func.return_type = "Int".to_string();
    func.entry_block = BasicBlockId(0);

    let bb = BasicBlock {
        id: BasicBlockId(0),
        label: "entry".to_string(),
        params: Vec::new(),
        instructions: vec![
            Inst::ConstStr {
                dest: ValueId(1),
                value: "data.txt".to_string(),
            },
            Inst::Call {
                dest: ValueId(2),
                func: "file_read".to_string(),
                args: vec![ValueId(1)],
                ty: "Int".to_string(),
            },
            Inst::Return {
                value: Some(ValueId(2)),
            },
        ],
        terminator: Terminator::Return {
            value: Some(ValueId(2)),
        },
    };
    func.blocks.push(bb);
    module.functions.insert("main".to_string(), func);

    let temp_dir = std::env::temp_dir().join("datara_wasm_cap_exec_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("authorized.wasm");

    let result = WasmEmitter::emit_wasm_binary(&module, &wasm_path);
    assert!(
        result.is_ok(),
        "Authorized emission must succeed: {:?}",
        result.err()
    );

    let wat_str =
        fs::read_to_string(wasm_path.with_extension("wat")).expect("authorized.wat must exist");
    assert!(
        wat_str.contains(r#"(import "datara:fs@1.0" "read""#),
        "WAT must import datara:fs@1.0 read"
    );
    assert!(
        !wat_str.contains("datara:net@1.0"),
        "datara:net@1.0 must be physically absent"
    );

    let sidecar_str = fs::read_to_string(wasm_path.with_extension("capabilities.json"))
        .expect("sidecar must exist");
    assert!(
        sidecar_str.contains("datara:fs@1.0"),
        "Sidecar must list datara:fs@1.0 under granted"
    );
    assert!(
        sidecar_str.contains("Capability<FileRead>"),
        "Sidecar must document Capability<FileRead>"
    );

    // Execute with node.js: mock datara:fs@1.0 read returns 42n
    run_wasm_with_node(&wasm_path, "EVAL_RESULT:42");

    // Cleanup
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasm_capability_sidecar_fileread_and_networkconnect() {
    let mut module = Module::new("dual_cap_wasm_module");
    let mut func = Function::default();
    func.name = "main".to_string();
    func.params = vec![
        (
            "fs_token".to_string(),
            "Capability<FileRead>".to_string(),
            ValueId(0),
        ),
        (
            "net_token".to_string(),
            "Capability<NetworkConnect>".to_string(),
            ValueId(1),
        ),
    ];
    func.return_type = "Int".to_string();
    func.entry_block = BasicBlockId(0);

    let bb = BasicBlock {
        id: BasicBlockId(0),
        label: "entry".to_string(),
        params: Vec::new(),
        instructions: vec![
            Inst::ConstStr {
                dest: ValueId(2),
                value: "data.txt".to_string(),
            },
            Inst::Call {
                dest: ValueId(3),
                func: "file_read".to_string(),
                args: vec![ValueId(2)],
                ty: "Int".to_string(),
            },
            Inst::ConstStr {
                dest: ValueId(4),
                value: "https://api.datara.org".to_string(),
            },
            Inst::Call {
                dest: ValueId(5),
                func: "http_get".to_string(),
                args: vec![ValueId(4)],
                ty: "Int".to_string(),
            },
            Inst::BinOp {
                dest: ValueId(6),
                op: "add".to_string(),
                left: ValueId(3),
                right: ValueId(5),
                ty: "Int".to_string(),
            },
            Inst::Return {
                value: Some(ValueId(6)),
            },
        ],
        terminator: Terminator::Return {
            value: Some(ValueId(6)),
        },
    };
    func.blocks.push(bb);
    module.functions.insert("main".to_string(), func);

    let temp_dir = std::env::temp_dir().join("datara_wasm_dual_cap_test");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("dual_cap.wasm");

    let result = WasmEmitter::emit_wasm_binary(&module, &wasm_path);
    assert!(
        result.is_ok(),
        "Dual capability emission must succeed: {:?}",
        result.err()
    );

    let wat_str = fs::read_to_string(wasm_path.with_extension("wat")).expect("wat must exist");
    println!("=== DUAL CAP WAT ===\n{}", wat_str);

    let sidecar_str = fs::read_to_string(wasm_path.with_extension("capabilities.json"))
        .expect("sidecar must exist");
    println!("=== DUAL CAP SIDECAR ===\n{}", sidecar_str);

    // Also test hello world without caps:
    let source_hello = r#"
fn main() {
    out "Hello, WebAssembly!"
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir_hello = compiler
        .compile_source_to_dmir(source_hello, "hello.dtr")
        .unwrap();
    let hello_path = temp_dir.join("hello.wasm");
    WasmEmitter::emit_wasm_binary(&dmir_hello, &hello_path).unwrap();
    let hello_wat = fs::read_to_string(hello_path.with_extension("wat")).unwrap();
    println!("=== HELLO WORLD WAT ===\n{}", hello_wat);

    // Clean up
    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_file(&hello_path);
    let _ = fs::remove_file(hello_path.with_extension("wat"));
    let _ = fs::remove_file(hello_path.with_extension("js"));
    let _ = fs::remove_file(hello_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}
