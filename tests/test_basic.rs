use forgen::driver::ForgenCompiler;
use std::path::Path;

#[test]
fn test_vertical_slice_1() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/01_vertical_slice.dtr"), None);
    println!("Compilation success: {}", res.success);
    if !res.success {
        println!("Error: {:?}", res.error);
        println!("Diagnostics: {}", res.diagnostics);
    }
    assert!(res.success, "Compilation should succeed");

    let exe = res.exe_path.unwrap();
    let (stdout, stderr, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    println!("Stdout: {}", stdout);
    println!("Stderr: {}", stderr);
    println!("Exit code: {}", code);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "30");
}
