use forgen::driver::ForgenCompiler;
use std::fs;

fn run_datara(code: &str, tag: &str) -> (String, i32) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(code, tag, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        tag, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    (stdout.trim().replace("\r\n", "\n"), code)
}

#[test]
fn test_unauthorized_file_read_fails() {
    let code = r#"
fn steal_data(path: String) -> String {
    let handle = fs_open(path)
    return "stolen"
}

fn main() {
    let _ = steal_data("secrets.txt")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unauthorized_read.dtr", None);
    assert!(!res.success, "Unauthorized fs_open must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0940"),
        "Diagnostics must contain error code E0940: {}",
        diag
    );
    assert!(
        diag.contains("Operation 'fs_open' requires 'Capability<FileRead>'"),
        "Diagnostics must describe missing FileRead capability: {}",
        diag
    );
}

#[test]
fn test_unauthorized_file_write_fails() {
    let code = r#"
fn tamper_data(path: String) {
    fs_write(path, "malicious payload")
}

fn main() {
    tamper_data("system.cfg")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unauthorized_write.dtr", None);
    assert!(!res.success, "Unauthorized fs_write must fail compilation");
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0940"),
        "Diagnostics must contain error code E0940: {}",
        diag
    );
    assert!(
        diag.contains("Operation 'fs_write' requires 'Capability<FileWrite>'"),
        "Diagnostics must describe missing FileWrite capability: {}",
        diag
    );
}

#[test]
fn test_unauthorized_net_connect_fails() {
    let code = r#"
fn exfiltrate(host: String, port: Int) {
    net_connect(host, port)
}

fn main() {
    exfiltrate("198.51.100.1", 443)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unauthorized_net.dtr", None);
    assert!(
        !res.success,
        "Unauthorized net_connect must fail compilation"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0940"),
        "Diagnostics must contain error code E0940: {}",
        diag
    );
    assert!(
        diag.contains("Operation 'net_connect' requires 'Capability<NetworkConnect>'"),
        "Diagnostics must describe missing NetworkConnect capability: {}",
        diag
    );
}

#[test]
fn test_unauthorized_process_exec_fails() {
    let code = r#"
fn launch_backdoor(cmd: String) {
    proc_spawn(cmd)
}

fn main() {
    launch_backdoor("calc.exe")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "unauthorized_exec.dtr", None);
    assert!(
        !res.success,
        "Unauthorized proc_spawn must fail compilation"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains("E0940"),
        "Diagnostics must contain error code E0940: {}",
        diag
    );
    assert!(
        diag.contains("Operation 'proc_spawn' requires 'Capability<ProcessExec>'"),
        "Diagnostics must describe missing ProcessExec capability: {}",
        diag
    );
}

#[test]
fn test_authorized_capability_delegation_and_execution() {
    // Write temporary test file
    let tmp_path = "test_cap_target.txt";
    let _ = fs::write(tmp_path, "DATARA_SECURE_PAYLOAD_2026");

    let code = r#"
fn read_config(path: String, token: Capability<FileRead>) -> String {
    let handle = token.open(path)
    return handle.read_all()
}

fn main(sys_caps: SystemCapabilities) {
    let safe_token = sys_caps.files.grant_readonly("test_cap_target.txt")
    let content = read_config("test_cap_target.txt", safe_token)
    out content
}
"#;
    let (out, code_ret) = run_datara(code, "authorized_caps.dtr");
    let _ = fs::remove_file(tmp_path);

    assert_eq!(code_ret, 0);
    assert!(
        out.contains("DATARA_SECURE_PAYLOAD_2026"),
        "Output should contain content read via authorized capability: {:?}",
        out
    );
}

#[test]
fn test_zero_cost_capability_witness() {
    let code = r#"
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
    let res = compiler.compile_source(code, "zero_cost_caps.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    // Verify CLIF does not contain heap allocations for capability tokens
    let clif = res.clif_source.expect("must produce clif");
    assert!(
        !clif.contains("malloc_capability"),
        "Capability tokens must be zero-cost witnesses without heap allocation"
    );
}
