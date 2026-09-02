use crate::dmir::{Function, Inst};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};

pub struct PipelineFusionOptimizer;

impl PipelineFusionOptimizer {
    /// Inspect pipeline-shaped IR without claiming a transformation that is not
    /// implemented. The current DMIR has no fused iterator/stream instruction,
    /// and the Cranelift backend lowers calls independently, so this pass must
    /// not mutate the function or report `Applied`.
    pub fn fuse_pipelines(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        for block in &f.blocks {
            let pipeline_stages = block
                .instructions
                .iter()
                .filter(|inst| match inst {
                    Inst::Call { func, .. } => func.contains("filter") || func.contains("map"),
                    Inst::MethodCall { method, .. } => method == "filter" || method == "map",
                    _ => false,
                })
                .count();

            if pipeline_stages >= 2 {
                trace.record(
                    "PipelineFusion",
                    &format!("{}:bb{}", f.name, block.id.0),
                    "Rejected",
                    "Candidate detected; no emitted fused-loop transformation",
                    "Requires iterator fusion IR and backend lowering",
                    "The block contains map/filter-shaped calls, but preserving calls is safer than claiming intermediate buffers were removed.",
                );
            }

            let arithmetic_stages = block.instructions.iter().filter(|inst| {
                matches!(inst, Inst::BinOp { left, right, .. }
                    if block.instructions.iter().any(|other| matches!(other, Inst::BinOp { dest, .. } if dest == left || dest == right)))
            }).count();

            if arithmetic_stages >= 2 {
                trace.record(
                    "ArithmeticPipelineFusion",
                    &format!("{}:bb{}", f.name, block.id.0),
                    "Rejected",
                    "Candidate detected; arithmetic expressions were not rewritten",
                    "Requires expression reassociation and verified cost model",
                    "Chained BinOps remain in DMIR and are lowered individually by Cranelift.",
                );
            }
        }

        0
    }
}
