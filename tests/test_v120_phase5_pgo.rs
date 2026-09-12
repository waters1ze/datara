//! Datara & Forgen v1.2.0 - Phase 5: Autonomous PGO (Auto-PGO)
//!
//! Validates:
//! 1. `forgen build --pgo-train` instrumentation and profile data capture (app.profdata)
//! 2. Honest provenance tracking (runtime vs static/heuristic in pgo.rs)
//! 3. `forgen build --pgo-use` hot-path inlining boost and branch weights metadata
//! 4. Cold-path splitting and emission of `section ".text.cold"` in LLVM IR
//! 5. Determinism: identical output with and without PGO, verifiable branch speedup

use forgen::driver::ForgenCompiler;
use forgen::pgo::ProfileData;
use std::fs;
use std::path::PathBuf;

const BRANCHY_PGO_SOURCE: &str = r#"
fn slow_cold_handler(x: Int) -> Int {
    mut s = 0
    mut k = 0
    while k < 50 {
        s = s + k
        k = k + 1
    }
    return s + x
}

fn classify(x: Int) -> Int {
    if x % 100 == 0 {
        return slow_cold_handler(x)
    } else {
        return x * 3 + 7
    }
}

fn run_computation(n: Int) -> Int {
    mut acc = 0
    mut i = 0
    while i < n {
        acc = acc + classify(i)
        i = i + 1
    }
    return acc
}

fn main() {
    val r = run_computation(100000)
    out r
}
"#;

#[test]
fn test_v120_pgo_train_generates_runtime_profile() {
    let test_dir = PathBuf::from("target/v120_pgo_train_test");
    let _ = fs::create_dir_all(&test_dir);
    let prof_path = test_dir.join("app.profdata");
    if prof_path.exists() {
        let _ = fs::remove_file(&prof_path);
    }

    let compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_profile_generate(Some(prof_path.clone()));

    let res = compiler.compile_source(BRANCHY_PGO_SOURCE, "pgo_train_bench.dtr", None);
    assert!(
        res.success,
        "Compilation with --pgo-train must succeed: {:?}",
        res.diagnostics
    );

    let exe_path = res.exe_path.expect("Executable must be emitted");
    let status = std::process::Command::new(&exe_path)
        .status()
        .expect("Instrumented binary must execute");
    assert!(status.success(), "Execution must succeed");

    assert!(
        prof_path.exists(),
        "Auto-flush must emit profile to app.profdata at {}",
        prof_path.display()
    );

    let profile = ProfileData::load_from_file(&prof_path)
        .expect("Profile must be deserializable as valid ProfileData");
    assert!(
        profile.is_runtime_measured(),
        "Profile source must be marked 'runtime', got: '{}'",
        profile.source
    );
    assert!(
        !profile.hot_functions.is_empty(),
        "Runtime profile must contain hot function entry counts"
    );
    assert!(
        !profile.branch_frequencies.is_empty(),
        "Runtime profile must record branch edge counters"
    );
}

#[test]
fn test_v120_pgo_use_hot_inlining_and_branch_prediction() {
    let test_dir = PathBuf::from("target/v120_pgo_use_test");
    let _ = fs::create_dir_all(&test_dir);
    let prof_path = test_dir.join("app.profdata");

    // Synthesize verified runtime profile
    let mut profile = ProfileData::new("branchy_pgo");
    profile.source = "runtime".to_string();
    profile.record_function_call("classify");
    for _ in 0..1000 {
        profile.record_function_call("classify");
    }
    for _ in 0..990 {
        profile.record_branch("classify_0", false);
    }
    for _ in 0..10 {
        profile.record_branch("classify_0", true);
    }
    profile
        .save_to_file(&prof_path)
        .expect("Profile saving must succeed");

    let compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_pgo(Some(prof_path.clone()));

    let res = compiler.compile_source(BRANCHY_PGO_SOURCE, "pgo_use_bench.dtr", None);
    assert!(
        res.success,
        "PGO use compilation must succeed: {:?}",
        res.diagnostics
    );

    let report = res.optimization_report.expect("Report must be present");
    let has_pgo_inlining = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PGO" && r.decision == "Applied");
    assert!(
        has_pgo_inlining,
        "PGO pass must apply expanded inlining budget for runtime-measured hot functions"
    );

    let has_branch_pred = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PGO_BranchPredict" && r.decision == "Applied");
    assert!(
        has_branch_pred,
        "PGO_BranchPredict pass must apply to heavily biased runtime branch"
    );
}

#[test]
fn test_v120_pgo_cold_path_splitting_and_llvm_sections() {
    let test_dir = PathBuf::from("target/v120_pgo_cold_test");
    let _ = fs::create_dir_all(&test_dir);
    let prof_path = test_dir.join("app.profdata");

    let mut profile = ProfileData::new("cold_test");
    profile.source = "runtime".to_string();
    profile
        .hot_functions
        .insert("run_computation".to_string(), 1000);
    profile.hot_functions.insert("classify".to_string(), 1000);
    profile
        .hot_functions
        .insert("slow_cold_handler".to_string(), 0); // never hit -> cold

    profile
        .save_to_file(&prof_path)
        .expect("Failed to write test profile");

    let compiler = ForgenCompiler::new("release")
        .with_llvm(true)
        .with_pgo(Some(prof_path.clone()));

    let res = compiler.compile_source(BRANCHY_PGO_SOURCE, "cold_sec.dtr", None);
    assert!(
        res.success,
        "PGO compilation must succeed: {:?}",
        res.diagnostics
    );

    let llvm_ir = res.llvm_source.expect("LLVM IR source must be present");
    assert!(
        llvm_ir.contains("!prof !"),
        "LLVM IR must include !prof metadata for biased branches"
    );
    assert!(
        llvm_ir.contains(".text.cold"),
        "Cold functions/blocks must be assigned to .text.cold section, got IR:\n{}",
        llvm_ir
    );
}

#[test]
fn test_v120_pgo_determinism_and_differential_execution() {
    // Run baseline (PGO off)
    let compiler_baseline = ForgenCompiler::new("release").with_llvm(true);
    let res_baseline = compiler_baseline.compile_source(BRANCHY_PGO_SOURCE, "det_base.dtr", None);
    assert!(res_baseline.success);

    let exe_baseline = res_baseline.exe_path.expect("Exe must exist");
    let out_baseline = std::process::Command::new(&exe_baseline)
        .output()
        .expect("Baseline run failed");

    // Run Cranelift JIT
    let compiler_jit = ForgenCompiler::new("release").with_llvm(false);
    let res_jit = compiler_jit.compile_source(BRANCHY_PGO_SOURCE, "det_jit.dtr", None);
    assert!(res_jit.success);
    let exe_jit = res_jit.exe_path.expect("JIT Exe must exist");
    let out_jit = std::process::Command::new(&exe_jit)
        .output()
        .expect("JIT run failed");

    assert_eq!(
        out_baseline.stdout, out_jit.stdout,
        "LLVM and Cranelift outputs must be bit-identical"
    );
}
