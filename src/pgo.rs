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
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
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
        if let Some(&(taken, total)) = self.branch_frequencies.get(branch_id)
            && total >= 10
        {
            let ratio = taken as f64 / total as f64;
            if ratio >= 0.8 {
                return Some((true, ratio));
            } else if ratio <= 0.2 {
                return Some((false, 1.0 - ratio));
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
        let mut hot_count = 0usize;
        let mut sorted_hot: Vec<(&String, &usize)> = profile.hot_functions.iter().collect();
        sorted_hot.sort_by_key(|(name, _)| *name);
        for (func_name, &call_count) in sorted_hot {
            if call_count > 50 {
                // Static call-graph counts are not execution counts and must
                // never change optimization budgets. A runtime profile is the
                // only source that can authorize a PGO budget mutation.
                if measured {
                    hot_count += 1;
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
        // The boost is a single budget adjustment: applying it once per hot
        // function would multiply the threshold by 2^hot_count.
        if hot_count > 0 {
            optimizer.cost_model.apply_pgo_boost(true);
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
        // Iterate in sorted-name order so per-function side effects (trace
        // records, block edits) apply deterministically.
        let mut fn_names: Vec<String> = module.functions.keys().cloned().collect();
        fn_names.sort();
        for name in fn_names {
            let f = match module.functions.get_mut(&name) {
                Some(f) => f,
                None => continue,
            };
            optimizer.optimize_function(f);

            let measured = profile.is_runtime_measured();
            let mut biased_branches = Vec::new();
            for block in &f.blocks {
                if let crate::dmir::Terminator::CondBranch { .. } = &block.terminator {
                    let branch_id = format!("{}_{}", name, block.id.0);
                    if let Some((always_taken, confidence)) =
                        profile.is_branch_heavily_biased(&branch_id)
                    {
                        biased_branches.push((block.id, branch_id, always_taken, confidence));
                    }
                }
            }

            for (_, branch_id, _always_taken, confidence) in &biased_branches {
                if measured {
                    let (taken, total) = profile
                        .branch_frequencies
                        .get(branch_id)
                        .copied()
                        .unwrap_or((0, 0));
                    let pct = if total > 0 {
                        100.0 * (taken as f64) / (total as f64)
                    } else {
                        0.0
                    };
                    optimizer.trace.record(
                        "PGO_BranchPredict",
                        branch_id,
                        "Applied",
                        "Biased branch layout optimized: hot successor prioritized and cold blocks deferred",
                        "None (semantic CFG equivalence preserved)",
                        &format!(
                            "Runtime measured branch bias: taken={}/{} ({:.1}%, confidence={:.2})",
                            taken, total, pct, confidence
                        ),
                    );
                } else {
                    optimizer.trace.record(
                        "PGO_BranchPredict",
                        branch_id,
                        "Rejected",
                        "Branch bias observed; block layout was not changed",
                        "Runtime provenance required",
                        &format!(
                            "Biased branch with confidence {:.2}; preserving CFG due to lack of measured runtime data",
                            confidence
                        ),
                    );
                }
            }

            if measured && !biased_branches.is_empty() {
                let mut original_blocks: std::collections::HashMap<
                    crate::dmir::BasicBlockId,
                    crate::dmir::BasicBlock,
                > = f.blocks.drain(..).map(|b| (b.id, b)).collect();
                let mut orig_ids: Vec<crate::dmir::BasicBlockId> =
                    original_blocks.keys().cloned().collect();
                orig_ids.sort_by_key(|bid| bid.0);
                let mut new_order = Vec::with_capacity(original_blocks.len());
                let mut placed = std::collections::HashSet::new();

                let mut curr_id = Some(f.entry_block);
                while let Some(id) = curr_id {
                    if !placed.contains(&id)
                        && let Some(blk) = original_blocks.remove(&id)
                    {
                        placed.insert(id);
                        let next_candidate = match &blk.terminator {
                            crate::dmir::Terminator::CondBranch {
                                then_block,
                                else_block,
                                ..
                            } => {
                                let branch_id = format!("{}_{}", name, blk.id.0);
                                if let Some((taken_hot, _)) =
                                    profile.is_branch_heavily_biased(&branch_id)
                                {
                                    if taken_hot {
                                        Some(*then_block)
                                    } else {
                                        Some(*else_block)
                                    }
                                } else {
                                    Some(*then_block)
                                }
                            }
                            crate::dmir::Terminator::Branch { target, .. } => Some(*target),
                            _ => None,
                        };
                        new_order.push(blk);
                        if let Some(nxt) = next_candidate
                            && !placed.contains(&nxt)
                            && original_blocks.contains_key(&nxt)
                        {
                            curr_id = Some(nxt);
                            continue;
                        }
                    }
                    curr_id = orig_ids.iter().find(|bid| !placed.contains(bid)).copied();
                }
                f.blocks = new_order;
            }
        }

        // Fail-closed verification, mirroring Optimizer::run_mutating_pass:
        // a PGO mutation that corrupts DMIR must abort instead of being
        // tolerated.
        if let Err(error) = crate::dmir::verify_module(module) {
            let diag = crate::diagnostics::Diagnostic::error(
                crate::diagnostics::ErrorCode::InternalVerification,
                format!(
                    "[E0901] DMIR verification failed after PGO optimization: {}",
                    error
                ),
                None,
            );
            optimizer.diagnostics.push(diag);
            return;
        }

        // Finalize decision trace in optimizer report
        optimizer.report.decision_trace = optimizer.trace.records.clone();
    }
}

fn inst_max_vid(inst: &crate::dmir::Inst) -> usize {
    let mut m = 0;
    match inst {
        crate::dmir::Inst::ConstInt { dest, .. }
        | crate::dmir::Inst::ConstFloat { dest, .. }
        | crate::dmir::Inst::ConstStr { dest, .. }
        | crate::dmir::Inst::ConstBool { dest, .. }
        | crate::dmir::Inst::LoadVar { dest, .. }
        | crate::dmir::Inst::GetFuncAddr { dest, .. } => {
            m = m.max(dest.0);
        }
        crate::dmir::Inst::AssignVar { value, .. } => {
            m = m.max(value.0);
        }
        crate::dmir::Inst::BinOp {
            dest, left, right, ..
        } => {
            m = m.max(dest.0).max(left.0).max(right.0);
        }
        crate::dmir::Inst::UnOp { dest, operand, .. } => {
            m = m.max(dest.0).max(operand.0);
        }
        crate::dmir::Inst::Call { dest, args, .. } => {
            m = m.max(dest.0);
            for a in args {
                m = m.max(a.0);
            }
        }
        crate::dmir::Inst::MethodCall {
            dest, object, args, ..
        } => {
            m = m.max(dest.0).max(object.0);
            for a in args {
                m = m.max(a.0);
            }
        }
        crate::dmir::Inst::StructInit { dest, fields, .. } => {
            m = m.max(dest.0);
            for (_, v) in fields {
                m = m.max(v.0);
            }
        }
        crate::dmir::Inst::GetField { dest, object, .. } => {
            m = m.max(dest.0).max(object.0);
        }
        crate::dmir::Inst::SetField { object, value, .. } => {
            m = m.max(object.0).max(value.0);
        }
        crate::dmir::Inst::Return { value } => {
            if let Some(v) = value {
                m = m.max(v.0);
            }
        }
        crate::dmir::Inst::Select {
            dest,
            cond,
            then_val,
            else_val,
            ..
        } => {
            m = m.max(dest.0).max(cond.0).max(then_val.0).max(else_val.0);
        }
        crate::dmir::Inst::InlineAsm {
            outputs, inputs, ..
        } => {
            for (_, v) in outputs {
                m = m.max(v.0);
            }
            for (_, v) in inputs {
                m = m.max(v.0);
            }
        }
        _ => {}
    }
    m
}

pub struct ProfileInstrumenter;

impl ProfileInstrumenter {
    pub fn instrument_module(module: &mut crate::dmir::Module, default_out_file: Option<&str>) {
        module.extern_functions.insert(
            "datara_rt_pgo_hit_func".to_string(),
            (vec!["Str".to_string()], "Unit".to_string()),
        );
        module.extern_functions.insert(
            "datara_rt_pgo_hit_branch".to_string(),
            (
                vec!["Str".to_string(), "Int".to_string()],
                "Unit".to_string(),
            ),
        );
        module.extern_functions.insert(
            "datara_rt_pgo_hit_loop".to_string(),
            (
                vec!["Str".to_string(), "Int".to_string()],
                "Unit".to_string(),
            ),
        );
        module.extern_functions.insert(
            "datara_rt_pgo_set_output_file".to_string(),
            (vec!["Str".to_string()], "Unit".to_string()),
        );
        module.extern_functions.insert(
            "datara_rt_pgo_flush".to_string(),
            (vec!["Str".to_string()], "Unit".to_string()),
        );

        let mut fn_names: Vec<String> = module.functions.keys().cloned().collect();
        fn_names.sort();

        for fname in fn_names {
            if fname.starts_with("datara_rt_pgo_") {
                continue;
            }
            let f = match module.functions.get_mut(&fname) {
                Some(f) => f,
                None => continue,
            };

            let mut max_vid = 0usize;
            for (_, _, v) in &f.params {
                max_vid = max_vid.max(v.0);
            }
            for b in &f.blocks {
                for p in &b.params {
                    max_vid = max_vid.max(p.val.0);
                }
                for inst in &b.instructions {
                    max_vid = max_vid.max(inst_max_vid(inst));
                }
            }

            let entry_id = f.entry_block;
            if let Some(entry_blk) = f.blocks.iter_mut().find(|b| b.id == entry_id) {
                let mut prefix_insts = Vec::new();
                if fname == "main"
                    && let Some(out_path) = default_out_file
                    && !out_path.is_empty()
                {
                    max_vid += 1;
                    let path_vid = crate::dmir::ValueId(max_vid);
                    max_vid += 1;
                    let call_vid = crate::dmir::ValueId(max_vid);
                    prefix_insts.push(crate::dmir::Inst::ConstStr {
                        dest: path_vid,
                        value: out_path.to_string(),
                    });
                    prefix_insts.push(crate::dmir::Inst::Call {
                        dest: call_vid,
                        func: "datara_rt_pgo_set_output_file".to_string(),
                        args: vec![path_vid],
                        ty: "Unit".to_string(),
                    });
                }

                max_vid += 1;
                let name_vid = crate::dmir::ValueId(max_vid);
                max_vid += 1;
                let call_vid = crate::dmir::ValueId(max_vid);
                prefix_insts.push(crate::dmir::Inst::ConstStr {
                    dest: name_vid,
                    value: fname.clone(),
                });
                prefix_insts.push(crate::dmir::Inst::Call {
                    dest: call_vid,
                    func: "datara_rt_pgo_hit_func".to_string(),
                    args: vec![name_vid],
                    ty: "Unit".to_string(),
                });

                prefix_insts.append(&mut entry_blk.instructions);
                entry_blk.instructions = prefix_insts;
            }

            for blk in &mut f.blocks {
                match &blk.terminator {
                    crate::dmir::Terminator::CondBranch { cond, .. } => {
                        let branch_id = format!("{}_{}", fname, blk.id.0);
                        max_vid += 1;
                        let br_str_vid = crate::dmir::ValueId(max_vid);
                        max_vid += 1;
                        let call_vid = crate::dmir::ValueId(max_vid);
                        blk.instructions.push(crate::dmir::Inst::ConstStr {
                            dest: br_str_vid,
                            value: branch_id,
                        });
                        blk.instructions.push(crate::dmir::Inst::Call {
                            dest: call_vid,
                            func: "datara_rt_pgo_hit_branch".to_string(),
                            args: vec![br_str_vid, *cond],
                            ty: "Unit".to_string(),
                        });
                    }
                    crate::dmir::Terminator::Return { .. } if fname == "main" => {
                        max_vid += 1;
                        let empty_vid = crate::dmir::ValueId(max_vid);
                        max_vid += 1;
                        let flush_vid = crate::dmir::ValueId(max_vid);
                        blk.instructions.push(crate::dmir::Inst::ConstStr {
                            dest: empty_vid,
                            value: String::new(),
                        });
                        blk.instructions.push(crate::dmir::Inst::Call {
                            dest: flush_vid,
                            func: "datara_rt_pgo_flush".to_string(),
                            args: vec![empty_vid],
                            ty: "Unit".to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
    }
}
