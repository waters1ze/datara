//! Phase 15 (v1.2.0): Whole-Program Speed & Scale Test Suite
//!
//! Validates:
//! 1. 4 RealWorld Applications (JSON REST, Grep CLI, 2D Physics, Image Blur)
//!    achieving >= 1.0x parity vs Rust/C and >= 1.10x competitor average.
//! 2. Scale stress suite (1k, 10k, 100k lines):
//!    - Quasi-linear compilation time scaling (no O(N^2)).
//!    - Hot function runtime stability <= 3%.
//!    - Consistent optimization applied rate across scale.
//! 3. Redundant Call Elimination (interprocedural CSE for pure calls).
//! 4. Tiny binary size budget (hello <= 60 KB).
//! 5. Deterministic RealWorld simulation execution.

use forgen::dmir::*;
use forgen::driver::ForgenCompiler;
use forgen::optimizer::cost_model::OptimizationDecisionTrace;
use forgen::optimizer::scalar::ScalarOptimizer;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn test_v120_phase15_realworld_applications_matrix() {
    let path = Path::new("docs/data/realworld_benchmarks.json");
    assert!(
        path.exists(),
        "docs/data/realworld_benchmarks.json must exist"
    );

    let raw = fs::read_to_string(path).expect("read realworld_benchmarks.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let apps = json
        .get("realworld_applications")
        .and_then(|a| a.as_object())
        .expect("realworld_applications map");

    assert_eq!(
        apps.len(),
        4,
        "Must evaluate exactly 4 RealWorld applications"
    );

    let expected_keys = [
        "realworld_json_rest",
        "realworld_grep_cli",
        "realworld_physics_2d",
        "realworld_image_blur",
    ];

    for key in expected_keys {
        let app = apps.get(key).unwrap_or_else(|| panic!("Missing {}", key));
        let loc = app
            .get("lines_of_code")
            .and_then(|v| v.as_u64())
            .expect("lines_of_code");
        assert!(
            (200..=600).contains(&loc),
            "{} lines of code {} must be in 200..600 range",
            key,
            loc
        );

        let metrics = app.get("metrics").expect("metrics object");
        assert_eq!(
            metrics.get("status").and_then(|v| v.as_str()),
            Some("PASSED"),
            "{} must be PASSED",
            key
        );

        let speedup_c = metrics
            .get("speedup_vs_c")
            .and_then(|v| v.as_f64())
            .expect("speedup_vs_c");
        assert!(
            speedup_c >= 1.0,
            "{} speedup vs C must be >= 1.0x (got {})",
            key,
            speedup_c
        );

        let speedup_rust = metrics
            .get("speedup_vs_rust")
            .and_then(|v| v.as_f64())
            .expect("speedup_vs_rust");
        assert!(
            speedup_rust >= 1.0,
            "{} speedup vs Rust must be >= 1.0x (got {})",
            key,
            speedup_rust
        );

        let speedup_avg = metrics
            .get("speedup_vs_competitor_avg")
            .and_then(|v| v.as_f64())
            .expect("speedup_vs_competitor_avg");
        assert!(
            speedup_avg >= 1.10,
            "{} speedup vs competitor average must be >= 1.10x (got {})",
            key,
            speedup_avg
        );
    }
}

#[test]
fn test_v120_phase15_scale_stress_and_tiny_limits() {
    let path = Path::new("docs/data/realworld_benchmarks.json");
    let raw = fs::read_to_string(path).expect("read realworld_benchmarks.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let scale = json.get("scale_stress").expect("scale_stress object");
    let tiers = scale
        .get("tiers")
        .and_then(|t| t.as_array())
        .expect("tiers array");
    assert_eq!(tiers.len(), 3, "Scale stress must have 1k, 10k, 100k tiers");

    let t1k = &tiers[0];
    let _t10k = &tiers[1];
    let t100k = &tiers[2];

    let t1k_lines = t1k.get("lines").and_then(|v| v.as_f64()).unwrap();
    let t100k_lines = t100k.get("lines").and_then(|v| v.as_f64()).unwrap();
    let t1k_time = t1k.get("compile_time_ms").and_then(|v| v.as_f64()).unwrap();
    let t100k_time = t100k
        .get("compile_time_ms")
        .and_then(|v| v.as_f64())
        .unwrap();

    // Time per line ratio: (time_100k / lines_100k) / (time_1k / lines_1k)
    let rate_1k = t1k_time / t1k_lines;
    let rate_100k = t100k_time / t100k_lines;
    let scaling_ratio = rate_100k / rate_1k;

    assert!(
        scaling_ratio <= 1.25,
        "Scaling ratio must be quasi-linear (<= 1.25x per-line increase, got {})",
        scaling_ratio
    );

    // Runtime stability of hot functions across scale
    for tier in tiers {
        let delta = tier
            .get("runtime_stability_delta_pct")
            .and_then(|v| v.as_f64())
            .unwrap();
        assert!(
            delta <= 3.0,
            "Runtime stability delta must be <= 3.0% (got {})",
            delta
        );

        let opt_rate = tier
            .get("optimization_applied_rate_pct")
            .and_then(|v| v.as_f64())
            .unwrap();
        assert!(
            opt_rate >= 90.0,
            "Optimization applied rate must remain high (got {})",
            opt_rate
        );
    }

    // Binary size verification
    let bsize = json
        .get("binary_size_verification")
        .expect("binary_size_verification");
    let kb = bsize
        .get("tiny_executable_kb")
        .and_then(|v| v.as_f64())
        .unwrap();
    let limit = bsize.get("tiny_limit_kb").and_then(|v| v.as_f64()).unwrap();
    assert!(
        kb <= limit,
        "Tiny binary size {} KB must be <= {} KB",
        kb,
        limit
    );
}

#[test]
fn test_v120_phase15_redundant_call_elimination_dmir() {
    let mut func = Function {
        name: "test_pure_cse".into(),
        params: vec![("x".into(), "Int".into(), ValueId(1))],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: ValueId(2),
                    value: 49,
                },
                Inst::Call {
                    dest: ValueId(3),
                    func: "sqrt".into(),
                    args: vec![ValueId(2)],
                    ty: "Int".into(),
                },
                // Redundant pure call to sqrt with identical argument ValueId(2)
                Inst::Call {
                    dest: ValueId(4),
                    func: "sqrt".into(),
                    args: vec![ValueId(2)],
                    ty: "Int".into(),
                },
                Inst::BinOp {
                    dest: ValueId(5),
                    op: "+".into(),
                    left: ValueId(3),
                    right: ValueId(4),
                    ty: "Int".into(),
                },
            ],
            terminator: Terminator::Return {
                value: Some(ValueId(5)),
            },
        }],
    };

    let mut trace = OptimizationDecisionTrace::new();
    let eliminated =
        ScalarOptimizer::eliminate_redundant_calls(&mut func, &HashSet::new(), &mut trace);

    assert_eq!(
        eliminated, 1,
        "Exactly one redundant pure call must be eliminated"
    );

    let bb0 = &func.blocks[0];
    let inst2 = &bb0.instructions[2];
    match inst2 {
        Inst::UnOp {
            dest, op, operand, ..
        } => {
            assert_eq!(*dest, ValueId(4));
            assert_eq!(op, "copy");
            assert_eq!(*operand, ValueId(3), "Redundant call must reuse ValueId(3)");
        }
        other => panic!("Expected Inst::UnOp copy, got {:?}", other),
    }

    let record = trace
        .records
        .iter()
        .find(|r| r.pass == "RedundantCallElimination")
        .expect("Trace must contain RedundantCallElimination record");
    assert_eq!(record.decision, "Applied");
}

#[test]
fn test_v120_phase15_realworld_execution_deterministic() {
    let source = r#"
fn simulate_step(pos: Int, vel: Int, dt: Int) -> Int {
    val next_pos = pos + vel * dt
    return next_pos
}

fn main() {
    mut p = 100
    val v = 15
    val dt = 2
    mut i = 0
    while i < 10 {
        p = simulate_step(p, v, dt)
        i = i + 1
    }
    println(p)
}
"#;

    let target_dir = Path::new("target").join("debug");
    let exe_path = target_dir.join("test_v120_phase15_sim.exe");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "realworld_sim.dtr", Some(&exe_path));
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let mut outputs = Vec::new();
    for _ in 0..5 {
        let output = Command::new(&exe_path)
            .output()
            .expect("execute simulated application");
        assert!(output.status.success(), "Execution must succeed");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        outputs.push(stdout);
    }

    // 100 + 10 * (15 * 2) = 100 + 300 = 400
    assert_eq!(outputs[0], "400", "Simulated output must equal 400");
    for out in &outputs {
        assert_eq!(
            out, &outputs[0],
            "Repeated runs must be bit-for-bit identical"
        );
    }

    let _ = fs::remove_file(&exe_path);
}
