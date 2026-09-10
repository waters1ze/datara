use forgen::driver::ForgenCompiler;
use forgen::schedule::{ScheduleEffectClass, SchedulePriority, ScheduleProof, WorkerPoolKind};

#[test]
fn test_schedule_proof_generation_on_compilation_result() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn pure_add(a: Int, b: Int) -> Int => a + b

fn main() -> Int {
    let x = pure_add(10, 20)
    return x
}
"#;
    let res = compiler.compile_source(src, "test_proof_gen.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res
        .schedule_proof
        .expect("ScheduleProof must be present in CompilationResult");

    assert!(
        !proof.tasks.is_empty(),
        "ScheduleProof should contain tasks"
    );
    assert!(
        !proof.waves.is_empty(),
        "ScheduleProof should contain waves"
    );

    let pure_task = proof
        .find_task("pure_add")
        .expect("pure_add task must exist in proof");
    assert_eq!(pure_task.effect_class, ScheduleEffectClass::Pure);
    assert_eq!(pure_task.pool, WorkerPoolKind::CPUPool);
    assert!(pure_task.deterministic);
}

#[test]
fn test_schedule_proof_effect_classes_and_pools() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn pure_worker(a: Int) -> Int => a * 2

fn io_worker(msg: Str) {
    out msg
}

fn net_worker() {
    http_get()
}

fn par_worker() {
    parallel for i in 0..10 {
        let _ = i + 1
    }
}

fn main() {
    let a = pure_worker(5)
    io_worker("hello")
    net_worker()
    par_worker()
}
"#;
    let res = compiler.compile_source(src, "test_proof_effects.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res
        .schedule_proof
        .expect("ScheduleProof must be present in CompilationResult");

    // 1. Pure
    let pure_task = proof.find_task("pure_worker").expect("pure_worker task");
    assert_eq!(pure_task.effect_class, ScheduleEffectClass::Pure);
    assert_eq!(pure_task.pool, WorkerPoolKind::CPUPool);
    assert!(pure_task.deterministic);

    // 2. IO
    let io_task = proof.find_task("io_worker").expect("io_worker task");
    assert_eq!(io_task.effect_class, ScheduleEffectClass::IO);
    assert_eq!(io_task.pool, WorkerPoolKind::IOPool);
    assert!(!io_task.deterministic);

    // 3. Network
    let net_task = proof.find_task("net_worker").expect("net_worker task");
    assert_eq!(net_task.effect_class, ScheduleEffectClass::Network);
    assert_eq!(net_task.pool, WorkerPoolKind::IOPool);
    assert!(!net_task.deterministic);

    // 4. Parallel
    let par_task = proof.find_task("par_worker").expect("par_worker task");
    assert_eq!(par_task.effect_class, ScheduleEffectClass::Parallel);
    assert_eq!(par_task.pool, WorkerPoolKind::CPUPool);
}

#[test]
fn test_schedule_proof_priorities_hot_and_cold() {
    let compiler = ForgenCompiler::new("release");
    let src = r#"
fn cold_task(x: Int) -> Int => x + 1

fn hot_loop_task(n: Int) -> Int {
    mut acc = 0
    mut i = 0
    while i < n {
        acc = acc + i
        i = i + 1
    }
    return acc
}

fn main() -> Int {
    let c = cold_task(10)
    let h = hot_loop_task(100)
    return c + h
}
"#;
    let res = compiler.compile_source(src, "test_proof_prio.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");

    let cold = proof.find_task("cold_task").expect("cold_task");
    assert_eq!(cold.priority, SchedulePriority::Cold);

    let hot = proof.find_task("hot_loop_task").expect("hot_loop_task");
    assert_eq!(hot.priority, SchedulePriority::Hot);
}

#[test]
fn test_schedule_proof_kahn_wavefront_acyclicity_and_topological_order() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn leaf_a() -> Int => 1
fn leaf_b() -> Int => 2

fn mid_layer(x: Int) -> Int {
    return x + leaf_a() + leaf_b()
}

fn root_main() -> Int {
    return mid_layer(10)
}

