//! Datara & Forgen v1.2.0 - Phase 16: Hardware/Runtime Capability Traps & Auto-PGO Loop Closure

use forgen::driver::ForgenCompiler;
use forgen::pgo::ProfileData;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CAP_FULL_ACCESS_SOURCE: &str = r#"
fn main(caps: SystemCapabilities) {
    val path = "target/v120_cap_test_file.txt"
    val content = "secure_datara_payload"
    file_write(path, content)
    val read_back = file_read(path)
    out read_back
}
"#;

const CAP_REVOKE_WRITE_SOURCE: &str = r#"
fn main(caps: SystemCapabilities) {
    val path = "target/v120_cap_write_blocked.txt"
    // Revoke FS_WRITE (0x02)
    cap_revoke(2)
    // This call must trigger the hardware capability trap
    file_write(path, "malicious_attempt")
}
"#;

const CAP_REVOKE_READ_SOURCE: &str = r#"
fn main(caps: SystemCapabilities) {
    val path = "target/v120_cap_read_blocked.txt"
    file_write(path, "readable_data")
    // Revoke FS_READ (0x01)
    cap_revoke(1)
    // This call must trigger the hardware capability trap
    val r = file_read(path)
    out r
}
"#;

const CAP_SANDBOX_SOURCE: &str = r#"
fn main(caps: SystemCapabilities) {
    val path = "target/v120_cap_sandbox_blocked.txt"
    // In DATARA_SANDBOX mode, FS_WRITE is prohibited by default
    file_write(path, "sandbox_write_attempt")
}
"#;

#[test]
fn test_v120_capability_lattice_full_access() {
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(CAP_FULL_ACCESS_SOURCE, "cap_full_access.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed: {:?}",
        res.diagnostics
    );

    let exe = res.exe_path.expect("Executable must be generated");
    let output = Command::new(&exe).output().expect("Executable must run");

    assert!(
        output.status.success(),
        "Execution must succeed with full capabilities"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("secure_datara_payload"),
        "Stdout must contain written text, got: {}",
        stdout
    );
}

#[test]
fn test_v120_capability_lattice_revoke_write_trap() {
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(CAP_REVOKE_WRITE_SOURCE, "cap_revoke_write.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed: {:?}",
        res.diagnostics
    );

    let exe = res.exe_path.expect("Executable must be generated");
    let output = Command::new(&exe).output().expect("Executable must run");

    // The execution must have failed (trapped)
    assert!(
        !output.status.success(),
        "Process must be terminated by capability trap"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[DATARA HARDWARE CAPABILITY TRAP]"),
        "Stderr must contain trap warning, got: {}",
        stderr
    );
    assert!(
        stderr.contains("fs::write"),
        "Stderr must mention operation name fs::write, got: {}",
        stderr
    );
}

#[test]
fn test_v120_capability_lattice_revoke_read_trap() {
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(CAP_REVOKE_READ_SOURCE, "cap_revoke_read.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed: {:?}",
        res.diagnostics
    );

    let exe = res.exe_path.expect("Executable must be generated");
    let output = Command::new(&exe).output().expect("Executable must run");

    assert!(
        !output.status.success(),
        "Process must be terminated by capability trap on file_read"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[DATARA HARDWARE CAPABILITY TRAP]"),
        "Stderr must contain trap warning, got: {}",
        stderr
    );
    assert!(
        stderr.contains("fs::read"),
        "Stderr must mention operation name fs::read, got: {}",
        stderr
    );
}

#[test]
fn test_v120_capability_lattice_sandbox_env_trap() {
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(CAP_SANDBOX_SOURCE, "cap_sandbox.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed: {:?}",
        res.diagnostics
    );

    let exe = res.exe_path.expect("Executable must be generated");
    let output = Command::new(&exe)
        .env("DATARA_SANDBOX", "1")
        .output()
        .expect("Executable must run");

    assert!(
        !output.status.success(),
        "Process must be terminated by sandbox capability trap"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[DATARA HARDWARE CAPABILITY TRAP]"),
        "Stderr must contain trap warning, got: {}",
        stderr
    );
    assert!(
        stderr.contains("fs::write"),
        "Stderr must mention operation name fs::write, got: {}",
        stderr
    );
}

#[test]
fn test_v120_auto_pgo_llvm_metadata_closure() {
    let test_dir = PathBuf::from("target/v120_auto_pgo_meta_test");
    let _ = fs::create_dir_all(&test_dir);
    let prof_path = test_dir.join("app.profdata");

    // Synthesize verified runtime profile with hot function and loop trip counts
    let mut profile = ProfileData::new("auto_pgo_test");
    profile.source = "runtime".to_string();
    profile.hot_functions.insert("work_loop".to_string(), 1200);
    profile
        .hot_functions
        .insert("process_item".to_string(), 1200);
    profile
        .loop_trip_counts
        .insert("work_loop_1".to_string(), 100);
    profile.save_to_file(&prof_path).expect("Profile must save");

    let source = r#"
fn work_loop(x: Int) -> Int {
    mut s = x
    mut i = 0
    while i < 100 {
        s = (s * 31 + i) % 1000007
        i = i + 1
    }
    return s
}

fn process_item(x: Int) -> Int {
    return work_loop(x)
}

fn main() {
    val n = now_ms() % 10 + 1
    val r = process_item(n)
    out r
}
"#;

    let compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_pgo(Some(prof_path));

    let res = compiler.compile_source(source, "auto_pgo_meta.dtr", None);
    assert!(
        res.success,
        "Compilation must succeed: {:?}",
        res.diagnostics
    );

    let ir = res.llvm_source.expect("LLVM IR must be present");
    assert!(
        ir.contains("section \".text.hot\"") || ir.contains("hot"),
        "LLVM IR must mark work_loop as hot: {}",
        ir
    );
    assert!(
        ir.contains("!\"function_entry_count\""),
        "LLVM IR must emit function entry count metadata: {}",
        ir
    );
    assert!(
        ir.contains("!\"llvm.loop.unroll.enable\""),
        "LLVM IR must emit loop metadata: {}",
        ir
    );
}
