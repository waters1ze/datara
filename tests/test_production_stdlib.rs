use forgen::driver::ForgenCompiler;
use std::fs;

fn run_datara(code: &str, tag: &str) -> String {
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
    assert_eq!(code, 0, "{} exited with {}", tag, code);

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_real_file_io_system() {
    let test_file = "test_stdlib_fs_output.txt";
    let _ = fs::remove_file(test_file);

    let code = format!(
        r#"
class File {{
    path: Str
}}

behavior File {{
    read(token: Capability<FileRead>) -> Str {{
        let _ = token
        return file_read(this.path)
    }}

    write(content: Str, token: Capability<FileWrite>) -> Bool {{
        let _ = token
        let res = file_write(this.path, content)
        return res == 1
    }}

    append(content: Str, token: Capability<FileWrite>) -> Bool {{
        let _ = token
        let res = file_append(this.path, content)
        return res == 1
    }}

    exists() -> Bool {{
        let res = file_exists(this.path)
        return res == 1
    }}
}}

fn main(sys_caps: SystemCapabilities) {{
    let r_token = sys_caps.files.grant_readonly("{test_file}")
    let w_token = sys_caps.files.grant_readwrite("{test_file}")
    let f = File {{ path: "{test_file}" }}
    out f.exists()
    f.write("Hello Datara Production FileSystem!\n", w_token)
    out f.exists()
    f.append("Second line appended.\n", w_token)
    let content = f.read(r_token)
    out content
}}
"#,
        test_file = test_file.replace('\\', "/")
    );

    let out = run_datara(&code, "test_file_io");
    let _ = fs::remove_file(test_file);

    assert!(out.contains("false"), "Initially exists should be false");
    assert!(out.contains("true"), "After write exists should be true");
    assert!(
        out.contains("Hello Datara Production FileSystem!"),
        "Content must match"
    );
    assert!(
        out.contains("Second line appended."),
        "Appended content must match"
    );
}

#[test]
fn test_string_primitives_suite() {
    let code = r#"
class StringUtils {
    prefix: Str
}

behavior StringUtils {
    contains(s: Str, sub: Str) -> Bool {
        let res = str_contains(s, sub)
        return res == 1
    }

    starts_with(s: Str, prefix: Str) -> Bool {
        let res = str_starts_with(s, prefix)
        return res == 1
    }

    ends_with(s: Str, suffix: Str) -> Bool {
        let res = str_ends_with(s, suffix)
        return res == 1
    }

    index_of(s: Str, sub: Str) -> Int {
        return str_index_of(s, sub)
    }

    trim(s: Str) -> Str {
        return str_trim(s)
    }

    to_int(s: Str) -> Int {
        return str_to_int(s)
    }
}

fn main() {
    let u = StringUtils { prefix: "" }
    out u.contains("compiler_architecture", "archi")
    out u.contains("compiler_architecture", "xyz")
    out u.starts_with("forgen_backend", "forgen")
    out u.ends_with("forgen_backend", "end")
    out u.index_of("hello_world", "world")
    out u.trim("   trimmed_content   ")
    out u.to_int("2026") + 10
}
"#;

    let out = run_datara(code, "test_str_primitives");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "true");
    assert_eq!(lines[1], "false");
    assert_eq!(lines[2], "true");
    assert_eq!(lines[3], "true");
    assert_eq!(lines[4], "6");
    assert_eq!(lines[5], "trimmed_content");
    assert_eq!(lines[6], "2036");
}

#[test]
fn test_local_mutation_effect_purity() {
    let code = r#"
fn pure_with_local_mutation(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}

fn main() {
    let res = pure_with_local_mutation(10)
    out res
}
"#;
    let out = run_datara(code, "test_local_purity");
    assert_eq!(out, "45");
}

#[test]
fn test_fast_math_primitives() {
    let code = r#"
fn main() {
    let s = math_sqrt(16.0)
    let p = math_pow(2.0, 10.0)
    let a = math_abs(-42.5)
    let m = math_max(10.5, 20.25)
    let mi = math_min_int(100, 50)
    let ma = math_max_int(100, 50)
    let ai = math_abs_int(-777)
    out s
    out p
    out a
    out m
    out mi
    out ma
    out ai
}
"#;
    let out = run_datara(code, "test_fast_math");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "4");
    assert_eq!(lines[1], "1024");
    assert_eq!(lines[2], "42.5");
    assert_eq!(lines[3], "20.25");
    assert_eq!(lines[4], "50");
    assert_eq!(lines[5], "100");
    assert_eq!(lines[6], "777");
}
