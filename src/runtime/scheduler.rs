//! Rust FFI bindings to the C Proof-Carrying Scheduler runtime.

use std::ffi::c_void;

#[repr(C)]
#[derive(Debug)]
pub struct DataraRtTaskNode {
    pub id: u32,
    pub effect_class: u32,
    pub priority: u32,
    pub pool: u32,
    pub deterministic: u32,
    pub dep_count: u32,
    pub deps: *mut u32,
    pub func: Option<extern "C" fn(*mut c_void)>,
    pub ctx: *mut c_void,
    pub result: *mut c_void,
    pub status: i32,
}

unsafe impl Send for DataraRtTaskNode {}
unsafe impl Sync for DataraRtTaskNode {}

unsafe extern "C" {
    /// Execute tasks respecting precomputed dependencies.
    /// Deterministic tasks are executed via Kahn wavefront flat parallel_for.
    /// Dynamic tasks are dispatched via the ready-queue.
    pub fn datara_rt_schedule_run(
        node_count: i64,
        nodes: *mut DataraRtTaskNode,
        region_id: i64,
    ) -> i64;

    /// Cancel all pending tasks for the given region.
    pub fn datara_rt_schedule_cancel(region_id: i64);

    /// Total pushes to the mutex-protected ready-queue.
    /// Guarantees 0 pushes on the deterministic CPU hot-path.
    pub fn datara_rt_scheduler_mutex_queue_pushes() -> u64;

    /// Total wavefront parallel iterations executed.
    pub fn datara_rt_scheduler_wave_executions() -> u64;

    /// Reset verification counters.
    pub fn datara_rt_scheduler_reset_stats();
}
