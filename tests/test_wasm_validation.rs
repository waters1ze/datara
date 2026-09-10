//! Spec-compliant validation of emitted WebAssembly modules via `wasmparser`
//! (the Bytecode Alliance validator embedded in wasmtime).
//!
//! The in-crate `WasmEmitter::validate_wasm_binary` only checks the binary
//! envelope (magic, version, section ordering/lengths). These tests feed the
//! emitted modules through a real spec validator, which checks the type
//! section, import/export correctness, instruction stream encoding and stack
//! discipline of every function body — the same validation a Wasm runtime
//! performs at instantiation time. CI runs this suite on every push, so wasm
//! emission regressions are caught without requiring Node or wasmtime
//! binaries in the environment.

use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;

fn validate_with_wasmparser(bytes: &[u8]) -> Result<(), String> {
    let mut validator =
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::default());
    validator
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn emit_and_validate(name: &str, source: &str) {
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, &format!("{}.dtr", name))
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join(format!("datara_wasm_validation_{}", name));
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join(format!("{}.wasm", name));

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(
        result.is_ok(),
        "[{}] WASM emission failed: {:?}",
        name,
        result.err()
    );

    let wasm_bytes = fs::read(&wasm_path).expect("emitted .wasm must exist");

    // The shallow in-crate structural check must pass first so its error
    // message (envelope-level) is not masked by a deep validation failure.
    if let Err(e) = WasmEmitter::validate_wasm_binary(&wasm_bytes) {
        panic!("[{}] in-crate structural validation failed: {}", name, e);
    }

    if let Err(e) = validate_with_wasmparser(&wasm_bytes) {
        panic!(
            "[{}] wasmparser (wasmtime-equivalent) validation failed: {}\nWAT:\n{}",
            name,
            e,
            fs::read_to_string(wasm_path.with_extension("wat")).unwrap_or_default()
        );
    }

    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_file(wasm_path.with_extension("capabilities.json"));
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_wasmparser_minimal_arithmetic_module() {
    emit_and_validate(
        "val_minimal",
        r#"
fn main() -> Int {
    return 6 * 7
}
"#,
    );
}

#[test]
fn test_wasmparser_recursion_and_control_flow() {
    emit_and_validate(
        "val_fib",
        r#"
fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() -> Int {
    return fib(7)
}
"#,
    );
}

#[test]
fn test_wasmparser_loop_module() {
    emit_and_validate(
        "val_loop",
        r#"
fn main() -> Int {
    mut sum = 0
    mut i = 1
    while i <= 10 {
        sum = sum + i
        i = i + 1
    }
    return sum
}
"#,
    );
}

#[test]
fn test_wasmparser_runtime_imports_list_ops() {
    // Exercises the datara:rt import section (alloc/list builtins).
    emit_and_validate(
        "val_list",
        r#"
fn main() -> Int {
    let numbers = [10, 20, 30]
    let second = numbers[1]
    return second + 22
}
"#,
    );
}

#[test]
fn test_wasmparser_simd_v128_module() {
    // v128/f32x4 code paths must produce spec-valid SIMD instruction streams.
    emit_and_validate(
        "val_simd",
        r#"
fn main() -> Float {
    let a = float4(1.0, 2.0, 3.0, 4.0)
    let b = float4(4.0, 3.0, 2.0, 1.0)
    return dot(a, b)
}
"#,
    );
}

#[test]
fn test_wasmparser_string_out_module() {
    // Void main with a string out: exercises the datara:rt print import and
    // the string constant/data sections (the "hello world" shape).
    emit_and_validate(
        "val_hello",
        r#"
fn main() {
    out "hello"
}
"#,
    );
}

#[test]
fn test_wasmparser_dynamic_guarded_module() {
    let temp_dir = std::env::temp_dir().join("datara_wasm_validation_guarded");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("val_guarded.wasm");

    let compiler = ForgenCompiler::new("release");
    let source = r#"
fn dynamic_move(flag: Int, v: Int) -> Int {
    if flag > 0 {
        destroy(v)
    }
    out v
    return v
}

fn main() {
    let r = dynamic_move(0, 42)
    out fmt"Result: {r}"
}
"#;
    let dmir = compiler
        .compile_source_to_dmir(source, "val_guarded.dtr")
        .expect("DMIR lowering must succeed");

    let result = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(result.is_ok(), "WASM emission failed: {:?}", result.err());

    let wasm_bytes = fs::read(&wasm_path).expect("emitted .wasm must exist");
    assert!(
        validate_with_wasmparser(&wasm_bytes).is_ok(),
        "wasmparser validation must succeed"
    );

    // Verify imported ownership functions via wasmparser
    let mut found_acquire = false;
    let mut found_release = false;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm_bytes) {
        if let Ok(wasmparser::Payload::ImportSection(reader)) = payload {
            for imp in reader {
                let imp = imp.expect("Valid import");
                if imp.module == "datara:rt" && imp.name == "own_acquire" {
                    found_acquire = true;
                }
                if imp.module == "datara:rt" && imp.name == "own_release" {
                    found_release = true;
                }
            }
        }
    }
    assert!(found_acquire, "Must contain import datara:rt/own_acquire");
    assert!(found_release, "Must contain import datara:rt/own_release");

    // Verify sidecar declares datara:rt/own
    let sidecar_path = wasm_path.with_extension("capabilities.json");
    let sidecar_str = fs::read_to_string(&sidecar_path).expect("sidecar must exist");
    assert!(
        sidecar_str.contains("\"module\": \"datara:rt/own\""),
        "Sidecar must declare datara:rt/own module: {}",
        sidecar_str
    );
    assert!(
        sidecar_str.contains("own_acquire") && sidecar_str.contains("own_release"),
        "Sidecar must declare own_acquire and own_release functions: {}",
        sidecar_str
    );

    let _ = fs::remove_file(&wasm_path);
    let _ = fs::remove_file(&sidecar_path);
    let _ = fs::remove_file(wasm_path.with_extension("wat"));
    let _ = fs::remove_file(wasm_path.with_extension("js"));
    let _ = fs::remove_dir(&temp_dir);
}
