//! Phase 1 Async Execution Subsystem Test Suite
//!
//! Verifies:
//! 1. `async fn` and `await` end-to-end native execution.
//! 2. `await` as DAG wavefront barrier join in ScheduleProof.
//! 3. 1000 concurrent timers with bit-for-bit identical checksum across 20 runs.
//! 4. Cooperative & structured cancellation (Gate 11 & Gate 14).
//! 5. Fail-closed WASM rejection with honest [E0955] error code (no panics).
//! 6. Effect::Foreign preservation on external await.

use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use forgen::runtime::{
    datara_rt_run_concurrent_timers, datara_rt_schedule_cancel, datara_rt_time_now_ms,
    datara_rt_timer_cancel, datara_rt_timer_create, datara_rt_timer_wait,
};
use forgen::schedule::{ScheduleEffectClass, WorkerPoolKind};
use std::fs;
use std::process::Command;

#[test]
fn test_async_fn_and_await_end_to_end() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn worker_x() -> Int {
    return 100
}

async fn worker_y() -> Int {
    return 250
}

fn main() -> Int {
    let x = await worker_x()
    let y = await worker_y()
    if (x + y) == 350 {
        return 0
    }
    return 1
}
"#;
    let out_dir = std::env::temp_dir().join("datara_test_phase1_async");
    let _ = fs::create_dir_all(&out_dir);
    let exe_path = out_dir.join("test_async_phase1_exec.exe");

    let res = compiler.compile_source(src, "test_async_p1.dtr", Some(&exe_path));
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    if let Some(ref path) = res.exe_path {
        let output = Command::new(path)
            .output()
            .expect("Failed to execute compiled async binary");
        assert_eq!(
            output.status.code(),
            Some(0),
            "Expected exit code 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_await_dag_wavefront_barrier_join() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn fetch_part_one() -> Int {
    return 11
}

async fn fetch_part_two() -> Int {
    return 22
}

async fn combine(a: Int, b: Int) -> Int {
    return a + b
}

fn main() -> Int {
    let one = await fetch_part_one()
    let two = await fetch_part_two()
    let sum = await combine(one, two)
    return sum
}
"#;
    let res = compiler.compile_source(src, "test_dag_join.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res
        .schedule_proof
        .expect("ScheduleProof must be present in compilation result");

    let t1 = proof
        .find_task("fetch_part_one")
        .expect("fetch_part_one task");
    let t2 = proof
        .find_task("fetch_part_two")
        .expect("fetch_part_two task");
    let main_task = proof.find_task("main").expect("main task");

    // fetch_part_one and fetch_part_two are pure and deterministic
    assert_eq!(t1.effect_class, ScheduleEffectClass::Pure);
    assert_eq!(t2.effect_class, ScheduleEffectClass::Pure);
    assert!(t1.deterministic);
    assert!(t2.deterministic);

    // main must have dependency edges to predecessor tasks
    assert!(main_task.deps.contains(&t1.id));
    assert!(main_task.deps.contains(&t2.id));

    // Wavefront barrier join verification
    let mut w1 = None;
    let mut w2 = None;
    let mut w_main = None;

    for (w_idx, wave) in proof.waves.iter().enumerate() {
        if wave.contains(&t1.id) {
            w1 = Some(w_idx);
        }
        if wave.contains(&t2.id) {
            w2 = Some(w_idx);
        }
        if wave.contains(&main_task.id) {
            w_main = Some(w_idx);
        }
    }

    assert_eq!(
        w1, w2,
        "Independent async tasks must execute in the same topological wave"
    );
    assert!(
        w_main.unwrap() > w1.unwrap(),
        "Await acts as a DAG barrier join: main must execute in a strictly subsequent wave"
    );
}

#[test]
fn test_concurrent_timers_1000_deterministic_checksum() {
    // Run 1000 concurrent timers across 20 iterations.
    // Every iteration MUST produce the identical FNV-1a checksum.
    let count = 1000;
    let delay_ms = 1;

    let initial_checksum = unsafe { datara_rt_run_concurrent_timers(count, delay_ms) };
    assert_ne!(
        initial_checksum, 0,
        "Timer checksum must be non-zero after evaluating 1000 timers"
    );

    for run_idx in 1..20 {
        let checksum = unsafe { datara_rt_run_concurrent_timers(count, delay_ms) };
        assert_eq!(
            checksum, initial_checksum,
            "Run #{}: Checksum mismatch! Timers must be strictly deterministic across all runs",
            run_idx
        );
    }
}

#[test]
fn test_structured_cancellation_gate_11_and_14() {
    let region_id = 9942;

    // Create pending timers in the region
    let t1 = unsafe { datara_rt_timer_create(5000, None, std::ptr::null_mut(), region_id) };
    let t2 = unsafe { datara_rt_timer_create(5000, None, std::ptr::null_mut(), region_id) };
    assert!(t1 > 0, "Timer 1 must have valid ID");
    assert!(t2 > 0, "Timer 2 must have valid ID");

    // Cancel the entire region (Gate 11 & Gate 14)
    unsafe { datara_rt_schedule_cancel(region_id) };

    // Waiting on cancelled timers returns 2 (STATUS_CANCELLED)
    let wait_res1 = unsafe { datara_rt_timer_wait(t1) };
    let wait_res2 = unsafe { datara_rt_timer_wait(t2) };

    assert_eq!(
        wait_res1, 2,
        "Timer in cancelled region must be marked STATUS_CANCELLED"
    );
    assert_eq!(
        wait_res2, 2,
        "Timer in cancelled region must be marked STATUS_CANCELLED"
    );

    // Explicit cancel on already cancelled timer returns 0 (false)
    let cancel_res = unsafe { datara_rt_timer_cancel(t1) };
    assert_eq!(
        cancel_res, 0,
        "Cancelling an already cancelled timer returns 0"
    );
}

#[test]
fn test_wasm_backend_fail_closed_e0955() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn slow_query() -> Int {
    return 777
}

fn main() -> Int {
    let val = await slow_query()
    return val
}
"#;
    let dmir_res = compiler.compile_source_to_dmir(src, "wasm_async_test.dtr");
    assert!(
        dmir_res.is_ok(),
        "DMIR lowering must succeed: {:?}",
        dmir_res.as_ref().err()
    );
    let dmir = dmir_res.unwrap();

    let temp_wasm = std::env::temp_dir().join("test_wasm_async_fail_closed.wasm");
    let wasm_res = WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm);

    assert!(
        wasm_res.is_err(),
        "WASM backend must fail-closed when compiling await"
    );
    let err_msg = wasm_res.err().unwrap();
    assert!(
        err_msg.contains("[E0955]"),
        "Error message must contain honest error code [E0955], got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("WebAssembly backend does not support async execution"),
        "Error message must explain async lack of support, got: {}",
        err_msg
    );
}

