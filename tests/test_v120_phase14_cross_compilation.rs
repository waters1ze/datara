//! Phase 14 (v1.2.0): Cross-Compilation Test Suite
//!
//! Validates:
//! 1. Target triple parsing and target models for Linux (GNU/musl), macOS (ARM/Intel), Windows, WASM.
//! 2. LLVM IR emission target triple stamping for cross targets (`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`).
//! 3. Diagnostic code `[E0980]` (CrossCompilationMissingToolchain) in EN/RU locales.
//! 4. `docs/CROSS_COMPILE.md` documentation integrity.

use forgen::codegen::target::*;
use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;

#[test]
fn test_v120_cross_compilation_target_triples_and_models() {
    let targets = [
        (
            "x86_64-unknown-linux-gnu",
            Arch::X86_64,
            Os::Linux,
            Abi::Gnu,
            CallingConvention::SystemV,
        ),
        (
            "x86_64-unknown-linux-musl",
            Arch::X86_64,
            Os::Linux,
            Abi::Musl,
            CallingConvention::SystemV,
        ),
        (
            "aarch64-unknown-linux-gnu",
            Arch::Aarch64,
            Os::Linux,
            Abi::Gnu,
            CallingConvention::Aarch64Standard,
        ),
        (
            "aarch64-apple-darwin",
            Arch::Aarch64,
            Os::MacOS,
            Abi::SysV,
            CallingConvention::Aarch64Standard,
        ),
        (
            "x86_64-apple-darwin",
            Arch::X86_64,
            Os::MacOS,
            Abi::SysV,
            CallingConvention::SystemV,
        ),
        (
            "x86_64-pc-windows-msvc",
            Arch::X86_64,
            Os::Windows,
            Abi::Msvc,
            CallingConvention::WindowsFastcall,
        ),
        (
            "wasm32-unknown-unknown",
            Arch::Wasm32,
            Os::Unknown,
            Abi::Wasm,
            CallingConvention::WasmStandard,
        ),
    ];

    for (triple, arch, os, abi, call_conv) in targets {
        let info = TargetInfo::from_triple(triple).expect("parse triple");
        assert_eq!(info.arch, arch, "Arch mismatch for {}", triple);
        assert_eq!(info.os, os, "OS mismatch for {}", triple);
        assert_eq!(info.abi, abi, "ABI mismatch for {}", triple);
        assert_eq!(
            info.calling_convention, call_conv,
            "Call conv mismatch for {}",
            triple
        );
    }
}

#[test]
fn test_v120_cross_compilation_llvm_ir_emission() {
    let source = r#"
fn compute_value(x: Int) -> Int {
    return x * 10
}

fn main() {
    val res = compute_value(5)
    println(res)
}
"#;

    // Cross-compile to Linux
    let linux_compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_target(Some("x86_64-unknown-linux-gnu".into()));
    let linux_res = linux_compiler.compile_source(source, "cross_linux.dtr", None);
    assert!(
        linux_res.success,
        "Linux LLVM IR generation failed: {:?}",
        linux_res.error
    );
    let linux_llvm = linux_res.llvm_source.expect("Linux LLVM IR source");
    assert!(
        linux_llvm.contains("target triple = \"x86_64-unknown-linux-gnu\""),
        "Emitted LLVM IR must contain Linux target triple"
    );

    // Cross-compile to macOS Darwin
    let darwin_compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_target(Some("aarch64-apple-darwin".into()));
    let darwin_res = darwin_compiler.compile_source(source, "cross_darwin.dtr", None);
    assert!(
        darwin_res.success,
        "Darwin LLVM IR generation failed: {:?}",
        darwin_res.error
    );
    let darwin_llvm = darwin_res.llvm_source.expect("Darwin LLVM IR source");
    assert!(
        darwin_llvm.contains("target triple = \"arm64-apple-macosx\"")
            || darwin_llvm.contains("target triple = \"aarch64-apple-darwin\""),
        "Emitted LLVM IR must contain Darwin target triple"
    );
}

#[test]
fn test_v120_cross_compilation_e0980_diagnostic_code() {
    let code = ErrorCode::CrossCompilationMissingToolchain;
    assert_eq!(code.as_str(), "E0980");

    let desc_en = code.description("en");
    assert!(desc_en.contains("Cross-compilation toolchain not found"));

    let desc_ru = code.description("ru");
    assert!(desc_ru.contains("Отсутствует инструментарий кросс-компиляции"));
}

#[test]
fn test_v120_docs_cross_compile_markdown_integrity() {
    let doc_path = Path::new("docs/CROSS_COMPILE.md");
    assert!(doc_path.exists(), "docs/CROSS_COMPILE.md must exist");

    let text = fs::read_to_string(doc_path).expect("read CROSS_COMPILE.md");
    let required_snippets = [
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "wasm32-unknown-unknown",
        "E0980",
        "--tune=native",
    ];

    for snippet in required_snippets {
        assert!(
            text.contains(snippet),
            "docs/CROSS_COMPILE.md must contain '{}'",
            snippet
        );
    }
}
