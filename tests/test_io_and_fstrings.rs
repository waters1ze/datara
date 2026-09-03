use forgen::driver::ForgenCompiler;
use forgen::repl::ReplSession;
use std::fs;

#[test]
fn test_python_fstrings_and_default_interpolation() {
    let compiler = ForgenCompiler::new("release");
    let temp_dir = std::env::temp_dir().join("datara_fstrings_test");
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Test Python-style f"..." prefix
    let src_fstring = temp_dir.join("test_fstring.dtr");
    let code_fstring = r#"
fn main() {
    let name = "Datara"
    let ver = 1
    println(f"Hello from {name} v{ver}!")
}
"#;
    fs::write(&src_fstring, code_fstring).unwrap();
    let res = compiler.run_file(&src_fstring, &[]);
    assert!(
        res.is_ok(),
        "f-string compilation must succeed: {:?}",
        res.err()
    );
    let (stdout, _, code, _) = res.unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Hello from Datara v1!");

    // 2. Test Datara default interpolation (without f prefix)
    let src_default = temp_dir.join("test_default_interp.dtr");
    let code_default = r#"
fn main() {
    let x = 10
    let y = 20
    println("Sum of {x} + {y} = {x + y}")
}
"#;
    fs::write(&src_default, code_default).unwrap();
    let res2 = compiler.run_file(&src_default, &[]);
    assert!(
        res2.is_ok(),
        "default interpolation must succeed: {:?}",
        res2.err()
    );
    let (stdout2, _, code2, _) = res2.unwrap();
    assert_eq!(code2, 0);
    assert_eq!(stdout2.trim(), "Sum of 10 + 20 = 30");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_variadic_multi_type_print_and_println() {
    let compiler = ForgenCompiler::new("release");
    let temp_dir = std::env::temp_dir().join("datara_variadic_print_test");
    let _ = fs::create_dir_all(&temp_dir);

    let src = temp_dir.join("test_variadic.dtr");
    let code = r#"
fn main() {
    // Zero args
    println()
    
    // Multiple polymorphic args: string, int, float, bool
    let x = 42
    let pi = 3.14159
    let flag = true
    println("Result:", x, "pi:", pi, "ok:", flag)
    
    // print without newline
    print("Part1 ")
    print("Part2\n")
}
"#;
    fs::write(&src, code).unwrap();
    let res = compiler.run_file(&src, &[]);
    assert!(res.is_ok(), "Variadic print must succeed: {:?}", res.err());
    let (stdout, _, code_res, _) = res.unwrap();
    assert_eq!(code_res, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 3);
    assert_eq!(lines[0], "");
    assert!(lines[1].contains("Result: 42 pi: 3.14159 ok: true"));
    assert!(lines[2].contains("Part1 Part2"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_repl_session_in_process_speed_and_state() {
    let mut session = ReplSession::new();

    // 1. Arithmetic evaluation
    let eval_res = session.eval_line("20 + 22").unwrap();
    assert_eq!(eval_res, "=> 42");

    // 2. Variable definition
    let def_x = session.eval_line("let x = 100").unwrap();
    assert_eq!(def_x, "defined x");

    // 3. State persistence: using x in next expression
    let use_x = session.eval_line("x * 3").unwrap();
    assert_eq!(use_x, "=> 300");

    // 4. Print inside REPL
    let print_res = session.eval_line("print(\"hello\", x)").unwrap();
    assert_eq!(print_res, "=> hello 100");

    // 5. F-string in REPL
    let fstr_res = session.eval_line("f\"Count: {x + 5}\"").unwrap();
    assert_eq!(fstr_res, "=> Count: 105");
}
