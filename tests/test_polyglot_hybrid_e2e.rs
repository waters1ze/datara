use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn get_rust_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fixture_rust_crate")
}

fn get_c_fixture_header() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("test_cimport.h")
}

#[test]
#[cfg(windows)]
fn test_polyglot_hybrid_e2e_all_languages() {
    let temp_dir = std::env::temp_dir().join(format!("datara_hybrid_e2e_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).expect("Must create temp src dir");

    let rust_fixture_path = get_rust_fixture_path();
    let rust_path_str = rust_fixture_path.to_string_lossy().replace('\\', "/");

    let c_fixture_header = get_c_fixture_header();
    let c_path_str = c_fixture_header.to_string_lossy().replace('\\', "/");

    let toml_content = format!(
        r#"[package]
name = "hybrid_polyglot_app"
version = "0.1.0"
edition = "2026"
entry = "src/main.dtr"

[dependencies.rust.fixture_rust_crate]
path = "{}"
functions = [
    "fn add(a: Int, b: Int) -> Int",
    "fn mult(a: Int, b: Int) -> Int"
]
"#,
        rust_path_str
    );

    fs::write(temp_dir.join("datara.toml"), &toml_content).expect("Must write datara.toml");

    let source = format!(
        r#"
import c "{}" with link("kernel32.lib");
import js
import python

fn main() {{
    // 1. C FFI via import c
    let c_ok = TEST_OK()
    let c_buf = BUFFER_SIZE()
    mut pid = 0
    unsafe(justification: "Calling Win32 GetCurrentProcessId via C import") {{
        pid = GetCurrentProcessId()
    }}
    out "C_OK: " + c_ok
    out "C_BUF: " + c_buf
    if pid > 0 {{
        out "C_PID_VALID: true"
    }}

    // 2. Rust crate bridge via [dependencies.rust]
    mut r_sum = 0
    mut r_prod = 0
    unsafe(justification: "Calling native Rust crate functions via staticlib wrapper") {{
        r_sum = add(20, 22)
        r_prod = mult(6, 7)
    }}
    out "RUST_SUM: " + r_sum
    out "RUST_PROD: " + r_prod

    // 3. JavaScript / Node-API engine via import js (JSON transform)
    let js = JS {{ version: "ES2022" }}
    let js_trans = js.eval("const item = JSON.parse('{{\"base\": 40, \"delta\": 2}}'); JSON.stringify({{ result: item.base + item.delta, engine: 'node-api' }});")
    out "JS_TRANS: " + js_trans

    // 4. Python dynamic bridge via import python
    let py = Py {{ version: "3" }}
    let py_eval = py.eval_int("21 * 2")
    if py_eval.is_success {{
        out "PY_STATUS: available"
        out "PY_VAL: " + py_eval.value
        let np_res = py.eval_int("try:\n    import numpy as np\n    a = np.array([[1, 2], [3, 4]])\n    b = np.array([[5, 6], [7, 8]])\n    c = np.matmul(a, b)\n    int(c[0][0])\nexcept Exception:\n    -1\n")
        if np_res.is_success && np_res.value > 0 {{
            out "PY_NUMPY_MATMUL: " + np_res.value
        }} else {{
            out "PY_NUMPY_MATMUL: skipped (numpy not present in host environment)"
        }}
    }} else {{
        out "PY_STATUS: skipped (host python3.dll not available)"
    }}

    out "HYBRID_POLYGLOT_SUCCESS: true"
}}
"#,
        c_path_str
    );

    let main_dtr = temp_dir.join("src").join("main.dtr");
    fs::write(&main_dtr, source).expect("Must write main.dtr");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(&temp_dir, None);
    assert!(
        res.success,
        "Hybrid polyglot project compilation failed:\nDiagnostics: {}\nError: {:?}",
        res.diagnostics, res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);

    // Verify C execution
    assert!(
        stdout.contains("C_OK: 0"),
        "Expected C_OK: 0, got: {}",
        stdout
    );
    assert!(
        stdout.contains("C_BUF: 1024"),
        "Expected C_BUF: 1024, got: {}",
        stdout
    );
    assert!(
        stdout.contains("C_PID_VALID: true"),
        "Expected C_PID_VALID: true, got: {}",
        stdout
    );

    // Verify Rust execution
    assert!(
        stdout.contains("RUST_SUM: 42"),
        "Expected RUST_SUM: 42, got: {}",
        stdout
    );
    assert!(
        stdout.contains("RUST_PROD: 42"),
        "Expected RUST_PROD: 42, got: {}",
        stdout
    );

    // Verify JS execution
    assert!(
        stdout.contains("JS_TRANS:"),
        "Expected JS_TRANS output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("\"result\":42"),
        "Expected result 42 in JS transform, got: {}",
        stdout
    );

    // Verify overall completion
    assert!(
        stdout.contains("HYBRID_POLYGLOT_SUCCESS: true"),
        "Expected HYBRID_POLYGLOT_SUCCESS: true, got: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_polyglot_e2e_universal_dce_zero_cost() {
    let temp_dir =
        std::env::temp_dir().join(format!("datara_universal_dce_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Must create temp dir");

    let source = r#"
fn factorial(n: Int) -> Int {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

fn main() {
    let f = factorial(6)
    out "FACT: " + f
}
"#;

    let main_dtr = temp_dir.join("pure_main.dtr");
    fs::write(&main_dtr, source).expect("Must write pure_main.dtr");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, main_dtr.to_str().unwrap(), None);
    assert!(
        res.success,
        "Pure program compilation failed:\nDiagnostics: {}\nError: {:?}",
        res.diagnostics, res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("FACT: 720"),
        "Expected FACT: 720, got: {}",
        stdout
    );

    // Check file size: must be compact
    let metadata = fs::metadata(&exe).expect("Must read executable metadata");
    let exe_size = metadata.len();
    println!("[PURE DATARA EXE SIZE]: {} bytes", exe_size);
    // Typical native Windows MSVC exe with Cranelift runtime is between 50KB and 500KB.
    // Ensure it is not bloated by unused polyglot engines.
    assert!(
        exe_size < 1_500_000,
        "DCE Violation: Executable size {} bytes is unexpectedly large",
        exe_size
    );

    // Read binary bytes and check forbidden polyglot symbols
    let bytes = fs::read(&exe).expect("Must read binary bytes");
    let binary_text = String::from_utf8_lossy(&bytes);

    let forbidden_symbols = [
        "python3.dll",
        "libpython3",
        "Py_Initialize",
        "PyGILState_Ensure",
        "PyRun_String",
        "napi_create_string_utf8",
        "napi_get_cb_info",
        "datara_py_",
        "datara_napi_",
        "datara_js_",
        "fixture_rust_crate",
    ];

    for sym in &forbidden_symbols {
        assert!(
            !binary_text.contains(sym),
            "DCE Violation: Forbidden polyglot symbol/string '{}' found in pure Datara executable",
            sym
        );
    }

    // If dumpbin is available next to the linker, verify imports list
    if let Ok(spec) = forgen::codegen::linker::ensure_linker() {
        let dumpbin = spec.program.with_file_name("dumpbin.exe");
        if dumpbin.exists() {
            if let Ok(output) = Command::new(&dumpbin)
                .args(["/IMPORTS", &exe.to_string_lossy()])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for sym in &forbidden_symbols {
                    assert!(
                        !text.contains(sym),
                        "dumpbin /IMPORTS found forbidden polyglot symbol: {}",
                        sym
                    );
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
