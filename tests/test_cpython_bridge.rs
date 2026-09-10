use forgen::driver::ForgenCompiler;
use std::process::Command;

unsafe extern "C" {
    fn datara_py_self_test() -> i32;
    fn datara_py_last_error() -> *const std::ffi::c_char;
}

#[test]
fn test_cpython_runtime_self_test() {
    let code = unsafe { datara_py_self_test() };
    if code != 0 {
        let err_ptr = unsafe { datara_py_last_error() };
        let err_msg = if !err_ptr.is_null() {
            unsafe {
                std::ffi::CStr::from_ptr(err_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        } else {
            String::new()
        };
        panic!(
            "datara_py_self_test failed with exit code {}: {}",
            code, err_msg
        );
    }
}

#[test]
fn test_cpython_datara_eval_and_call() {
    let source = r#"
import python

fn main() {
    let py = Py { version: "3" }
    let r1 = py.eval_int("100 + 42")
    let r2 = py.eval_float("2.5 * 4.0")
    let r3 = py.call("math.sqrt", "[144.0]")
    if r1.is_ok() && r2.is_ok() && r3.is_ok() {
        out "INT: " + r1.unwrap()
        out "FLOAT: " + r2.unwrap()
        out "CALL: " + r3.unwrap()
    } else {
        out "ERR"
    }
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_py_eval_run.dtr", None);
    assert!(
        res.success,
        "Python eval compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("INT: 142"),
        "Expected INT: 142, got: {}",
        stdout
    );
    assert!(
        stdout.contains("FLOAT: 10"),
        "Expected FLOAT: 10, got: {}",
        stdout
    );
    assert!(
        stdout.contains("CALL: 12.0"),
        "Expected CALL: 12.0, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_cpython_datara_zerocopy_mutation() {
    let source = r#"
import python

fn main() {
    let py = Py { version: "3" }
    let buf = [10.0, 20.0, 30.0, 40.0]
    let bind_res = py.bind_buffer("arr", buf)
    if !bind_res.is_ok() {
        out "BIND_FAIL"
        return
    }

    let same_ptr = datara_py_assert_same_ptr("arr", buf)
    if same_ptr != 1 {
        out "PTR_MISMATCH"
        return
    }

    // Mutate buffer inside Python runtime
    py.exec("arr[0] = 777.0\narr[2] = 888.0\n")

    // Datara observes in-place mutation without any copy
    let v0 = buf[0]
    let v2 = buf[2]
    out "V0: " + v0
    out "V2: " + v2
    out "SAME_PTR: " + same_ptr
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_py_zerocopy_run.dtr", None);
    assert!(
        res.success,
        "Python zero-copy compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("SAME_PTR: 1"),
        "Expected SAME_PTR: 1, got: {}",
        stdout
    );
    assert!(
        stdout.contains("V0: 777"),
        "Expected V0: 777, got: {}",
        stdout
    );
    assert!(
        stdout.contains("V2: 888"),
        "Expected V2: 888, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_cpython_datara_error_traceback() {
    let source = r#"
import python

fn main() {
    let py = Py { version: "3" }
    let err_res = py.eval("10 / 0")
    if err_res.is_err() {
        out "ERR_OK: " + err_res.error_msg
    } else {
        out "UNEXPECTED_OK"
    }
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_py_err_run.dtr", None);
    assert!(
        res.success,
        "Python error test compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("ZeroDivisionError"),
        "Expected ZeroDivisionError traceback in output, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_cpython_dce_zero_cost() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int => a * 3 + b

fn main() {
    let val = compute(10, 5)
    out val
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_pure_py_dce.dtr", None);
    assert!(
        res.success,
        "Pure program compilation failed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert_eq!(stdout.trim(), "35");

    // Verify binary contains zero CPython symbols
    let bytes = std::fs::read(&exe).expect("Must read exe binary bytes");
    let contents = String::from_utf8_lossy(&bytes);

    let forbidden_patterns = [
        "python3.dll",
        "libpython3",
        "Py_Initialize",
        "PyGILState_Ensure",
        "datara_py_",
    ];

    for pat in &forbidden_patterns {
        assert!(
            !contents.contains(pat),
            "DCE Violation: Polyglot symbol/string '{}' found in pure Datara executable",
            pat
        );
    }

    if let Ok(spec) = forgen::codegen::linker::ensure_linker() {
        let dumpbin = spec.program.with_file_name("dumpbin.exe");
        if dumpbin.exists() {
            if let Ok(output) = Command::new(&dumpbin)
                .args(["/IMPORTS", &exe.to_string_lossy()])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for pat in &forbidden_patterns {
                    assert!(
                        !text.contains(pat),
                        "dumpbin /IMPORTS found forbidden polyglot reference: {}",
                        pat
                    );
                }
            }
        }
    }

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_cpython_use_python_syntax_and_effects() {
    let source = r#"
use python numpy as np

fn compute_with_py() {
    let py = Py { version: "3" }
    let res = py.eval("10 + 20")
    if res.is_ok() {
        out res.unwrap()
    }
}

fn main() {
    compute_with_py()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_py_effects.dtr", None);
    assert!(
        res.success,
        "use python numpy as np should compile cleanly: {:?}",
        res.diagnostics
    );

    let graph = res.semantic_graph.expect("Semantic graph must be produced");
    let opt = graph
        .inspect_optimization("compute_with_py")
        .expect("compute_with_py node must exist");

    // Check that effects contain Foreign and Nondeterministic
    let effects_str = opt.get("effects").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        effects_str.contains("Foreign"),
        "Expected Foreign effect, got: {}",
        effects_str
    );
    assert!(
        effects_str.contains("Nondeterministic"),
        "Expected Nondeterministic effect, got: {}",
        effects_str
    );

    // Check that isDeterministic is false
    let is_det = opt
        .get("isDeterministic")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(
        !is_det,
        "compute_with_py must be marked non-deterministic due to Python call"
    );

    // Check inlining fact reason indicates Foreign/nondeterministic
    let inlining_reason = opt
        .get("optimizationFacts")
        .and_then(|o| o.get("inlining"))
        .and_then(|i| i.get("reason"))
        .and_then(|r| r.as_str())
        .unwrap_or("");
    assert!(
        inlining_reason
            .contains("Foreign or non-deterministic call; must not be inlined or folded"),
        "Inlining reason must explain foreign call boundary: {}",
        inlining_reason
    );

    if let Some(ref exe) = res.exe_path {
        let _ = std::fs::remove_file(exe);
        let _ = std::fs::remove_file(exe.with_extension("obj"));
        let _ = std::fs::remove_file(exe.with_extension("pdb"));
    }
}

#[test]
fn test_cpython_numpy_zerocopy_and_benchmarks() {
    let source = r#"
use python numpy as np

fn main() {
    let py = Py { version: "3" }
    let buf = [1.5, 2.5, 3.5, 4.5]
    let bind_res = py.bind_buffer("np_arr", buf)
    if !bind_res.is_ok() {
        out "BIND_FAIL"
        return
    }

    let same_ptr = datara_py_assert_same_ptr("np_arr", buf)
    if same_ptr != 1 {
        out "PTR_MISMATCH"
        return
    }

    // In-place mutation through Python/NumPy buffer protocol
    py.exec("np_arr[1] = 99.25\nnp_arr[3] = 123.75\n")

    // Verify Datara sees the mutation in-place with zero copying
    let v1 = buf[1]
    let v3 = buf[3]
    out "MUT_V1: " + v1
    out "MUT_V3: " + v3
    out "NUMPY_ZEROCOPY_OK"
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_np_zerocopy_run.dtr", None);
    assert!(
        res.success,
        "NumPy zero-copy compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("MUT_V1: 99.25"),
        "Expected MUT_V1: 99.25, got: {}",
        stdout
    );
    assert!(
        stdout.contains("MUT_V3: 123.75"),
        "Expected MUT_V3: 123.75, got: {}",
        stdout
    );
    assert!(
        stdout.contains("NUMPY_ZEROCOPY_OK"),
        "Expected NUMPY_ZEROCOPY_OK, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}
