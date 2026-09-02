use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;

#[test]
fn test_negative_use_after_move() {
    let source = r#"
fn bad() {
    let data = 100
    destroy(data)
    out data
}

fn main() {
    bad()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_move.dtr", None);
    assert!(!res.success, "Should fail on use after move");
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::BorrowUseAfterMove.as_str()) || diag.contains("moved"),
        "Must report use after move"
    );
}

#[test]
fn test_negative_mutate_during_active_view() {
    let source = r#"
fn bad() {
    mut data = 42
    let a = view(data)
    data = 99
}

fn main() {
    bad()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_view_mut.dtr", None);
    assert!(!res.success, "Should fail on mutation during active view");
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::BorrowConflictActiveView.as_str()) || diag.contains("borrowed"),
        "Must report conflict with active view"
    );
}

#[test]
fn test_negative_multiple_mutable_views() {
    let source = r#"
fn bad() {
    let data = 42
    let a = mut_view(data)
    let b = mut_view(data)
}

fn main() {
    bad()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_multiple_mut.dtr", None);
    assert!(!res.success, "Should fail on multiple mutable views");
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::BorrowMultipleMutableViews.as_str()) || diag.contains("mutable"),
        "Must report multiple mutable views conflict"
    );
}

#[test]
fn test_negative_mutate_immutable_binding() {
    let source = r#"
fn bad() {
    let x = 10
    x = 20
}

fn main() {
    bad()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_immutable.dtr", None);
    assert!(!res.success, "Should fail when mutating immutable binding");
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::BorrowCannotMutateImmutable.as_str())
            || diag.contains("immutable"),
        "Must report immutable binding mutation error"
    );
}
