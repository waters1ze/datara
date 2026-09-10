use forgen::runtime::{
    DataraRtTaskNode, datara_rt_schedule_run, datara_rt_scheduler_mutex_queue_pushes,
    datara_rt_scheduler_reset_stats, datara_rt_scheduler_wave_executions,
};
use forgen::schedule::{ScheduleEffectClass, SchedulePriority, TaskNode, WorkerPoolKind};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct TaskContext {
    id: u32,
    shared: Arc<SharedLog>,
}

struct SharedLog {
    starts: Mutex<HashMap<u32, Instant>>,
    ends: Mutex<HashMap<u32, Instant>>,
    order: Mutex<Vec<u32>>,
}

extern "C" fn recording_task(ctx: *mut c_void) {
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

static TEST_SCHED_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn audit_scheduler_dag_execution_order_and_counters() {
    let _lock = TEST_SCHED_LOCK.lock().unwrap();
    unsafe {
        datara_rt_scheduler_reset_stats();
    }

    let shared = Arc::new(SharedLog {
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
            func: Some(recording_task),
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
            func: Some(recording_task),
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
            func: Some(recording_task),
            ctx: &mut ctx_c as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
    ];

    let completed = unsafe { datara_rt_schedule_run(3, nodes.as_mut_ptr(), 0) };
    assert_eq!(completed, 3, "All 3 tasks in DAG must complete");

    for (idx, n) in nodes.iter().enumerate() {
        assert_eq!(n.status, 2, "Task {} must have status 2 (completed)", idx);
    }

    let starts = shared.starts.lock().unwrap();
    let ends = shared.ends.lock().unwrap();
    let order = shared.order.lock().unwrap();

    let start_c = starts[&2];
    let end_a = ends[&0];
    let end_b = ends[&1];

    assert!(
        start_c >= end_a,
        "Dependency violation: Task C started ({:?}) before Task A ended ({:?})",
        start_c,
        end_a
    );
    assert!(
        start_c >= end_b,
        "Dependency violation: Task C started ({:?}) before Task B ended ({:?})",
        start_c,
        end_b
    );

    let pos_a = order.iter().position(|&x| x == 0).unwrap();
    let pos_b = order.iter().position(|&x| x == 1).unwrap();
    let pos_c = order.iter().position(|&x| x == 2).unwrap();
    assert!(
        pos_c > pos_a && pos_c > pos_b,
        "Order violation: C must complete after A and B, got order: {:?}",
        *order
    );

    let pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert_eq!(
        pushes, 0,
        "Deterministic scheduler hot path must NEVER push to mutex ready-queue"
    );

    let waves = unsafe { datara_rt_scheduler_wave_executions() };
    assert!(
        waves >= 2,
        "Expected at least 2 wavefront executions, got: {}",
        waves
    );
}

#[test]
fn audit_scheduler_cycle_dependency_residual_and_no_hang() {
    let cyclic_tasks = vec![
        TaskNode {
            id: 0,
            name: "task_a".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![1],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
        TaskNode {
            id: 1,
            name: "task_b".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![0],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
    ];

    let waves = forgen::schedule::ScheduleProof::compute_kahn_waves(&cyclic_tasks);
    assert!(
        !waves.is_empty(),
        "Kahn waves on cyclic graph must not be empty"
    );
    let last_wave = waves.last().unwrap();
    assert_eq!(
        last_wave,
        &vec![0, 1],
        "Cyclic residual must contain all unresolvable tasks [0, 1] without hang"
    );

    let mut dep_0 = vec![1u32];
    let mut dep_1 = vec![0u32];

    let mut nodes = vec![
        DataraRtTaskNode {
            id: 0,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 1,
            deps: dep_0.as_mut_ptr(),
            func: None,
            ctx: std::ptr::null_mut(),
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 1,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 1,
            deps: dep_1.as_mut_ptr(),
            func: None,
            ctx: std::ptr::null_mut(),
            result: std::ptr::null_mut(),
            status: 0,
        },
    ];

    let completed = unsafe { datara_rt_schedule_run(2, nodes.as_mut_ptr(), 0) };
    assert!(
        completed < 2,
        "Cyclic graph must not report full completion, returned: {}",
        completed
    );
    assert_eq!(
        completed, 0,
        "Cyclic nodes with in_degree > 0 must not execute"
    );
}

#[test]
fn audit_scheduler_mixed_deterministic_and_dynamic_graph() {
    let _lock = TEST_SCHED_LOCK.lock().unwrap();
    unsafe {
        datara_rt_scheduler_reset_stats();
    }

    let shared = Arc::new(SharedLog {
        starts: Mutex::new(HashMap::new()),
        ends: Mutex::new(HashMap::new()),
        order: Mutex::new(Vec::new()),
    });

    let mut ctx_0 = TaskContext {
        id: 0,
        shared: shared.clone(),
    };
    let mut ctx_1 = TaskContext {
        id: 1,
        shared: shared.clone(),
    };
    let mut ctx_2 = TaskContext {
        id: 2,
        shared: shared.clone(),
    };

    let mut deps_1 = vec![0u32];
    let mut deps_2 = vec![1u32];

    let mut nodes = vec![
        DataraRtTaskNode {
            id: 0,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 0,
            deps: std::ptr::null_mut(),
            func: Some(recording_task),
            ctx: &mut ctx_0 as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 1,
            effect_class: 1,
            priority: 0,
            pool: 1,
            deterministic: 0,
            dep_count: 1,
            deps: deps_1.as_mut_ptr(),
            func: Some(recording_task),
            ctx: &mut ctx_1 as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
        DataraRtTaskNode {
            id: 2,
            effect_class: 0,
            priority: 0,
            pool: 0,
            deterministic: 1,
            dep_count: 1,
            deps: deps_2.as_mut_ptr(),
            func: Some(recording_task),
            ctx: &mut ctx_2 as *mut _ as *mut c_void,
            result: std::ptr::null_mut(),
            status: 0,
        },
    ];

    let completed = unsafe { datara_rt_schedule_run(3, nodes.as_mut_ptr(), 0) };
    assert_eq!(
        completed, 3,
        "Mixed graph must complete all 3 tasks without hang"
    );

    for (i, n) in nodes.iter().enumerate() {
        assert_eq!(
            n.status, 2,
            "Node {} in mixed graph must reach completed status 2",
            i
        );
    }

    let pushes = unsafe { datara_rt_scheduler_mutex_queue_pushes() };
    assert!(
        pushes > 0,
        "Dynamic scheduler path must utilize ready-queue (pushes > 0), got: {}",
        pushes
    );

    let order = shared.order.lock().unwrap();
    assert_eq!(
        *order,
        vec![0, 1, 2],
        "Chain execution order must be [0, 1, 2]"
    );
}
