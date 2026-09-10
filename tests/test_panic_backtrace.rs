use forgen::codegen::cranelift::jit::datara_rt_print_backtrace;
use std::process::Command;

#[test]
fn test_runtime_print_backtrace() {
    unsafe {
        datara_rt_print_backtrace();
    }
}

#[test]
fn test_panic_subprocess_prints_backtrace() {
    if std::env::var("DATARA_TRIGGER_TEST_PANIC").is_ok() {
        unsafe {
            let msg = std::ffi::CString::new("test panic for backtrace").unwrap();
            forgen::codegen::cranelift::jit::datara_rt_panic(msg.as_ptr());
        }
        return;
    }

    let exe = std::env::current_exe().expect("current test exe");
    let output = Command::new(exe)
        .env("DATARA_TRIGGER_TEST_PANIC", "1")
        .arg("--exact")
        .arg("test_panic_subprocess_prints_backtrace")
        .output()
        .expect("failed to run child process");

    assert!(
        !output.status.success(),
        "Panicking process must exit with failure"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("panic: test panic for backtrace"),
        "stderr must contain panic message: {}",
        stderr
    );
    assert!(
        stderr.contains("stack backtrace:"),
        "stderr must contain 'stack backtrace:': {}",
        stderr
    );
}
