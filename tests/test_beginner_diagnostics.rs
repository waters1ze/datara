use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;

#[test]
fn test_diag_01_variable_name_typo() {
    let source = r#"
fn main() {
    let counter_value = 42
    out countr_value
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "typo_var.dtr", None);
    assert!(!res.success, "Should fail on typo in variable name");
    assert!(
        res.diagnostics
            .contains(ErrorCode::ResolveUndefinedSymbol.as_str()),
        "Must contain E-RESOLVE-001, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("counter_value") || res.diagnostics.contains("similar name"),
        "Must suggest 'counter_value', got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_02_function_name_typo() {
    let source = r#"
fn calculate_score() -> Int {
    return 100
}

fn main() {
    let s = calculat_score()
    out s
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "typo_fn.dtr", None);
    assert!(!res.success, "Should fail on typo in function name");
    assert!(
        res.diagnostics
            .contains(ErrorCode::ResolveUndefinedSymbol.as_str()),
        "Must contain E-RESOLVE-001, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("calculate_score") || res.diagnostics.contains("similar name"),
        "Must suggest 'calculate_score', got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_03_field_name_typo() {
    let source = r#"
class PlayerStats {
    health: Int,
    mana: Int,
}

fn main() {
    let p = PlayerStats { health: 100, mana: 50 }
    out p.healt
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "typo_field.dtr", None);
    assert!(!res.success, "Should fail on typo in struct field name");
    assert!(
        res.diagnostics
            .contains(ErrorCode::TypeInvalidMemberAccess.as_str()),
        "Must contain E-TYPE-006, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("health") || res.diagnostics.contains("similar name"),
        "Must suggest 'health', got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_04_class_name_typo() {
    let source = r#"
class UserProfile {
    user_id: Int,
}

fn main() {
    let u = UserProfil { user_id: 1 }
    out u.user_id
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "typo_class.dtr", None);
    assert!(!res.success, "Should fail on typo in class name");
    assert!(
        res.diagnostics
            .contains(ErrorCode::ResolveUndefinedSymbol.as_str()),
        "Must contain E-RESOLVE-001, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("UserProfile") || res.diagnostics.contains("similar name"),
        "Must suggest 'UserProfile', got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_05_reassign_immutable_missing_mut() {
    let source = r#"
fn main() {
    let max_retries = 3
    max_retries = 5
    out max_retries
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "immutable_reassign.dtr", None);
    assert!(
        !res.success,
        "Should fail on reassigning immutable variable"
    );
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowCannotMutateImmutable.as_str()),
        "Must contain E-BORROW-002, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("mut max_retries") || res.diagnostics.contains("mutable"),
        "Must suggest declaring with 'mut', got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_06_type_mismatch() {
    let source = r#"
fn main() {
    let count: Int = "not_a_number"
    out count
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "type_mismatch.dtr", None);
    assert!(!res.success, "Should fail on type mismatch");
    assert!(
        res.diagnostics.contains(ErrorCode::TypeMismatch.as_str()),
        "Must contain E-TYPE-001, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("expected 'Int', got 'String'")
            || res.diagnostics.contains("str_to_int"),
        "Must provide actionable type mismatch diagnostics, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_07_use_after_move() {
    let source = r#"
fn main() {
    let asset = 100
    destroy(asset)
    out asset
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "use_after_move.dtr", None);
    assert!(!res.success, "Should fail on use after move");
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowUseAfterMove.as_str()),
        "Must contain E-BORROW-001, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("moved") || res.diagnostics.contains("cloning"),
        "Must mention moved value or cloning suggestion, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_08_conflicting_mutable_views() {
    let source = r#"
fn main() {
    let buffer = 10
    let v1 = mut_view(buffer)
    let v2 = mut_view(buffer)
    out v1
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "conflicting_views.dtr", None);
    assert!(!res.success, "Should fail on multiple mutable views");
    assert!(
        res.diagnostics
            .contains(ErrorCode::BorrowMultipleMutableViews.as_str()),
        "Must contain E-BORROW-004, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("XOR") || res.diagnostics.contains("only one mutable view"),
        "Must provide actionable XOR view suggestion, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_09_missing_return_in_function() {
    let source = r#"
fn compute_total(a: Int, b: Int) -> Int {
    let c = a + b
}

fn main() {
    let t = compute_total(1, 2)
    out t
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "missing_return.dtr", None);
    assert!(!res.success, "Should fail on missing return in function");
    assert!(
        res.diagnostics
            .contains(ErrorCode::TypeMissingReturn.as_str()),
        "Must contain E-TYPE-003, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("return") || res.diagnostics.contains("compute_total"),
        "Must suggest adding return statement, got:\n{}",
        res.diagnostics
    );
}

#[test]
fn test_diag_10_missing_module_import() {
    let source = r#"
use non_existent_game_engine

fn main() {
    out 1
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "missing_module.dtr", None);
    assert!(!res.success, "Should fail on non-existent module import");
    assert!(
        res.diagnostics
            .contains(ErrorCode::ResolveUnreachableModule.as_str()),
        "Must contain E-RESOLVE-005, got:\n{}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.contains("not found") || res.diagnostics.contains("verify the module path"),
        "Must provide actionable module suggestion, got:\n{}",
        res.diagnostics
    );
}
