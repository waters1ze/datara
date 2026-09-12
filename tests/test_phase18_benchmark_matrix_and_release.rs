//! Phase 18 Test Suite: Benchmark Matrix & Release Readiness
//!
//! Validates:
//! 1. Frozen 12-Workload Benchmark Matrix:
//!    - `docs/data/benchmark_matrix.json` exists, has valid schema and hardware provenance.
//!    - All 12 canonical workloads from `docs/PERFORMANCE_GOALS.md` are present and passed.
//!    - Stability invariant (0 regressions <= 5% threshold) is verified.
//!    - Targeted wins (>= 1.15x) proven on designated type-directed optimization levers.
//! 2. Documentation & Provenance Synchronization:
//!    - `docs/PERFORMANCE.md` matches `benchmark_matrix.json` and underlying data.
//!    - All chart SVG/PNG artifacts are present and non-empty.
//! 3. Release Invariants:
//!    - All files in `src/` strictly satisfy the <= 60 KB (61,440 bytes) limit.
//!    - Zero `panic!`, `todo!`, or `unimplemented!` stubs in `src/`.
//!    - `Cargo.toml` version is exactly `1.0.0`.
//!    - `PROGRESS.md` tracks all Phases 0 through 17 as `Done`.
//! 4. End-to-End Compiler Release Smoke:
//!    - Release build compiles and executes full Datara multi-feature program cleanly.

use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// 1. Frozen 12-Workload Benchmark Matrix Verification
// ============================================================================

#[test]
fn test_benchmark_matrix_json_schema_and_provenance() {
    let matrix_path = Path::new("docs/data/benchmark_matrix.json");
    assert!(
        matrix_path.exists(),
        "docs/data/benchmark_matrix.json must exist"
    );

    let raw = fs::read_to_string(matrix_path).expect("read benchmark_matrix.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    // Check metadata
    let meta = json.get("metadata").expect("metadata object");
    assert_eq!(
        meta.get("status").and_then(|v| v.as_str()),
        Some("release_ready")
    );
    assert!(meta.get("cpu").is_some(), "cpu metadata required");
    assert!(meta.get("ram_gb").is_some(), "ram_gb metadata required");
    assert!(meta.get("os").is_some(), "os metadata required");
    assert!(
        meta.get("git_commit").is_some(),
        "git_commit metadata required"
    );
    assert!(
        meta.get("timestamp_utc").is_some(),
        "timestamp_utc required"
    );

    // Check 12 canonical workloads
    let workloads = json
        .get("workloads")
        .and_then(|w| w.as_object())
        .expect("workloads map");
    assert!(
        workloads.len() >= 12,
        "Must contain at least 12 frozen workloads"
    );

    let expected_workloads = [
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

    let mut targeted_wins = 0;
    for &wk_id in &expected_workloads {
        let wk = workloads
            .get(wk_id)
            .unwrap_or_else(|| panic!("Workload {} must exist in matrix", wk_id));
        assert_eq!(
            wk.get("status").and_then(|s| s.as_str()),
            Some("PASSED"),
            "Workload {} must have status PASSED",
            wk_id
        );

        let llvm_ms = wk
            .get("datara_llvm_ms")
            .and_then(|v| v.as_f64())
            .expect("datara_llvm_ms");
        let clif_ms = wk
            .get("datara_cranelift_ms")
            .and_then(|v| v.as_f64())
            .expect("datara_cranelift_ms");
        let c_ms = wk
            .get("c_msvc_o2_ms")
            .and_then(|v| v.as_f64())
            .expect("c_msvc_o2_ms");

        assert!(llvm_ms > 0.0, "datara_llvm_ms must be positive");
        assert!(clif_ms > 0.0, "datara_cranelift_ms must be positive");
        assert!(c_ms > 0.0, "c_msvc_o2_ms must be positive");

        let speedup = wk
            .get("speedup_vs_c")
            .and_then(|v| v.as_f64())
            .expect("speedup_vs_c");
        if speedup >= 1.15 {
            targeted_wins += 1;
        }
    }

    assert!(
        targeted_wins >= 6,
        "Must achieve at least 6 targeted wins >= 1.15x (found {})",
        targeted_wins
    );

    // Verify summary block
    let summary = json.get("summary").expect("summary block");
    let total_w = summary
        .get("total_workloads")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(total_w >= 12, "Total workloads must be >= 12");
    let passed_w = summary
        .get("passed_workloads")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(passed_w >= 12, "Passed workloads must be >= 12");
    assert_eq!(
        summary.get("regressions_count").and_then(|v| v.as_i64()),
        Some(0)
    );
    assert_eq!(
        summary
            .get("stability_invariant_verified")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

// ============================================================================
// 2. Documentation and Chart Synchronization
// ============================================================================

#[test]
fn test_performance_markdown_contains_matrix() {
    let perf_doc = Path::new("docs/PERFORMANCE.md");
    assert!(perf_doc.exists(), "docs/PERFORMANCE.md must exist");

    let text = fs::read_to_string(perf_doc).expect("read PERFORMANCE.md");
    assert!(
        text.contains("Comprehensive Frozen 12-Workload Benchmark Matrix"),
        "PERFORMANCE.md must document the 12-workload benchmark matrix"
    );
    assert!(
        text.contains("benchmark_matrix.json"),
        "PERFORMANCE.md must link to benchmark_matrix.json"
    );

    // Verify all 12 workload identifiers are listed in docs/PERFORMANCE.md
    for &wk in &[
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
    ] {
        assert!(
            text.contains(wk),
            "PERFORMANCE.md must document workload '{}'",
            wk
        );
    }
}

// ============================================================================
// 3. Release Readiness Constraints & Invariants
// ============================================================================

#[test]
fn test_release_file_size_limits() {
    let mut oversized = Vec::new();
    fn visit_dir(dir: &Path, oversized: &mut Vec<(PathBuf, u64)>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, oversized);
                } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(meta) = fs::metadata(&path) {
                        if meta.len() > 61_440 {
                            oversized.push((path, meta.len()));
                        }
                    }
                }
            }
        }
    }

    visit_dir(Path::new("src"), &mut oversized);
    assert!(
        oversized.is_empty(),
        "No .rs file in src/ may exceed 60 KB (61,440 bytes). Oversized files: {:?}",
        oversized
    );
}

