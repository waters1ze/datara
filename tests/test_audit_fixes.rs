use forgen::driver::ForgenCompiler;
use std::fs;

fn run_datara(code: &str, tag: &str) -> (String, i32) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(code, tag, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        tag, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    (stdout.trim().replace("\r\n", "\n"), code)
}

#[test]
fn test_char_literals_and_escapes() {
    let code = r#"
fn main() {
    let c = 'A'
    let text = "Line1\nLine2\tTabbed\\Escaped\"Quote\"\{LiteralBrace}"
    out "CHAR_OK"
}
"#;
    let (out, code_ret) = run_datara(code, "test_char_and_escapes.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("CHAR_OK"));
}

#[test]
fn test_real_pattern_matching_with_guards() {
    let code = r#"
fn classify(x: Int) -> Str {
    let res = match x {
        1 => "ONE"
        2 => "TWO"
        n if n > 10 => "LARGE"
        _ => "OTHER"
    }
    return res
}

fn main() {
    let a = classify(1)
    let b = classify(2)
    let c = classify(15)
    let d = classify(5)
    out "{a},{b},{c},{d}"
}
"#;
    let (out, code_ret) = run_datara(code, "test_pattern_matching.dtr");
    assert_eq!(code_ret, 0);
    assert_eq!(out, "ONE,TWO,LARGE,OTHER");
}

#[test]
fn test_for_and_parallel_for_scoping() {
    let code = r#"
fn main() {
    mut sum1: Int = 0
    for i in 1..5 {
        sum1 = sum1 + i
    }

    mut sum2: Int = 0
    parallel for j in 1..5 {
        sum2 = sum2 + j
    }

    out "SUMS: {sum1},{sum2}"
}
"#;
    let (out, code_ret) = run_datara(code, "test_for_scoping.dtr");
    assert_eq!(code_ret, 0);
    assert_eq!(out, "SUMS: 10,10");
}

#[test]
fn test_dec128_type_and_builtin_resolution() {
    let code = r#"
fn main() {
    let t = now_ms()
    let count = args_count()
    out "BUILTINS_OK:{count}"
}
"#;
    let (out, code_ret) = run_datara(code, "test_builtins.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("BUILTINS_OK:"));
}

#[test]
fn test_bidirectional_option_narrowing() {
    let code = r#"
fn check_opt(x: Int?) -> Str {
    if x != None {
        return "SOME"
    } else {
        return "NONE"
    }
}

fn main() {
    let s = check_opt(42)
    let n = check_opt(None)
    out "{s},{n}"
}
"#;
    let (out, code_ret) = run_datara(code, "test_opt_narrowing.dtr");
    assert_eq!(code_ret, 0);
    assert_eq!(out, "SOME,NONE");
}

#[test]
fn test_tuple_creation_and_lowering() {
    let code = r#"
fn main() {
    let t = (10, 20, 30)
    out "TUPLE_OK"
}
"#;
    let (out, code_ret) = run_datara(code, "test_tuple.dtr");
    assert_eq!(code_ret, 0);
    assert_eq!(out, "TUPLE_OK");
}

#[test]
fn test_str_to_float_and_conversions() {
    let code = r#"
fn main() {
    let i = str_to_int("42")
    let f = str_to_float("3.14159")
    out "{i}"
    out "{f}"
    out "CONVERSIONS_OK"
}
"#;
    let (out, code_ret) = run_datara(code, "test_conversions.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("42"));
    assert!(out.contains("3.14159"));
    assert!(out.contains("CONVERSIONS_OK"));
}

#[test]
fn test_input_compiles_and_links() {
    let code = r#"
fn test_parser_input() {
    // Verifies that input and read_line are valid syntax, resolve, typecheck, and link
    if false {
        let name = input("Enter name: ")
        let line = read_line()
        out "{name}: {line}"
    }
}

fn main() {
    test_parser_input()
    out "INPUT_LINKED_OK"
}
"#;
    let (out, code_ret) = run_datara(code, "test_input_pipeline.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("INPUT_LINKED_OK"));
}
