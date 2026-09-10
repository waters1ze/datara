use forgen::driver::ForgenCompiler;
use std::path::Path;

#[test]
fn test_real_cli_multimodule_compilation_and_execution() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(Path::new("examples/real_cli"), None);
    if !res.success {
        panic!(
            "real_cli compilation failed:\n{}\n{:?}",
            res.diagnostics, res.error
        );
    }
    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    if code != 0 {
        panic!(
            "real_cli execution failed (code {}):\nstdout: {}\nstderr: {}",
            code, stdout, stderr
        );
    }
    let lines: Vec<&str> = stdout.trim().lines().map(|l| l.trim()).collect();
    assert_eq!(lines[0], "=== Datara Search CLI v1.0.0 ===");
    assert_eq!(lines[1], "-> core/lexer.dtr: 320 lines");
}
