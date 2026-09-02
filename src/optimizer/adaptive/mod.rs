pub mod decision;
pub mod execution;
pub mod representation;
pub mod strategy;

pub use decision::{AdaptationCategory, AdaptationDecisionLog, AdaptationRecord};
pub use execution::ExecutionAdapter;
pub use representation::RepresentationAdapter;
pub use strategy::StrategyAdapter;

use crate::dmir::Module;

pub struct SemanticAdaptationEngine {
    pub mode: String,
    pub log: AdaptationDecisionLog,
}

impl SemanticAdaptationEngine {
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.to_string(),
            log: AdaptationDecisionLog::new(),
        }
    }

    /// Orchestrates full semantic adaptation across DMIR module
    pub fn adapt_module(&mut self, module: &mut Module) {
        if self.mode == "quick" || self.mode == "start" {
            // Quick mode skips heavy SAE passes for sub-millisecond turnarounds
            return;
        }

        for (_, f) in module.functions.iter_mut() {
            // 1. Data representation adaptation (Scalar vs Stack vs Heap)
            RepresentationAdapter::adapt_representation(f, &mut self.log);

            // 2. Call dispatch & inlining strategy
            for b in &f.blocks {
                for inst in &b.instructions {
                    if let crate::dmir::Inst::Call { func, .. } = inst {
                        StrategyAdapter::select_dispatch_strategy(
                            &format!("{}:call:{}", f.name, func),
                            func,
                            1, // monomorphic in standard Datara lowering
                            true,
                            15,
                            &mut self.log,
                        );
                    }
                }
            }

            // 3. Execution strategy adaptation (Sequential vs SIMD vs Parallel)
            //
            // The inputs used to be hardcoded (100_000 iterations, 8 cores,
            // always pure, never I/O), which made every function look like a
            // hot numerical loop. They are now derived from the function's real
            // CFG and from this machine.
            let cfg = crate::dmir::cfg::ControlFlowGraph::build(f);
            if !cfg.loops.is_empty() {
                let mut io_effects = false;
                let mut pure = true;
                for lp in &cfg.loops {
                    for &bid in &lp.blocks {
                        if let Some(blk) = f.get_block(bid) {
                            for inst in &blk.instructions {
                                match inst {
                                    crate::dmir::Inst::Out { .. }
                                    | crate::dmir::Inst::Err { .. } => {
                                        io_effects = true;
                                        pure = false;
                                    }
                                    crate::dmir::Inst::Call { .. }
                                    | crate::dmir::Inst::MethodCall { .. } => {
                                        pure = false;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                ExecutionAdapter::select_execution_strategy(
                    &format!("{}:loop", f.name),
                    0, // trip count is not statically known
                    pure,
                    io_effects,
                    Self::cpu_cores(),
                    &mut self.log,
                );
            }
        }
    }

    /// Real logical CPU count for this machine, instead of an assumed constant.
    fn cpu_cores() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }
}
