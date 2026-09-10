use forgen::driver::ForgenCompiler;
use std::path::Path;

#[test]
fn test_01_vertical_slice() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/01_vertical_slice.dtr"), None);
    assert!(
        res.success,
        "01_vertical_slice compilation failed: {:?}",
        res.error
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "30");
}

#[test]
fn test_02_class_modern_oop() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/02_class_modern_oop.dtr"), None);
    assert!(
        res.success,
        "02_class_modern_oop compilation failed: {:?}",
        res.error
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Hello Alex");
}

#[test]
fn test_03_split_behavior() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/03_split_behavior.dtr"), None);
    assert!(
        res.success,
        "03_split_behavior compilation failed: {:?}",
        res.error
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Hello Maria (25)");
}

#[test]
fn test_04_decide_and_control() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/04_decide_and_control.dtr"), None);
    assert!(
        res.success,
        "04_decide_and_control compilation failed: {:?}",
        res.error
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().map(|l| l.trim()).collect();
    assert_eq!(lines, vec!["Score 85 Grade: B", "Score 50 Grade: F"]);
}

#[test]
fn test_05_pipeline_dataflow() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/05_pipeline_dataflow.dtr"), None);
    assert!(
        res.success,
        "05_pipeline_dataflow compilation failed: {:?}",
        res.error
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Pipeline result: 30");
}

#[test]
fn test_06_phase1_complete_app() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(Path::new("examples/06_phase1_complete_app.dtr"), None);
    assert!(
        res.success,
        "06_phase1_complete_app compilation error: {:?}",
        res.diagnostics
    );
    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    if code != 0 {
        panic!(
            "Run failed (code {}):\nstdout: {}\nstderr: {}",
            code, stdout, stderr
        );
    }
    let lines: Vec<&str> = stdout.trim().lines().map(|l| l.trim()).collect();
    assert_eq!(
        lines,
        vec![
            "ADMIN [Arthur] Level AUD-8842",
            "User(id=Arthur, audit=AUD-8842)",
            "Tx processed for Arthur: Total 1530"
        ]
    );
}
