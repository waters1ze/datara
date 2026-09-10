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
fn audit_wasm_hello_world_has_zero_unauthorized_capability_imports() {
    let source = r#"
fn main() {
    out "hello zero trust"
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "audit_wasm_hello.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("audit_wasm_hello_sandbox");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_file = temp_dir.join("audit_hello.wasm");

    let res = WasmEmitter::emit_wasm_binary(&dmir, &wasm_file);
    assert!(res.is_ok(), "WASM emission must succeed");

    let wasm_bytes = fs::read(&wasm_file).expect("WASM file must exist");

    // Spec compliance validation via wasmparser
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    validator
        .validate_all(&wasm_bytes)
        .expect("Emitted WASM module must be strictly spec-compliant");

    let observed_imports = parse_wasm_imports(&wasm_bytes);
    println!(
        "Observed imports in hello-world WASM: {:?}",
        observed_imports
    );

    for mod_name in observed_imports.keys() {
        assert!(
            !mod_name.starts_with("datara:fs"),
            "CRITICAL SECURITY VIOLATION: Hello World binary physically contains unauthorized filesystem import '{}'!",
            mod_name
        );
        assert!(
            !mod_name.starts_with("datara:net"),
            "CRITICAL SECURITY VIOLATION: Hello World binary physically contains unauthorized network import '{}'!",
            mod_name
        );
        assert!(
            !mod_name.starts_with("datara:sys"),
            "CRITICAL SECURITY VIOLATION: Hello World binary physically contains unauthorized system import '{}'!",
            mod_name
        );
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn audit_wasm_file_read_capability_and_sidecar_bit_for_bit_agreement() {
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
        .compile_source_to_dmir(source, "audit_wasm_fs.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("audit_wasm_fs_sandbox");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_file = temp_dir.join("audit_fs.wasm");

    let res = WasmEmitter::emit_wasm_binary(&dmir, &wasm_file);
    assert!(
        res.is_ok(),
        "WASM emission must succeed for capability code"
    );

    let wasm_bytes = fs::read(&wasm_file).expect("WASM file must exist");

    // 1. Validator spec check
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    validator
        .validate_all(&wasm_bytes)
        .expect("Capability WASM module must pass wasmparser::Validator");

    // 2. Physical imports inspection
    let observed_imports = parse_wasm_imports(&wasm_bytes);
    println!("Observed capability imports: {:?}", observed_imports);

    // datara:fs MUST be present
    let has_fs = observed_imports.keys().any(|m| m.starts_with("datara:fs"));
    assert!(
        has_fs,
        "datara:fs MUST be present when Capability<FileRead> is requested"
    );

    // datara:net and datara:sys MUST NOT be present
    let has_net = observed_imports.keys().any(|m| m.starts_with("datara:net"));
    let has_sys = observed_imports.keys().any(|m| m.starts_with("datara:sys"));
    assert!(
        !has_net,
        "datara:net must be physically absent when not granted"
    );
    assert!(
        !has_sys,
        "datara:sys must be physically absent when not granted"
    );

    // 3. Sidecar bit-for-bit check
    let sidecar_file = wasm_file.with_extension("capabilities.json");
    assert!(
        sidecar_file.exists(),
        "Sidecar capabilities.json must be written"
    );

    let sidecar_content = fs::read_to_string(&sidecar_file).expect("Read sidecar json");
    let sidecar: CapabilitySidecar = serde_json::from_str(&sidecar_content)
        .expect("Sidecar must parse into CapabilitySidecar schema");

    // Verify sidecar declared functions match observed wasmparser imports exactly
    for group in &sidecar.granted_capabilities {
        let observed_funcs = observed_imports.get(&group.module).unwrap_or_else(|| {
            panic!(
                "P0 MISMATCH: Sidecar declares granted module '{}', but it is missing in WASM imports!",
                group.module
            )
        });

        for func in &group.functions {
            assert!(
                observed_funcs.contains(func),
                "P0 MISMATCH: Sidecar declares function '{}.{}', but it is NOT in WASM imports!",
                group.module,
                func
            );
        }
    }

    // Also check reverse: every imported function from capability modules must be in the sidecar
    for (mod_name, funcs) in &observed_imports {
        if mod_name.starts_with("datara:fs")
            || mod_name.starts_with("datara:net")
            || mod_name.starts_with("datara:sys")
        {
            let sidecar_group = sidecar
                .granted_capabilities
                .iter()
                .find(|g| g.module == *mod_name)
                .unwrap_or_else(|| {
                    panic!(
                        "P0 MISMATCH: WASM physically imports '{}', but sidecar DOES NOT declare it!",
                        mod_name
                    )
                });
            for f in funcs {
                assert!(
                    sidecar_group.functions.contains(f),
                    "P0 MISMATCH: WASM imports '{}.{}', but sidecar omits it!",
                    mod_name,
                    f
                );
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
