use forgen::doc::generate_docs;
use forgen::export::export_c_header;
use forgen::repl::ReplSession;
use std::fs;

#[test]
fn test_repl_session_eval_and_commands() {
    let mut session = ReplSession::new();

    // 1. Meta commands
    let help_res = session.eval_line(":help").unwrap();
    assert!(help_res.contains("Datara REPL Commands"));

    let vars_empty = session.eval_line(":vars").unwrap();
    assert!(vars_empty.contains("No active variables"));

    // 2. Variable declaration
    let def_a = session.eval_line("let a = 100").unwrap();
    assert!(def_a.contains("defined a"));

    let vars_after = session.eval_line(":vars").unwrap();
    assert!(vars_after.contains("Active variables: a"));

    // 3. Clear session
    let clear_res = session.eval_line(":clear").unwrap();
    assert!(clear_res.contains("cleared"));
    assert!(session.variable_names.is_empty());
}

#[test]
fn test_doc_generator_spa_html() {
    let temp_dir = std::env::temp_dir().join("datara_test_doc");
    let _ = fs::create_dir_all(&temp_dir);

    let sample_file = temp_dir.join("sample_math.dtr");
    let sample_code = r#"
/// Adds two numbers with pure mathematical precision
pub fn add(a: Int, b: Int) -> Int {
    return a + b
}

/// 3D Vector primitive for graphics
class Vector3 {
    x: Float
    y: Float
    z: Float
}
"#;
    fs::write(&sample_file, sample_code).unwrap();

    let out_html = temp_dir.join("index.html");
    let result = generate_docs(&sample_file, &out_html);
    assert!(result.is_ok(), "Doc generation failed: {:?}", result.err());

    let html_content = fs::read_to_string(&out_html).unwrap();
    assert!(html_content.contains("<!DOCTYPE html>"));
    assert!(html_content.contains("Datara API Documentation"));
    assert!(html_content.contains("add"));
    assert!(html_content.contains("Vector3"));
    assert!(html_content.contains("filterDocs"));

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_export_c_header() {
    let temp_dir = std::env::temp_dir().join("datara_test_export");
    let _ = fs::create_dir_all(&temp_dir);

    let sample_file = temp_dir.join("geom.dtr");
    let sample_code = r#"
class Point {
    x: Int
    y: Int
}

pub fn distance_squared(p: Point) -> Int {
    return p.x * p.x + p.y * p.y
}
"#;
    fs::write(&sample_file, sample_code).unwrap();

    let out_h = temp_dir.join("geom.h");
    let result = export_c_header(&sample_file, &out_h);
    assert!(result.is_ok(), "C-Header export failed: {:?}", result.err());

    let h_content = fs::read_to_string(&out_h).unwrap();
    assert!(h_content.contains("#ifndef DATARA_GEOM_H"));
    assert!(h_content.contains("typedef struct {"));
    assert!(h_content.contains("int64_t x;"));
    assert!(h_content.contains("int64_t y;"));
    assert!(h_content.contains("} Point;"));
    assert!(h_content.contains("DATARA_API int64_t distance_squared(p: Point);"));
    assert!(h_content.contains("extern \"C\""));

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}
