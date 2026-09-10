use forgen::driver::ForgenCompiler;
use std::ffi::{CStr, CString};
use std::process::Command;

unsafe extern "C" {
    fn datara_js_eval(code: *const std::ffi::c_char) -> *const std::ffi::c_char;
    fn datara_js_eval_int(code: *const std::ffi::c_char) -> i64;
    fn datara_js_eval_float(code: *const std::ffi::c_char) -> f64;
}

fn js_eval_str(code: &str) -> String {
    let c = CString::new(code).unwrap();
    let ptr = unsafe { datara_js_eval(c.as_ptr()) };
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    }
}

fn js_eval_int(code: &str) -> i64 {
    let c = CString::new(code).unwrap();
    unsafe { datara_js_eval_int(c.as_ptr()) }
}

fn js_eval_float(code: &str) -> f64 {
    let c = CString::new(code).unwrap();
    unsafe { datara_js_eval_float(c.as_ptr()) }
}

#[test]
fn test_node_runtime_self_test() {
    // 1. Buffer alloc, writeDoubleLE, readDoubleLE
    let flt_val =
        js_eval_float("let b = Buffer.alloc(16); b.writeDoubleLE(123.456, 0); b.readDoubleLE(0);");
    assert!(
        (flt_val - 123.456).abs() < 1e-6,
        "Expected 123.456, got: {}",
        flt_val
    );

    // 2. Buffer uint8 write and index read
    let int_val = js_eval_int("let b = Buffer.alloc(8); b.writeUInt8(199, 0); b[0];");
    assert_eq!(int_val, 199, "Expected 199, got: {}", int_val);

    // 3. Buffer.from string and toString()
    let str_val = js_eval_str("let b = Buffer.from('hello node'); b.toString();");
    assert_eq!(
        str_val, "hello node",
        "Expected 'hello node', got: {}",
        str_val
    );

    // 4. Promises, microtask queue, and await
    let promise_val = js_eval_str("let p = Promise.resolve(100).then(x => x * 2); await p;");
    assert_eq!(promise_val, "200", "Expected '200', got: {}", promise_val);

    // 5. EventEmitter: on and emit
    let ee_val = js_eval_str(
        "let ee = new EventEmitter(); let sum = 0; ee.on('add', (x) => { sum = sum + x; }); ee.emit('add', 50); ee.emit('add', 25); sum;",
    );
    assert_eq!(ee_val, "75", "Expected '75', got: {}", ee_val);
}

