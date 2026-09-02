use super::decision::{AdaptationCategory, AdaptationDecisionLog, AdaptationRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStrategy {
    SingleFusedLoop,           // Zero intermediate allocations; stream consumer
    MaterializedBuffer(usize), // Allocate intermediate buffer for multi-consumer or sort
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallDispatchStrategy {
    DirectInlined,          // Fully inlined body
    DirectStaticCall,       // Static function call (no vtable)
    PolymorphicInlineCache, // Guarded fast path + vtable fallback
    IndirectVirtualCall,    // Dynamic vtable dispatch
}

pub struct StrategyAdapter;

impl StrategyAdapter {
    /// Adapts a data pipeline into a single fused loop or materialized buffer
    pub fn select_pipeline_strategy(
        pipeline_name: &str,
        stage_count: usize,
        has_multiple_consumers: bool,
        requires_sorting: bool,
        log: &mut AdaptationDecisionLog,
    ) -> PipelineStrategy {
        if has_multiple_consumers || requires_sorting {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Strategy,
                pipeline_name,
                "Candidate:MaterializeIntermediateBuffer",
                4.0,
                15.0,
                "Pipeline candidate has multiple downstream consumers or requires global sorting; no buffer rewrite emitted",
                format!("Stages: {}, multi_consumer={}, sorting={}", stage_count, has_multiple_consumers, requires_sorting),
            ));
            PipelineStrategy::MaterializedBuffer(stage_count)
        } else {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Strategy,
                pipeline_name,
                "Candidate:SingleFusedLoop",
                0.0,
                45.0,
                "Linear dataflow candidate; no fused-loop DMIR instruction or backend lowering exists",
                format!("{} stages matched a fusion candidate; IR and allocations remain unchanged", stage_count),
            ));
            PipelineStrategy::SingleFusedLoop
        }
    }

    /// Evaluates call dispatch strategy: devirtualization, inlining, or dynamic dispatch
    pub fn select_dispatch_strategy(
        call_site: &str,
        callee_name: &str,
        implementer_count: usize,
        is_pure: bool,
        body_instruction_count: usize,
        log: &mut AdaptationDecisionLog,
    ) -> CallDispatchStrategy {
        if implementer_count == 1 {
            if body_instruction_count <= 25 && is_pure {
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Strategy,
                    call_site,
                    format!("Candidate:DirectInlined({})", callee_name),
                    0.0,
                    20.0,
                    "Monomorphic pure callee candidate; this adapter does not rewrite the callsite",
                    format!(
                        "Implementers: 1, size: {} insts, pure: true",
                        body_instruction_count
                    ),
                ));
                CallDispatchStrategy::DirectInlined
            } else {
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Strategy,
                    call_site,
                    format!("Candidate:DirectStaticCall({})", callee_name),
                    0.0,
                    12.0,
                    "Monomorphic call target candidate; callsite remains unchanged by this adapter",
                    "Single concrete implementer proven by semantic hierarchy analysis",
                ));
                CallDispatchStrategy::DirectStaticCall
            }
        } else if implementer_count <= 3 {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Strategy,
                call_site,
                "Candidate:PolymorphicInlineCache",
                2.0,
                10.0,
                "Small closed polymorphic hierarchy candidate; no inline-cache lowering is emitted",
                format!("Implementers: {} <= 3", implementer_count),
            ));
            CallDispatchStrategy::PolymorphicInlineCache
        } else {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Strategy,
                call_site,
                "Candidate:IndirectVirtualCall",
                4.0,
                0.0,
                "Open polymorphic hierarchy; dynamic dispatch remains preserved",
                format!("Implementers: {} > 3", implementer_count),
            ));
            CallDispatchStrategy::IndirectVirtualCall
        }
    }
}
