use forgen::codegen::wasm::{CapabilitySidecar, WasmEmitter};
use forgen::driver::ForgenCompiler;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

fn parse_wasm_imports(bytes: &[u8]) -> BTreeMap<String, BTreeSet<String>> {
    let mut imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let parser = Parser::new(0);

    for payload in parser.parse_all(bytes) {
        if let Ok(Payload::ImportSection(import_sec)) = payload {
            for imp in import_sec.into_iter().flatten() {
                imports
                    .entry(imp.module.to_string())
                    .or_default()
                    .insert(imp.name.to_string());
            }
        }
    }
    imports
}

#[test]
fn audit_wasm_hello_world_has_zero_unauthorized_imports() {
    let source = r#"
fn main() {
    out "zero trust hello"
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "hello_audit.dtr")
        .unwrap();

    let temp_dir = std::env::temp_dir().join("audit_wasm_hw");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_file = temp_dir.join("hello.wasm");

    WasmEmitter::emit_wasm_binary(&dmir, &wasm_file).expect("emit wasm");
    let wasm_bytes = fs::read(&wasm_file).expect("read wasm");
    let _ = fs::remove_dir_all(&temp_dir);

    // Strict validator check
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    validator
        .validate_all(&wasm_bytes)
        .expect("Valid WASM binary");

    let imports = parse_wasm_imports(&wasm_bytes);
    for mod_name in imports.keys() {
        assert!(
            !mod_name.starts_with("datara:fs"),
            "Unauthorized fs import in hello world: {}",
            mod_name
        );
        assert!(
            !mod_name.starts_with("datara:net"),
            "Unauthorized net import in hello world: {}",
            mod_name
        );
        assert!(
            !mod_name.starts_with("datara:sys"),
            "Unauthorized sys import in hello world: {}",
            mod_name
        );
    }
}

#[test]
fn audit_wasm_file_read_capability_and_sidecar_bit_for_bit_match() {
    let source = r#"
fn read_cfg(path: String, token: Capability<FileRead>) -> String {
    let h = token.open(path)
    return h.read_all()
}
fn main(caps: SystemCapabilities) {
    let t = caps.files.grant_readonly("app.conf")
    out read_cfg("app.conf", t)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "caps_audit.dtr")
        .unwrap();

    let temp_dir = std::env::temp_dir().join("audit_wasm_caps_sidecar");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_file = temp_dir.join("caps.wasm");

    WasmEmitter::emit_wasm_binary(&dmir, &wasm_file).expect("emit wasm");
    let wasm_bytes = fs::read(&wasm_file).expect("read wasm");

    // 1. Physical inspection via wasmparser
    let imports = parse_wasm_imports(&wasm_bytes);
    let has_fs = imports.keys().any(|m| m.starts_with("datara:fs"));
    let has_net = imports.keys().any(|m| m.starts_with("datara:net"));
    let has_sys = imports.keys().any(|m| m.starts_with("datara:sys"));

    assert!(
        has_fs,
        "Capability<FileRead> module MUST physically import datara:fs! Imports: {:?}",
        imports
    );
    assert!(
        !has_net,
        "Module without net permission MUST NOT import datara:net"
    );
    assert!(
        !has_sys,
        "Module without sys permission MUST NOT import datara:sys"
    );

    // 2. Sidecar verification (bit-for-bit agreement)
    let sidecar_file = temp_dir.join("caps.capabilities.json");
    assert!(
        sidecar_file.exists(),
        "Sidecar capabilities.json MUST be emitted alongside WASM"
    );
    let sidecar_bytes = fs::read(&sidecar_file).expect("read sidecar");
    let sidecar: CapabilitySidecar =
        serde_json::from_slice(&sidecar_bytes).expect("deserialize sidecar");

    // Check sidecar vs observed capability imports
    for (module_name, funcs) in &imports {
        if module_name != "datara:rt" && module_name.starts_with("datara:") {
            let group = sidecar
                .granted_capabilities
                .iter()
                .find(|g| &g.module == module_name);
            assert!(
                group.is_some(),
                "Sidecar is missing observed module '{}'",
                module_name
            );
            let group = group.unwrap();
            for func in funcs {
                assert!(
                    group.functions.contains(func),
                    "Sidecar missing observed function '{}:{}'",
                    module_name,
                    func
                );
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn audit_wasm_no_mapped_call_bypass_pattern_in_source() {
    let wasm_emitter_source = concat!(
        include_str!("../../src/codegen/wasm/mod.rs"),
        include_str!("../../src/codegen/wasm/emit_inst.rs"),
        include_str!("../../src/codegen/wasm/emit_call.rs")
    );
    // Ensure no bypass comment or dummy mapped call stub replaces real calls
    assert!(
        !wasm_emitter_source.contains("mapped-call")
            && !wasm_emitter_source.contains("bypass_import"),
        "WASM emitter must not contain bypass/dummy mapped call patterns"
    );
}
