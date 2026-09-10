use forgen::driver::ForgenCompiler;

#[test]
fn test_use_declarations_and_parallel_blocks() {
    let source = r#"
fn process_data(val: Int) -> Int {
    parallel {
        let a = val * 2
        let b = val + 10
    }
    return val * 3
}

fn main() {
    let res = process_data(10)
    out fmt"Processed: {res}"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_concurrency.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Processed: 30"));
}
