//! Phase 10 Test Suite: Performance Baseline & Goals Governance
//!
//! Validates:
//! 1. `docs/PERFORMANCE_GOALS.md` exists with all three verifiable tiers and target wins
//! 2. `docs/data/baseline_environment.json` captures hardware, toolchain, and OS metadata
//! 3. `docs/data/baseline_runtime.json` records median-of-7 baseline benchmark results
//! 4. Frozen benchmark suite integrity and non-empty results

use std::fs;
use std::path::Path;

#[test]
fn test_performance_goals_document_integrity() {
    let doc_path = Path::new("docs/PERFORMANCE_GOALS.md");
    assert!(doc_path.exists(), "docs/PERFORMANCE_GOALS.md must exist");

    let content = fs::read_to_string(doc_path).expect("read PERFORMANCE_GOALS.md");
    assert!(
        content.contains("Universal Parity Everywhere"),
        "Must define Tier 1: Universal Parity Everywhere"
    );
    assert!(
        content.contains("Targeted Wins Where C Lacks Information"),
        "Must define Tier 2: Targeted Wins"
    );
    assert!(
        content.contains("Zero Regressions"),
        "Must define Tier 3: Zero Regressions"
    );
    assert!(
        content.contains("Bounds-Check Elimination (BCE)"),
        "Must list BCE win case"
    );
    assert!(
        content.contains("Ownership-Derived IR Attributes"),
        "Must list ownership IR attributes win case"
    );
    assert!(
        content.contains("Interprocedural Engine"),
        "Must list IPO win case"
    );
    assert!(
        content.contains("Frozen Benchmark Matrix"),
        "Must define frozen benchmark matrix"
    );
}

#[test]
fn test_baseline_environment_json_integrity() {
    let env_path = Path::new("docs/data/baseline_environment.json");
    assert!(env_path.exists(), "baseline_environment.json must exist");

    let raw = fs::read_to_string(env_path).expect("read baseline_environment.json");
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("baseline_environment.json must be valid JSON");

    assert_eq!(
        json.pointer("/metadata/status").and_then(|v| v.as_str()),
        Some("frozen_baseline")
    );
    assert!(json.pointer("/hardware/cpu").is_some());
    assert!(json.pointer("/hardware/ram_gb").is_some());
    assert!(json.pointer("/hardware/os").is_some());
    assert!(json.pointer("/toolchains/rustc").is_some());
    assert!(json.pointer("/toolchains/cargo").is_some());
    assert!(json.pointer("/toolchains/datara_version").is_some());
}

#[test]
fn test_baseline_runtime_json_integrity() {
    let runtime_path = Path::new("docs/data/baseline_runtime.json");
    assert!(runtime_path.exists(), "baseline_runtime.json must exist");

    let raw = fs::read_to_string(runtime_path).expect("read baseline_runtime.json");
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("baseline_runtime.json must be valid JSON");

    let results = json
        .get("results")
        .and_then(|r| r.as_object())
        .expect("results map must be present in baseline_runtime.json");

    assert!(
        !results.is_empty(),
        "baseline_runtime.json results must not be empty"
    );

    // Verify key baseline workloads exist with median_ms values
    for (workload, data) in results {
        let obj = data.as_object().expect("workload data object");
        for (variant, metrics) in obj {
            let median = metrics.get("median_ms").and_then(|m| m.as_f64());
            assert!(
                median.is_some(),
                "Workload {} variant {} must have median_ms",
                workload,
                variant
            );
        }
    }
}