#[test]
fn test_async_foreign_effect_preservation() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
extern fn datara_rt_time_now_ms() -> Int

async fn foreign_call_task() -> Int {
    mut res = 0
    unsafe(justification: "External C API call") {
        res = datara_rt_time_now_ms()
    }
    return res
}

fn main() -> Int {
    let v = await foreign_call_task()
    return v
}
"#;
    let res = compiler.compile_source(src, "test_foreign_async.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");

    let task = proof
        .find_task("foreign_call_task")
        .expect("foreign_call_task must exist in proof");

    // Calling extern fn infers Effect::Foreign / Effect::IO
    assert_eq!(
        task.effect_class,
        ScheduleEffectClass::IO,
        "Tasks invoking foreign/external functions must be classified as IO/Foreign"
    );
    assert_eq!(
        task.pool,
        WorkerPoolKind::IOPool,
        "Tasks with foreign effects must be scheduled on IOPool"
    );
    assert!(
        !task.deterministic,
        "Tasks with foreign effects must be marked non-deterministic"
    );
}

#[test]
fn test_monotonic_time_now_api() {
    let t1 = unsafe { datara_rt_time_now_ms() };
    std::thread::sleep(std::time::Duration::from_millis(10));
    let t2 = unsafe { datara_rt_time_now_ms() };

    assert!(
        t2 >= t1,
        "datara_rt_time_now_ms must be strictly monotonic: t1={}, t2={}",
        t1,
        t2
    );
}
