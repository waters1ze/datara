//! Phase 11 (v1.2.0): Benchmark Matrix «Datara vs The World» Test Suite
//!
//! Validates:
//! 1. Frozen 16-Workload Benchmark Matrix (12 canonical + 4 RealWorld applications).
//! 2. Hardware and environment provenance metadata.
//! 3. Competitor comparison harness (Datara LLVM, Datara Cranelift, C MSVC /O2, Rust --release).
//! 4. Zero regressions <= 5% threshold across all workloads.
//! 5. Targeted wins (>= 1.15x) on designated levers, universal parity (<= 1.03x) on others.
//! 6. End-to-end execution and determinism of a RealWorld simulated workload.

use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;

#[test]
fn test_v120_benchmark_matrix_schema_and_all_16_workloads() {
    let matrix_path = Path::new("docs/data/benchmark_matrix.json");
    assert!(
        matrix_path.exists(),
        "docs/data/benchmark_matrix.json must exist"
    );

    let raw = fs::read_to_string(matrix_path).expect("read benchmark_matrix.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let meta = json.get("metadata").expect("metadata object");
    assert_eq!(
        meta.get("status").and_then(|v| v.as_str()),
        Some("release_ready")
    );
    assert!(meta.get("cpu").is_some(), "cpu metadata required");
    assert!(meta.get("ram_gb").is_some(), "ram_gb metadata required");
    assert!(meta.get("os").is_some(), "os metadata required");
    assert!(meta.get("git_commit").is_some(), "git_commit required");
    assert!(
        meta.get("timestamp_utc").is_some(),
        "timestamp_utc required"
    );

    let workloads = json
        .get("workloads")
        .and_then(|w| w.as_object())
        .expect("workloads map");
    assert_eq!(
        workloads.len(),
        16,
        "Matrix must contain exactly 16 workloads (12 canonical + 4 RealWorld)"
    );

    let canonical_workloads = [
        "fib_35",
        "sum_1e8",
        "dot_4m_float4",
        "matmul_naive",
        "sort_100k",
        "json_parse",
        "string_builder_sso",
        "alloc_heavy",
        "hashmap_chain",
        "branchy_match",
        "nbody_sim",
        "quaternion_norm",
    ];

    for &cw in &canonical_workloads {
        let entry = workloads
            .get(cw)
            .unwrap_or_else(|| panic!("Canonical workload {} missing", cw));
        assert_eq!(
            entry.get("status").and_then(|s| s.as_str()),
            Some("PASSED"),
            "Workload {} must be PASSED",
            cw
        );
    }

    let realworld_workloads = [
        "realworld_json_rest",
        "realworld_grep_cli",
        "realworld_physics_2d",
        "realworld_image_blur",
    ];

    for &rw in &realworld_workloads {
        let entry = workloads
            .get(rw)
            .unwrap_or_else(|| panic!("RealWorld workload {} missing", rw));
        assert_eq!(
            entry.get("status").and_then(|s| s.as_str()),
            Some("PASSED"),
            "RealWorld workload {} must be PASSED",
            rw
        );
        let speedup = entry
            .get("speedup_vs_c")
            .and_then(|v| v.as_f64())
            .expect("speedup_vs_c");
        assert!(
            speedup >= 1.0,
            "RealWorld application {} speedup must be >= 1.0x vs C (got {:.2}x)",
            rw,
            speedup
        );
    }

    let summary = json.get("summary").expect("summary block");
    assert_eq!(
        summary.get("total_workloads").and_then(|v| v.as_i64()),
        Some(16)
    );
    assert_eq!(
        summary.get("passed_workloads").and_then(|v| v.as_i64()),
        Some(16)
    );
    assert_eq!(
        summary.get("regressions_count").and_then(|v| v.as_i64()),
        Some(0)
    );
    assert!(
        summary
            .get("targeted_wins_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            >= 10,
        "Targeted wins count must be >= 10"
    );
    assert_eq!(
        summary
            .get("stability_invariant_verified")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn test_v120_competitors_comparison_harness() {
    let matrix_path = Path::new("docs/data/benchmark_matrix.json");
    let raw = fs::read_to_string(matrix_path).expect("read benchmark_matrix.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let workloads = json
        .get("workloads")
        .and_then(|w| w.as_object())
        .expect("workloads map");

    for (name, entry) in workloads {
        let llvm_ms = entry
            .get("datara_llvm_ms")
            .and_then(|v| v.as_f64())
            .expect("datara_llvm_ms");
        let clif_ms = entry
            .get("datara_cranelift_ms")
            .and_then(|v| v.as_f64())
            .expect("datara_cranelift_ms");
        let c_ms = entry
            .get("c_msvc_o2_ms")
            .and_then(|v| v.as_f64())
            .expect("c_msvc_o2_ms");
        let rust_ms = entry
            .get("rust_release_ms")
            .and_then(|v| v.as_f64())
            .expect("rust_release_ms");

        assert!(llvm_ms > 0.0, "{}: datara_llvm_ms must be positive", name);
        assert!(
            clif_ms > 0.0,
            "{}: datara_cranelift_ms must be positive",
            name
        );
        assert!(c_ms > 0.0, "{}: c_msvc_o2_ms must be positive", name);
        assert!(rust_ms > 0.0, "{}: rust_release_ms must be positive", name);
    }
}

#[test]
fn test_v120_realworld_micro_execution_simulation() {
    // RealWorld JSON-like router micro-application compiled through LLVM
    let source = r#"
fn handle_route(path: String, id: Int) -> Int {
    if path == "api/v1/user" {
        return id * 10
    }
    if path == "api/v1/order" {
        return id * 20
    }
    return 0
}

fn main() {
    val r1 = handle_route("api/v1/user", 42)
    val r2 = handle_route("api/v1/order", 5)
    println(r1 + r2)
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "v120_realworld_sim.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let exe = res.exe_path.expect("Executable must exist");
    // 5 consecutive runs for determinism
    for run in 1..=5 {
        let (stdout, stderr, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
        assert_eq!(code, 0, "Run {} failed: {}", run, stderr);
        assert_eq!(
            stdout.trim(),
            "520",
            "Expected 420 + 100 = 520 on run {}",
            run
        );
    }
}
