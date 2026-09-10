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
fn test_string_concatenation_coercions() {
    let code = r#"
fn main() {
    let s1 = "Items: " + 42
    let s2 = 100 + " meters"
    let s3 = "Pi: " + 3.14
    let s4 = "Flag: " + true
    let s5 = false + " is status"
    let s6 = "Multi: " + 1 + ", " + 2 + ", " + 3
    out s1
    out s2
    out s3
    out s4
    out s5
    out s6
}
"#;
    let (out, code_ret) = run_datara(code, "test_str_concat_coercion.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("Items: 42"));
    assert!(out.contains("100 meters"));
    assert!(out.contains("Pi: 3.14"));
    assert!(out.contains("Flag: true"));
    assert!(out.contains("false is status"));
    assert!(out.contains("Multi: 1, 2, 3"));
}

#[test]
fn test_string_equality_content() {
    let code = r#"
fn main() {
    let a = "hello"
    let b = "world"
    let c = "hello"
    
    if a == c {
        out "EQ_OK"
    }
    if a != b {
        out "NEQ_OK"
    }
    let res_false = a == b
    if res_false == false {
        out "FALSE_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_str_eq.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("EQ_OK"));
    assert!(out.contains("NEQ_OK"));
    assert!(out.contains("FALSE_OK"));
}

#[test]
fn test_multi_value_fstring_interpolation() {
    let code = r#"
fn main() {
    let user = "Alice"
    let age = 30
    let balance = 125.75
    let msg = fmt"User {user} is {age} years old with balance {balance}"
    out msg
}
"#;
    let (out, code_ret) = run_datara(code, "test_fstring_multi.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("User Alice is 30 years old with balance 125.75"));
}

#[test]
fn test_stdlib_string_and_format_builtins() {
    let code = r#"
fn main() {
    let rep = str_repeat("abc", 3)
    let pl = str_pad_left("42", 5, "0")
    let pr = str_pad_right("hi", 5, ".")
    let rep_str = str_replace("foo bar foo", "foo", "baz")
    let up = str_to_upper("hello")
    let low = str_to_lower("WORLD")
    let pct = format_percent(0.854, 1)
    let commas = format_int_with_commas(1234567)
    
    out rep
    out pl
    out pr
    out rep_str
    out up
    out low
    out pct
    out commas
}
"#;
    let (out, code_ret) = run_datara(code, "test_str_format_builtins.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("abcabcabc"));
    assert!(out.contains("00042"));
    assert!(out.contains("hi..."));
    assert!(out.contains("baz bar baz"));
    assert!(out.contains("HELLO"));
    assert!(out.contains("world"));
    assert!(out.contains("85.4%"));
    assert!(out.contains("1,234,567"));
}

#[test]
fn test_js_interop_inprocess_engine() {
    let code = r#"
fn main() {
    let s_eval = js_eval("2 + 3 * 4")
    let i_eval = js_eval_int("10 + 25")
    let f_eval = js_eval_float("3.14 * 2.0")
    
    let set_ok = js_set_global("my_var", "100")
    let get_val = js_get_global("my_var")
    
    let fn_eval = js_eval("let add = (a, b) => a + b; add(20, 22)")
    
    out "S_EVAL: " + s_eval
    out "I_EVAL: " + i_eval
    out "F_EVAL: " + f_eval
    out "GET_VAL: " + get_val
    out "FN_EVAL: " + fn_eval
}
"#;
    let (out, code_ret) = run_datara(code, "test_js_engine.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("S_EVAL: 14"));
    assert!(out.contains("I_EVAL: 35"));
    assert!(out.contains("F_EVAL: 6.28"));
    assert!(out.contains("GET_VAL: 100"));
    assert!(out.contains("FN_EVAL: 42"));
}

#[test]
fn test_stdlib_classes_interop() {
    let code = r#"
class JS {
    version: Str
}
behavior JS {
    eval(code: Str) -> Str {
        return js_eval(code)
    }
    eval_int(code: Str) -> Int {
        return js_eval_int(code)
    }
}

class StringUtils {
    prefix: Str
}
behavior StringUtils {
    repeat(s: Str, count: Int) -> Str {
        return str_repeat(s, count)
    }
    to_upper(s: Str) -> Str {
        return str_to_upper(s)
    }
}

fn main() {
    let js = JS { version: "1.0" }
    let res = js.eval("Math.min(50, 20)")
    let n = js.eval_int("100 * 3")
    
    let su = StringUtils { prefix: "" }
    let rep = su.repeat("xyz", 2)
    let up = su.to_upper("abc")
    
    out "JS_RES: " + res
    out "JS_INT: " + n
    out "REP: " + rep
    out "UP: " + up
}
"#;
    let (out, code_ret) = run_datara(code, "test_classes_interop.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("JS_RES: 20"));
    assert!(out.contains("JS_INT: 300"));
    assert!(out.contains("REP: xyzxyz"));
    assert!(out.contains("UP: ABC"));
}
