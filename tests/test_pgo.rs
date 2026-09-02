use forgen::optimizer::Optimizer;
use forgen::pgo::{ProfileData, ProfileGuidedOptimizer};

#[test]
fn test_pgo_profile_serialization() {
    let temp_prof = std::env::temp_dir().join("test_pgo_profile.json");
    let mut prof = ProfileData::new("test_project");

    prof.record_function_call("compute_heavy");
    for _ in 0..150 {
        prof.record_function_call("hot_loop_fn");
    }
    prof.record_allocation("Point");
    prof.record_loop_iterations("loop_1", 1000);

    assert!(prof.save_to_file(&temp_prof).is_ok());

    let loaded = ProfileData::load_from_file(&temp_prof).expect("Must load profile");
    assert_eq!(loaded.project_name, "test_project");
    assert!(loaded.is_hot("hot_loop_fn"));
    assert!(!loaded.is_hot("compute_heavy"));

    let _ = std::fs::remove_file(&temp_prof);
}

#[test]
fn test_pgo_guided_optimization_boost() {
    let mut optimizer = Optimizer::new("domain");
    let initial_inlining_budget = optimizer.cost_model.inlining_threshold;

    let mut prof = ProfileData::new("test_project");
    // This test models a profile produced by real runtime instrumentation.
    prof.source = "runtime".to_string();
    for _ in 0..200 {
        prof.record_function_call("hot_kernel");
    }

    ProfileGuidedOptimizer::apply_profile_to_optimizer(&mut optimizer, &prof);

    assert!(
        optimizer.cost_model.inlining_threshold > initial_inlining_budget,
        "Hot PGO function must increase inlining budget"
    );

    let trace = &optimizer.trace.records;
    assert!(
        trace
            .iter()
            .any(|r| r.pass == "PGO" && r.candidate == "hot_kernel")
    );
}
