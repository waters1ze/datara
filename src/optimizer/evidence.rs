//! Evidence gate for the optimizer.
//!
//! The project rule is "a line in the trace does not prove an optimization".
//! This module makes that rule mechanical: every mutating pass is wrapped in
//! a structural IR fingerprint comparison, and any `Applied` decision emitted
//! by a pass that left the IR byte-identical is downgraded to `Rejected`
//! automatically. Counters follow the same rule: no IR delta, no counter
//! movement.

use crate::dmir::{Function, Module};

/// A deterministic structural fingerprint of the DMIR.
///
/// Uses the `Debug` rendering of instructions and terminators, which is
/// complete (it covers every field of every variant) and stable within a
/// compiler build. Function names are visited in sorted order because
/// `Module.functions` is a `HashMap` whose iteration order is not stable.
pub fn ir_fingerprint(module: &Module) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};

    let mut names: Vec<&String> = module.functions.keys().collect();
    names.sort();
    for name in names {
        name.hash(&mut hasher);
        fingerprint_function(&module.functions[name.as_str()], &mut hasher);
    }
    hasher.finish()
}

fn fingerprint_function(
    function: &Function,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    use std::hash::Hash;
    function.name.hash(hasher);
    function.return_type.hash(hasher);
    function.entry_block.hash(hasher);
    for (name, ty, value) in &function.params {
        name.hash(hasher);
        ty.hash(hasher);
        value.hash(hasher);
    }
    // Blocks are visited in declaration order; block ids are hashed too so a
    // reordering that changes semantics is still a visible delta.
    for block in &function.blocks {
        block.id.hash(hasher);
        block.label.hash(hasher);
        for param in &block.params {
            param.val.hash(hasher);
            param.ty.hash(hasher);
            param.name.hash(hasher);
        }
        for inst in &block.instructions {
            // Debug format covers every field of every Inst variant.
            format!("{:?}", inst).hash(hasher);
        }
        format!("{:?}", block.terminator).hash(hasher);
    }
}

/// Downgrades every record in `records[start..]` whose decision claims a
/// transformation (`Applied`) into `Rejected`, appending the mechanical
/// reason. `Candidate` / `Rejected` / `Preserved` records are left alone:
/// they never claimed a change.
pub fn downgrade_applied_without_delta(
    records: &mut [crate::optimizer::cost_model::DecisionRecord],
    start: usize,
) -> usize {
    let mut downgraded = 0;
    for record in records.iter_mut().skip(start) {
        if record.decision == "Applied" {
            record.decision = "Rejected".to_string();
            record.reason = format!(
                "{} [downgraded: pass reported Applied but IR is unchanged]",
                record.reason
            );
            downgraded += 1;
        }
    }
    downgraded
}

/// Snapshot of the mutable counters in `OptimizationReport` that passes may
/// bump. If a pass leaves the IR unchanged, its counter movements are
/// reverted so the report cannot claim work that never happened.
#[derive(Debug, Clone, Copy, Default)]
pub struct CountersSnapshot {
    pub removed_symbols: usize,
    pub constants_folded: usize,
    pub dead_instructions_removed: usize,
    pub functions_inlined: usize,
    pub allocations_eliminated: usize,
}

impl CountersSnapshot {
    pub fn capture(report: &crate::optimizer::OptimizationReport) -> Self {
        Self {
            removed_symbols: report.removed_symbols,
            constants_folded: report.constants_folded,
            dead_instructions_removed: report.dead_instructions_removed,
            functions_inlined: report.functions_inlined,
            allocations_eliminated: report.allocations_eliminated,
        }
    }

    pub fn restore(&self, report: &mut crate::optimizer::OptimizationReport) {
        report.removed_symbols = self.removed_symbols;
        report.constants_folded = self.constants_folded;
        report.dead_instructions_removed = self.dead_instructions_removed;
        report.functions_inlined = self.functions_inlined;
        report.allocations_eliminated = self.allocations_eliminated;
    }
}

/// Result of running one mutating pass through the evidence gate.
#[derive(Debug, Clone, Copy)]
pub struct PassEvidence {
    pub changed: bool,
    pub downgraded_records: usize,
}

