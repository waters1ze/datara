use forgen::ForgenCompiler;

#[test]
fn test_comptime_expression_folding() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
fn main() -> Int {
    let x: Int = comptime { 10 + 20 * 2 }
    return x
}
"#;
    let res = compiler.check_source(src, "test_comptime.dtr");
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);
}

#[test]
fn test_structural_derive_display_and_json() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
@derive(Display, Json, Hash, Clone, Deserialize)
class UserProfile {
    id: Int
    name: Str
    active: Bool
}

fn main() -> Str {
    let p = UserProfile { id: 42, name: "Alice", active: true }
    let s: Str = p.to_string()
    let j: Str = p.to_json()
    let h: Int = p.hash()
    let c = p.clone()
    let parsed = UserProfile.from_json("{\"id\":1}")
    return s
}
"#;
    let res = compiler.check_source(src, "test_derive.dtr");
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);
}

#[test]
fn test_derive_methods_execution() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
@derive(Display, Json, Hash, Clone)
class Point {
    x: Int
    y: Int
}

fn main() -> Int {
    let p1 = Point { x: 10, y: 20 }
    let p2 = p1.clone()
    let h1 = p1.hash()
    let h2 = p2.hash()
    if h1 == h2 {
        return 0
    }
    return 1
}
"#;
    let out_dir = std::env::temp_dir().join("datara_test_derive_bin");
    let _ = std::fs::create_dir_all(&out_dir);
    let exe_path = out_dir.join("test_derive.exe");

    let res = compiler.compile_source(src, "test_point.dtr", Some(&exe_path));
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    if let Some(ref path) = res.exe_path {
        let output = std::process::Command::new(path)
            .output()
            .expect("Failed to execute compiled binary");
        assert_eq!(output.status.code(), Some(0));
    }
}
