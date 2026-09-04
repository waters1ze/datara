//! Integration Tests for Flow Pipelines, UFCS, Val Promotion, Null Narrowing & FFI
//! Verifies |> flow, UFCS pipelines, val promotion, smart type narrowing, and extern "C" FFI.

use forgen::driver::ForgenCompiler;

fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, None);
    assert!(
        res.success,
        "compilation failed for {}: {:?}",
        name, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", name, code);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_pipeline_flow_stages() {
    let out = run_datara(
        r#"
fn step_one(x: Int) -> Int {
    return x * 2
}

fn step_two(x: Int) -> Int {
    return x + 5
}

fn main() {
    let res = 10 |> flow step_one |> flow step_two
    out res
}
"#,
        "test_flow_pipe",
    );
    assert_eq!(out, "25");
}

#[test]
fn test_pipeline_ufcs_chain() {
    let out = run_datara(
        r#"
fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn main() {
    let res = 20 |> add(5) |> add(10)
    out res
}
"#,
        "test_pipe_ufcs",
    );
    assert_eq!(out, "35");
}

#[test]
fn test_dynamic_val_promotion() {
    let out = run_datara(
        r#"
fn main() {
    val a = 100
    val b = 200
    out a + b
}
"#,
        "test_val_promote",
    );
    assert_eq!(out, "300");
}

#[test]
fn test_null_safety_smart_narrowing() {
    let out = run_datara(
        r#"
fn check_opt(opt: Int?) -> Int {
    if opt != None {
        return opt + 10
    }
    return 0
}

fn main() {
    mut x: Int? = 5
    out check_opt(x)
    mut y: Int? = None
    out check_opt(y)
}
"#,
        "test_smart_narrow",
    );
    assert_eq!(out, "15\n0");
}

#[test]
fn test_native_ffi_extern_c() {
    let out = run_datara(
        r#"
extern "C" fn datara_rt_now_ms() -> Int

fn main() {
    mut t: Int = 0
    unsafe(justification: "reading platform high-resolution hardware clock") {
        t = datara_rt_now_ms()
    }
    assert(t > 0, "native FFI timestamp must be positive")
    println("FFI OK")
}
"#,
        "test_native_ffi",
    );
    assert_eq!(out, "FFI OK");
}

#[test]
fn test_packet_bitfields() {
    let out = run_datara(
        r#"
packet Header {
    version: 4
    ihl: 4
}

fn main() {
    let h = Header { version: 4, ihl: 5 }
    out h.version
    out h.ihl
}
"#,
        "test_packet_bits",
    );
    assert_eq!(out, "4\n5");
}

#[test]
fn test_top_level_flow_rejected() {
    let compiler = ForgenCompiler::new("release");
    let source = r#"
flow process_stream(data: Int) -> Int {
    return data * 2
}

fn main() {
    out 42
}
"#;
    let res = compiler.compile_source(source, "test_flow_err.dtr", None);
    assert!(!res.success, "Top-level flow must be rejected!");
    let err_msg = res.error.unwrap_or_default();
    assert!(
        err_msg.contains("'flow' is not a top-level declaration"),
        "Expected top-level flow rejection, got: {}",
        err_msg
    );
}

#[test]
fn test_semicolons_optional() {
    let out = run_datara(
        r#"
fn main() {
    let a = 10;
    let b = 20;
    mut c = a + b;
    c = c * 2;
    out c;
}
"#,
        "test_semis",
    );
    assert_eq!(out, "60");
}