fn main() -> Int {
    return root_main()
}
"#;
    let res = compiler.compile_source(src, "test_proof_waves.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");

    // Check that Kahn topological wavefronts satisfy DAG dependency constraints:
    // Any task in wave W can only depend on tasks in waves < W.
    let mut task_to_wave = std::collections::BTreeMap::new();
    for (wave_idx, wave) in proof.waves.iter().enumerate() {
        for &task_id in wave {
            task_to_wave.insert(task_id, wave_idx);
        }
    }

    for task in &proof.tasks {
        if let Some(&task_wave) = task_to_wave.get(&task.id) {
            for &dep_id in &task.deps {
                if let Some(&dep_wave) = task_to_wave.get(&dep_id) {
                    assert!(
                        dep_wave < task_wave,
                        "Task {} (wave {}) depends on task {} (wave {}), violating topological ordering",
                        task.id,
                        task_wave,
                        dep_id,
                        dep_wave
                    );
                }
            }
        }
    }

    // Check leaf nodes have 0 dependencies and are placed in early waves (wave 0)
    let leaf_a = proof.find_task("leaf_a").expect("leaf_a");
    assert_eq!(leaf_a.deps.len(), 0);
    assert_eq!(task_to_wave.get(&leaf_a.id), Some(&0));

    let leaf_b = proof.find_task("leaf_b").expect("leaf_b");
    assert_eq!(leaf_b.deps.len(), 0);
    assert_eq!(task_to_wave.get(&leaf_b.id), Some(&0));
}

#[test]
fn test_schedule_proof_json_serialization_roundtrip() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn calc(a: Int) -> Int => a * 10

fn main() -> Int {
    return calc(42)
}
"#;
    let res = compiler.compile_source(src, "test_proof_json.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");
    let json = proof.to_json().expect("Serialization to JSON failed");
    println!("SCHEDULE_PROOF_JSON:\n{}", json);

    assert!(json.contains("\"tasks\":"), "JSON should contain tasks key");
    assert!(json.contains("\"waves\":"), "JSON should contain waves key");
    assert!(
        json.contains("\"is_deterministic\":"),
        "JSON should contain is_deterministic key"
    );
    assert!(json.contains("\"name\": \"calc\""));

    // Deserialization round-trip check
    let roundtrip: ScheduleProof =
        serde_json::from_str(&json).expect("Deserialization from JSON failed");
    assert_eq!(proof.as_ref(), &roundtrip);
}

