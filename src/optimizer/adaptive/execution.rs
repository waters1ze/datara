use super::decision::{AdaptationCategory, AdaptationDecisionLog, AdaptationRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    SequentialScalar,
    SIMDVectorized(usize),     // 4-lane or 8-lane SIMD
    ParallelThreadPool(usize), // Worker thread count (e.g. 4, 8, 16)
    AsyncTaskReactor,
}

pub struct ExecutionAdapter;

impl ExecutionAdapter {
    /// Records the execution strategy for a loop.
    ///
    /// `estimated_iterations` of `0` means the trip count is **not** statically
    /// known, which is the normal case.
    ///
    /// IMPORTANT: this reports what the compiler can actually emit today. The
    /// Cranelift backend produces straight-line scalar code — there is no SIMD
    /// lowering, no thread pool wired into codegen (`ParallelRuntime` exists in
    /// `runtime::parallel` but nothing ever constructs it) and no async
    /// reactor. Selecting `SIMDVectorized` / `ParallelThreadPool` /
    /// `AsyncTaskReactor` would only ever write a log line describing work that
    /// never happens, so those are reported as *not selected*, together with
    /// what is missing, and the real decision is always `SequentialScalar`.
    pub fn select_execution_strategy(
        loop_name: &str,
        estimated_iterations: usize,
        is_pure_computation: bool,
        has_io_effects: bool,
        cpu_cores: usize,
        log: &mut AdaptationDecisionLog,
    ) -> ExecutionStrategy {
        let trip = if estimated_iterations == 0 {
            "trip count not statically known".to_string()
        } else {
            format!("static estimate of {} iterations", estimated_iterations)
        };

        let mut rationale = format!("Sequential scalar execution ({}).", trip);

        if has_io_effects {
            rationale.push_str(
                " AsyncTaskReactor NOT selected: the backend has no async runtime, so the \
                 I/O call stays a blocking call.",
            );
        } else if is_pure_computation {
            rationale.push_str(&format!(
                " SIMDVectorized NOT selected: the backend has no vector lowering. \
                 ParallelThreadPool NOT selected: no thread pool is wired into codegen \
                 ({} logical cores detected).",
                cpu_cores
            ));
        } else {
            rationale.push_str(
                " Loop body is not a pure computation, so neither SIMD nor parallel \
                 execution would be legal.",
            );
        }

        log.record(AdaptationRecord::new(
            AdaptationCategory::Execution,
            loop_name,
            "SequentialScalar",
            0.0,
            0.0,
            &rationale,
            format!(
                "pure={}, io_effects={}, cores={}, iterations={}",
                is_pure_computation, has_io_effects, cpu_cores, estimated_iterations
            ),
        ));

        ExecutionStrategy::SequentialScalar
    }
}
