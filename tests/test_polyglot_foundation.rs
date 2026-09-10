use forgen::driver::ForgenCompiler;

unsafe extern "C" {
    fn datara_rt_polyglot_foundation_test() -> i32;
}

#[test]
fn test_c_runtime_polyglot_foundation_harness() {
    let res = unsafe { datara_rt_polyglot_foundation_test() };
    assert_eq!(res, 1, "C runtime polyglot foundation test harness failed");
}

#[test]
fn test_ffi_roundtrip_flat_buffer_mutation_and_sum() {
    let source = r#"
extern fn datara_rt_test_list_f64_multiply(list: List<Float>, factor: Float) -> Int
extern fn datara_rt_test_list_f64_sum(list: List<Float>) -> Int

fn main() {
    let items = [1.0, 2.0, 3.0, 4.0]
    unsafe(justification: "Roundtrip FFI buffer mutation via DataraMemoryView") {
        datara_rt_test_list_f64_multiply(items, 3.0)
    }
    mut sum = 0
    unsafe(justification: "Roundtrip FFI buffer sum via DataraMemoryView") {
        sum = datara_rt_test_list_f64_sum(items)
    }
    out sum
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_ffi_buffer_roundtrip.dtr", None);
    assert!(
        res.success,
        "FFI flat buffer compilation failed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    // [1.0, 2.0, 3.0, 4.0] * 3.0 = [3.0, 6.0, 9.0, 12.0] -> sum = 30
    assert_eq!(stdout.trim(), "30", "Buffer sum output mismatch");

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}
