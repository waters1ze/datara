use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_ledger_and_semantic_graph_two_artifact_generation() {
    let source = r#"
class Point {
    x: Int
    y: Int
}

behavior Point {
    sum() -> Int {
        return this.x + this.y
    }
}

fn compute(n: Int) -> Int {
    mut p = Point { x: n, y: 5 }
    return p.sum()
}

fn main() {
    out compute(10)
}
"#;

    let target_dir = PathBuf::from("target/test_artifacts");
    let _ = fs::create_dir_all(&target_dir);
    let target_exe = target_dir.join("two_artifacts.exe");
    let ledger_path = target_dir.join("two_artifacts.ledger.json");
    let graph_path = target_dir.join("two_artifacts.graph.json");

    let compiler = ForgenCompiler::new("release");
    let res1 = compiler.compile_source(source, "artifacts_run1.dtr", Some(&target_exe));
    assert!(res1.success, "Compilation 1 failed: {:?}", res1.error);

    let report1 = res1
        .optimization_report
        .expect("Optimization report missing");
    let graph1 = res1.semantic_graph.expect("Semantic graph missing");

    let ledger_json_1 = serde_json::to_string_pretty(&report1.decision_trace).unwrap();
    let graph_json_1 = serde_json::to_string_pretty(&graph1).unwrap();

    fs::write(&ledger_path, &ledger_json_1).unwrap();
    fs::write(&graph_path, &graph_json_1).unwrap();

    assert!(ledger_path.exists(), "Ledger artifact must exist");
    assert!(graph_path.exists(), "Graph artifact must exist");

    // Idempotence proof: recompiling same source in same mode yields identical ledger
    let res2 = compiler.compile_source(source, "artifacts_run2.dtr", Some(&target_exe));
    assert!(res2.success);
    let report2 = res2.optimization_report.unwrap();
    let ledger_json_2 = serde_json::to_string_pretty(&report2.decision_trace).unwrap();

    assert_eq!(
        ledger_json_1, ledger_json_2,
        "Optimization ledger must be structurally idempotent across builds"
    );

    // Verify ledger contains honest records
    assert!(
        !report1.decision_trace.is_empty(),
        "Ledger must record optimization decisions"
    );
    for r in &report1.decision_trace {
        if r.decision == "Applied" {
            assert!(
                !r.estimated_benefit.is_empty(),
                "Applied record must state physical benefit"
            );
        }
    }

    // Verify graph contains typed nodes and edges
    assert!(!graph1.nodes.is_empty(), "Semantic graph must contain nodes");
    assert!(!graph1.edges.is_empty(), "Semantic graph must contain edges");
}
