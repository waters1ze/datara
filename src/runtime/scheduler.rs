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

    /// Current monotonic time in milliseconds.
    pub fn datara_rt_time_now_ms() -> i64;

    /// Current monotonic time in nanoseconds.
    pub fn datara_rt_time_now_ns() -> i64;

    /// High-resolution sub-millisecond monotonic time in milliseconds.
    pub fn datara_rt_time_precise_ms() -> f64;

    /// Elapsed delta time in milliseconds since the previous call to this function.
    pub fn datara_rt_time_delta_ms() -> f64;

    /// Reset the delta time baseline.
    pub fn datara_rt_time_reset_delta();

    /// Create an asynchronous timer that fires after delay_ms milliseconds.
    pub fn datara_rt_timer_create(
        delay_ms: i64,
        callback: Option<extern "C" fn(*mut c_void)>,
        ctx: *mut c_void,
        region_id: i64,
    ) -> i64;

    /// Wait for a timer to complete or be cancelled.
    pub fn datara_rt_timer_wait(timer_id: i64) -> i64;

    /// Cancel a timer.
    pub fn datara_rt_timer_cancel(timer_id: i64) -> i32;

    /// Run concurrent timers and return deterministic checksum.
    pub fn datara_rt_run_concurrent_timers(count: i64, delay_ms: i64) -> i64;
}
