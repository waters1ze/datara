use forgen::driver::ForgenCompiler;

#[test]
fn test_effects_pure_function() {
    let source = r#"
fn add(a: Int, b: Int) -> Int => a + b

fn main() {
    mut res = 0

    res = add(10, 20)
    out res
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "pure_fn.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must be built");
    let eff = graph
        .inspect_effects("add")
        .expect("Effects for add must exist");
    println!("add effects: {}", eff);
    assert_eq!(eff["effects"], "Pure");
}

#[test]
fn test_effects_io_function() {
    let source = r#"
fn log_message(msg: String) {
    out msg
}

fn main() {
    log_message("Hello")
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "io_fn.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must be built");
    let eff = graph
        .inspect_effects("log_message")
        .expect("Effects for log_message must exist");
    println!("log_message effects: {}", eff);
    assert_eq!(eff["effects"], "IO");
}

#[test]
fn test_effects_network_propagation() {
    let source = r#"
fn fetch_data() {
    http_get()
}

fn main() {
    fetch_data()
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "net_fn.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must be built");
    let eff = graph
        .inspect_effects("fetch_data")
        .expect("Effects for fetch_data must exist");
    println!("fetch_data effects: {}", eff);
    let eff_str = eff["effects"].as_str().unwrap();
    assert!(eff_str.contains("Network") && eff_str.contains("IO"));
}
