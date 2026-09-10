use forgen::driver::ForgenCompiler;
use forgen::runtime::{
    DataraRtTaskNode, datara_rt_schedule_cancel, datara_rt_schedule_run,
    datara_rt_scheduler_mutex_queue_pushes, datara_rt_scheduler_reset_stats,
    datara_rt_scheduler_wave_executions,
};
use std::ffi::c_void;
use std::fs;
use std::process::Command;

use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

struct MapTaskCtx {
    id: usize,
    output: *mut i64,
}

extern "C" fn map_worker(raw_ctx: *mut c_void) {
    unsafe {
        let ctx = &*(raw_ctx as *const MapTaskCtx);
        let mut val = (ctx.id as i64) + 1;
        let mut k = 0;
        while k < 50 {
            val = (val * 31 + 17) % 1000003;
            k += 1;
        }
        *ctx.output.add(ctx.id) = val;
    }
}

/// Test 1: Parallel map over 1000 items, collect, print — execute 20x, assert byte-identical stdout/results across all 20 runs.
#[test]
fn test_parallel_map_1000_items_determinism_20_runs() {
    let _guard = TEST_MUTEX.lock().unwrap();
    const NUM_ITEMS: usize = 1000;
    const NUM_RUNS: usize = 20;

    let mut baseline_results: Option<Vec<u8>> = None;

    for run_idx in 0..NUM_RUNS {
        unsafe {
            datara_rt_scheduler_reset_stats();
        }

        let mut output_buf = vec![0i64; NUM_ITEMS];
        let mut contexts: Vec<MapTaskCtx> = (0..NUM_ITEMS)
            .map(|id| MapTaskCtx {
                id,
                output: output_buf.as_mut_ptr(),
            })
            .collect();

        let mut nodes: Vec<DataraRtTaskNode> = (0..NUM_ITEMS)
            .map(|id| DataraRtTaskNode {
                id: id as u32,
                effect_class: 0,  // Pure
                priority: 1,      // Hot
                pool: 0,          // CPUPool
                deterministic: 1, // Deterministic
                dep_count: 0,
                deps: std::ptr::null_mut(),
                func: Some(map_worker),
                ctx: &mut contexts[id] as *mut MapTaskCtx as *mut c_void,
                result: std::ptr::null_mut(),
                status: 0,
            })
            .collect();

        let status = unsafe { datara_rt_schedule_run(NUM_ITEMS as i64, nodes.as_mut_ptr(), 0) };
        assert_eq!(
            status, NUM_ITEMS as i64,
            "Schedule run failed on run {}",
            run_idx
        );

        // Serialize output buffer to byte representation
        let mut output_bytes = Vec::new();
        for &val in &output_buf {
            output_bytes.extend_from_slice(&val.to_le_bytes());
        }

        if let Some(ref baseline) = baseline_results {
            assert_eq!(
                baseline, &output_bytes,
                "Run {} produced non-deterministic output differing from baseline!",
                run_idx
            );
        } else {
            baseline_results = Some(output_bytes);
        }
    }

    assert!(baseline_results.is_some());
}

/// Test 2: Prove hot-path CPU tasks never touch the mutex queue (datara_rt_scheduler_mutex_queue_pushes() == 0).
#[test]
fn test_hot_path_cpu_tasks_zero_mutex_queue_pushes() {
    let _guard = TEST_MUTEX.lock().unwrap();
    const NUM_ITEMS: usize = 1000;

    unsafe {
        datara_rt_scheduler_reset_stats();
    }

    let mut output_buf = vec![0i64; NUM_ITEMS];
    let mut contexts: Vec<MapTaskCtx> = (0..NUM_ITEMS)
        .map(|id| MapTaskCtx {
            id,
            output: output_buf.as_mut_ptr(),
        })
        .collect();

    let mut nodes: Vec<DataraRtTaskNode> = (0..NUM_ITEMS)
        .map(|id| DataraRtTaskNode {
            id: id as u32,
            effect_class: 0,  // Pure
            priority: 1,      // Hot
            pool: 0,          // CPUPool
            deterministic: 1, // Deterministic
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(map_worker),
            ctx: &mut contexts[id] as *mut MapTaskCtx as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        })
        .collect();

    let status = unsafe { datara_rt_schedule_run(NUM_ITEMS as i64, nodes.as_mut_ptr(), 0) };
    assert_eq!(status, NUM_ITEMS as i64);

    let queue_pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert_eq!(
        queue_pushes, 0,
        "Proof-Carrying Scheduler contract violated: CPU hot-path tasks touched mutex queue {} times!",
        queue_pushes
    );

    // Contrastive verification: If tasks are non-deterministic, they MUST use the ready queue
    let mut dynamic_node = DataraRtTaskNode {
        id: 0,
        effect_class: 1, // IO
        priority: 0,
        pool: 1,          // IOPool
        deterministic: 0, // Dynamic
        dep_count: 0,
        deps: std::ptr::null_mut(),
        func: Some(map_worker),
        ctx: &mut contexts[0] as *mut MapTaskCtx as *mut c_void,
        result: std::ptr::null_mut(),
        status: 0,
    };

    let dyn_status =
        unsafe { datara_rt_schedule_run(1, &mut dynamic_node as *mut DataraRtTaskNode, 0) };
    assert_eq!(dyn_status, 1);

    let dynamic_pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert!(
        dynamic_pushes > 0,
        "Dynamic task was expected to route through the ready-queue fallback"
    );
}

