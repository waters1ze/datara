use forgen::driver::ForgenCompiler;

#[test]
fn test_borrow_released_on_scope_exit() {
    let source = r#"
fn compute() {
    mut data = 100
    if true {
        mut v = data.view()
        out v
    }
    // Since view 'v' went out of scope at the end of if block, mutating 'data' here is legal
    data = 200
    out data
}

fn main() {
    compute()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "scope_release.dtr", None);
    assert!(
        res.success,
        "Mutating source after inner view scope ends must succeed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines[0], "100");
    assert_eq!(lines[1], "200");
}
