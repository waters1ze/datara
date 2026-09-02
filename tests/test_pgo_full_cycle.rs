use forgen::dmir::*;
use forgen::optimizer::Optimizer;
use forgen::pgo::{ProfileData, ProfileGuidedOptimizer};

#[test]
fn test_pgo_full_cycle_inlining_and_branch_prediction() {
    // 1. Build a DMIR module with a caller and callee
    let mut module = Module::new("pgo_test_module");

    // Callee function
    let callee_entry = BasicBlockId(0);
    let v_arg = ValueId(0);
    let v_const = ValueId(1);
    let v_res = ValueId(2);
    let callee_block = BasicBlock {
        id: callee_entry,
        label: "entry".to_string(),
        params: vec![BlockParam {
            val: v_arg,
            ty: "Int".to_string(),
            name: Some("x".to_string()),
        }],
        instructions: vec![
            Inst::ConstInt {
                dest: v_const,
                value: 42,
            },
            Inst::BinOp {
                dest: v_res,
                op: "+".to_string(),
                left: v_arg,
                right: v_const,
                ty: "Int".to_string(),
            },
        ],
        terminator: Terminator::Return { value: Some(v_res) },
    };
    let callee = Function {
        name: "calculate_offset".to_string(),
        params: vec![("x".to_string(), "Int".to_string(), v_arg)],
        return_type: "Int".to_string(),
        entry_block: callee_entry,
        blocks: vec![callee_block],
    };
    module
        .functions
        .insert("calculate_offset".to_string(), callee);

    // Caller function
    let caller_entry = BasicBlockId(0);
    let v_in = ValueId(10);
    let v_call_dest = ValueId(11);
    let caller_block = BasicBlock {
        id: caller_entry,
        label: "entry".to_string(),
        params: vec![BlockParam {
            val: v_in,
            ty: "Int".to_string(),
            name: Some("input".to_string()),
        }],
        instructions: vec![Inst::Call {
            dest: v_call_dest,
            func: "calculate_offset".to_string(),
            args: vec![v_in],
            ty: "Int".to_string(),
        }],
        terminator: Terminator::Return {
            value: Some(v_call_dest),
        },
    };
    let caller = Function {
        name: "process_input".to_string(),
        params: vec![("input".to_string(), "Int".to_string(), v_in)],
        return_type: "Int".to_string(),
        entry_block: caller_entry,
        blocks: vec![caller_block],
    };
    module.functions.insert("process_input".to_string(), caller);

    // 2. Baseline without PGO profile: optimize with low threshold
    let mut baseline_opt = Optimizer::new("release");
    let mut baseline_module = module.clone();
    baseline_opt.optimize_module(&mut baseline_module);

    // 3. Create execution profile recording 500 hot invocations & 99% branch taken
    let mut profile = ProfileData::new("pgo_test_module");
    // The full-cycle test is explicitly a runtime-profile path. Static
    // call-site counts are rejected and must not widen optimizer budgets.
    profile.source = "runtime".to_string();
    for _ in 0..500 {
        profile.record_function_call("calculate_offset");
    }
    for _ in 0..100 {
        profile.record_branch("process_input_0", true);
    }
    profile.record_branch("process_input_0", false); // 1 not taken => 99% taken

    assert!(profile.is_hot("calculate_offset"));
    assert!(
        profile
            .is_branch_heavily_biased("process_input_0")
            .is_some()
    );

    // 4. Re-optimize with full PGO cycle
    let mut pgo_opt = Optimizer::new("release");
    let mut pgo_module = module.clone();
    ProfileGuidedOptimizer::optimize_module(&mut pgo_opt, &mut pgo_module, &profile);

    // 5. Verify that PGO applied optimization decisions
    let pgo_records = pgo_opt.trace.find_records_for("calculate_offset");
    assert!(
        !pgo_records.is_empty(),
        "PGO must record optimization decisions for hot function"
    );
    assert!(
        pgo_records
            .iter()
            .any(|r| r.pass == "PGO" && r.decision == "Applied"),
        "PGO boost must be applied to hot function"
    );

    // Verify caller function has inlined the call
    let pgo_caller = pgo_module.functions.get("process_input").unwrap();
    let has_call = pgo_caller.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|inst| matches!(inst, Inst::Call { .. }))
    });
    assert!(
        !has_call,
        "Hot function call must be inlined in caller during PGO pass"
    );
}
