use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptationCategory {
    Representation, // Scalar vs Aggregate, Stack vs Heap, AoS vs SoA
    Execution,      // Sequential vs SIMD vs Parallel ThreadPool vs Task
    Layout,         // Memory alignment, struct packing, field reordering
    Strategy,       // Fused vs Materialized, Direct vs Devirtualized, Intrinsic vs Runtime
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptationRecord {
    pub category: AdaptationCategory,
    pub candidate: String,
    pub decision: String,
    pub cost: f64,
    pub benefit: f64,
    pub reason: String,
    pub evidence: String,
}

impl AdaptationRecord {
    pub fn new(
        category: AdaptationCategory,
        candidate: impl Into<String>,
        decision: impl Into<String>,
        cost: f64,
        benefit: f64,
        reason: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            category,
            candidate: candidate.into(),
            decision: decision.into(),
            cost,
            benefit,
            reason: reason.into(),
            evidence: evidence.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdaptationDecisionLog {
    pub records: Vec<AdaptationRecord>,
}

impl AdaptationDecisionLog {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn record(&mut self, rec: AdaptationRecord) {
        self.records.push(rec);
    }
}
