use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub pass: String,
    pub candidate: String,
    pub decision: String, // Applied, Candidate, Rejected, Preserved
    pub estimated_benefit: String,
    pub estimated_cost: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptimizationDecisionTrace {
    pub records: Vec<DecisionRecord>,
}

impl OptimizationDecisionTrace {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        pass: &str,
        candidate: &str,
        decision: &str,
        benefit: &str,
        cost: &str,
        reason: &str,
    ) {
        self.records.push(DecisionRecord {
            pass: pass.to_string(),
            candidate: candidate.to_string(),
            decision: decision.to_string(),
            estimated_benefit: benefit.to_string(),
            estimated_cost: cost.to_string(),
            reason: reason.to_string(),
        });
    }

    pub fn find_records_for(&self, symbol: &str) -> Vec<&DecisionRecord> {
        self.records
            .iter()
            .filter(|r| r.candidate.contains(symbol))
            .collect()
    }
}

pub struct CostModel {
    pub inlining_threshold: usize,
    pub sroa_max_fields: usize,
    pub cse_max_expressions: usize,
    pub max_unroll_trip_count: usize,
    pub parallel_dispatch_threshold: usize,
    pub vectorization_width: usize,
}

impl CostModel {
    pub fn new(mode: &str) -> Self {
        match mode {
            "domain" => Self {
                inlining_threshold: 35,
                sroa_max_fields: 16,
                cse_max_expressions: 128,
                max_unroll_trip_count: 8,
                parallel_dispatch_threshold: 50_000,
                vectorization_width: 4,
            },
            "release" => Self {
                inlining_threshold: 20,
                sroa_max_fields: 8,
                cse_max_expressions: 64,
                max_unroll_trip_count: 4,
                parallel_dispatch_threshold: 100_000,
                vectorization_width: 4,
            },
            _ => Self {
                inlining_threshold: 0,
                sroa_max_fields: 0,
                cse_max_expressions: 0,
                max_unroll_trip_count: 0,
                parallel_dispatch_threshold: usize::MAX,
                vectorization_width: 0,
            },
        }
    }

    pub fn apply_pgo_boost(&mut self, is_hot: bool) {
        if is_hot {
            self.inlining_threshold = self.inlining_threshold.saturating_mul(2);
            self.max_unroll_trip_count = self.max_unroll_trip_count.saturating_mul(2);
        }
    }

    pub fn evaluate_inlining(
        &self,
        fn_name: &str,
        inst_count: usize,
        is_pure: bool,
        is_recursive: bool,
    ) -> (bool, String, String, String) {
        self.evaluate_inlining_effect_guided(fn_name, inst_count, is_pure, is_recursive, 1)
    }

    pub fn evaluate_inlining_effect_guided(
        &self,
        _fn_name: &str,
        inst_count: usize,
        is_pure: bool,
        is_recursive: bool,
        effect_budget_multiplier: usize,
    ) -> (bool, String, String, String) {
        if is_recursive {
            return (
                false,
                "None".into(),
                "Infinite".into(),
                "Recursive functions cannot be inlined safely".into(),
            );
        }
        if !is_pure {
            return (
                false,
                "Low".into(),
                "High".into(),
                "Function has side-effects; outline dispatch preserved at effect boundary".into(),
            );
        }
        let effective_budget = self
            .inlining_threshold
            .saturating_mul(effect_budget_multiplier.max(1));
        if inst_count > effective_budget {
            return (
                false,
                "Moderate".into(),
                format!("{} insts > budget {}", inst_count, effective_budget),
                format!(
                    "Callee size ({}) insts exceeds effect-guided inlining threshold ({})",
                    inst_count, effective_budget
                ),
            );
        }

        (
            true,
            "Eliminates call overhead, enables constant propagation across pure boundary".into(),
            format!("{} insts code expansion", inst_count),
            format!(
                "Pure leaf function within effect-guided budget ({} <= {})",
                inst_count, effective_budget
            ),
        )
    }