struct WaveDepCtx {
    task_id: usize,
    wave_id: usize,
    log: *mut Vec<(usize, usize)>,
    mutex: *mut std::sync::Mutex<()>,
}

extern "C" fn wave_dep_worker(raw_ctx: *mut c_void) {
    unsafe {
        let ctx = &*(raw_ctx as *const WaveDepCtx);
        let _guard = (*ctx.mutex).lock().unwrap();
        (*ctx.log).push((ctx.wave_id, ctx.task_id));
    }
}

/// Test 3: Wave-level parallelism measurements and multi-wave topological execution order.
#[test]
fn test_wave_level_parallelism_measurements() {
    let _guard = TEST_MUTEX.lock().unwrap();
    unsafe {
        datara_rt_scheduler_reset_stats();
    }

    // Construct a 3-wave DAG:
    // Wave 0: Tasks 0..100 (100 independent leaf tasks)
    // Wave 1: Tasks 100..200 (100 tasks, task 100+i depends on task i)
    // Wave 2: Task 200 (single join task depending on tasks 100 and 101)
    const WAVE0_COUNT: usize = 100;
    const WAVE1_COUNT: usize = 100;
    const TOTAL_TASKS: usize = WAVE0_COUNT + WAVE1_COUNT + 1;

    let log_mutex = std::sync::Mutex::new(());
    let mut execution_log: Vec<(usize, usize)> = Vec::with_capacity(TOTAL_TASKS);

    let mut contexts: Vec<WaveDepCtx> = Vec::with_capacity(TOTAL_TASKS);
    for i in 0..WAVE0_COUNT {
        contexts.push(WaveDepCtx {
            task_id: i,
            wave_id: 0,
            log: &mut execution_log as *mut _,
            mutex: &log_mutex as *const _ as *mut _,
        });
    }
    for i in 0..WAVE1_COUNT {
        contexts.push(WaveDepCtx {
            task_id: WAVE0_COUNT + i,
            wave_id: 1,
            log: &mut execution_log as *mut _,
            mutex: &log_mutex as *const _ as *mut _,
        });
    }
    contexts.push(WaveDepCtx {
        task_id: 200,
        wave_id: 2,
        log: &mut execution_log as *mut _,
        mutex: &log_mutex as *const _ as *mut _,
    });

    let mut dep_storage: Vec<Vec<u32>> = Vec::with_capacity(TOTAL_TASKS);
    for _i in 0..WAVE0_COUNT {
        dep_storage.push(vec![]);
    }
    for i in 0..WAVE1_COUNT {
        dep_storage.push(vec![i as u32]);
    }
    // Join task 200 depends on tasks 100 and 101
    dep_storage.push(vec![100, 101]);

    let mut nodes: Vec<DataraRtTaskNode> = (0..TOTAL_TASKS)
        .map(|id| DataraRtTaskNode {
            id: id as u32,
            effect_class: 0,  // Pure
            priority: 1,      // Hot
            pool: 0,          // CPUPool
            deterministic: 1, // Deterministic wavefront
            dep_count: dep_storage[id].len() as u32,
            deps: if dep_storage[id].is_empty() {
                std::ptr::null_mut()
            } else {
                dep_storage[id].as_mut_ptr()
            },
            func: Some(wave_dep_worker),
            ctx: &mut contexts[id] as *mut WaveDepCtx as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        })
        .collect();

    let status = unsafe { datara_rt_schedule_run(TOTAL_TASKS as i64, nodes.as_mut_ptr(), 0) };
    assert_eq!(status, TOTAL_TASKS as i64);

    let wave_execs = unsafe { datara_rt_scheduler_wave_executions() };
    assert_eq!(
        wave_execs, 3,
        "Expected exactly 3 wavefront executions, got {}",
        wave_execs
    );

    let queue_pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert_eq!(
        queue_pushes, 0,
        "Deterministic multi-wave graph must not touch mutex queue"
    );

    println!("WAVE_PARALLELISM_MEASUREMENT:");
    println!("  Total tasks dispatched: {}", TOTAL_TASKS);
    println!("  Wavefront levels: {}", wave_execs);
    println!("  Wave 0 width (leaf tasks): {}", WAVE0_COUNT);
    println!("  Wave 1 width (dependent layer): {}", WAVE1_COUNT);
    println!("  Wave 2 width (join point): 1");
    println!("  Mutex queue pushes: {}", queue_pushes);

    // Verify all tasks in Wave 0 completed before Wave 1 began,
    // and all tasks in Wave 1 completed before Wave 2 ran.
    assert_eq!(execution_log.len(), TOTAL_TASKS);

    let mut max_wave_seen = 0;
    for &(wave_id, _task_id) in &execution_log {
        assert!(
            wave_id >= max_wave_seen,
            "Wave ordering violated: executed wave {} after having reached wave {}",
            wave_id,
            max_wave_seen
        );
        max_wave_seen = wave_id;
    }
}

