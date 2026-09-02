use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;

#[test]
fn test_views_safety_positive_zero_copy_view() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
class Dataset {
    name: String
    size: Int
}

behavior Dataset {
    summary() -> String => this.name + " (" + this.size + " items)"
}

fn main() {
    let data = Dataset { name: "AuditLogs", size: 4200 }
    let v = data.view()
    out v.summary()
}
"#;
    let res = compiler.compile_source(code, "view_pos.dtr", None);
    assert!(res.success, "Positive view test failed: {:?}", res.error);
    let (stdout, _, code_res, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_res, 0);
    assert_eq!(stdout.trim(), "AuditLogs (4200 items)");
}

#[test]
fn test_views_safety_negative_view_after_move() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
class Buffer {
    capacity: Int
}

fn main() {
    let buf = Buffer { capacity: 1024 }
    destroy(buf)
    let v = view(buf)
}
"#;
    let res = compiler.compile_source(code, "view_after_move.dtr", None);
    assert!(
        !res.success,
        "Expected failure when creating view after move"
    );
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowUseAfterMove.as_str()),
        "Expected BorrowUseAfterMove diagnostic, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_views_safety_negative_mutation_during_view() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
class Buffer {
    capacity: Int
}

fn main() {
    mut buf = Buffer { capacity: 1024 }
    let v = view(buf)
    mut buf = Buffer { capacity: 2048 }
}
"#;
    let res = compiler.compile_source(code, "mutate_during_view.dtr", None);
    assert!(
        !res.success,
        "Expected failure when mutating buffer during active view"
    );
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowConflictActiveView.as_str()),
        "Expected BorrowConflictActiveView diagnostic, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_views_safety_negative_escaping_local_view() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
class Record {
    id: Int
}

fn get_local_view() -> Record {
    let rec = Record { id: 99 }
    let v = view(rec)
    return v
}
"#;
    let res = compiler.compile_source(code, "escaping_view.dtr", None);
    assert!(!res.success, "Expected failure when returning local view");
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowEscapingView.as_str()),
        "Expected BorrowEscapingView diagnostic, got:\n{}",
        res.diagnostics
    );
}
