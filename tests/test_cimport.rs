use forgen::driver::ForgenCompiler;

#[test]
#[cfg(windows)]
fn test_cimport_header_parsing_and_native_execution() {
    let source = r#"
import c "tests/fixtures/test_cimport.h" with link("kernel32.lib");

fn main() {
    let ok = TEST_OK()
    let running = STATUS_RUNNING()
    let buf_sz = BUFFER_SIZE()
    mut pid = 0
    unsafe(justification: "Calling Win32 GetCurrentProcessId from kernel32") {
        pid = GetCurrentProcessId()
    }
    if pid > 0 && ok == 0 && running == 200 && buf_sz == 1024 {
        out 42
    } else {
        out 0
    }
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_cimport_run.dtr", None);
    assert!(
        res.success,
        "CImport compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert_eq!(
        stdout.trim(),
        "42",
        "Expected output 42 from CImport program"
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_cimport_unsupported_construct_diagnostics() {
    let source = r#"
import c "tests/fixtures/test_unsupported.h";

fn main() {
    out 1
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.check_source(source, "test_unsupported_check.dtr");
    // Unsupported constructs in C headers must produce honest diagnostics with line/column
    assert!(
        res.diagnostics
            .contains("Unsupported C construct at line 2")
            || res.diagnostics.contains("line 2"),
        "Diagnostics must contain line/col of unsupported C construct. Got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_cimport_missing_header_diagnostic() {
    let source = r#"
import c "nonexistent_header_12345.h";

fn main() {
    out 1
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.check_source(source, "test_missing_header.dtr");
    assert!(!res.success, "Must fail on missing C header");
    assert!(
        res.diagnostics.contains("E0960") || res.diagnostics.contains("not found"),
        "Diagnostics must report missing C header (E0960). Got:\n{}",
        res.diagnostics
    );
}
