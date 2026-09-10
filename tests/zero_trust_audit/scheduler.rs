use forgen::runtime::{
    DataraRtTaskNode, datara_rt_schedule_run, datara_rt_scheduler_mutex_queue_pushes,
    datara_rt_scheduler_reset_stats, datara_rt_scheduler_wave_executions,
};
use forgen::schedule::{
    ScheduleEffectClass, SchedulePriority, ScheduleProof, TaskNode, WorkerPoolKind,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct TaskContext {
    id: u32,
    shared: Arc<SchedulerLog>,
}

struct SchedulerLog {
    starts: Mutex<HashMap<u32, Instant>>,
    ends: Mutex<HashMap<u32, Instant>>,
    order: Mutex<Vec<u32>>,
}

extern "C" fn audited_task_callback(ctx: *mut c_void) {
    let tctx = unsafe { &*(ctx as *const TaskContext) };
    let start = Instant::now();
    {
        let mut starts = tctx.shared.starts.lock().unwrap();
        starts.insert(tctx.id, start);
    }

    std::thread::sleep(Duration::from_millis(15));

    let end = Instant::now();
    {
        let mut ends = tctx.shared.ends.lock().unwrap();
        ends.insert(tctx.id, end);
        let mut order = tctx.shared.order.lock().unwrap();
        order.push(tctx.id);
    }
}

static SCHED_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn audit_scheduler_c_never_starts_before_a_and_b_and_zero_mutex_pushes() {
    let _guard = SCHED_TEST_MUTEX.lock().unwrap();
    unsafe {
        datara_rt_scheduler_reset_stats();
    }

    let shared = Arc::new(SchedulerLog {
        starts: Mutex::new(HashMap::new()),
        ends: Mutex::new(HashMap::new()),
        order: Mutex::new(Vec::new()),
    });

    let mut ctx_a = TaskContext {
        id: 0,
        shared: shared.clone(),
    };
    let mut ctx_b = TaskContext {
        id: 1,
        shared: shared.clone(),
    };
    let mut ctx_c = TaskContext {
        id: 2,
        shared: shared.clone(),
    };

    let mut c_deps = vec![0u32, 1u32];

    let mut nodes = vec![
        DataraRtTaskNode {
            id: 0,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(audited_task_callback),
            ctx: &mut ctx_a as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 1,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(audited_task_callback),
            ctx: &mut ctx_b as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 2,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 2,
            deps: c_deps.as_mut_ptr(),
            func: Some(audited_task_callback),
            ctx: &mut ctx_c as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
    ];

    let run_res = unsafe { datara_rt_schedule_run(nodes.len() as i64, nodes.as_mut_ptr(), 0) };
    assert_eq!(
        run_res, 3,
        "Scheduler DAG execution failed: expected 3 completed nodes"
    );

    let starts = shared.starts.lock().unwrap();
    let ends = shared.ends.lock().unwrap();
    let order = shared.order.lock().unwrap();

    let start_c = starts[&2];
    let end_a = ends[&0];
    let end_b = ends[&1];

    assert!(
        start_c >= end_a,
        "Dependency violation: Task C started before Task A finished! Start C: {:?}, End A: {:?}",
        start_c,
        end_a
    );
    assert!(
        start_c >= end_b,
        "Dependency violation: Task C started before Task B finished! Start C: {:?}, End B: {:?}",
        start_c,
        end_b
    );

    let pos_a = order.iter().position(|&x| x == 0).unwrap();
    let pos_b = order.iter().position(|&x| x == 1).unwrap();
    let pos_c = order.iter().position(|&x| x == 2).unwrap();
    assert!(
        pos_c > pos_a && pos_c > pos_b,
        "Task C must follow A and B in completion order: {:?}",
        *order
    );

    // Verify 0 mutex queue pushes on deterministic path
    let mutex_pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert_eq!(
        mutex_pushes, 0,
        "CRITICAL ARCHITECTURAL REQUIREMENT: Deterministic path MUST HAVE ZERO mutex queue pushes! Got: {}",
        mutex_pushes
    );

    let waves = unsafe { datara_rt_scheduler_wave_executions() };
    assert!(
        waves >= 2,
        "Expected at least 2 wave executions, got: {}",
        waves
    );
}

#[test]
fn audit_scheduler_cycle_deps_yields_residual_without_hang() {
    let cyclic_tasks = vec![
        TaskNode {
            id: 0,
            name: "task_x".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![1],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
        TaskNode {
            id: 1,
            name: "task_y".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![0],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
    ];

    let start = Instant::now();
    let waves = ScheduleProof::compute_kahn_waves(&cyclic_tasks);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "Kahn computation on cycle must not hang"
    );

    let residual = waves.last().expect("waves non-empty");
    assert_eq!(
        residual,
        &vec![0, 1],
        "Residual wave must contain both cyclic tasks without deadlocking"
    );
}

#[test]
fn audit_scheduler_mixed_graph_does_not_hang() {
    let _guard = SCHED_TEST_MUTEX.lock().unwrap();

    let shared = Arc::new(SchedulerLog {
        starts: Mutex::new(HashMap::new()),
        ends: Mutex::new(HashMap::new()),
        order: Mutex::new(Vec::new()),
    });

    let mut ctx_det = TaskContext {
        id: 0,
        shared: shared.clone(),
    };
    let mut ctx_dyn = TaskContext {
        id: 1,
        shared: shared.clone(),
    };

    let mut nodes = vec![
        DataraRtTaskNode {
            id: 0,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(audited_task_callback),
            ctx: &mut ctx_det as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 1,
            effect_class: 1,
            priority: 1,
            pool: 1,
            deterministic: 0, // dynamic task
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(audited_task_callback),
            ctx: &mut ctx_dyn as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
    ];

    let start = Instant::now();
    let run_res = unsafe { datara_rt_schedule_run(nodes.len() as i64, nodes.as_mut_ptr(), 0) };
    let elapsed = start.elapsed();

    assert_eq!(run_res, 2);
    assert!(
        elapsed < Duration::from_secs(2),
        "Mixed graph execution must not hang!"
    );

    let order = shared.order.lock().unwrap();
    assert_eq!(
        order.len(),
        2,
        "Both deterministic and dynamic tasks must complete"
    );
}

#[test]
fn audit_scheduler_init_once_implementation_in_c_code() {
    let c_source = include_str!("../../src/runtime/datara_rt_scheduler.c");
    assert!(
        c_source.contains("INIT_ONCE") || c_source.contains("InitOnceExecuteOnce"),
        "datara_rt_scheduler.c MUST use InitOnce on Windows to prevent static init race conditions"
    );
    assert!(
        c_source.contains("pthread_once"),
        "datara_rt_scheduler.c MUST use pthread_once on POSIX"
    );
}