#[test]
fn test_node_core_modules() {
    let source = r#"
import js

fn main() {
    let js = JS { version: "ES2022" }

    // 1. Path module
    let joined = js.eval("const path = require('path'); path.join('foo', 'bar', 'baz.txt');")
    let base = js.eval("const path = require('path'); path.basename('foo/bar/baz.txt');")
    let ext = js.eval("const path = require('path'); path.extname('app.test.js');")

    out "JOINED: " + joined
    out "BASE: " + base
    out "EXT: " + ext

    // 2. FS module
    let temp_file = "test_fs_roundtrip.txt"
    js.set_global("temp_path", temp_file)
    js.eval("const fs = require('fs'); fs.writeFileSync(temp_path, 'DataraFS_Payload_42');")
    let exists = js.eval("const fs = require('fs'); fs.existsSync(temp_path);")
    let content = js.eval("const fs = require('fs'); fs.readFileSync(temp_path);")
    js.eval("const fs = require('fs'); fs.unlinkSync(temp_path);")
    let exists_after = js.eval("const fs = require('fs'); fs.existsSync(temp_path);")

    out "EXISTS: " + exists
    out "CONTENT: " + content
    out "EXISTS_AFTER: " + exists_after

    // 3. Crypto module (SHA-256)
    let hash = js.eval("const crypto = require('crypto'); crypto.createHash('sha256').update('hello').digest('hex');")
    out "HASH: " + hash
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_node_core_run.dtr", None);
    assert!(
        res.success,
        "Node core modules compilation failed: {:?}\nDiagnostics:\n{}",
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
        stdout.contains("BASE: baz.txt"),
        "Expected BASE: baz.txt, got: {}",
        stdout
    );
    assert!(
        stdout.contains("EXT: .js"),
        "Expected EXT: .js, got: {}",
        stdout
    );
    assert!(
        stdout.contains("EXISTS: true"),
        "Expected EXISTS: true, got: {}",
        stdout
    );
    assert!(
        stdout.contains("CONTENT: DataraFS_Payload_42"),
        "Expected CONTENT: DataraFS_Payload_42, got: {}",
        stdout
    );
    assert!(
        stdout.contains("EXISTS_AFTER: false"),
        "Expected EXISTS_AFTER: false, got: {}",
        stdout
    );
    assert!(
        stdout.contains("HASH: 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
        "Unexpected SHA256: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_node_http_roundtrip() {
    let source = r#"
import js

fn main() {
    let js = JS { version: "ES2022" }
    let code = "const http = require('http'); let server = http.createServer((req, res) => { res.writeHead(200); res.end('PONG_DATARA_HTTP'); }); server.listen(18094, () => { http.get('http://127.0.0.1:18094/ping', (res) => { res.on('data', (chunk) => { console.log('HTTP_PAYLOAD:' + chunk); console.log('HTTP_STATUS:' + res.statusCode); server.close(); }); }); });"
    js.eval(code)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_node_http_run.dtr", None);
    assert!(
        res.success,
        "HTTP test compilation failed: {:?}\nDiagnostics:\n{}",
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
        stdout.contains("HTTP_STATUS:200"),
        "Expected HTTP_STATUS:200, got: {}",
        stdout
    );
    assert!(
        stdout.contains("HTTP_PAYLOAD:PONG_DATARA_HTTP"),
        "Expected HTTP_PAYLOAD:PONG_DATARA_HTTP, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
#[cfg(windows)]
fn test_node_napi_addon_compilation_and_call() {
    // 1. Build test N-API addon DLL
    let linker_spec = forgen::codegen::linker::ensure_linker().expect("Linker must be available");
    let cl_exe = linker_spec.program.with_file_name("cl.exe");
    assert!(cl_exe.exists(), "cl.exe must exist next to link.exe");

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = manifest_dir.join("src").join("runtime");

    let addon_c_path = manifest_dir.join("fixture_test_addon.c");
    let addon_node_path = manifest_dir.join("fixture_test_addon.node");

    let c_code = r#"
#include "datara_napi.h"

static napi_value Add(napi_env env, napi_callback_info info) {
    size_t argc = 2;
    napi_value args[2];
    napi_get_cb_info(env, info, &argc, args, NULL, NULL);

    int64_t a = 0, b = 0;
    napi_get_value_int64(env, args[0], &a);
    napi_get_value_int64(env, args[1], &b);

    napi_value sum;
    napi_create_int64(env, a + b, &sum);
    return sum;
}

NAPI_MODULE_INIT() {
    napi_value fn;
    napi_create_function(env, "add", NAPI_AUTO_LENGTH, Add, NULL, &fn);
    napi_set_named_property(env, exports, "add", fn);
    return exports;
}
"#;
    std::fs::write(&addon_c_path, c_code).expect("Write fixture_test_addon.c");

    let mut cmd = Command::new(&cl_exe);
    cmd.arg("/LD")
        .arg("/MD")
        .arg(format!("/I{}", runtime_dir.display()));

    // Add include dirs from Windows Kits if available
    let mut curr = cl_exe.parent();
    while let Some(p) = curr {
        let inc = p.join("include");
        if inc.exists() {
            cmd.arg(format!("/I{}", inc.display()));
            break;
        }
        curr = p.parent();
    }

    let pf =
        std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
    let wk_include = std::path::PathBuf::from(&pf).join("Windows Kits\\10\\Include");
    if let Ok(entries) = std::fs::read_dir(&wk_include) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                for sub in ["ucrt", "shared", "um"] {
                    let sub_p = p.join(sub);
                    if sub_p.exists() {
                        cmd.arg(format!("/I{}", sub_p.display()));
                    }
                }
            }
        }
    }

    cmd.arg(&addon_c_path)
        .arg(format!("/Fe:{}", addon_node_path.display()))
        .arg("/link");

    for p in &linker_spec.lib_paths {
        cmd.arg(format!("/LIBPATH:{}", p.display()));
    }

    let compile_output = cmd.output().expect("Execute cl.exe to compile addon");
    assert!(
        compile_output.status.success(),
        "Addon compilation failed: {}",
        String::from_utf8_lossy(&compile_output.stdout)
    );
    assert!(
        addon_node_path.exists(),
        "fixture_test_addon.node must exist"
    );

    // 2. Invoke addon from Datara native code
    let source = r#"
import js

fn main() {
    let js = JS { version: "ES2022" }
    let res = js.eval("const addon = require('./fixture_test_addon.node'); addon.add(20, 22);")
    out "ADDON_RESULT: " + res
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_node_addon_run.dtr", None);
    assert!(
        res.success,
        "Addon test compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("ADDON_RESULT: 42"),
        "Expected ADDON_RESULT: 42, got: {}",
        stdout
    );

    // Cleanup
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
    let _ = std::fs::remove_file(&addon_c_path);
    let _ = std::fs::remove_file(&addon_node_path);
    let _ = std::fs::remove_file(addon_node_path.with_extension("obj"));
    let _ = std::fs::remove_file(addon_node_path.with_extension("lib"));
    let _ = std::fs::remove_file(addon_node_path.with_extension("exp"));
}

#[test]
fn test_node_datara_zerocopy_mutation() {
    let source = r#"
import js

fn main() {
    let js = JS { version: "ES2022" }
    let buf = [10.0, 20.0, 30.0, 40.0]
    let bind_res = js.export_list_f64("shared_buf", buf)
    if bind_res != 1 {
        out "BIND_FAIL"
        return
    }

    let same_ptr = js.assert_same_ptr("shared_buf", buf)
    if same_ptr != 1 {
        out "PTR_MISMATCH"
        return
    }

    // Mutate buffer inside JS runtime
    js.eval("shared_buf.writeDoubleLE(888.5, 8); shared_buf.writeDoubleLE(999.25, 24);")

    // Datara observes in-place mutation without copying
    let v1 = buf[1]
    let v3 = buf[3]
    out "V1: " + v1
    out "V3: " + v3
    out "SAME_PTR: " + same_ptr
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_node_zerocopy_run.dtr", None);
    assert!(
        res.success,
        "Zero-copy test compilation failed: {:?}\nDiagnostics:\n{}",
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
        stdout.contains("V1: 888.5"),
        "Expected V1: 888.5, got: {}",
        stdout
    );
    assert!(
        stdout.contains("V3: 999.25"),
        "Expected V3: 999.25, got: {}",
        stdout
    );
    assert!(
        stdout.contains("SAME_PTR: 1"),
        "Expected SAME_PTR: 1, got: {}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_node_dce_zero_cost() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int => a * 2 + b

fn main() {
    let res = compute(21, 58)
    out res
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_node_dce_run.dtr", None);
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
    assert_eq!(stdout.trim(), "100", "Execution output mismatch");

    // Zero-Cost Verification
    let bytes = std::fs::read(&exe).expect("Must read exe binary bytes");
    let contents = String::from_utf8_lossy(&bytes);

    let forbidden_patterns = [
        "napi_create_string_utf8",
        "datara_napi_",
        "python3.dll",
        "libpython3",
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
