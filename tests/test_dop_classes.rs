//! Stage 2: Data-Oriented Programming (DOP) Classes, Methods, UFCS & Composition Tests

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
fn test_method_inside_class_declaration() {
    let out = run_datara(
        r#"
class Counter {
    val: Int
    fn get_val() -> Int {
        return this.val
    }
}

fn main() {
    let c = Counter { val: 42 }
    out c.get_val()
}
"#,
        "test_class_method",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ufcs_pipeline_and_method_syntax() {
    let source = r#"
fn double(x: Int) -> Int {
    return x * 2
}

fn add_ten(x: Int) -> Int {
    return x + 10
}

fn main() {
    let a = 5
    // Method call style via UFCS:
    let r1 = a.double()
    // Pipeline style:
    let r2 = a |> double() |> add_ten()
    out r1
    out r2
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_ufcs.dtr", None);
    if let Some(m) = &res.dmir_module {
        if let Some(main_fn) = m.functions.get("main") {
            for b in &main_fn.blocks {
                eprintln!("MAIN BLOCK {}:", b.id.0);
                for i in &b.instructions {
                    eprintln!("  {:?}", i);
                }
            }
        }
    }
    let out = run_datara(source, "test_ufcs");
    assert_eq!(out, "10\n20");
}

#[test]
fn test_using_flat_composition() {
    let out = run_datara(
        r#"
class User {
    id: Int
    fn get_id() -> Int {
        return this.id
    }
}

class Admin {
    using User
    level: Int
}

fn main() {
    let a = Admin { id: 101, level: 5 }
    out a.id
    out a.level
    out a.get_id()
}
"#,
        "test_using_composition",
    );
    assert_eq!(out, "101\n5\n101");
}
