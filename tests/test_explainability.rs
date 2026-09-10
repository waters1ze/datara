use forgen::driver::ForgenCompiler;

#[test]
fn test_explainability_context_api() {
    let source = r#"
class Cart {
    id: Int
    total: Int
}

class User {
    id: Int
}

behavior User {
    checkout() -> String => "ok"
}

fn main() {
    mut u = User { id: 1 }
    out u.checkout()
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "why_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let graph = res.semantic_graph.expect("Semantic graph must be built");
    let rep = res
        .optimization_report
        .expect("Optimization report must be built");

    // Test find_symbol on User.checkout
    let checkout_node = graph
        .find_symbol("User.checkout")
        .expect("Must find User.checkout");
    assert_eq!(checkout_node.effects, "Pure");
    assert_eq!(checkout_node.ownership, "Borrowed (this)");

    // Test decision trace
    let trace_records = &rep.decision_trace;
    assert!(
        !trace_records.is_empty(),
        "Decision trace must contain optimization records"
    );
}
