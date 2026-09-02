use forgen::driver::ForgenCompiler;
use std::path::Path;

#[test]
fn test_datara_find_full_application() {
    let compiler = ForgenCompiler::new("release");
    let project_dir = Path::new("examples/datara_find/src");

    let res = compiler.compile_project(project_dir, None);
    assert!(
        res.success,
        "datara_find compilation failed:\n{}\n{:?}",
        res.diagnostics, res.error
    );

    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0, "Execution failed with stderr: {}", stderr);

    println!("[DATARA_FIND OUTPUT]:\n{}", stdout);

    assert!(stdout.contains("Datara Find Utility v2.0.0"));
    assert!(stdout.contains("Query: 'ERROR' in './logs'"));
    assert!(stdout.contains("./logs/auth.log"));
    assert!(stdout.contains("./logs/gateway.log"));
    assert!(stdout.contains("Scanned: 2, Errors: 1, Warnings: 1"));
}
