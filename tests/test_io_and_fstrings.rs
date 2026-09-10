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

    // 2. Test Datara Format Stream Template: fmt"..."
    let src_fmt = temp_dir.join("test_fmt_stream.dtr");
    let code_fmt = r#"
fn main() {
    let x = 10
    let y = 20
    println(fmt"Sum of {x} + {y} = {x + y}")
    println($"Dollar stream: {x * y}")
    // 3. Test that regular strings are 100% literal (no accidental interpolation of {})
    println("Literal: {x} + {y} and JSON: {\"key\": 100}")
}
"#;
    fs::write(&src_fmt, code_fmt).unwrap();
    let res2 = compiler.run_file(&src_fmt, &[]);
    assert!(
        res2.is_ok(),
        "fmt stream compilation must succeed: {:?}",
        res2.err()
    );
    let (stdout2, _, code2, _) = res2.unwrap();
    assert_eq!(code2, 0);
    let lines: Vec<&str> = stdout2.lines().collect();
    assert_eq!(lines[0], "Sum of 10 + 20 = 30");
    assert_eq!(lines[1], "Dollar stream: 200");
    assert_eq!(lines[2], "Literal: {x} + {y} and JSON: {\"key\": 100}");

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

    // 5. Fmt-stream in REPL
    let fmt_res = session.eval_line("fmt\"Count: {x + 5}\"").unwrap();
    assert_eq!(fmt_res, "=> Count: 105");

    // 6. Bare function name introspection
    let input_fn_res = session.eval_line("input").unwrap();
    assert!(input_fn_res.contains("<built-in function input"));
    assert!(input_fn_res.contains("input()"));

    let print_fn_res = session.eval_line("print").unwrap();
    assert!(print_fn_res.contains("<built-in function print"));
}
