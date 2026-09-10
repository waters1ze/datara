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

#[test]
fn test_negative_read_during_active_mutable_view() {
    let source = r#"
fn bad() {
    mut data = 42
    let a = mut_view(data)
    out data
}

fn main() {
    bad()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_read_mut.dtr", None);
    assert!(
        !res.success,
        "Should fail on read during active mutable view"
    );
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::BorrowConflict.as_str())
            || diag.contains("actively mutably borrowed"),
        "Must report borrow conflict when reading actively mutably borrowed variable"
    );
}

#[test]
fn test_contract_static_violation() {
    let source = r#"
fn divide(a: Int, b: Int) -> Int
require b != 0, "divisor cannot be zero";
{
    return a / b
}

fn main() {
    let x = divide(10, 0)
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "bad_contract.dtr", None);
    assert!(
        !res.success,
        "Should fail at compile-time on static contract violation"
    );
    let diag = res.diagnostics;
    println!("Diagnostics:\n{}", diag);
    assert!(
        diag.contains(ErrorCode::ContractViolation.as_str())
            || diag.contains("Contract Violation")
            || diag.contains("divisor cannot be zero"),
        "Must report contract violation error code E0948"
    );
}

#[test]
fn test_contract_proven_elimination() {
    let source = r#"
fn safe_func(a: Int) -> Int
require true, "always true";
ensure true, "always true";
{
    return a * 2
}

fn main() {
    let x = safe_func(10)
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "proven_contract.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let dmir = res.dmir_module.expect("DMIR module present");
    let f = dmir.functions.get("safe_func").expect("safe_func present");
    let has_assert = f.blocks.iter().any(|b| {
        b.instructions.iter().any(|inst| {
            matches!(inst, forgen::dmir::Inst::Call { func, .. } if func == "datara_rt_assert")
        })
    });
    assert!(
        !has_assert,
        "Proven contract should be eliminated (zero-cost)"
    );
}
