use forgen::driver::ForgenCompiler;

#[test]
fn test_trait_declaration_and_impl() {
    let source = r#"
trait Printable {
    fn print_val(&self) -> Int;
}

class Point {
    x: Int,
    y: Int,
}

impl Printable for Point {
    fn print_val(&self) -> Int {
        out self.x;
        0
    }
}

fn main() {
    let p = Point { x: 42, y: 100 };
    p.print_val();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_basic.dtr", None);
    assert!(
        res.success,
        "Trait declaration and impl compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn test_multiple_trait_impls() {
    let source = r#"
trait Area {
    fn calculate_area(&self) -> Int;
}

class Square {
    side: Int,
}

class Rectangle {
    w: Int,
    h: Int,
}

impl Area for Square {
    fn calculate_area(&self) -> Int {
        self.side * self.side
    }
}

impl Area for Rectangle {
    fn calculate_area(&self) -> Int {
        self.w * self.h
    }
}

fn main() {
    let s = Square { side: 5 };
    let r = Rectangle { w: 4, h: 6 };
    out s.calculate_area();
    out r.calculate_area();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_multi_impl.dtr", None);
    assert!(
        res.success,
        "Multiple trait impls compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["25", "24"]);
}

#[test]
fn test_trait_generic_bound_monomorphization() {
    let source = r#"
trait Printable {
    fn print_val(&self) -> Int;
}

class Point {
    x: Int,
    y: Int,
}

impl Printable for Point {
    fn print_val(&self) -> Int {
        out self.x;
        0
    }
}

fn show_item<T: Printable>(item: T) -> Int {
    item.print_val();
    0
}

fn main() {
    let p = Point { x: 77, y: 88 };
    show_item(p);
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_bound.dtr", None);
    assert!(
        res.success,
        "Trait generic bound compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "77");
}

#[test]
fn test_inherent_impl_block() {
    let source = r#"
class Vector2 {
    x: Int,
    y: Int,
}

impl Vector2 {
    fn sum(&self) -> Int {
        self.x + self.y
    }
}

fn main() {
    let v = Vector2 { x: 15, y: 25 };
    out v.sum();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "inherent_impl.dtr", None);
    assert!(
        res.success,
        "Inherent impl compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "40");
}

#[test]
fn test_missing_trait_method_rejection() {
    let source = r#"
trait Greet {
    fn hello(&self) -> Int;
    fn goodbye(&self) -> Int;
}

class Person {
    age: Int,
}

impl Greet for Person {
    fn hello(&self) -> Int {
        1
    }
}

fn main() {
    let p = Person { age: 20 };
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.check_source(source, "missing_method.dtr");
    assert!(
        !res.success,
        "Compiler should reject impl missing required trait method"
    );
    assert!(
        res.diagnostics
            .contains("Missing implementation of trait method 'goodbye'")
    );
}

#[test]
fn test_trait_bound_violation_rejection() {
    let source = r#"
trait Serializable {
    fn serialize(&self) -> Int;
}

class Secret {
    value: Int,
}

fn write_data<T: Serializable>(data: T) -> Int {
    data.serialize();
    0
}

fn main() {
    let s = Secret { value: 999 };
    write_data(s);
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.check_source(source, "bound_violation.dtr");
    assert!(
        !res.success,
        "Compiler should reject calling bounded function with unsatisfied trait"
    );
    assert!(
        res.diagnostics
            .contains("does not satisfy trait bound 'Serializable'")
    );
}

#[test]
fn test_trait_default_method_inherited() {
    let source = r#"
trait Greet {
    fn name(&self) -> Int;
    fn greeting(&self) -> Int {
        out 123;
        0
    }
}

class User {
    id: Int,
}

impl Greet for User {
    fn name(&self) -> Int {
        self.id
    }
}

fn main() {
    let u = User { id: 7 };
    u.greeting();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_default_inherited.dtr", None);
    assert!(
        res.success,
        "Trait default method compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "123");
}

#[test]
fn test_trait_default_method_overridden() {
    let source = r#"
trait Greet {
    fn greeting(&self) -> Int {
        out 111;
        0
    }
}

class SpecialUser {
    val: Int,
}

impl Greet for SpecialUser {
    fn greeting(&self) -> Int {
        out 999;
        0
    }
}

fn main() {
    let u = SpecialUser { val: 5 };
    u.greeting();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_default_overridden.dtr", None);
    assert!(
        res.success,
        "Trait default overridden compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "999");
}

#[test]
fn test_trait_super_trait_default_inheritance() {
    let source = r#"
trait Base {
    fn base_action(&self) -> Int {
        out 888;
        0
    }
}

trait Sub : Base {
    fn sub_action(&self) -> Int;
}

class Robot {
    code: Int,
}

impl Sub for Robot {
    fn sub_action(&self) -> Int {
        self.code
    }
}

fn main() {
    let r = Robot { code: 10 };
    r.base_action();
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "super_trait_default.dtr", None);
    assert!(
        res.success,
        "Super-trait default method compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "888");
}

#[test]
fn test_generic_trait_bound_invalid_method_rejected() {
    let source = r#"
trait Printable {
    fn print_val(&self) -> Int;
}

fn test_invalid<T: Printable>(item: T) -> Int {
    item.non_existent_method();
    0
}

fn main() {
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.check_source(source, "invalid_bound_method.dtr");
    assert!(
        !res.success,
        "Compiler should reject method not declared in trait bounds"
    );
    assert!(
        res.diagnostics
            .contains("Method 'non_existent_method' not found for type parameter 'T'")
    );
}

#[test]
fn test_closure_capture_inference_and_inlining() {
    let source = r#"
fn main() {
    let add = (a, b) => a + b;
    let result = add(30, 12);
    out result;
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "closure_inline.dtr", None);
    assert!(
        res.success,
        "Closure inlining compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn test_closure_capture_analysis_unit() {
    use forgen::ast::{CaptureKind, Expr, Param, SourceSpan, infer_captures};
    use std::collections::HashSet;

    let span = SourceSpan::default();
    let params = vec![Param {
        name: "x".to_string(),
        type_node: None,
        ownership_mode: "val".to_string(),
        span: span.clone(),
    }];
    // body: x + y + z
    let body = Expr::Binary {
        op: "+".to_string(),
        left: Box::new(Expr::Binary {
            op: "+".to_string(),
            left: Box::new(Expr::Identifier("x".to_string(), span.clone())),
            right: Box::new(Expr::Identifier("y".to_string(), span.clone())),
            span: span.clone(),
        }),
        right: Box::new(Expr::Identifier("z".to_string(), span.clone())),
        span: span.clone(),
    };

    let mut enclosing = HashSet::new();
    enclosing.insert("y".to_string());
    enclosing.insert("z".to_string());
    enclosing.insert("other".to_string());

    let non_escaping_captures = infer_captures(&params, &body, &enclosing, false);
    assert_eq!(non_escaping_captures.len(), 2);
    assert!(
        non_escaping_captures
            .iter()
            .all(|c| c.kind == CaptureKind::ByRef)
    );

    let escaping_captures = infer_captures(&params, &body, &enclosing, true);
    assert_eq!(escaping_captures.len(), 2);
    assert!(
        escaping_captures
            .iter()
            .all(|c| c.kind == CaptureKind::ByMove)
    );
}

#[test]
fn test_trait_self_return_type_and_constructor() {
    let source = r#"
trait Builder {
    fn create(v: Int) -> Self {
        Self { val: v }
    }
}

class Widget {
    val: Int,
}

impl Builder for Widget {
}

fn main() {
    let w = Widget { val: 99 };
    let w2 = w.create(777);
    out w2.val;
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "trait_self_builder.dtr", None);
    assert!(
        res.success,
        "Trait Self return type and constructor compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "777");
}
