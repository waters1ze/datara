//! Phase 7 Test Suite: Ultra-Compact Embed Profile (СТОЛП 6)
//!
//! Validates:
//! 1. `--tiny` profile compilation, minimal size footprint, and clean execution.
//! 2. C ABI Header generation for classes, records, and structs with parameter type conversion.
//! 3. Embed runtime C API header (`datara_embed.h`) exposing `forgen_init`, `forgen_load_module`,
//!    `forgen_call_fn`, `forgen_shutdown`, `forgen_last_error`.
//! 4. In-process C ABI execution via `forgen::c_api`.
//! 5. `export_embed_package` generating native shared library + module header + embed runtime header.
//! 6. Documentation and provenance in `docs/EMBEDDING.md`, `docs/ENTERPRISE.md`, and `docs/data/binary_sizes.json`.

use forgen::c_api;
use forgen::driver::ForgenCompiler;
use forgen::export::{export_c_header, export_embed_header, export_embed_package};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

fn setup_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("datara_test_v120_p7_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    dir
}

#[test]
fn test_v120_embed_c_header_generation() {
    let dir = setup_test_dir("header_gen");
    let src_path = dir.join("game_types.dtr");
    let out_h = dir.join("game_types.h");
    let out_embed_h = dir.join("datara_embed.h");

    let dtr_code = r#"
record Vector3 {
    x: Float,
    y: Float,
    z: Float
}

struct PlayerState {
    health: Int,
    is_alive: Bool
}

pub fn add(a: Int, b: Int) -> Int {
    return a + b
}

pub fn dot(a: Vector3, b: Vector3) -> Float {
    return (a.x * b.x) + (a.y * b.y) + (a.z * b.z)
}
"#;
    fs::write(&src_path, dtr_code).expect("write src");

    let h_res = export_c_header(&src_path, &out_h).expect("export_c_header");
    assert!(h_res.exists());
    let h_content = fs::read_to_string(&out_h).expect("read header");

    // Verify struct Vector3
    assert!(
        h_content
            .contains("typedef struct {\n    double x;\n    double y;\n    double z;\n} Vector3;"),
        "Header must contain typedef struct Vector3 with double fields, got:\n{}",
        h_content
    );

    // Verify struct PlayerState
    assert!(
        h_content
            .contains("typedef struct {\n    int64_t health;\n    bool is_alive;\n} PlayerState;"),
        "Header must contain typedef struct PlayerState, got:\n{}",
        h_content
    );

    // Verify function signatures
    assert!(
        h_content.contains("DATARA_API int64_t add(int64_t a, int64_t b);"),
        "Header must contain C-typed add function signature, got:\n{}",
        h_content
    );
    assert!(
        h_content.contains("DATARA_API double dot(Vector3 a, Vector3 b);"),
        "Header must contain C-typed dot function signature, got:\n{}",
        h_content
    );

    // Verify datara_embed.h
    let embed_res = export_embed_header(&out_embed_h).expect("export_embed_header");
    assert!(embed_res.exists());
    let embed_content = fs::read_to_string(&out_embed_h).expect("read embed header");
    assert!(embed_content.contains("forgen_init"));
    assert!(embed_content.contains("forgen_load_module"));
    assert!(embed_content.contains("forgen_call_fn"));
    assert!(embed_content.contains("forgen_shutdown"));
    assert!(embed_content.contains("forgen_last_error"));
}

#[test]
fn test_v120_embed_in_process_c_api() {
    let dir = setup_test_dir("c_api_inprocess");
    let src_path = dir.join("calculator.dtr");

    let dtr_code = r#"
fn multiply(a: Int, b: Int) -> Int {
    return a * b
}

fn sum_three(a: Int, b: Int, c: Int) -> Int {
    return a + b + c
}
"#;
    fs::write(&src_path, dtr_code).expect("write src");

    // 1. Initialize runtime
    let init_status = c_api::forgen_init();
    assert_eq!(init_status, 0, "forgen_init must return 0");

    // 2. Load module
    let path_c = CString::new(src_path.to_str().unwrap()).unwrap();
    let load_status = unsafe { c_api::forgen_load_module(path_c.as_ptr()) };
    assert_eq!(load_status, 0, "forgen_load_module must succeed");

    // 3. Call multiply(6, 7) -> 42
    let fn_mul = CString::new("multiply").unwrap();
    let args_mul = [6i64, 7i64];
    let mut res_mul = 0i64;
    let call_status = unsafe {
        c_api::forgen_call_fn(
            fn_mul.as_ptr(),
            args_mul.as_ptr(),
            args_mul.len(),
            &mut res_mul as *mut i64,
        )
    };
    assert_eq!(call_status, 0, "forgen_call_fn multiply must succeed");
    assert_eq!(res_mul, 42, "multiply(6, 7) must equal 42");

    // 4. Call sum_three(10, 20, 30) -> 60
    let fn_sum = CString::new("sum_three").unwrap();
    let args_sum = [10i64, 20i64, 30i64];
    let mut res_sum = 0i64;
    let call_sum_status = unsafe {
        c_api::forgen_call_fn(
            fn_sum.as_ptr(),
            args_sum.as_ptr(),
            args_sum.len(),
            &mut res_sum as *mut i64,
        )
    };
    assert_eq!(call_sum_status, 0, "forgen_call_fn sum_three must succeed");
    assert_eq!(res_sum, 60, "sum_three(10, 20, 30) must equal 60");

    // 5. Call non-existent function -> error code -1
    let fn_invalid = CString::new("non_existent").unwrap();
    let mut res_invalid = 0i64;
    let call_err_status = unsafe {
        c_api::forgen_call_fn(
            fn_invalid.as_ptr(),
            std::ptr::null(),
            0,
            &mut res_invalid as *mut i64,
        )
    };
    assert_eq!(call_err_status, -1, "invalid call must return -1");
    let err_ptr = c_api::forgen_last_error();
    assert!(!err_ptr.is_null());
    let err_str = unsafe { std::ffi::CStr::from_ptr(err_ptr).to_str().unwrap() };
    assert!(
        err_str.contains("not found"),
        "Error message must mention 'not found', got: {}",
        err_str
    );

    // 6. Shutdown runtime
    let shut_status = c_api::forgen_shutdown();
    assert_eq!(shut_status, 0, "forgen_shutdown must return 0");
}