#[test]
fn test_schedule_proof_runtime_execution_determinism_and_waves() {
    use forgen::runtime::{
        DataraRtTaskNode, datara_rt_schedule_run, datara_rt_scheduler_mutex_queue_pushes,
        datara_rt_scheduler_reset_stats, datara_rt_scheduler_wave_executions,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TASK_A_RUN: AtomicUsize = AtomicUsize::new(0);
    static TASK_B_RUN: AtomicUsize = AtomicUsize::new(0);
    static TASK_C_RUN: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn run_a(_ctx: *mut std::ffi::c_void) {
        TASK_A_RUN.fetch_add(1, Ordering::SeqCst);
    }
    extern "C" fn run_b(_ctx: *mut std::ffi::c_void) {
        TASK_B_RUN.fetch_add(1, Ordering::SeqCst);
    }
    extern "C" fn run_c(_ctx: *mut std::ffi::c_void) {
        TASK_C_RUN.fetch_add(1, Ordering::SeqCst);
    }

    TASK_A_RUN.store(0, Ordering::SeqCst);
    TASK_B_RUN.store(0, Ordering::SeqCst);
    TASK_C_RUN.store(0, Ordering::SeqCst);

    unsafe {
        datara_rt_scheduler_reset_stats();

        // Wave 0: task 0, task 1 (both have 0 dependencies)
        // Wave 1: task 2 (depends on task 0 and task 1)
        let mut deps_c = vec![0u32, 1u32];

        let mut nodes = vec![
            DataraRtTaskNode {
                id: 0,
                effect_class: 0, // Pure
                priority: 1,     // Hot
                pool: 0,         // CPUPool
                deterministic: 1,
                dep_count: 0,
                deps: std::ptr::null_mut(),
                func: Some(run_a),
                ctx: std::ptr::null_mut(),
                result: std::ptr::null_mut(),
                status: 0,
            },
            DataraRtTaskNode {
                id: 1,
                effect_class: 0, // Pure
                priority: 1,     // Hot
                pool: 0,         // CPUPool
                deterministic: 1,
                dep_count: 0,
                deps: std::ptr::null_mut(),
                func: Some(run_b),
                ctx: std::ptr::null_mut(),
                result: std::ptr::null_mut(),
                status: 0,
            },
            DataraRtTaskNode {
                id: 2,
                effect_class: 0, // Pure
                priority: 1,     // Hot
                pool: 0,         // CPUPool
                deterministic: 1,
                dep_count: 2,
                deps: deps_c.as_mut_ptr(),
                func: Some(run_c),
                ctx: std::ptr::null_mut(),
                result: std::ptr::null_mut(),
                status: 0,
            },
        ];

        let completed = datara_rt_schedule_run(nodes.len() as i64, nodes.as_mut_ptr(), 0);
        assert_eq!(completed, 3, "All 3 tasks should complete");

        assert_eq!(TASK_A_RUN.load(Ordering::SeqCst), 1);
        assert_eq!(TASK_B_RUN.load(Ordering::SeqCst), 1);
        assert_eq!(TASK_C_RUN.load(Ordering::SeqCst), 1);

        assert_eq!(nodes[0].status, 2);
        assert_eq!(nodes[1].status, 2);
        assert_eq!(nodes[2].status, 2);

        // Verification of zero mutex queue touches on deterministic path
        let mutex_pushes = datara_rt_scheduler_mutex_queue_pushes();
        assert_eq!(
            mutex_pushes, 0,
            "Deterministic path must never touch the mutex ready-queue"
        );

        // Verification of Kahn topological wavefront execution
        let waves = datara_rt_scheduler_wave_executions();
        assert!(
            waves >= 2,
            "Must execute in at least 2 waves (Wave 0 for A & B, Wave 1 for C), got: {}",
            waves
        );
    }
}

#[test]
fn test_schedule_proof_class_method_task_deduplication() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
class Vector2 {
    x: Int
    y: Int
}

behavior Vector2 {
    length_sq() -> Int => this.x * this.x + this.y * this.y
}

fn calculate(a: Int, b: Int) -> Int => a + b

fn main() -> Int {
    let v = Vector2 { x: 3, y: 4 }
    let l = v.length_sq()
    let c = calculate(l, 10)
    return c
}
"#;
    let res = compiler.compile_source(src, "test_proof_dedup.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");

    // Unique functions in program:
    // 1. Vector2_length_sq (method)
    // 2. calculate
    // 3. main
    // Previously, Vector2.length_sq AND Vector2_length_sq were both inserted, giving 4 tasks.
    // With deduplication and canonicalization, tasks.len() must be exactly 3.
    assert_eq!(
        proof.tasks.len(),
        3,
        "tasks.len() must equal the number of unique symbols (no duplicate AST vs DMIR method tasks). Found: {:?}",
        proof.tasks.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Looking up either by AST format "Vector2.length_sq" or canonical "Vector2_length_sq" succeeds
    assert!(
        proof.find_task("Vector2.length_sq").is_some(),
        "Must find task via AST name"
    );
    assert!(
        proof.find_task("Vector2_length_sq").is_some(),
        "Must find task via canonical name"
    );
    assert!(
        proof.find_task("calculate").is_some(),
        "Must find calculate"
    );
    assert!(proof.find_task("main").is_some(), "Must find main");
}

#[test]
fn test_schedule_proof_cyclic_tasks_wavefront() {
    let compiler = ForgenCompiler::new("debug");
    let src = r#"
fn ping(n: Int) -> Int {
    if n <= 0 {
        return 0
    }
    return pong(n - 1)
}

fn pong(n: Int) -> Int {
    if n <= 0 {
        return 0
    }
    return ping(n - 1)
}

fn main() -> Int {
    return ping(10)
}
"#;
    let res = compiler.compile_source(src, "test_proof_cycle.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let proof = res.schedule_proof.expect("ScheduleProof must be present");

    // All three functions must be uniquely represented: ping, pong, main
    assert_eq!(proof.tasks.len(), 3);
    assert!(proof.find_task("ping").is_some());
    assert!(proof.find_task("pong").is_some());
    assert!(proof.find_task("main").is_some());

    // Cycle detection check:
    // Waves must exist and contain all tasks without infinite looping.
    let total_scheduled: usize = proof.waves.iter().map(|w| w.len()).sum();
    assert_eq!(
        total_scheduled,
        proof.tasks.len(),
        "All tasks including cyclic ones must be partitioned into waves (with residual wave)"
    );
}