    pub fn evaluate_sroa(
        &self,
        struct_name: &str,
        field_count: usize,
        is_escaping: bool,
    ) -> (bool, String, String, String) {
        if is_escaping {
            return (
                false,
                "None".into(),
                "Escaping pointer".into(),
                format!(
                    "Struct '{}' escapes local stack frame; heap layout required",
                    struct_name
                ),
            );
        }
        if field_count > self.sroa_max_fields {
            return (
                false,
                "Low".into(),
                format!("Register pressure: {} fields", field_count),
                format!(
                    "Struct field count ({}) exceeds SROA threshold ({})",
                    field_count, self.sroa_max_fields
                ),
            );
        }

        (
            true,
            "Eliminates object allocation, scalarizes fields into registers".into(),
            "Zero cost".into(),
            format!(
                "Local non-escaping aggregate with {} field(s) scalarized via SROA",
                field_count
            ),
        )
    }

    pub fn evaluate_cse(&self, expr_key: &str, count: usize) -> (bool, String, String, String) {
        if count <= 1 {
            return (
                false,
                "None".into(),
                "None".into(),
                "Expression is evaluated only once".into(),
            );
        }
        (
            true,
            format!(
                "Eliminates {} redundant evaluation(s) of pure subexpression",
                count - 1
            ),
            "1 register allocation".into(),
            format!(
                "Common subexpression '{}' reused across {} sites",
                expr_key, count
            ),
        )
    }

    pub fn evaluate_licm(
        &self,
        inst_repr: &str,
        is_invariant: bool,
    ) -> (bool, String, String, String) {
        if !is_invariant {
            return (
                false,
                "None".into(),
                "Semantic violation".into(),
                "Instruction operands depend on loop induction variable".into(),
            );
        }
        (
            true,
            "Hoists computation outside loop header; executed once instead of per-iteration".into(),
            "1 register live across loop".into(),
            format!(
                "Invariant computation '{}' hoisted to pre-header",
                inst_repr
            ),
        )
    }

    /// Cost-model candidate only. The actual unroller is disabled until it can
    /// create fresh SSA values, restructure CFG edges, and pass verification.
    pub fn evaluate_loop_unroll(&self, trip_count: usize) -> (bool, String, String, String) {
        (
            false,
            "Candidate only; no unrolling emitted".into(),
            format!("Would expand code for trip count {}", trip_count),
            format!(
                "Loop unrolling is disabled until SSA renaming and CFG verification are implemented (budget {}).",
                self.max_unroll_trip_count
            ),
        )
    }

    /// Analytical cost only. No parallel code is emitted by the current
    /// compiler because no thread-pool lowering is connected to DMIR.
    pub fn evaluate_parallelization(
        &self,
        workload_estimate: usize,
    ) -> (bool, String, String, String) {
        (
            false,
            "Candidate only; no parallel code emitted".into(),
            "Would require task dispatch, join, and effect proof".into(),
            format!(
                "Workload estimate {} is analytical; backend has no parallel lowering (threshold {}).",
                workload_estimate, self.parallel_dispatch_threshold
            ),
        )
    }

    /// Analytical SIMD eligibility only. A true eligibility result is still
    /// rejected because the current backend has no vector instruction lowering.
    pub fn evaluate_vectorization(&self, is_simd_eligible: bool) -> (bool, String, String, String) {
        (
            false,
            if is_simd_eligible {
                "Candidate only; no SIMD code emitted"
            } else {
                "Rejected"
            }
            .into(),
            "Requires vector lowering".into(),
            if is_simd_eligible {
                format!(
                    "Loop is analytically eligible for width {}, but Cranelift lowering is scalar-only.",
                    self.vectorization_width
                )
            } else {
                "Loop contains scalar control flow or aliasing preventing SIMD.".into()
            },
        )
    }
}
