//! Integration test for Wave 5.6: DPM Rust Bridge generator.
//!
//! Verifies that `dpm rust-bridge <crate> --api manifest.toml`:
//! 1. Generates the Rust shim crate with catch_unwind and zero-copy view support.
//! 2. Compiles the cdylib and import library via cargo.
//! 3. Emits valid Datara binding modules (.dtr) and C headers (.h).

use std::fs;
use std::path::PathBuf;

#[test]
fn test_dpm_rust_bridge_regex() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let api_path = manifest_dir
        .join("examples")
        .join("showcase")
        .join("rust_bridge")
        .join("regex")
        .join("regex_api.toml");
    assert!(api_path.exists(), "regex_api.toml must exist");

    let temp_dir = std::env::temp_dir().join(format!("forgen_dpm_rb_regex_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    let args = vec![
        "dpm".to_string(),
        "rust-bridge".to_string(),
        "regex".to_string(),
        "--api".to_string(),
        api_path.to_string_lossy().to_string(),
        "--out-dir".to_string(),
        temp_dir.to_string_lossy().to_string(),
    ];

    let res = forgen::rust_bridge::run_rust_bridge_cli(&args);
    assert!(res.is_ok(), "dpm rust-bridge regex failed: {:?}", res);

    // Verify generated files
    assert!(
        temp_dir.join("Cargo.toml").exists(),
        "Cargo.toml must exist"
    );
    assert!(
        temp_dir.join("src").join("lib.rs").exists(),
        "src/lib.rs must exist"
    );
    assert!(
        temp_dir.join("regex_bridge.h").exists(),
        "regex_bridge.h must exist"
    );
    assert!(
        temp_dir.join("regex_bridge.dtr").exists(),
        "regex_bridge.dtr must exist"
    );

    let lib_rs = fs::read_to_string(temp_dir.join("src").join("lib.rs")).unwrap();
    assert!(
        lib_rs.contains("catch_unwind"),
        "lib.rs must contain panic safety barrier"
    );
    assert!(
        lib_rs.contains("pub extern \"C\" fn regex_is_match"),
        "lib.rs must export regex_is_match"
    );
    assert!(
        lib_rs.contains("pub extern \"C\" fn regex_find"),
        "lib.rs must export regex_find"
    );

    let dtr = fs::read_to_string(temp_dir.join("regex_bridge.dtr")).unwrap();
    assert!(
        dtr.contains("extern fn regex_is_match"),
        "dtr must declare regex_is_match"
    );
    assert!(
        dtr.contains("make_buffer_view"),
        "dtr must contain buffer view constructor"
    );

    let (cdylib_name, static_name) = if cfg!(windows) {
        ("regex_bridge.dll", "regex_bridge.lib")
    } else if cfg!(target_os = "macos") {
        ("libregex_bridge.dylib", "libregex_bridge.a")
    } else {
        ("libregex_bridge.so", "libregex_bridge.a")
    };

    assert!(
        temp_dir.join(cdylib_name).exists(),
        "cdylib {} must exist",
        cdylib_name
    );
    assert!(
        temp_dir.join(static_name).exists(),
        "static library {} must exist",
        static_name
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_dpm_rust_bridge_serde_json() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let api_path = manifest_dir
        .join("examples")
        .join("showcase")
        .join("rust_bridge")
        .join("serde_json")
        .join("serde_json_api.toml");
    assert!(api_path.exists(), "serde_json_api.toml must exist");

    let temp_dir = std::env::temp_dir().join(format!("forgen_dpm_rb_json_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    let args = vec![
        "dpm".to_string(),
        "rust-bridge".to_string(),
        "serde_json".to_string(),
        "--api".to_string(),
        api_path.to_string_lossy().to_string(),
        "--out-dir".to_string(),
        temp_dir.to_string_lossy().to_string(),
    ];

    let res = forgen::rust_bridge::run_rust_bridge_cli(&args);
    assert!(res.is_ok(), "dpm rust-bridge serde_json failed: {:?}", res);

    assert!(
        temp_dir.join("Cargo.toml").exists(),
        "Cargo.toml must exist"
    );
    assert!(
        temp_dir.join("serde_json_bridge.dtr").exists(),
        "dtr must exist"
    );

    let dtr = fs::read_to_string(temp_dir.join("serde_json_bridge.dtr")).unwrap();
    assert!(
        dtr.contains("extern fn json_get_string"),
        "dtr must declare json_get_string"
    );
    assert!(
        dtr.contains("extern fn json_get_int"),
        "dtr must declare json_get_int"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_dpm_rust_bridge_image_zero_copy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let api_path = manifest_dir
        .join("examples")
        .join("showcase")
        .join("rust_bridge")
        .join("image")
        .join("image_api.toml");
    assert!(api_path.exists(), "image_api.toml must exist");

    let temp_dir = std::env::temp_dir().join(format!("forgen_dpm_rb_img_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    let args = vec![
        "dpm".to_string(),
        "rust-bridge".to_string(),
        "image".to_string(),
        "--api".to_string(),
        api_path.to_string_lossy().to_string(),
        "--out-dir".to_string(),
        temp_dir.to_string_lossy().to_string(),
    ];

    let res = forgen::rust_bridge::run_rust_bridge_cli(&args);
    assert!(res.is_ok(), "dpm rust-bridge image failed: {:?}", res);

    assert!(
        temp_dir.join("Cargo.toml").exists(),
        "Cargo.toml must exist"
    );
    assert!(
        temp_dir.join("image_bridge.dtr").exists(),
        "image_bridge.dtr must exist"
    );

    let lib_rs = fs::read_to_string(temp_dir.join("src").join("lib.rs")).unwrap();
    assert!(
        lib_rs.contains("std::slice::from_raw_parts"),
        "lib.rs must contain zero-copy slice usage"
    );
    assert!(
        lib_rs.contains("pub extern \"C\" fn encode_rgb_png"),
        "lib.rs must export encode_rgb_png"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
