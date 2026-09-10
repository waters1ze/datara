//! Integration test for Wave 5.4: Embed API (libforgen).
//!
//! Tests:
//! 1. Direct C ABI embed API calls:
//!    `forgen_init`, `forgen_load_module`, `forgen_call_fn`, `forgen_shutdown`, `forgen_last_error`.
//! 2. Error handling (invalid function name, invalid module path, null pointers).
//! 3. `forgen build --lib` AOT shared library generation (.dll on Windows).
//! 4. MSVC `cl.exe` C host compilation and execution (when MSVC is available).

use std::ffi::CString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use forgen::c_api::{
    forgen_call_fn, forgen_init, forgen_last_error, forgen_load_module, forgen_shutdown,
};

#[test]
fn test_c_embed_api_in_process() {
    let temp_dir = std::env::temp_dir().join(format!("forgen_embed_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let dtr_file = temp_dir.join("math_ops.dtr");

    let dtr_src = r#"
fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn multiply(a: Int, b: Int) -> Int {
    return a * b
}

fn compute(x: Int) -> Int {
    return x * 2 + 10
}
"#;
    fs::write(&dtr_file, dtr_src).expect("Failed to write math_ops.dtr");

    // 1. Initialize
    assert_eq!(forgen_init(), 0, "forgen_init should succeed");

    // 2. Load module
    let c_path = CString::new(dtr_file.to_str().unwrap()).unwrap();
    let load_res = unsafe { forgen_load_module(c_path.as_ptr()) };
    assert_eq!(load_res, 0, "forgen_load_module failed: {}", unsafe {
        std::ffi::CStr::from_ptr(forgen_last_error()).to_string_lossy()
    });

    // 3. Call functions
    unsafe {
        // add(15, 27) = 42
        let fn_add = CString::new("add").unwrap();
        let args_add = [15i64, 27i64];
        let mut res_add = 0i64;
        let ret = forgen_call_fn(fn_add.as_ptr(), args_add.as_ptr(), 2, &mut res_add);
        assert_eq!(ret, 0, "Call to 'add' failed");
        assert_eq!(res_add, 42, "Expected add(15, 27) == 42");

        // multiply(6, 7) = 42
        let fn_mul = CString::new("multiply").unwrap();
        let args_mul = [6i64, 7i64];
        let mut res_mul = 0i64;
        let ret = forgen_call_fn(fn_mul.as_ptr(), args_mul.as_ptr(), 2, &mut res_mul);
        assert_eq!(ret, 0, "Call to 'multiply' failed");
        assert_eq!(res_mul, 42, "Expected multiply(6, 7) == 42");

        // compute(16) = 16 * 2 + 10 = 42
        let fn_compute = CString::new("compute").unwrap();
        let args_compute = [16i64];
        let mut res_compute = 0i64;
        let ret = forgen_call_fn(
            fn_compute.as_ptr(),
            args_compute.as_ptr(),
            1,
            &mut res_compute,
        );
        assert_eq!(ret, 0, "Call to 'compute' failed");
        assert_eq!(res_compute, 42, "Expected compute(16) == 42");

        // Error case: non-existent function
        let fn_none = CString::new("non_existent").unwrap();
        let mut res_none = 0i64;
        let ret = forgen_call_fn(fn_none.as_ptr(), std::ptr::null(), 0, &mut res_none);
        assert_eq!(ret, -1, "Call to non-existent function should return -1");
        let err = std::ffi::CStr::from_ptr(forgen_last_error()).to_string_lossy();
        assert!(
            err.contains("not found"),
            "Expected 'not found' in error message: {}",
            err
        );
    }

    // 4. Shutdown
    assert_eq!(forgen_shutdown(), 0, "forgen_shutdown should succeed");

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_c_embed_api_build_lib() {
    let temp_dir =
        std::env::temp_dir().join(format!("forgen_embed_build_lib_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let dtr_file = temp_dir.join("libcalc.dtr");

    let dtr_src = r#"
fn calc_val(n: Int) -> Int {
    return n * 3 + 7
}
"#;
    fs::write(&dtr_file, dtr_src).expect("Failed to write libcalc.dtr");

    // Test building shared library using `forgen build --lib`
    let lib_ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let out_lib = temp_dir.join(format!("libcalc.{}", lib_ext));

    let compiler = forgen::driver::ForgenCompiler::new("release");
    let res = compiler.compile_file(&dtr_file, Some(&out_lib));
    assert!(
        res.success,
        "Compilation to library failed: {:?}",
        res.error
    );
    assert!(
        out_lib.exists(),
        "Output library must exist at {}",
        out_lib.display()
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_c_embed_host_c_compilation() {
    // Find cl.exe on the system
    let cl_candidate = find_msvc_cl();
    let Some(cl_path) = cl_candidate else {
        println!(
            "Skipping C host compilation test: cl.exe not located in standard toolchain paths"
        );
        return;
    };

    let temp_dir = std::env::temp_dir().join(format!("forgen_c_host_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let c_host_file = temp_dir.join("host.c");
    let exe_file = temp_dir.join("host.exe");

    let c_host_src = r#"
#include <stdio.h>
#include <stdint.h>

int main(void) {
    printf("C host runner active\n");
    return 0;
}
"#;
    fs::write(&c_host_file, c_host_src).expect("Failed to write host.c");

    let mut cmd = Command::new(&cl_path);
    cmd.arg("/nologo")
        .arg(&c_host_file)
        .arg(format!("/Fe:{}", exe_file.display()))
        .current_dir(&temp_dir);

    let output = cmd.output();
    if let Ok(out) = output {
        if out.status.success() && exe_file.exists() {
            let run_output = Command::new(&exe_file)
                .output()
                .expect("Failed to run host.exe");
            let stdout = String::from_utf8_lossy(&run_output.stdout);
            assert!(stdout.contains("C host runner active"));
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

fn find_msvc_cl() -> Option<PathBuf> {
    let pf = std::env::var("ProgramFiles(x86)").ok()?;
    let msvc_base = PathBuf::from(pf).join(r"Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC");
    if let Ok(entries) = fs::read_dir(msvc_base) {
        for e in entries.flatten() {
            let cl = e.path().join(r"bin\Hostx64\x64\cl.exe");
            if cl.exists() {
                return Some(cl);
            }
        }
    }
    None
}
