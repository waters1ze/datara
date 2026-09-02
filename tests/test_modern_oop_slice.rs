use forgen::driver::ForgenCompiler;

#[test]
fn test_modern_oop_composition_execution() {
    let source = r#"
component Audited {
    audit_id: Str
}

role Serializable {
    serialize() -> Str
}

class User {
    id: Int
    name: Str
    greet() -> Str => "Hello " + this.name
}

class Admin with Serializable {
    using User
    using Audited
    role_level: Int
    serialize() -> Str => "Admin:" + this.name
}

behavior Admin {
    replaces User.greet() -> Str => "Admin Hello " + this.name
}

fn main() {
    let admin = Admin {
        id: 1,
        name: "Alice",
        audit_id: "AUDIT-99",
        role_level: 10
    }
    out admin.greet()
    out admin.serialize()
    out admin.audit_id
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_admin.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    println!(
        "[MODERN_OOP CLIF]:\n{}",
        res.clif_source.as_deref().unwrap_or("")
    );
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    println!("[MODERN_OOP STDOUT]:\n{}", stdout);
    assert_eq!(code, 0);
    assert!(stdout.contains("Admin Hello Alice"));
    assert!(stdout.contains("Admin:Alice"));
    assert!(stdout.contains("AUDIT-99"));
}

#[test]
fn test_class_inheritance_from_rejected() {
    let source = r#"
class Base { id: Int }
class Child from Base { extra: Int }
fn main() { out "hi" }
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_from_fail.dtr", None);
    assert!(!res.success);
    let err = res.error.unwrap_or_default();
    assert!(err.contains("Class inheritance ('from'/'extends') has been removed"));
}

#[test]
fn test_modern_oop_negative_ambiguous_override() {
    let source = r#"
class Service {
    name: Str
    start() -> Str => "Starting service"
}

behavior Service {
    start() -> Str => "Conflicting start"
}

fn main() {
    out "done"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_ambiguous.dtr", None);
    assert!(!res.success, "Must fail with ambiguous override");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("[E-AMBIGUOUS-OVERRIDE]"),
        "Error was: {}",
        err_str
    );
}

#[test]
fn test_modern_oop_negative_unsatisfied_role() {
    let source = r#"
role Printable {
    format() -> Str
}

class Invoice with Printable {
    amount: Int
}

fn main() {
    out "done"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_role_fail.dtr", None);
    assert!(!res.success, "Must fail with unsatisfied role");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("[E-ROLE-UNSATISFIED]"),
        "Error was: {}",
        err_str
    );
}
