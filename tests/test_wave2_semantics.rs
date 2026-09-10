use forgen::driver::ForgenCompiler;
use std::ffi::{CStr, CString};

#[test]
fn test_wave2_overflow_semantics_and_trap() {
    let source_overflow = r#"
fn main() {
    mut max = 9223372036854775807
    mut ovf = max + 1
    out ovf
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source_overflow, "ovf_trap.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let exe = res.exe_path.unwrap();
    let res_run = compiler.codegen.run_executable(&exe, &[]);
    if let Ok((stdout, _, code, _)) = res_run {
        assert_ne!(
            code, 0,
            "Debug mode must trap on integer overflow, but exited with 0 (stdout: {})",
            stdout
        );
    }

    let source_wrap = r#"
fn main() {
    mut max = 9223372036854775807
    mut wrapped = wrapping(max + 1)
    out wrapped
}
"#;
    let res2 = compiler.compile_source(source_wrap, "ovf_wrap.dtr", None);
    assert!(res2.success, "Compilation failed: {:?}", res2.error);
    let exe2 = res2.exe_path.unwrap();
    let (stdout2, _, code2, _) = compiler.codegen.run_executable(&exe2, &[]).unwrap();
    assert_eq!(code2, 0);
    assert_eq!(stdout2.trim(), "-9223372036854775808");
}

#[test]
fn test_wave2_match_exhaustiveness_missing_pattern_diagnostics() {
    let compiler = ForgenCompiler::new("release");

    // 1. Missing enum variants (Pending, Closed)
    let source_enum = r#"
enum Status {
    Active,
    Pending,
    Closed,
}

fn check_status(s: Status) -> Int {
    match s {
        Status.Active => 1,
    }
}

fn main() {
    out check_status(Status.Active)
}
"#;
    let res1 = compiler.compile_source(source_enum, "missing_enum.dtr", None);
    assert!(!res1.success, "Missing variants must fail compilation");
    let diag1 = res1.diagnostics.to_lowercase();
    assert!(
        diag1.contains("e0310") || diag1.contains("non-exhaustive"),
        "Expected E0310/non-exhaustive, got: {}",
        res1.diagnostics
    );
    assert!(
        diag1.contains("missing pattern:")
            && (diag1.contains("pending") || diag1.contains("closed")),
        "Expected missing pattern example, got: {}",
        res1.diagnostics
    );

    // 2. Missing false on Bool
    let source_bool = r#"
fn check_bool(b: Bool) -> Int {
    match b {
        true => 1,
    }
}
fn main() {
    out check_bool(true)
}
"#;
    let res2 = compiler.compile_source(source_bool, "missing_bool.dtr", None);
    assert!(!res2.success, "Missing false must fail compilation");
    let diag2 = res2.diagnostics.to_lowercase();
    assert!(
        diag2.contains("false"),
        "Expected missing pattern false, got: {}",
        res2.diagnostics
    );

    // 3. Guards only without fallback
    let source_guard = r#"
fn check_guard(x: Int) -> Int {
    match x {
        v if v > 0 => 1,
    }
}
fn main() {
    out check_guard(10)
}
"#;
    let res3 = compiler.compile_source(source_guard, "guard_no_fallback.dtr", None);
    assert!(
        !res3.success,
        "Guards without fallback must fail compilation"
    );
    let diag3 = res3.diagnostics.to_lowercase();
    assert!(
        diag3.contains("missing pattern: _") || diag3.contains("non-exhaustive"),
        "Expected missing wildcard pattern suggestion, got: {}",
        res3.diagnostics
    );
}

#[test]
fn test_wave2_unicode_byte_len_char_len_and_o1_byte_access() {
    let source = r#"
fn main() {
    let s = "Привет 🦀"
    out str_len(s)
    out str_chars(s)
    out str_byte_at(s, 0)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "unicode_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    // "Привет" is 12 bytes + 1 space + 4 bytes for crab = 17 bytes
    assert_eq!(lines[0], "17");
    // "Привет 🦀" is 6 letters + 1 space + 1 crab = 8 chars
    assert_eq!(lines[1], "8");
    // First byte of 'П' (0xD0 0x9F) is 0xD0 = 208
    assert_eq!(lines[2], "208");
}

#[test]
fn test_wave2_unicode_sanitization_runtime() {
    unsafe extern "C" {
        fn datara_rt_byte_len(s: *const std::os::raw::c_char) -> usize;
        fn datara_rt_char_len(s: *const std::os::raw::c_char) -> i64;
        fn datara_rt_str_byte_at(s: *const std::os::raw::c_char, idx: i64) -> i64;
        fn datara_rt_str_sanitize_utf8(s: *const std::os::raw::c_char)
        -> *mut std::os::raw::c_char;
        fn datara_rt_free(ptr: *mut std::os::raw::c_void);
    }

    unsafe {
        let valid_str = CString::new("Привет 🦀").unwrap();
        assert_eq!(datara_rt_byte_len(valid_str.as_ptr()), 17);
        assert_eq!(datara_rt_char_len(valid_str.as_ptr()), 8);
        assert_eq!(datara_rt_str_byte_at(valid_str.as_ptr(), 0), 0xD0);
        assert_eq!(datara_rt_str_byte_at(valid_str.as_ptr(), 1), 0x9F);
        assert_eq!(datara_rt_str_byte_at(valid_str.as_ptr(), 100), -1);

        // Test invalid UTF-8 sequence replaced by U+FFFD (0xEF 0xBF 0xBD)
        let invalid_seq = [b'a', 0xFF, 0xFE, b'b', 0x00];
        let sanitized_ptr = datara_rt_str_sanitize_utf8(invalid_seq.as_ptr() as *const _);
        assert!(!sanitized_ptr.is_null());
        let sanitized_cstr = CStr::from_ptr(sanitized_ptr);
        let sanitized_str = sanitized_cstr
            .to_str()
            .expect("Sanitized string must be valid UTF-8");
        // Each invalid byte 0xFF and 0xFE replaced by replacement character '\u{FFFD}'
        assert!(sanitized_str.contains('\u{FFFD}'));
        assert_eq!(sanitized_str, "a\u{FFFD}\u{FFFD}b");
        datara_rt_free(sanitized_ptr as *mut std::os::raw::c_void);
    }
}

#[test]
fn test_wave2_trait_bounds_and_default_methods() {
    let source = r#"
trait Greetable {
    fn greeting_code(&self) -> Int {
        42
    }
}

class User {
    id: Int,
}

impl Greetable for User {
}

fn main() {
    let u = User { id: 1 };
    let code = u.greeting_code();
    out code;
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "trait_default_method.dtr", None);
    assert!(
        res.success,
        "Trait default method compilation failed: {:?}",
        res.error
    );
    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn test_wave2_parser_depth_limit_e0105() {
    let mut nested = String::from("1");
    for _ in 0..70 {
        nested = format!("({})", nested);
    }
    let source = format!("fn main() {{ out {} }}", nested);

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(&source, "depth_limit.dtr", None);
    assert!(
        !res.success,
        "Deeply nested expression must fail compilation"
    );
    assert!(
        res.diagnostics.contains("E0105"),
        "Diagnostics must contain error code E0105, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.to_lowercase().contains("nesting")
            || res.diagnostics.to_lowercase().contains("limit"),
        "Diagnostics must describe nesting limit exceeded, got:\n{}",
        res.diagnostics
    );
}