#[test]
fn test_v120_embed_tiny_profile_binary_execution() {
    let dir = setup_test_dir("tiny_profile");
    let src_path = dir.join("main.dtr");
    let exe_path = dir.join(if cfg!(windows) {
        "app_tiny.exe"
    } else {
        "app_tiny"
    });

    let code = r#"
fn compute_val() -> Int {
    mut acc = 0
    mut i = 1
    while i <= 100 {
        acc = acc + i
        i = i + 1
    }
    return acc
}

fn main() {
    let v = compute_val()
    println(v)
}
"#;
    fs::write(&src_path, code).expect("write src");

    let compiler = ForgenCompiler::new("tiny").with_llvm(true);
    let res = compiler.compile_file(&src_path, Some(&exe_path));
    assert!(
        res.success,
        "Tiny profile compilation must succeed: {:?}",
        res.error
    );
    assert!(exe_path.exists(), "Tiny executable must be produced");

    // Execute tiny binary
    let output = std::process::Command::new(&exe_path)
        .output()
        .expect("run tiny exe");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(stdout, "5050");

    let size = fs::metadata(&exe_path).expect("metadata").len();
    println!(
        "[Tiny Profile] Generated binary size: {} bytes ({:.1} KB)",
        size,
        size as f64 / 1024.0
    );
    assert!(size > 0);
}

#[test]
fn test_v120_embed_export_embed_package() {
    let dir = setup_test_dir("embed_pkg");
    let src_path = dir.join("physics_core.dtr");
    let out_dir = dir.join("dist");

    let code = r#"
record Particle {
    mass: Float,
    speed: Float
}

pub fn kinetic_energy(p: Particle) -> Float {
    return 0.5 * p.mass * p.speed * p.speed
}

pub fn clamp_speed(s: Float, max_s: Float) -> Float {
    if s > max_s {
        return max_s
    }
    return s
}
"#;
    fs::write(&src_path, code).expect("write src");

    let pkg_res = export_embed_package(&src_path, &out_dir, "tiny");
    assert!(
        pkg_res.is_ok(),
        "export_embed_package must succeed: {:?}",
        pkg_res.err()
    );

    let (lib_path, mod_h, embed_h) = pkg_res.unwrap();
    assert!(
        lib_path.exists(),
        "Shared library must exist: {}",
        lib_path.display()
    );
    assert!(
        mod_h.exists(),
        "Module header must exist: {}",
        mod_h.display()
    );
    assert!(
        embed_h.exists(),
        "Embed runtime header must exist: {}",
        embed_h.display()
    );

    let mod_h_content = fs::read_to_string(&mod_h).expect("read mod header");
    assert!(mod_h_content.contains("Particle"));
    assert!(mod_h_content.contains("kinetic_energy"));
    assert!(mod_h_content.contains("clamp_speed"));

    let embed_h_content = fs::read_to_string(&embed_h).expect("read embed header");
    assert!(embed_h_content.contains("forgen_call_fn"));
}

#[test]
fn test_v120_embed_documentation_and_provenance() {
    let embed_doc = Path::new("docs/EMBEDDING.md");
    assert!(embed_doc.exists(), "docs/EMBEDDING.md must exist");
    let embed_txt = fs::read_to_string(embed_doc).expect("read docs/EMBEDDING.md");
    assert!(embed_txt.contains("Unreal Engine"));
    assert!(embed_txt.contains("Godot 4"));
    assert!(embed_txt.contains("Raylib"));
    assert!(embed_txt.contains("datara_embed.h"));
    assert!(embed_txt.contains("--tiny"));

    let ent_doc = Path::new("docs/ENTERPRISE.md");
    assert!(ent_doc.exists(), "docs/ENTERPRISE.md must exist");
    let ent_txt = fs::read_to_string(ent_doc).expect("read docs/ENTERPRISE.md");
    assert!(ent_txt.contains("ABI Stability Guarantees"));
    assert!(ent_txt.contains("Containerization"));

    let sizes_json = Path::new("docs/data/binary_sizes.json");
    assert!(
        sizes_json.exists(),
        "docs/data/binary_sizes.json must exist"
    );
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sizes_json).expect("read sizes")).unwrap();
    let results = json.get("results").unwrap();
    assert!(
        results.get("Datara-Tiny").is_some(),
        "Datara-Tiny must be in binary_sizes.json"
    );
    assert!(
        results.get("Datara-Embed-SharedLib").is_some(),
        "Datara-Embed-SharedLib must be in binary_sizes.json"
    );
}
