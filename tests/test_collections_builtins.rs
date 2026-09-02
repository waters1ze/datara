//! Integration Tests for Built-in Collections, Slices, and Core Prelude
//! Verifies List, Array, Map, slicing [start..end], and Prelude (println, len, now, assert).

use forgen::driver::ForgenCompiler;

fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, None);
    assert!(
        res.success,
        "compilation failed for {}: {:?}",
        name, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", name, code);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_list_push_pop_len() {
    let out = run_datara(
        r#"
fn main() {
    mut xs = [10, 20, 30]
    xs.push(40)
    out xs.len()
    let last = xs.pop()
    out last
    out xs.len()
}
"#,
        "test_list_ops",
    );
    assert_eq!(out, "4\n40\n3");
}

#[test]
fn test_list_slicing() {
    let out = run_datara(
        r#"
fn main() {
    let xs = [10, 20, 30, 40, 50]
    let sub = xs[1..4]
    out sub.len()
    out sub[0]
    out sub[1]
    out sub[2]
}
"#,
        "test_list_slice",
    );
    assert_eq!(out, "3\n20\n30\n40");
}

#[test]
fn test_array_repeat_literal() {
    let out = run_datara(
        r#"
fn main() {
    let arr = [7; 4]
    out arr.len()
    out arr[0]
    out arr[3]
}
"#,
        "test_array_repeat",
    );
    assert_eq!(out, "4\n7\n7");
}

#[test]
fn test_map_literal_and_lookup() {
    let out = run_datara(
        r#"
fn main() {
    let m = ["alpha": 100, "beta": 200]
    out m["alpha"]
    out m["beta"]
}
"#,
        "test_map_ops",
    );
    assert_eq!(out, "100\n200");
}

#[test]
fn test_map_insert_and_lookup() {
    let out = run_datara(
        r#"
fn main() {
    mut m = ["alpha": 10, "beta": 20]
    m.insert("gamma", 30)
    out m["alpha"]
    out m["gamma"]
}
"#,
        "test_map_insert",
    );
    assert_eq!(out, "10\n30");
}

#[test]
fn test_prelude_functions() {
    let out = run_datara(
        r#"
fn main() {
    println("Core Prelude Active")
    let msg = "Datara 2026"
    out len(msg)
    let t = now()
    assert(t > 0, "timestamp must be positive")
    println("Verified")
}
"#,
        "test_prelude_builtins",
    );
    assert_eq!(out, "Core Prelude Active\n11\nVerified");
}