/// The full gate around one mutating pass: fingerprint before, run, verify
/// after (fail-closed), fingerprint after, and downgrade unverifiable claims.
pub fn gate_pass<F: FnOnce(&mut Module)>(
    label: &str,
    module: &mut Module,
    report: &mut crate::optimizer::OptimizationReport,
    trace: &mut crate::optimizer::cost_model::OptimizationDecisionTrace,
    pass: F,
) -> PassEvidence {
    let before = ir_fingerprint(module);
    let records_start = trace.records.len();
    let counters = CountersSnapshot::capture(report);

    pass(module);

    if let Err(error) = crate::dmir::verify_module(module) {
        panic!(
            "DMIR verification failed after optimizer pass '{}': {}",
            label, error
        );
    }

    let after = ir_fingerprint(module);
    let changed = after != before;
    let mut downgraded_records = 0;
    if !changed {
        downgraded_records = downgrade_applied_without_delta(&mut trace.records, records_start);
        counters.restore(report);
    }
    PassEvidence {
        changed,
        downgraded_records,
    }
}

/// Convenience for tests and tooling: rebuild the function map sorted by name.
pub fn sorted_function_names(module: &Module) -> Vec<String> {
    let mut names: Vec<String> = module.functions.keys().cloned().collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmir::*;

    fn trivial_module() -> Module {
        let mut module = Module::new("evidence_test");
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

    #[test]
    fn fingerprint_is_stable_for_identical_ir() {
        let module = trivial_module();
        assert_eq!(ir_fingerprint(&module), ir_fingerprint(&module));
    }

    #[test]
    fn fingerprint_changes_when_instruction_changes() {
        let mut module = trivial_module();
        let before = ir_fingerprint(&module);
        if let Some(f) = module.functions.get_mut("f") {
            f.blocks[0].instructions[0] = Inst::ConstInt {
                dest: ValueId(1),
                value: 43,
            };
        }
        assert_ne!(before, ir_fingerprint(&module));
    }

    #[test]
    fn fingerprint_changes_when_terminator_changes() {
        let mut module = trivial_module();
        let before = ir_fingerprint(&module);
        if let Some(f) = module.functions.get_mut("f") {
            f.blocks[0].terminator = Terminator::Return { value: None };
        }
        assert_ne!(before, ir_fingerprint(&module));
    }

    #[test]
    fn downgrade_only_touches_applied_records_in_range() {
        use crate::optimizer::cost_model::DecisionRecord;
        let mut records = vec![
            DecisionRecord {
                pass: "earlier".into(),
                candidate: "c".into(),
                decision: "Applied".into(),
                estimated_benefit: String::new(),
                estimated_cost: String::new(),
                reason: "kept".into(),
            },
            DecisionRecord {
                pass: "suspect".into(),
                candidate: "c".into(),
                decision: "Applied".into(),
                estimated_benefit: String::new(),
                estimated_cost: String::new(),
                reason: "claimed".into(),
            },
            DecisionRecord {
                pass: "suspect".into(),
                candidate: "c".into(),
                decision: "Candidate".into(),
                estimated_benefit: String::new(),
                estimated_cost: String::new(),
                reason: "honest".into(),
            },
        ];
        let n = downgrade_applied_without_delta(&mut records, 1);
        assert_eq!(n, 1);
        assert_eq!(records[0].decision, "Applied");
        assert_eq!(records[0].reason, "kept");
        assert_eq!(records[1].decision, "Rejected");
        assert!(records[1].reason.contains("downgraded"));
        assert_eq!(records[2].decision, "Candidate");
    }

    #[test]
    fn counters_snapshot_restores() {
        use crate::optimizer::OptimizationReport;
        let mut report = OptimizationReport::default();
        report.constants_folded = 5;
        let snapshot = CountersSnapshot::capture(&report);
        report.constants_folded = 9;
        report.allocations_eliminated = 3;
        snapshot.restore(&mut report);
        assert_eq!(report.constants_folded, 5);
        assert_eq!(report.allocations_eliminated, 0);
    }
}
