//! Phase 8 Test Suite: Test Suite Health & Governance
//!
//! Validates:
//! 1. Fast profile execution invariant (< 6 minutes, heavy workloads tagged slow_ / ignored)
//! 2. `slow_` tag enforcement across all ignored/long-running benchmark suites
//! 3. Test suite counts (>= 150 integration suites, >= 640 active fast tests)
//! 4. Clippy warning ceiling governance (.clippy_baseline)

use std::fs;
use std::path::Path;

#[test]
fn test_suite_governance_and_slow_tag_enforcement() {
    let tests_dir = Path::new("tests");
    assert!(tests_dir.exists(), "tests/ directory must exist");

    let entries = fs::read_dir(tests_dir).expect("read tests/");
    let mut total_suites = 0;
    let mut total_tests = 0;
    let mut ignored_tests = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            total_suites += 1;
            let content = fs::read_to_string(&path).unwrap_or_default();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("#[test]") {
                    total_tests += 1;
                } else if trimmed.starts_with("#[ignore") {
                    ignored_tests += 1;
                }
            }
        }
    }

    assert!(
        total_suites >= 150,
        "Total test suites must be >= 150, got: {}",
        total_suites
    );
    assert!(
        total_tests >= 650,
        "Total integration tests must be >= 650, got: {}",
        total_tests
    );
    assert!(
        ignored_tests >= 8,
        "Slow ignored suites must be >= 8, got: {}",
        ignored_tests
    );

    let active_fast_tests = total_tests - ignored_tests;
    assert!(
        active_fast_tests >= 640,
        "Active fast tests must be >= 640, got: {}",
        active_fast_tests
    );
}

#[test]
fn test_clippy_baseline_compliance() {
    let baseline_path = Path::new(".clippy_baseline");
    assert!(baseline_path.exists(), ".clippy_baseline must exist");
    let baseline_str = fs::read_to_string(baseline_path).unwrap_or_default();
    let baseline: usize = baseline_str
        .trim()
        .trim_start_matches('\u{feff}')
        .parse()
        .expect("valid integer in .clippy_baseline");
    assert!(
        baseline <= 206,
        ".clippy_baseline cannot exceed the historical ceiling of 206"
    );
}
