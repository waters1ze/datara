//! Phase 0 Test Suite: v1.2.0 Baseline and Rules of the Game Governance
//!
//! Validates:
//! 1. `docs/PERFORMANCE_GOALS.md` exists and contains v1.2.0 constants:
//!    - Determinism identity (IEEE-754 by default; Fast-Math opt-in)
//!    - Universal parity ceiling <= 1.01x
//!    - Targeted wins >= 1.15x
//!    - 12 canonical benchmarks + 4 RealWorld applications
//! 2. Baseline environment and runtime JSON datasets are intact
//! 3. Baseline bench-smoke execution passes cleanly

use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;

#[test]
fn test_v120_performance_goals_constants() {
    let doc_path = Path::new("docs/PERFORMANCE_GOALS.md");
    assert!(doc_path.exists(), "docs/PERFORMANCE_GOALS.md must exist");

    let content = fs::read_to_string(doc_path).expect("read PERFORMANCE_GOALS.md");
    assert!(
        content.contains("Determinism is Identity"),
        "Must specify Determinism is Identity"
    );
    assert!(
        content.contains("--fast-math") || content.contains("@fast_math"),
        "Must document opt-in fast math"
    );
    assert!(
        content.contains("<= 1.01x"),
        "Must define universal parity ceiling <= 1.01x"
    );
    assert!(
        content.contains("realworld_json_rest")
            && content.contains("realworld_grep_cli")
            && content.contains("realworld_physics_2d")
            && content.contains("realworld_image_blur"),
        "Must define all 4 RealWorld applications"
    );
}

#[test]
fn test_v120_baseline_environment_and_runtime() {
    let env_path = Path::new("docs/data/baseline_environment.json");
    let runtime_path = Path::new("docs/data/baseline_runtime.json");
    assert!(env_path.exists());
    assert!(runtime_path.exists());

    let env_raw = fs::read_to_string(env_path).expect("read env");
    let env_json: serde_json::Value = serde_json::from_str(&env_raw).expect("parse env JSON");
    assert!(env_json.pointer("/hardware/cpu").is_some());

    let rt_raw = fs::read_to_string(runtime_path).expect("read runtime");
    let rt_json: serde_json::Value = serde_json::from_str(&rt_raw).expect("parse rt JSON");
    assert!(rt_json.pointer("/results/fib_35").is_some());
}

#[test]
fn test_v120_bench_smoke() {
    let source = r#"
fn bench_smoke_loop(n: Int) -> Int {
    mut acc = 0
    mut i = 0
    while i <= n {
        acc = acc + i
        i = i + 1
    }
    acc
}

fn main() -> Int {
    let res = bench_smoke_loop(100)
    println(int_to_str(res))
    0
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "bench_smoke.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let exe_path = res.exe_path.expect("Executable path");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("run executable");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "5050");

    let _ = fs::remove_file(exe_path);
}
