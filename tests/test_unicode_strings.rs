use forgen::driver::ForgenCompiler;
use std::ffi::CString;
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
fn test_unicode_cyrillic_scalars_and_loop() {
    let code = r#"
fn main() {
    let s = "Привет"
    out str_len(s)
    out str_chars(s)
    mut count = 0
    for ch in s {
        count = count + 1
        out ch
    }
    out count
}
"#;
    let (stdout, code) = run_datara(code, "test_cyrillic");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    // str_len("Привет") is 12 bytes
    assert_eq!(lines[0], "12");
    // str_chars("Привет") is 6 scalars
    assert_eq!(lines[1], "6");
    // 6 characters printed: П, р, и, в, е, т
    assert_eq!(lines[2], "П");
    assert_eq!(lines[3], "р");
    assert_eq!(lines[4], "и");
    assert_eq!(lines[5], "в");
    assert_eq!(lines[6], "е");
    assert_eq!(lines[7], "т");
    // count is 6
    assert_eq!(lines[8], "6");
}

#[test]
fn test_unicode_emoji_scalars_and_loop() {
    let code = r#"
fn main() {
    let s = "🦀🚀🎉"
    out str_len(s)
    out str_chars(s)
    mut count = 0
    for em in s {
        count = count + 1
        out em
    }
    out count
}
"#;
    let (stdout, code) = run_datara(code, "test_emoji");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    // 3 emojis * 4 bytes each = 12 bytes
    assert_eq!(lines[0], "12");
    // 3 scalar values
    assert_eq!(lines[1], "3");
    assert_eq!(lines[2], "🦀");
    assert_eq!(lines[3], "🚀");
    assert_eq!(lines[4], "🎉");
    assert_eq!(lines[5], "3");
}

#[test]
fn test_unicode_combining_marks() {
    let code = r#"
fn main() {
    // "e\u{0301}" is 'e' followed by combining acute accent: 2 scalars, 3 bytes
    let s = "é"
    out str_len(s)
    out str_chars(s)
}
"#;
    let (stdout, code) = run_datara(code, "test_combining");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "3");
    assert_eq!(lines[1], "2");
}

#[test]
fn test_unicode_utf8_validation_in_runtime() {
    // Test runtime validate_utf8 directly via FFI
    unsafe extern "C" {
        fn datara_rt_validate_utf8(s: *const std::os::raw::c_char) -> i32;
        fn datara_rt_str_chars(s: *const std::os::raw::c_char) -> i64;
    }

    unsafe {
        // Valid UTF-8
        let valid_ascii = CString::new("Hello World").unwrap();
        assert_eq!(datara_rt_validate_utf8(valid_ascii.as_ptr()), 1);

        let valid_cyrillic = CString::new("Мир").unwrap();
        assert_eq!(datara_rt_validate_utf8(valid_cyrillic.as_ptr()), 1);
        assert_eq!(datara_rt_str_chars(valid_cyrillic.as_ptr()), 3);

        let valid_emoji = CString::new("🎉").unwrap();
        assert_eq!(datara_rt_validate_utf8(valid_emoji.as_ptr()), 1);
        assert_eq!(datara_rt_str_chars(valid_emoji.as_ptr()), 1);

        // Invalid: overlong ASCII (0xC0 0xAF)
        let invalid_overlong = [0xC0u8, 0xAFu8, 0x00u8];
        assert_eq!(
            datara_rt_validate_utf8(invalid_overlong.as_ptr() as *const _),
            0
        );

        // Invalid: lone surrogate (0xED 0xA0 0x80 = U+D800)
        let invalid_surrogate = [0xEDu8, 0xA0u8, 0x80u8, 0x00u8];
        assert_eq!(
            datara_rt_validate_utf8(invalid_surrogate.as_ptr() as *const _),
            0
        );

        // Invalid: truncated 4-byte sequence (0xF0 0x9F 0x90 without 4th byte)
        let truncated = [0xF0u8, 0x9Fu8, 0x90u8, 0x00u8];
        assert_eq!(datara_rt_validate_utf8(truncated.as_ptr() as *const _), 0);

        // Invalid byte: 0xFF
        let invalid_byte = [0xFFu8, 0x00u8];
        assert_eq!(
            datara_rt_validate_utf8(invalid_byte.as_ptr() as *const _),
            0
        );
    }
}

#[test]
fn test_str_split_and_join_execution() {
    let code = r#"
fn main() {
    let text = "apple,banana,cherry"
    let parts = str_split(text, ",")
    for p in parts {
        out p
    }
    let joined = str_join(parts, " -> ")
    out joined
}
"#;
    let (stdout, code) = run_datara(code, "test_split_join");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "apple");
    assert_eq!(lines[1], "banana");
    assert_eq!(lines[2], "cherry");
    assert_eq!(lines[3], "apple -> banana -> cherry");
}

#[test]
fn test_str_split_and_join_unicode() {
    let code = r#"
fn main() {
    let text = "один::два::три"
    let parts = str_split(text, "::")
    let joined = str_join(parts, " + ")
    out joined
}
"#;
    let (stdout, code) = run_datara(code, "test_split_join_unicode");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "один + два + три");
}
