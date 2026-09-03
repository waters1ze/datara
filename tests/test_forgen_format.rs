use forgen::fmt::rules::{FormatOptions, format_source};

#[test]
fn test_format_indentation_and_braces() {
    let unformatted = r#"
fn calculate(a: Int, b: Int) -> Int {
let x = a + b
if x > 10 {
out x
}
return x
}
"#;

    let opts = FormatOptions {
        indent: true,
        operators: false,
        loops: false,
        blank_lines: true,
        ..Default::default()
    };

    let (formatted, diffs) = format_source(unformatted, &opts);
    assert!(!diffs.is_empty(), "Diffs should be reported");
    assert!(formatted.contains("    let x = a + b"));
    assert!(formatted.contains("    if x > 10 {"));
    assert!(formatted.contains("        out x"));
    assert!(formatted.contains("    return x"));
}

#[test]
fn test_format_operators() {
    let unformatted = "let total=x+y*2-z\nlet eq=(a==b)&&(c!=d)\n";
    let opts = FormatOptions {
        indent: false,
        operators: true,
        loops: false,
        blank_lines: false,
        ..Default::default()
    };

    let (formatted, _) = format_source(unformatted, &opts);
    assert!(
        formatted.contains("let total = x + y * 2 - z"),
        "Formatted: {}",
        formatted
    );
    assert!(
        formatted.contains("let eq = (a == b) && (c != d)"),
        "Formatted: {}",
        formatted
    );
}

#[test]
fn test_format_loops_and_branches() {
    let unformatted = r#"
for (i in 0..10){
    if (i > 5){
        out i
    }
}
"#;
    let opts = FormatOptions {
        indent: true,
        operators: true,
        loops: true,
        blank_lines: true,
        ..Default::default()
    };

    let (formatted, _) = format_source(unformatted, &opts);
    assert!(
        formatted.contains("for i in 0..10 {"),
        "Formatted: {}",
        formatted
    );
    assert!(formatted.contains("if i > 5 {"), "Formatted: {}", formatted);
}

#[test]
fn test_format_preserves_strings_and_comments() {
    let unformatted = r#"
fn test_str() {
    let msg = "do not format + or == inside strings!"
    // also do not touch: a+b=c inside comments
    out msg
}
"#;
    let opts = FormatOptions::default();
    let (formatted, _) = format_source(unformatted, &opts);
    assert!(formatted.contains(r#"let msg = "do not format + or == inside strings!""#));
    assert!(formatted.contains("// also do not touch: a+b=c inside comments"));
}

#[test]
fn test_format_blank_line_collapse() {
    let unformatted = "let a = 1\n\n\n\n\nlet b = 2\n";
    let opts = FormatOptions {
        blank_lines: true,
        ..Default::default()
    };

    let (formatted, diffs) = format_source(unformatted, &opts);
    assert!(!diffs.is_empty());
    assert_eq!(formatted, "let a = 1\n\nlet b = 2\n");
}
