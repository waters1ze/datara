use forgen::codegen::linker::ensure_linker;
use forgen::driver::ForgenCompiler;
use std::path::{Path, PathBuf};
use std::process::Command;

fn compile_c_fixture_to_lib(c_src: &Path, out_lib: &Path) -> Result<(), String> {
    let spec = ensure_linker().map_err(|e| format!("Linker not found: {}", e))?;
    let bin_dir = spec
        .program
        .parent()
        .ok_or_else(|| "Failed to get linker directory".to_string())?;
    let cl_exe = bin_dir.join(if cfg!(windows) { "cl.exe" } else { "gcc" });
    let lib_exe = bin_dir.join(if cfg!(windows) { "lib.exe" } else { "ar" });

    let obj_path = out_lib.with_extension("obj");

    // 1. Compile C file to object file
    let mut cl_cmd = Command::new(&cl_exe);
    cl_cmd.args([
        "/c",
        "/O2",
        "/nologo",
        "/GS-",
        &format!("/Fo:{}", obj_path.display()),
        &format!("{}", c_src.display()),
    ]);
    let cl_res = cl_cmd
        .output()
        .map_err(|e| format!("Failed to invoke cl.exe at {}: {}", cl_exe.display(), e))?;
    if !cl_res.status.success() {
        return Err(format!(
            "cl.exe failed: {}\nstdout: {}\nstderr: {}",
            cl_res.status,
            String::from_utf8_lossy(&cl_res.stdout),
            String::from_utf8_lossy(&cl_res.stderr)
        ));
    }

    // 2. Create static library archive (.lib)
    let mut lib_cmd = Command::new(&lib_exe);
    lib_cmd.args([
        "/nologo",
        &format!("/OUT:{}", out_lib.display()),
        &format!("{}", obj_path.display()),
    ]);
    let lib_res = lib_cmd
        .output()
        .map_err(|e| format!("Failed to invoke lib.exe at {}: {}", lib_exe.display(), e))?;
    if !lib_res.status.success() {
        return Err(format!(
            "lib.exe failed: {}\nstdout: {}\nstderr: {}",
            lib_res.status,
            String::from_utf8_lossy(&lib_res.stdout),
            String::from_utf8_lossy(&lib_res.stderr)
        ));
    }

    let _ = std::fs::remove_file(&obj_path);
    Ok(())
}

#[test]
#[cfg(windows)]
fn test_cimport_abi_struct_and_callbacks() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_h = manifest_dir
        .join("tests")
        .join("fixtures")
        .join("test_abi_lib.h");
    let fixture_c = manifest_dir
        .join("tests")
        .join("fixtures")
        .join("test_abi_lib.c");
    let out_lib = manifest_dir
        .join("tests")
        .join("fixtures")
        .join("test_abi_lib.lib");

    assert!(fixture_h.exists(), "test_abi_lib.h must exist");
    assert!(fixture_c.exists(), "test_abi_lib.c must exist");

    // Compile C fixture to .lib
    compile_c_fixture_to_lib(&fixture_c, &out_lib)
        .expect("Must compile C fixture into static library");
    assert!(out_lib.exists(), "test_abi_lib.lib must exist");

    let h_path_str = fixture_h.to_string_lossy().replace('\\', "/");
    let lib_path_str = out_lib.to_string_lossy().replace('\\', "/");

    let source = format!(
        r#"
import c "{}" with link("{}");

fn my_callback(x: Int) -> Int => x * 2

fn my_callback_add(x: Int) -> Int => x + 100

fn main() {{
    let p1 = Point {{ x: 15, y: 27 }}
    let p2 = Point {{ x: 100, y: 200 }}
    mut s_pt = 0
    mut s_two = 0
    mut cb_res = 0
    mut cb_inline_res = 0
    unsafe(justification: "Calling C ABI functions with struct passing and bidirectional callbacks") {{
        s_pt = sum_point(p1)
        s_two = sum_two_points(p1, p2)
        cb_res = apply_callback(21, my_callback)
        cb_inline_res = apply_callback_inline(50, my_callback_add)
    }}

    out "SUM_PT: " + s_pt
    out "SUM_TWO: " + s_two
    out "CB: " + cb_res
    out "CB_INLINE: " + cb_inline_res
}}
"#,
        h_path_str, lib_path_str
    );

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(&source, "test_cimport_abi_run.dtr", None);
    assert!(
        res.success,
        "CImport ABI compilation failed: {:?}\nDiagnostics:\n{}",
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
        stdout.contains("SUM_PT: 42"),
        "Expected SUM_PT: 42, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("SUM_TWO: 342"),
        "Expected SUM_TWO: 342, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("CB: 42"),
        "Expected CB: 42, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("CB_INLINE: 150"),
        "Expected CB_INLINE: 150, got:\n{}",
        stdout
    );

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    let _ = std::fs::remove_file(exe.with_extension("pdb"));
    let _ = std::fs::remove_file(&out_lib);
}
