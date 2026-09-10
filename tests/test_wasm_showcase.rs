use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_wasm_page_showcase_compilation_and_execution() {
    let showcase_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("showcase")
        .join("wasm_page");
    let main_dtr = showcase_dir.join("main.dtr");
    let wasm_out = showcase_dir.join("main.wasm");
    let capabilities_json = showcase_dir.join("capabilities.json");
    assert!(main_dtr.exists());
    assert!(capabilities_json.exists());
    let compiler = ForgenCompiler::new("release");
    let dmir_mod = compiler
        .compile_file_to_dmir(&main_dtr)
        .expect("compile dmir");
    let emit_res = WasmEmitter::emit_wasm_binary(&dmir_mod, &wasm_out);
    assert!(emit_res.is_ok());
    let wasm_bytes = fs::read(&wasm_out).expect("read wasm");
    let val_res = WasmEmitter::validate_wasm_binary(&wasm_bytes);
    assert!(val_res.is_ok());
    let _ = fs::remove_file(&wasm_out);
    let _ = fs::remove_file(wasm_out.with_extension("wat"));
    let _ = fs::remove_file(wasm_out.with_extension("js"));
}
