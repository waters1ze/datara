use forgen::driver::ForgenCompiler;
use std::fs;

fn run_datara(code: &str, tag: &str) -> String {
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
    assert_eq!(code, 0, "{} exited with {}", tag, code);

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_react_compat_component_render() {
    let code = r##"
class ReactComponent {
    name: Str
    title: Str
    state_val: Str
}

behavior ReactComponent {
    set_state(new_val: Str) -> ReactComponent {
        return ReactComponent {
            name: this.name,
            title: this.title,
            state_val: new_val
        }
    }

    render() -> Str {
        return "<div id='react-root-" + this.name + "'><h2>" + this.title + "</h2><span>" + this.state_val + "</span></div>"
    }
}

fn main() {
    let comp = ReactComponent {
        name: "counter_app",
        title: "React Style Datara UI",
        state_val: "0"
    }
    out comp.render()

    let next_comp = comp.set_state("42")
    out next_comp.render()
}
"##;

    let out = run_datara(code, "test_react_compat.dtr");
    println!("REACT COMPAT OUT:\n{}", out);
    assert!(out.contains("<span>0</span>"), "Actual: {}", out);
    assert!(out.contains("<span>42</span>"), "Actual: {}", out);
    assert!(out.contains("React Style Datara UI"), "Actual: {}", out);
}

#[test]
fn test_python_bridge_spec_generation() {
    let code = r##"
class PythonBridge {
    runtime_name: Str
}

behavior PythonBridge {
    create_wrapper_spec(module_name: Str, functions_csv: Str) -> Str {
        let header = "# Auto-generated Datara Python FFI Bridge for " + module_name + " "
        let dll_load = "_lib = ctypes.CDLL(" + module_name + ") "
        return header + dll_load + "# Exports: " + functions_csv
    }
}

fn main() {
    let bridge = PythonBridge { runtime_name: "CPython3.14" }
    let spec = bridge.create_wrapper_spec("datara_ai_core", "compute_tensor,matmul")
    out spec
}
"##;

    let out = run_datara(code, "test_python_bridge.dtr");
    assert!(out.contains("Auto-generated Datara Python FFI Bridge for datara_ai_core"));
    assert!(out.contains("datara_ai_core"));
    assert!(out.contains("compute_tensor,matmul"));
}

#[test]
fn test_rust_bridge_ffi_spec() {
    let code = r##"
class RustBridge {
    crate_name: Str
}

behavior RustBridge {
    cargo_binding_snippet(func_name: Str) -> Str {
        return "pub extern fn " + func_name + "() -> Int"
    }
}

fn main() {
    let bridge = RustBridge { crate_name: "rust_crypto" }
    let snippet = bridge.cargo_binding_snippet("sha256_hash")
    out snippet
}
"##;

    let out = run_datara(code, "test_rust_bridge.dtr");
    println!("RUST BRIDGE OUT:\n{}", out);
    assert!(
        out.contains("pub extern fn sha256_hash() -> Int"),
        "Actual: {}",
        out
    );
}

#[test]
fn test_smart_foreign_imports_diagnostics() {
    let compiler = ForgenCompiler::new("check");

    // 1. Installed Python module check (math is standard in Python)
    let valid_code = "use python.math\nfn main() { out 1 }\n";
    let res_valid = compiler.check_source(valid_code, "test_py_valid.dtr");
    assert!(
        res_valid.success,
        "use python.math must resolve successfully"
    );

    // 2. Uninstalled Python module check
    let invalid_py = "use python.uninstalled_imaginary_pkg\nfn main() { out 1 }\n";
    let res_invalid = compiler.check_source(invalid_py, "test_py_invalid.dtr");
    assert!(
        !res_invalid.success,
        "Uninstalled package must fail with diagnostic"
    );
    let err_msg = res_invalid.diagnostics;
    assert!(
        err_msg.contains("pip install uninstalled_imaginary_pkg"),
        "Must suggest pip install: {}",
        err_msg
    );

    // 3. Uninstalled Rust crate check
    let invalid_rust = "use rust.non_existent_crate\nfn main() { out 1 }\n";
    let res_rust = compiler.check_source(invalid_rust, "test_rust_invalid.dtr");
    assert!(!res_rust.success);
    let rust_err = res_rust.diagnostics;
    assert!(
        rust_err.contains("cargo add non_existent_crate"),
        "Must suggest cargo add: {}",
        rust_err
    );

    // 4. System C library check (user32 exists in Windows System32)
    if cfg!(windows) {
        let valid_c = "use c.user32\nfn main() { out 1 }\n";
        let res_c = compiler.check_source(valid_c, "test_c_valid.dtr");
        assert!(
            res_c.success,
            "use c.user32 must find system library in System32"
        );
    }

    // 5. Uninstalled C++ library check
    let invalid_cpp = "use cpp.imaginary_render_engine\nfn main() { out 1 }\n";
    let res_cpp = compiler.check_source(invalid_cpp, "test_cpp_invalid.dtr");
    assert!(!res_cpp.success);
    let cpp_err = res_cpp.diagnostics;
    assert!(
        cpp_err.contains("C/C++ library 'imaginary_render_engine' not found"),
        "Must report missing C/C++ lib: {}",
        cpp_err
    );

    // 6. Uninstalled JS/TS library check
    let invalid_js = "use js.imaginary_framework\nfn main() { out 1 }\n";
    let res_js = compiler.check_source(invalid_js, "test_js_invalid.dtr");
    assert!(!res_js.success);
    let js_err = res_js.diagnostics;
    assert!(
        js_err.contains("npm install -g imaginary_framework"),
        "Must suggest npm install: {}",
        js_err
    );
}

#[test]
fn test_web_html_css_library() {
    let code = r##"
class HtmlTag {
    name: Str
    class_name: Str
    content: Str
}

behavior HtmlTag {
    render() -> Str {
        return "<" + this.name + " class='" + this.class_name + "'>" + this.content + "</" + this.name + ">"
    }
}

class WebApp {
    title: Str
}

behavior WebApp {
    card(title: Str, body: Str) -> Str {
        return "<div class='web-card'><h3>" + title + "</h3><p>" + body + "</p></div>"
    }

    render_page(body: Str) -> Str {
        return "<!DOCTYPE html><html><head><title>" + this.title + "</title></head><body>" + body + "</body></html>"
    }
}

fn main() {
    let app = WebApp { title: "Datara Native Web" }
    let card_html = app.card("Analytics", "Fast native rendering without JS!")
    let page_html = app.render_page(card_html)
    out page_html
}
"##;

    let out = run_datara(code, "test_web_page.dtr");
    assert!(out.contains("<!DOCTYPE html>"));
    assert!(out.contains("<div class='web-card'><h3>Analytics</h3>"));
    assert!(out.contains("Fast native rendering without JS!"));
}