/// Test 4: End-to-end compiled Datara program executed 20x verifying byte-identical stdout.
#[test]
fn test_compiled_datara_program_determinism_20x() {
    let compiler = ForgenCompiler::new("release");
    let src = r#"
fn compute_item(x: Int) -> Int {
    mut acc = x + 1
    mut k = 0
    while k < 30 {
        acc = (acc * 31 + 17) % 1000003
        k = k + 1
    }
    return acc
}

fn main() {
    mut checksum = 0
    mut i = 0
    while i < 1000 {
        checksum = (checksum + compute_item(i)) % 1000000007
        i = i + 1
    }
    out "DETERMINISTIC_CHECKSUM:" + checksum
}
"#;
    let out_dir = std::env::temp_dir().join("datara_test_determinism_bin");
    let _ = fs::create_dir_all(&out_dir);
    let exe_path = out_dir.join("test_det_20x.exe");

    let res = compiler.compile_source(src, "test_det_20x.dtr", Some(&exe_path));
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let exe = res.exe_path.expect("exe_path missing");
    let mut baseline_stdout: Option<String> = None;

    for i in 0..20 {
        let output = Command::new(&exe)
            .output()
            .unwrap_or_else(|e| panic!("Failed to execute compiled binary on run {}: {}", i, e));
        assert_eq!(output.status.code(), Some(0));

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        assert!(stdout.contains("DETERMINISTIC_CHECKSUM:"));

        if let Some(ref baseline) = baseline_stdout {
            assert_eq!(
                baseline, &stdout,
                "Run {} stdout differed from baseline stdout!",
                i
            );
        } else {
            baseline_stdout = Some(stdout);
        }
    }

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
}

extern "C" fn dummy_worker(_raw_ctx: *mut c_void) {}

/// Test 5: Verify Wave A4 guarantees:
/// 1. ensure_cancel_cs() ensures race-free region cancellation.
/// 2. Scheduler returns completed count cleanly on empty queue without hanging.
#[test]
fn test_scheduler_cancel_and_empty_queue_handling() {
    let _guard = TEST_MUTEX.lock().unwrap();

    // 1. Region cancellation test
    unsafe {
        datara_rt_schedule_cancel(999);
        let mut node = DataraRtTaskNode {
            id: 0,
            effect_class: 0,
            priority: 1,
            pool: 0,
            deterministic: 1,
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(dummy_worker),
            ctx: std::ptr::null_mut(),
            result: std::ptr::null_mut(),
            status: 0,
        };
        let status = datara_rt_schedule_run(1, &mut node as *mut DataraRtTaskNode, 999);
        assert_eq!(status, -1, "Cancelled region must return -1");
        assert_eq!(
            node.status, 3,
            "Cancelled task status must be 3 (cancelled)"
        );
    }

    // 2. Empty ready queue / cyclic dependency non-blocking return test
    unsafe {
        let mut dep0 = [1u32];
        let mut dep1 = [0u32];
        let mut nodes = [
            DataraRtTaskNode {
                id: 0,
                effect_class: 0,
                priority: 1,
                pool: 0,
                deterministic: 0, // Dynamic path
                dep_count: 1,
                deps: dep0.as_mut_ptr(),
                func: Some(dummy_worker),
                ctx: std::ptr::null_mut(),
                result: std::ptr::null_mut(),
                status: 0,
            },
            DataraRtTaskNode {
                id: 1,
                effect_class: 0,
                priority: 1,
                pool: 0,
                deterministic: 0, // Dynamic path
                dep_count: 1,
                deps: dep1.as_mut_ptr(),
                func: Some(dummy_worker),
                ctx: std::ptr::null_mut(),
                result: std::ptr::null_mut(),
                status: 0,
            },
        ];

        let completed = datara_rt_schedule_run(2, nodes.as_mut_ptr(), 0);
        assert_eq!(
            completed, 0,
            "Cyclic unsatisfied tasks must return 0 completed without hanging"
        );
        assert_eq!(nodes[0].status, 0, "Unrun task must remain status 0");
        assert_eq!(nodes[1].status, 0, "Unrun task must remain status 0");
    }
}
