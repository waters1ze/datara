//! Phase 13 (v1.2.0): Cross-Platform & Auto-Multiversioning Test Suite
//!
//! Validates:
//! 1. Target triple parsing and cross-platform target models (x86_64, AArch64, WASM, Linux, Windows, macOS).
//! 2. Auto-multiversioning variant selection (FastAvx2, FastNeon, Generic).
//! 3. WASM backend emission and spec-compliant validation via wasmparser.
//! 4. `--tune=native` compilation flag and native feature execution.

use forgen::codegen::target::*;
use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_v120_target_triple_cross_platform_mappings() {
    let triples = [
        (
            "x86_64-pc-windows-msvc",
            Arch::X86_64,
            Os::Windows,
            Abi::Msvc,
        ),
        (
            "x86_64-unknown-linux-gnu",
            Arch::X86_64,
            Os::Linux,
            Abi::Gnu,
        ),
        (
            "x86_64-unknown-linux-musl",
            Arch::X86_64,
            Os::Linux,
            Abi::Musl,
        ),
        (
            "aarch64-unknown-linux-gnu",
            Arch::Aarch64,
            Os::Linux,
            Abi::Gnu,
        ),
        ("aarch64-apple-darwin", Arch::Aarch64, Os::MacOS, Abi::SysV),
        (
            "wasm32-unknown-unknown",
            Arch::Wasm32,
            Os::Unknown,
            Abi::Wasm,
        ),
    ];

    for (triple, exp_arch, exp_os, exp_abi) in triples {
        let info = TargetInfo::from_triple(triple)
            .unwrap_or_else(|e| panic!("Failed to parse triple {}: {}", triple, e));
        assert_eq!(info.arch, exp_arch, "Arch mismatch for {}", triple);
        assert_eq!(info.os, exp_os, "OS mismatch for {}", triple);
        assert_eq!(info.abi, exp_abi, "ABI mismatch for {}", triple);

        if exp_arch == Arch::Wasm32 {
            assert_eq!(info.pointer_width, 32);
        } else {
            assert_eq!(info.pointer_width, 64);
        }
    }

    // Native target verification
    let native = TargetInfo::native();
    assert!(native.is_native());
    assert!(native.pointer_width == 64);
}

#[test]
fn test_v120_auto_multiversioning_and_vector_variants() {
    // x86_64 with AVX2
    let x86_target = TargetInfo::x86_64_windows();
    assert!(x86_target.has_avx2());
    assert_eq!(
        x86_target.select_version_variant(),
        VersionVariant::FastAvx2
    );

    // AArch64 with NEON
    let aarch64_target = TargetInfo::aarch64_linux();
    assert!(aarch64_target.has_neon());
    assert_eq!(
        aarch64_target.select_version_variant(),
        VersionVariant::FastNeon
    );

    // Generic x86 (SSE2 only)
    let generic_target = TargetInfo::generic_x86_64(Os::Linux, Abi::Gnu);
    assert!(!generic_target.has_avx2());
    assert_eq!(
        generic_target.select_version_variant(),
        VersionVariant::Generic
    );

    // WASM (no vector extensions)
    let wasm_target = TargetInfo::wasm32();
    assert_eq!(
        wasm_target.select_version_variant(),
        VersionVariant::Generic
    );
}

#[test]
fn test_v120_wasm_backend_spec_compliant_validation() {
    let source = r#"
fn calculate_score(a: Int, b: Int) -> Int {
    return (a * 3) + (b * 7)
}

fn main() {
    let res = calculate_score(10, 20)
    println(res)
}
"#;

    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "v120_wasm_val.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("v120_wasm_val");
    let _ = fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("v120_wasm_val.wasm");

    let res = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
    assert!(res.is_ok(), "WASM emission must succeed: {:?}", res.err());

    let wasm_bytes = fs::read(&wasm_path).expect("emitted .wasm file must exist");
    // 1. In-crate structural validation
    WasmEmitter::validate_wasm_binary(&wasm_bytes)
        .expect("In-crate structural validation must succeed");

    // 2. Strict wasmparser spec validation
    let mut validator =
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::default());
    validator
        .validate_all(&wasm_bytes)
        .expect("Strict wasmparser spec validation must pass");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_v120_native_tuning_and_end_to_end_execution() {
    let source = r#"
fn vector_dot_product(len: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < len {
        sum = sum + (i * 2)
        i = i + 1
    }
    return sum
}

fn main() {
    val res = vector_dot_product(50)
    println(res)
}
"#;

    // Compile with native tune flag
    let compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_native(true);
    let res = compiler.compile_source(source, "v120_native_tune.dtr", None);
    assert!(
        res.success,
        "Native compilation with --tune=native must succeed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Executable must exist");
    for run in 1..=3 {
        let (stdout, stderr, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
        assert_eq!(code, 0, "Execution run {} failed: {}", run, stderr);
        // sum_{i=0..49} (i*2) = 2 * (49*50/2) = 2450
        assert_eq!(stdout.trim(), "2450", "Result mismatch on run {}", run);
    }
}
