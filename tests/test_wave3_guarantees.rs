use forgen::driver::ForgenCompiler;
use forgen::schedule::{
    ScheduleEffectClass, SchedulePriority, ScheduleProof, TaskNode, WorkerPoolKind,
};
use std::time::{Duration, Instant};

#[test]
fn test_wave3_ownership_borrow_conflict_rejection() {
    let compiler = ForgenCompiler::new("release");

    // 1. Multiple mutable views
    let source_mut_borrow = r#"
class Data {
    v: Int,
}

fn main() {
    mut d = Data { v: 10 };
    let r1 = mut_view(d);
    let r2 = mut_view(d);
    out d.v;
}
"#;
    let res1 = compiler.compile_source(source_mut_borrow, "multi_mut_borrow.dtr", None);
    assert!(
        !res1.success,
        "Multiple mutable borrows must fail compilation"
    );
    let diag1 = res1.diagnostics.to_lowercase();
    assert!(
        diag1.contains("borrow") || diag1.contains("e-borrow-004") || diag1.contains("conflict"),
        "Expected borrow conflict error, got: {}",
        res1.diagnostics
    );

    // 2. Use after move
    let source_move = r#"
class Item {
    val: Int,
}

fn consume(i: Item) {
}

fn main() {
    let item = Item { val: 42 };
    consume(item);
    out item.val;
}
"#;
    let res2 = compiler.compile_source(source_move, "use_after_move.dtr", None);
    assert!(!res2.success, "Use after move must fail compilation");
    let diag2 = res2.diagnostics.to_lowercase();
    assert!(
        diag2.contains("moved") || diag2.contains("e-borrow-001"),
        "Expected use after move error, got: {}",
        res2.diagnostics
    );
}

#[test]
fn test_wave3_contracts_requires_ensures() {
    // 1. Proven contract elimination / execution
    let source_proven = r#"
type NonZero = Int in 1..100

fn safe_div(a: Int, b: NonZero) -> Int
    require b != 0
{
    return a / b
}

fn main() {
    let d: NonZero = 2
    out safe_div(42, d)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source_proven, "contract_proven.dtr", None);
    assert!(
        res.success,
        "Proven contract compilation failed: {:?}",
        res.error
    );
    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "21");

    // 2. Unproven contract violation traps at runtime
    let source_violation = r#"
fn get_zero() -> Int {
    return 0
}

fn divide_contract(a: Int, b: Int) -> Int
    require b != 0, "CONTRACT_VIOLATION: zero divisor"
{
    return a / b
}

fn main() {
    let z = get_zero()
    out divide_contract(100, z)
}
"#;
    let comp_rel = ForgenCompiler::new("release");
    let res2 = comp_rel.compile_source(source_violation, "contract_violation.dtr", None);
    assert!(
        res2.success,
        "Compilation succeeds (checked at runtime): {:?}",
        res2.error
    );
    let exe2 = res2.exe_path.unwrap();
    let (_, stderr, code2, _) = comp_rel.cranelift.run_executable(&exe2, &[]).unwrap();
    assert_ne!(
        code2, 0,
        "Contract violation must trap with non-zero exit code"
    );
    assert!(
        stderr.contains("CONTRACT_VIOLATION") || stderr.contains("zero divisor"),
        "Error output must contain violation reason: {}",
        stderr
    );
}

#[test]
fn test_wave3_zero_alloc_real_time_mode() {
    let compiler = ForgenCompiler::new("check");

    // 1. @no_alloc violation
    let source_alloc = r#"
class Task {
    id: Int
}

behavior Task {
    @no_alloc
    fn run_bad() -> Int {
        let items = [1, 2, 3]
        return 0
    }
}

fn main() -> Int {
    return 0
}
"#;
    let res1 = compiler.check_source(source_alloc, "bad_alloc.dtr");
    assert!(
        !res1.success,
        "@no_alloc with heap allocation must fail check"
    );
    assert!(
        res1.diagnostics.contains("E0950") || res1.diagnostics.contains("Allocation Violation"),
        "Expected E0950 / Allocation Violation, got:\n{}",
        res1.diagnostics
    );

    // 2. @no_panic violation
    let source_panic = r#"
class Controller {
    mode: Int
}

behavior Controller {
    @no_panic
    fn trigger_fail() -> Void {
        panic("unexpected failure")
    }
}

fn main() -> Int {
    return 0
}
"#;
    let res2 = compiler.check_source(source_panic, "bad_panic.dtr");
    assert!(!res2.success, "@no_panic with panic call must fail check");
    assert!(
        res2.diagnostics.contains("E0951") || res2.diagnostics.contains("Panic Violation"),
        "Expected E0951 / Panic Violation, got:\n{}",
        res2.diagnostics
    );
}

#[test]
fn test_wave3_scheduler_proof_dag_and_cycles() {
    // 1. DAG Kahn waves
    let tasks = vec![
        TaskNode {
            id: 0,
            name: "task_a".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![],
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
        TaskNode {
            id: 2,
            name: "task_c".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![1],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
    ];

    let waves = ScheduleProof::compute_kahn_waves(&tasks);
    assert_eq!(
        waves.len(),
        3,
        "Linear chain A->B->C must yield 3 wavefronts"
    );
    assert_eq!(waves[0], vec![0]);
    assert_eq!(waves[1], vec![1]);
    assert_eq!(waves[2], vec![2]);

    // 2. Cyclic dependencies do not hang
    let cyclic_tasks = vec![
        TaskNode {
            id: 0,
            name: "node_1".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![1],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
        TaskNode {
            id: 1,
            name: "node_2".to_string(),
            effect_class: ScheduleEffectClass::Pure,
            priority: SchedulePriority::Cold,
            deps: vec![0],
            pool: WorkerPoolKind::CPUPool,
            deterministic: true,
        },
    ];

    let start = Instant::now();
    let cyclic_waves = ScheduleProof::compute_kahn_waves(&cyclic_tasks);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "Cyclic schedule computation must not hang"
    );
    let residual = cyclic_waves.last().expect("must contain residual wave");
    assert_eq!(residual, &vec![0, 1]);
}
