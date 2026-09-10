use forgen::driver::ForgenCompiler;

#[test]
fn test_with_resource_block_execution() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
class TempResource {
    handle: Str
}

behavior TempResource {
    read_data() -> Str => "Resource payload for " + this.handle
}

fn main() {
    with res = TempResource { handle: "RES-770" } {
        let msg = res.read_data()
        out msg
    }
}
"#;
    let res = compiler.compile_source(code, "with_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let (stdout, _, code_res, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_res, 0);
    assert_eq!(stdout.trim(), "Resource payload for RES-770");
}
