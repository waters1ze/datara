//! Phase 7 Test Suite: Network & I/O over Effects
//!
//! Validates:
//! 1. `examples/showcase/http_server`: Echo endpoint, JSON endpoints, and 20 consecutive deterministic runs
//! 2. `examples/showcase/file_io`: Capability boundary enforcement (unauthorized rejected with E0940, authorized succeeds)
//! 3. Execution of both showcases via project compilation and runtime verification

use forgen::driver::ForgenCompiler;
use std::path::Path;

#[test]
fn test_http_server_showcase_determinism_20_runs() {
    let compiler = ForgenCompiler::new("release");
    let project_path = Path::new("examples/showcase/http_server");
    assert!(project_path.exists(), "http_server project must exist");

    let comp_res = compiler.compile_project(project_path, None);
    assert!(
        comp_res.success,
        "Compilation of http_server failed: {:?}",
        comp_res.error
    );
    let exe = comp_res.exe_path.expect("http_server binary path");

    // First run to establish baseline output
    let (baseline_out, baseline_err, baseline_code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("Baseline run of http_server");

    assert_eq!(
        baseline_code, 0,
        "HTTP server showcase must exit with code 0"
    );
    assert!(
        baseline_out.contains("Verification: 100% OK"),
        "Must verify 100% OK: {}",
        baseline_out
    );
    assert!(
        baseline_out.contains("/echo"),
        "Must contain echo endpoint dispatch: {}",
        baseline_out
    );
    assert!(
        baseline_out.contains("/api/status"),
        "Must contain JSON status endpoint dispatch: {}",
        baseline_out
    );

    // 20 consecutive runs to guarantee byte-for-byte determinism
    for run_idx in 1..=20 {
        let (run_out, run_err, run_code, _) = compiler
            .codegen
            .run_executable(&exe, &[])
            .unwrap_or_else(|e| panic!("Run #{} failed: {}", run_idx, e));

        assert_eq!(run_code, 0);
        assert_eq!(
            baseline_out, run_out,
            "Run #{} output was not byte-for-byte identical to baseline!",
            run_idx
        );
        assert_eq!(
            baseline_err, run_err,
            "Run #{} stderr was not byte-for-byte identical to baseline!",
            run_idx
        );
    }
}

#[test]
fn test_file_io_capability_boundary() {
    let compiler = ForgenCompiler::new("release");

    // 1. Negative test: unauthorized file write without capability MUST fail with E0940
    let unauthorized_source = r#"
fn bad_write(path: String, data: String) {
    file_write(path, data)
}

fn main() {
    bad_write("target/leak.txt", "unauthorized")
}
"#;
    let bad_res = compiler.compile_source(unauthorized_source, "bad_io.dtr", None);
    assert!(
        !bad_res.success,
        "Unauthorized file_write without capability token must fail compilation"
    );
    assert!(
        bad_res.diagnostics.contains("E0940"),
        "Diagnostics must contain E0940 for unauthorized file_write: {}",
        bad_res.diagnostics
    );

    // 2. Negative test: unauthorized file read without capability MUST fail with E0940
    let unauthorized_read = r#"
fn bad_read(path: String) -> String {
    return file_read(path)
}

fn main() {
    let _ = bad_read("target/secret.txt")
}
"#;
    let bad_read_res = compiler.compile_source(unauthorized_read, "bad_read.dtr", None);
    assert!(
        !bad_read_res.success,
        "Unauthorized file_read without capability token must fail compilation"
    );
    assert!(
        bad_read_res.diagnostics.contains("E0940"),
        "Diagnostics must contain E0940 for unauthorized file_read: {}",
        bad_read_res.diagnostics
    );

    // 3. Positive test: run official showcase project with authorized SystemCapabilities
    let project_path = Path::new("examples/showcase/file_io");
    assert!(project_path.exists(), "file_io project must exist");

    let comp_res = compiler.compile_project(project_path, None);
    assert!(
        comp_res.success,
        "Compilation of file_io failed: {:?}",
        comp_res.error
    );
    let exe = comp_res.exe_path.expect("file_io binary path");

    let (out, _err, code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("Execution of file_io showcase");

    assert_eq!(code, 0, "file_io showcase must exit with 0");
    assert!(
        out.contains("Capability boundary verification: 100% PASS"),
        "file_io showcase must output verification pass: {}",
        out
    );
    assert!(
        out.contains("Data integrity: MATCH"),
        "file_io showcase must verify read-back data integrity: {}",
        out
    );
}
