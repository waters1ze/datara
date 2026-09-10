use forgen::driver::ForgenCompiler;

#[test]
fn test_collections_list_map_range_execution() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
fn main() {
    // 1. List literal and indexing
    let numbers = [10, 20, 30, 40]
    out numbers[1]
    out numbers[3]

    // 2. Range 0..5
    let r = 0..5
    out r

    // 3. Map literal
    let scores = {"Alice": 95, "Bob": 88}
    out scores["Alice"]
}
"#;
    let res = compiler.compile_source(code, "collections_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let (stdout, _, code_res, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_res, 0);
    let lines: Vec<&str> = stdout.trim().lines().map(|l| l.trim()).collect();
    assert_eq!(lines[0], "20");
    assert_eq!(lines[1], "40");
    assert_eq!(lines[3], "95");
}

#[test]
fn test_pipeline_composition_and_lambdas() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
fn double_val(x: Int) -> Int => x * 2
fn add_five(x: Int) -> Int => x + 5

fn main() {
    let initial = 10
    let res = initial |> double_val() |> add_five()
    out res
}
"#;
    let res = compiler.compile_source(code, "pipeline_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let (stdout, _, code_res, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_res, 0);
    assert_eq!(stdout.trim(), "25");
}
