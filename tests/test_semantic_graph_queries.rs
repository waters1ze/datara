use forgen::driver::ForgenCompiler;
use forgen::semantic_graph::NodeKind;

#[test]
fn test_semantic_graph_2_query_api() {
    let source = r#"
class Point {
    x: Int
    y: Int
}

behavior Point {
    distance() -> String => "calc"
}

fn add(a: Int, b: Int) -> Int => a + b

fn main() {
    mut p = Point { x: 10, y: 20 }
    mut res = 0

    res = add(p.x, p.y)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "query_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph required");

    // 1. find_symbol
    let point_node = graph.find_symbol("Point").expect("Must find Point class");
    assert_eq!(point_node.kind, NodeKind::Class);

    let add_node = graph.find_symbol("add").expect("Must find add function");
    assert_eq!(add_node.kind, NodeKind::Function);

    let main_node = graph
        .find_symbol("main")
        .expect("Must find main entry point");
    assert_eq!(main_node.kind, NodeKind::EntryPoint);

    // 2. find_reachable
    let reachable_ids = graph.find_reachable();
    assert!(reachable_ids.iter().any(|id| id == "fn:main"));

    // 3. find_effects
    let effects_json = graph
        .find_effects("add")
        .expect("Must find effects for add");
    assert_eq!(effects_json["effects"], "Pure");

    // 4. find_runtime_dependencies
    let runtime_deps = graph.find_runtime_dependencies();
    assert!(runtime_deps.contains(&"core".to_string()));
}