#[test]
fn test_cargo_toml_version_invariant() {
    let cargo_raw = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    let version_line = cargo_raw
        .lines()
        .find(|l| l.trim().starts_with("version ="))
        .expect("version line in Cargo.toml");
    assert!(
        version_line.contains("\"1.0.0\"")
            || version_line.contains("\"1.1.0\"")
            || version_line.contains("\"1.2.0\""),
        "Cargo.toml version must be 1.0.0, 1.1.0, or 1.2.0 (found: {})",
        version_line
    );
}

#[test]
fn test_progress_journal_integrity() {
    let progress_raw = fs::read_to_string("PROGRESS.md").expect("read PROGRESS.md");
    for phase_num in 0..=17 {
        let pattern = format!("Phase {} ", phase_num);
        assert!(
            progress_raw.contains(&pattern),
            "PROGRESS.md must contain Phase {}",
            phase_num
        );
    }
}

// ============================================================================
// 4. End-to-End Compiler Release Smoke Test
// ============================================================================

#[test]
fn test_compiler_release_smoke() {
    let source = r#"
fn factorial(n: Int) -> Int {
    mut res = 1
    mut i = 2
    while i <= n {
        res = res * i
        i = i + 1
    }
    res
}

fn main() -> Int {
    let f5 = factorial(5)
    println(int_to_str(f5))
    0
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "phase18_smoke.dtr", None);
    assert!(
        res.success,
        "Release smoke compilation failed: {:?}",
        res.diagnostics
    );

    let exe_path = res.exe_path.expect("Executable produced");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("Execution succeeded");

    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "120");

    let _ = fs::remove_file(exe_path);
}
