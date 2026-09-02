//! Mechanical enforcement of the `Applied` honesty contract.
//!
//! Rule: `Applied` requires a physical IR delta. A pass that reports Applied
//! while leaving the DMIR unchanged must be downgraded to `Rejected` by the
//! evidence gate, and its counter movements reverted. The verifier must also
//! fail the build if a pass corrupts the IR.

use forgen::dmir::{BasicBlock, BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use forgen::driver::ForgenCompiler;
use forgen::optimizer::cost_model::DecisionRecord;
use forgen::optimizer::{Optimizer, evidence};

fn trivial_module() -> Module {
    let mut module = Module::new("gate_test");
    let mut function = Function {
        name: "f".into(),
        params: vec![("x".into(), "Int".into(), ValueId(0))],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: Vec::new(),
    };
    function.blocks.push(BasicBlock {
        id: BasicBlockId(0),
        label: "entry".into(),
        params: vec![],
        instructions: vec![Inst::ConstInt {
            dest: ValueId(1),
            value: 42,
        }],
        terminator: Terminator::Return {
            value: Some(ValueId(1)),
        },
    });
    module.functions.insert("f".to_string(), function);
    module
}

fn record(pass: &str, decision: &str) -> DecisionRecord {
    DecisionRecord {
        pass: pass.to_string(),
        candidate: "test-candidate".to_string(),
        decision: decision.to_string(),
        estimated_benefit: String::new(),
        estimated_cost: String::new(),
        reason: "gate test".to_string(),
    }
}

#[test]
fn gate_downgrades_applied_without_ir_delta() {
    let mut module = trivial_module();
    let before = evidence::ir_fingerprint(&module);

    let mut optimizer = Optimizer::new("domain");
    // A pass that lies: reports Applied during the pass, changes nothing,
    // bumps counters — exactly how a dishonest pass emits its records.
    optimizer.run_mutating_pass("lying_pass", &mut module, |opt, _m| {
        opt.trace.records.push(record("lying_pass", "Applied"));
        opt.trace.records.push(record("lying_pass", "Candidate"));
        opt.report.constants_folded += 5;
        opt.report.allocations_eliminated += 3;
    });

    assert_eq!(
        evidence::ir_fingerprint(&module),
        before,
        "no-op pass must leave IR identical"
    );

    let applied: Vec<&DecisionRecord> = optimizer
        .trace
        .records
        .iter()
        .filter(|r| r.pass == "lying_pass" && r.decision == "Applied")
        .collect();
    assert!(
        applied.is_empty(),
        "Applied without IR delta must be downgraded: {applied:?}"
    );

    let rejected: Vec<&DecisionRecord> = optimizer
        .trace
        .records
        .iter()
        .filter(|r| r.decision == "Rejected" && r.reason.contains("downgraded"))
        .collect();
    assert_eq!(
        rejected.len(),
        1,
        "exactly the Applied record is downgraded"
    );

    let candidate: Vec<&DecisionRecord> = optimizer
        .trace
        .records
        .iter()
        .filter(|r| r.decision == "Candidate")
        .collect();
    assert_eq!(candidate.len(), 1, "Candidate records are untouched");

    // Counter movements reverted: no delta, no claimed work.
    assert_eq!(optimizer.report.constants_folded, 0);
    assert_eq!(optimizer.report.allocations_eliminated, 0);
    assert_eq!(optimizer.report.evidence_downgrades, 1);
}

#[test]
fn gate_keeps_applied_with_real_ir_delta() {
    let mut module = trivial_module();

    let mut optimizer = Optimizer::new("domain");
    optimizer
        .trace
        .records
        .push(record("honest_pass", "Applied"));

    optimizer.run_mutating_pass("honest_pass", &mut module, |_opt, m| {
        let f = m.functions.get_mut("f").unwrap();
        f.blocks[0].instructions[0] = Inst::ConstInt {
            dest: ValueId(1),
            value: 43,
        };
    });

    let applied: Vec<&DecisionRecord> = optimizer
        .trace
        .records
        .iter()
        .filter(|r| r.decision == "Applied")
        .collect();
    assert_eq!(
        applied.len(),
        1,
        "Applied backed by a real IR delta must survive the gate"
    );
    assert_eq!(optimizer.report.evidence_downgrades, 0);
}

#[test]
#[should_panic(expected = "DMIR verification failed after optimizer pass")]
fn gate_fails_closed_when_pass_corrupts_ir() {
    let mut module = trivial_module();
    let mut optimizer = Optimizer::new("domain");

    optimizer.run_mutating_pass("corrupting_pass", &mut module, |_opt, m| {
        // Referencing an undefined value: the verifier must reject this.
        let f = m.functions.get_mut("f").unwrap();
        f.blocks[0].terminator = Terminator::Return {
            value: Some(ValueId(999)),
        };
    });
}

#[test]
fn real_pipeline_survives_gate_and_reports_honest_counters() {
    // End-to-end: the real optimizer pipeline runs through the gate without
    // downgrades on a program where the passes genuinely transform the IR.
    let source = r#"
fn compute(n: Int) -> Int {
    mut sum = 0
    for i in 0..n {
        sum = sum + i
    }
    return sum
}
fn main() {
    out compute(10)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "gate_pipeline.dtr", None);
    assert!(res.success, "compilation failed: {:?}", res.error);

    let report = res.optimization_report.as_ref().unwrap();
    assert_eq!(
        report.evidence_downgrades,
        0,
        "no pass in the real pipeline may claim Applied without an IR delta; \
         downgrades: {:?}",
        report
            .decision_trace
            .iter()
            .filter(|r| r.reason.contains("downgraded"))
            .collect::<Vec<_>>()
    );
}
