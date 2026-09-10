use forgen::driver::ForgenCompiler;

#[test]
fn test_generic_box_specialization() {
    let source = r#"
class Box<T> {
    value: T
}

fn main() {
    mut a = Box<Int> { value: 42 }
    out a.value
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "generic_box.dtr", None);
    assert!(
        res.success,
        "Generic box compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42");

    let rep = res.optimization_report.unwrap();
    println!("Specializations: {:?}", rep.generic_specializations);
    assert!(
        rep.generic_specializations
            .iter()
            .any(|s| s.contains("Box<Int>"))
    );
}
