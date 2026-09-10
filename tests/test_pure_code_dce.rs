use forgen::driver::ForgenCompiler;
use std::process::Command;

#[test]
fn test_pure_code_dce_zero_cost_polyglot() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int => a * 2 + b

fn main() {
    let res = compute(21, 58)
    out res
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_pure_dce_run.dtr", None);
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

    // Zero-Cost Verification:
    // Read the binary bytes and ensure no polyglot imports / symbols are referenced
    let bytes = std::fs::read(&exe).expect("Must read exe binary bytes");
    let contents = String::from_utf8_lossy(&bytes);

    let forbidden_patterns = [
        "python3.dll",
        "libpython3",
        "Py_Initialize",
        "PyGILState_Ensure",
        "sqlite3.dll",
        "sqlite3_open",
        "napi_create_string_utf8",
        "datara_py_",
        "datara_napi_",
    ];

    for pat in &forbidden_patterns {
        assert!(
            !contents.contains(pat),
            "DCE Violation: Polyglot symbol/string '{}' found in pure Datara executable",
            pat
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
