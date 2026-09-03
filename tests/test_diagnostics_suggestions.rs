use forgen::diagnostics::suggestions::{find_best_match, levenshtein_distance};
use forgen::driver::ForgenCompiler;

#[test]
fn test_fuzzy_matching_core() {
    assert_eq!(levenshtein_distance("count", "countr"), 1);
    assert_eq!(levenshtein_distance("user_name", "user_nam"), 1);
    assert_eq!(levenshtein_distance("calculate_total", "calc_total"), 5);

    let candidates = ["my_counter", "total_items", "user_account"];
    assert_eq!(
        find_best_match("my_countr", candidates.iter().copied()),
        Some("my_counter")
    );
    assert_eq!(
        find_best_match("totl_items", candidates.iter().copied()),
        Some("total_items")
    );
    assert_eq!(
        find_best_match("completely_unrelated", candidates.iter().copied()),
        None
    );
}

#[test]
fn test_compiler_suggests_typo_variable() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn main() {
    let user_balance = 500
    out user_balanc
}
"#;
    let res = compiler.compile_source_native(src, "typo_var", None);
    assert!(
        !res.success,
        "Compilation should fail for undefined typo var"
    );
    let diag = res.error.unwrap_or(res.diagnostics);
    assert!(
        diag.contains("user_balance"),
        "Diagnostics should suggest 'user_balance': {}",
        diag
    );
    assert!(
        diag.contains("similar name"),
        "Diagnostics should note similar name: {}",
        diag
    );
}

#[test]
fn test_compiler_suggests_mutability_on_immutable_reassign() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn main() {
    let total = 0
    total = total + 1
    out total
}
"#;
    let res = compiler.compile_source_native(src, "immutable_reassign", None);
    assert!(
        !res.success,
        "Compilation should fail for immutable reassignment"
    );
    let diag = res.error.unwrap_or(res.diagnostics);
    assert!(
        diag.contains("mut total"),
        "Diagnostics should suggest 'mut total': {}",
        diag
    );
}

#[test]
fn test_compiler_suggests_type_fix_on_float_to_int_mismatch() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn main() {
    let x: Int = 3.14
    out x
}
"#;
    let res = compiler.compile_source_native(src, "type_mismatch", None);
    assert!(!res.success, "Compilation should fail for type mismatch");
    let diag = res.error.unwrap_or(res.diagnostics);
    assert!(
        diag.contains("as Int") || diag.contains("math_floor"),
        "Diagnostics should suggest int conversion: {}",
        diag
    );
}
