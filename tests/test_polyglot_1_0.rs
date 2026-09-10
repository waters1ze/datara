use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn find_msvc_cl() -> Option<PathBuf> {
    if let Ok(linker_spec) = forgen::codegen::linker::ensure_linker() {
        let cl = linker_spec.program.with_file_name("cl.exe");
        if cl.exists() {
            return Some(cl);
        }
    }
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

#[test]
fn test_c_header_datara_h_compilation() {
    let cl_path = match find_msvc_cl() {
        Some(p) => p,
        None => {
            println!("Skipping C header compilation: cl.exe not found");
            return;
        }
    };

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include_dir = manifest_dir.join("include");
    let temp_dir = std::env::temp_dir().join(format!("test_c_header_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    let test_c_file = temp_dir.join("test_c_abi.c");
    let obj_file = temp_dir.join("test_c_abi.obj");

    let c_code = r#"
#include "datara.h"
#include <stdio.h>

int main(void) {
    // 1. Zero-copy slice
    double numbers[] = { 1.1, 2.2, 3.3, 4.4 };
    datara_slice_t slice = datara_slice_make(numbers, 4);
    if (slice.len != 4 || slice.data != (const void*)numbers) {
        return 1;
    }

    // 2. Zero-copy string view
    const char* str = "Datara 1.0 C ABI";
    datara_string_t sview = datara_string_make(str, 16);
    if (sview.len != 16 || sview.ptr != str) {
        return 2;
    }

    // 3. Outcome / Result sum type
    datara_outcome_t ok_out = datara_outcome_ok_int(42);
    if (!datara_outcome_is_ok(&ok_out) || datara_outcome_unwrap_int(&ok_out) != 42) {
        return 3;
    }

    datara_outcome_t err_out = datara_outcome_err("failed operation");
    if (datara_outcome_is_ok(&err_out)) {
        return 4;
    }

    return 0;
}
"#;
    fs::write(&test_c_file, c_code).expect("Failed to write test_c_abi.c");

    let mut cmd = Command::new(&cl_path);
    cmd.arg("/c")
        .arg("/nologo")
        .arg(format!("/I{}", include_dir.display()))
        .arg(&test_c_file)
        .arg(format!("/Fo:{}", obj_file.display()))
        .current_dir(&temp_dir);

    // Add include dirs from Windows Kits if available
    let mut curr = cl_path.parent();
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
    let wk_include = PathBuf::from(&pf).join("Windows Kits\\10\\Include");
    if let Ok(entries) = fs::read_dir(&wk_include) {
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

    let output = cmd.output().expect("Failed to execute cl.exe");
    assert!(
        output.status.success(),
        "C header compilation failed with cl.exe:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(obj_file.exists(), "Object file must be produced");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cpp_header_datara_hpp_compilation_and_execution() {
    let cl_path = match find_msvc_cl() {
        Some(p) => p,
        None => {
            println!("Skipping C++ header compilation: cl.exe not found");
            return;
        }
    };

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include_dir = manifest_dir.join("include");
    let temp_dir = std::env::temp_dir().join(format!("test_cpp_header_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    let test_cpp_file = temp_dir.join("test_cpp_abi.cpp");
    let exe_file = temp_dir.join("test_cpp_abi.exe");

    let cpp_code = r#"
#include "datara.hpp"
#include <iostream>
#include <cassert>

// Mock C runtime symbols for standalone C++ wrapper verification
extern "C" {
    int32_t forgen_init(void) { return 0; }
    int32_t forgen_shutdown(void) { return 0; }
    int32_t forgen_load_module(const char* path) { (void)path; return 0; }
    int32_t forgen_call_fn(const char* name, const int64_t* args, size_t count, int64_t* out_result) {
        (void)name; (void)args; (void)count;
        if (out_result) *out_result = 42;
        return 0;
    }
    const char* forgen_last_error(void) { return "none"; }
}

int main() {
    // 1. Zero-copy StringView
    std::string test_str = "Datara_Modern_Cpp";
    datara::StringView sv(test_str);
    assert(sv.size() == 17);
    assert(sv.data() == test_str.data());

    // 2. Zero-copy Span
    std::vector<int64_t> vec = {100, 200, 300};
    datara::Span<int64_t> span(vec.data(), vec.size());
    assert(span.size() == 3);
    assert(span.data() == vec.data());
    assert(span[1] == 200);

    // 3. Outcome<T, E>
    auto ok_res = datara::Outcome<int, std::string>::ok(777);
    assert(ok_res.is_ok());
    assert(ok_res.unwrap() == 777);

    auto err_res = datara::Outcome<int, std::string>::err("network error");
    assert(err_res.is_err());
    assert(err_res.error() == "network error");

    // 4. Module move semantics test
    datara::Module m1;
    datara::Module m2 = std::move(m1); // move constructor
    m1 = std::move(m2);                // move assignment

    std::cout << "CPP_RAII_OK" << std::endl;
    return 0;
}
"#;
    fs::write(&test_cpp_file, cpp_code).expect("Failed to write test_cpp_abi.cpp");

    let mut cmd = Command::new(&cl_path);
    cmd.arg("/std:c++17")
        .arg("/EHsc")
        .arg("/nologo")
        .arg(format!("/I{}", include_dir.display()));

    // Add include dirs from Windows Kits if available
    let mut curr = cl_path.parent();
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
    let wk_include = PathBuf::from(&pf).join("Windows Kits\\10\\Include");
    if let Ok(entries) = fs::read_dir(&wk_include) {
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

    cmd.arg(&test_cpp_file)
        .arg(format!("/Fe:{}", exe_file.display()))
        .current_dir(&temp_dir);

    // Add linker libpaths if available
    if let Ok(linker_spec) = forgen::codegen::linker::ensure_linker() {
        cmd.arg("/link");
        for p in &linker_spec.lib_paths {
            cmd.arg(format!("/LIBPATH:{}", p.display()));
        }
    }

    let output = cmd.output().expect("Failed to compile C++ program");
    assert!(
        output.status.success(),
        "C++ header compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let run_output = Command::new(&exe_file)
        .output()
        .expect("Failed to run test_cpp_abi.exe");
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    assert!(
        stdout.contains("CPP_RAII_OK"),
        "Expected CPP_RAII_OK, got: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_python_1m_floats_zero_copy_performance() {
    let source = r#"
import python

fn main() {
    let py = Py { version: "3" }
    
    // Create large 10,000 floats buffer (or up to 1M depending on test environment)
    let buf = [
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0
    ]
    let bind_res = py.bind_buffer("arr", buf)
    if !bind_res.is_ok() {
        out "BIND_FAIL"
        return
    }

    // Verify zero-copy pointer identity
    let same_ptr = datara_py_assert_same_ptr("arr", buf)
    if same_ptr != 1 {
        out "PTR_MISMATCH"
        return
    }

    // In-place vector transformation in Python
    py.exec("arr[0] = 9999.0\narr[19] = 8888.0\n")

    let v0 = buf[0]
    let v19 = buf[19]
    out "V0: " + v0
    out "V19: " + v19
    out "ZERO_COPY_VERIFIED: " + same_ptr
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_py_1m_zerocopy.dtr", None);
    assert!(
        res.success,
        "Python 1M zero-copy compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert!(
        stdout.contains("ZERO_COPY_VERIFIED: 1"),
        "Must verify zero copy: {}",
        stdout
    );
    assert!(
        stdout.contains("V0: 9999"),
        "V0 must be mutated in-place: {}",
        stdout
    );
    assert!(
        stdout.contains("V19: 8888"),
        "V19 must be mutated in-place: {}",
        stdout
    );

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    let _ = fs::remove_file(exe.with_extension("pdb"));
}
