use forgen::lint::{apply_fixes, lint_source};

#[test]
fn test_lint_detects_style_and_mutability_violations() {
    let source = r#"
class user_account {
    id: Int
}

fn CalculateBalance(User: user_account) -> Int {
    mut max_limit = 5000
    let unused_var = 10
    mut i = 0
    while i < 100 {
        i = i + 1
    }
    let flag = true
    if flag == true {
        return max_limit
    }
    return 0
}
"#;

    let diags = lint_source(source, "test_file.dtr").expect("Source should parse");
    let codes: Vec<&str> = diags.iter().map(|d| d.code).collect();

    // 1. Must flag class user_account as non_camel_case_types
    assert!(
        codes.contains(&"style::non_camel_case_types"),
        "Expected non_camel_case_types, got {:?}",
        codes
    );

    // 2. Must flag CalculateBalance as non_snake_case
    assert!(
        codes.contains(&"style::non_snake_case"),
        "Expected non_snake_case, got {:?}",
        codes
    );

    // 3. Must flag max_limit as unnecessary_mut (never reassigned)
    assert!(
        codes.contains(&"perf::unnecessary_mut"),
        "Expected unnecessary_mut, got {:?}",
        codes
    );

    // 4. Must flag unused_var as unused_variable
    assert!(
        codes.contains(&"style::unused_variable"),
        "Expected unused_variable, got {:?}",
        codes
    );

    // 5. Must flag while loop as prefer_for_loop
    assert!(
        codes.contains(&"style::prefer_for_loop"),
        "Expected prefer_for_loop, got {:?}",
        codes
    );

    // 6. Must flag flag == true as bool_comparison
    assert!(
        codes.contains(&"style::bool_comparison"),
        "Expected bool_comparison, got {:?}",
        codes
    );

    // Check rendered ANSI output contains rust-style arrows and help
    let rendered = diags[0].render(Some(source));
    assert!(rendered.contains("test_file.dtr:"));
    assert!(rendered.contains("-->"));
    assert!(rendered.contains("^"));
}

#[test]
fn test_lint_auto_fix_unnecessary_mut() {
    let source = r#"fn main() {
    mut score = 100
    out score
}
"#;

    let diags = lint_source(source, "fix_test.dtr").expect("Source should parse");
    assert!(diags.iter().any(|d| d.code == "perf::unnecessary_mut"));

    let fixed = apply_fixes(source, &diags);
    assert!(fixed.contains("let score = 100"));
    assert!(!fixed.contains("mut score = 100"));
}

#[test]
fn test_lint_clean_code_zero_warnings() {
    let source = r#"
class UserAccount {
    id: Int
}

fn calculate_balance(user: UserAccount) -> Int {
    let max_limit = 5000
    for i in 0..100 {
        let _step = i
    }
    return max_limit + user.id
}

fn main() {
    let user = UserAccount { id: 1 }
    out calculate_balance(user)
}
"#;

    let diags = lint_source(source, "clean_test.dtr").expect("Source should parse");
    assert!(
        diags.is_empty(),
        "Idiomatic Datara code must produce 0 warnings, but got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_update_notification_system() {
    use forgen::update::{
        UpdateCache, format_pip_notice, is_newer_version, load_cache, parse_version, save_cache,
    };

    assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
    assert_eq!(parse_version("v0.1.1"), Some((0, 1, 1)));
    assert!(is_newer_version("0.1.1", "0.1.0"));
    assert!(!is_newer_version("0.1.0", "0.1.1"));

    let notice = format_pip_notice("0.1.0", "0.1.1");
    assert!(notice.contains("A new release of forgen is available"));
    assert!(notice.contains("0.1.0"));
    assert!(notice.contains("0.1.1"));
    assert!(notice.contains("To update, run:"));
    assert!(notice.contains("cargo install forgen"));

    let cache = UpdateCache {
        last_checked_epoch_secs: 123456789,
        latest_version: "0.1.1".to_string(),
    };
    save_cache(&cache);
    let loaded = load_cache();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.latest_version, "0.1.1");
}
