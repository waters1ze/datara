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
fn test_enum_unit_variants() {
    let code = r#"
enum Color {
    Red,
    Green,
    Blue,
}

fn get_code(c: Color) -> Int {
    match c {
        Color.Red => 10,
        Color.Green => 20,
        Color.Blue => 30,
        _ => 0,
    }
}

fn main() {
    let c = Color.Green
    let v = get_code(c)
    if v == 20 {
        out "ENUM_UNIT_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_enum_unit_variants.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("ENUM_UNIT_OK"));
}

#[test]
fn test_enum_payload_variants_adt() {
    let code = r#"
enum Shape {
    Circle(Float),
    Rect(Float, Float),
    Point,
}

fn compute_val(s: Shape) -> Int {
    match s {
        Shape.Circle(r) => 100,
        Shape.Rect(w, h) => 200,
        Shape.Point => 300,
        _ => 0,
    }
}

fn main() {
    let c = Shape.Circle(10.5)
    let v1 = compute_val(c)
    let p = Shape.Point
    let v2 = compute_val(p)
    if v1 == 100 && v2 == 300 {
        out "ENUM_ADT_OK"
    }
}
"#;
    let (out, code_ret) = run_datara(code, "test_enum_payload_variants_adt.dtr");
    assert_eq!(code_ret, 0);
    assert!(out.contains("ENUM_ADT_OK"));
}
