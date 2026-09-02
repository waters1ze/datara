use crate::optimizer::Optimizer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileData {
    pub project_name: String,
    /// Where the numbers in this profile came from.
    ///
    /// `"static"`  - derived from the compiler's own call graph. NOT measured:
    ///               counts are numbers of *call sites*, not of executions.
    /// `"runtime"` - collected by actually instrumenting and running the program.
    ///
    /// Only a `"runtime"` profile can support real profile-guided decisions.
    #[serde(default)]
    pub source: String,
    pub hot_functions: HashMap<String, usize>,
    pub branch_frequencies: HashMap<String, (usize, usize)>, // (taken, total)
    pub loop_trip_counts: HashMap<String, usize>,
    pub allocation_hotspots: HashMap<String, usize>,
    pub type_feedback: HashMap<String, String>,
}

impl ProfileData {
    pub fn new(project: &str) -> Self {
        Self {
            project_name: project.to_string(),
            source: "static".to_string(),
            hot_functions: HashMap::new(),
            branch_frequencies: HashMap::new(),
            loop_trip_counts: HashMap::new(),
            allocation_hotspots: HashMap::new(),
            type_feedback: HashMap::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read profile '{}': {}", path.display(), e))?;
        serde_json::from_str(&content).map_err(|e| format!("Invalid profile format: {}", e))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_function_call(&mut self, func_name: &str) {
        *self.hot_functions.entry(func_name.to_string()).or_insert(0) += 1;
    }

    pub fn record_loop_iterations(&mut self, loop_id: &str, count: usize) {
        self.loop_trip_counts.insert(loop_id.to_string(), count);
    }

    pub fn record_branch(&mut self, branch_id: &str, taken: bool) {
        let entry = self
            .branch_frequencies
            .entry(branch_id.to_string())
            .or_insert((0, 0));
        if taken {
            entry.0 += 1;
        }
        entry.1 += 1;
    }

    pub fn record_allocation(&mut self, struct_name: &str) {
        *self
            .allocation_hotspots
            .entry(struct_name.to_string())
            .or_insert(0) += 1;
    }

    /// True only when the numbers come from an actual instrumented run.
    pub fn is_runtime_measured(&self) -> bool {
        self.source == "runtime"
    }

    pub fn is_hot(&self, func_name: &str) -> bool {
        self.hot_functions.get(func_name).copied().unwrap_or(0) > 100
    }

    pub fn is_branch_heavily_biased(&self, branch_id: &str) -> Option<(bool, f64)> {
        if let Some(&(taken, total)) = self.branch_frequencies.get(branch_id) {
            if total >= 10 {
                let ratio = taken as f64 / total as f64;
                if ratio >= 0.8 {
                    return Some((true, ratio));
                } else if ratio <= 0.2 {
                    return Some((false, 1.0 - ratio));
                }
            }
        }
        None
    }
}

pub struct ProfileGuidedOptimizer;

impl ProfileGuidedOptimizer {
    /// Ingest PGO profile data and boost optimization budgets for hot paths
    pub fn apply_profile_to_optimizer(optimizer: &mut Optimizer, profile: &ProfileData) {
        let measured = profile.is_runtime_measured();
        for (func_name, &call_count) in &profile.hot_functions {
            if call_count > 50 {
                // Static call-graph counts are not execution counts and must
                // never change optimization budgets. A runtime profile is the
                // only source that can authorize a PGO budget mutation.
                if measured {
                    optimizer.cost_model.apply_pgo_boost(true);
                    optimizer.trace.record(
                        "PGO",
                        func_name,
                        "Applied",
                        "Expanded inlining threshold 2x for runtime-hot function",
                        "None (semantic invariant preserved)",
                        &format!(
                            "instrumented run measured {} invocations (> 50 hot threshold)",
                            call_count
                        ),
                    );
                } else {
                    optimizer.trace.record(
                        "PGO",
                        func_name,
                        "Rejected",
                        "No budget change from static profile",
                        "Runtime provenance required",
                        &format!(
                            "{} call site(s) target '{}' in the static call graph; this is not an execution count",
                            call_count, func_name
                        ),
                    );
                }
            }
        }
    }

    /// Full-cycle PGO optimization on DMIR module using gathered runtime profile
    pub fn optimize_module(
        optimizer: &mut Optimizer,
        module: &mut crate::dmir::Module,
        profile: &ProfileData,
    ) {
        Self::apply_profile_to_optimizer(optimizer, profile);

        // 1. Hot function inlining pass with expanded PGO budget
        optimizer.inline_pure_functions(module);

        // 2. Intra-procedural optimizations (Branch re-ordering & SROA)
        for (name, f) in module.functions.iter_mut() {
            optimizer.optimize_function(f);

            // If branch is heavily biased towards hot path, optimize block layout
            for block in &mut f.blocks {
                if let crate::dmir::Terminator::CondBranch {
                    cond: _,
                    then_block: _,
                    else_block: _,
                    ..
                } = &mut block.terminator
                {
                    let branch_id = format!("{}_{}", name, block.id.0);
                    if let Some((always_taken, confidence)) =
                        profile.is_branch_heavily_biased(&branch_id)
                    {
                        if confidence > 0.95 && always_taken {
                            optimizer.trace.record(
                                "PGO_BranchPredict",
                                &branch_id,
                                "Rejected",
                                "Branch bias observed; block layout was not changed",
                                "Backend block-reordering proof required",
                                &format!(
                                    "Biased branch with confidence {:.2}; preserving CFG",
                                    confidence
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
}
