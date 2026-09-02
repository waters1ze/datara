use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;

#[test]
fn test_soundness_positive_multiple_immutable_views() {
    let source = r#"
fn inspect_data(a: String, b: String) {
    out a
    out b
}

fn main() {
    let data = "Datara Language"
    let v1 = view(data)
    let v2 = view(data)
    out data
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "sound_views.dtr", None);
    assert!(
        res.success,
        "Multiple immutable views must be valid: {:?}",
        res.error
    );
}

#[test]
fn test_soundness_negative_move_while_actively_borrowed() {
    let source = r#"
fn bad() {
    let data = 100
    let v = view(data)
    destroy(data)
    out v
}

fn main() {
    bad()
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_destroy_borrow.dtr", None);
    assert!(
        !res.success,
        "Moving value while actively borrowed must fail"
    );
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowConflictActiveView.as_str())
            || res.diagnostics.contains("actively borrowed")
    );
}

#[test]
fn test_soundness_negative_call_simultaneous_alias() {
    let source = r#"
fn process(a: String, b: String) {
    out a
}

fn main() {
    mut data = "Shared"
    process(view(data), mut_view(data))
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_call_alias.dtr", None);
    assert!(
        !res.success,
        "Simultaneous mutable and immutable alias in call args must fail"
    );
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowConflictActiveView.as_str())
            || res.diagnostics.contains("alias")
    );
}

#[test]
fn test_soundness_negative_escaping_local_view() {
    let source = r#"
fn create_dangling() -> String {
    let local_val = "Stack Memory"
    let dangling = view(local_val)
    return dangling
}

fn main() {
    let s = create_dangling()
    out s
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_escape.dtr", None);
    assert!(!res.success, "Returning view of local variable must fail");
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowEscapingView.as_str())
            || res.diagnostics.contains("local variable")
    );
}
