use forgen::driver::ForgenCompiler;
use forgen::schedule::ScheduleEffectClass;

#[test]
fn test_async_fn_declaration_parsing_and_typecheck() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn compute(x: Int) -> Int {
    return x * 2
}

async function fetch_msg() -> Str {
    return "done"
}

async task background_work() -> Int {
    return 42
}

fn main() -> Int {
    let a = compute(21)
    let m = fetch_msg()
    let b = background_work()
    if a == 42 && b == 42 && m == "done" {
        return 0
    }
    return 1
}
"#;
    let res = compiler.check_source(src, "test_async_decls.dtr");
    assert!(
        res.success,
        "Async function parsing/checking failed: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_async_method_in_class_and_behavior() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
class AsyncWorker {
    tag: Int
}

behavior AsyncWorker {
    async fn run() -> Int {
        return this.tag + 100
    }
}

fn main() -> Int {
    let worker = AsyncWorker { tag: 23 }
    let res = worker.run()
    if res == 123 {
        return 0
    }
    return 1
}
"#;
    let res = compiler.check_source(src, "test_async_method.dtr");
    assert!(
        res.success,
        "Async method in behavior failed: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_await_future_and_async_call() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
use stdlib.async.future

async fn get_number() -> Int {
    return 42
}

fn main() -> Int {
    let num = await get_number()
    let f = Future.ready("datara_future")
    let s = await f
    if num == 42 && s == "datara_future" {
        return 0
    }
    return 1
}
"#;
    let res = compiler.check_source(src, "test_await_future.dtr");
    assert!(
        res.success,
        "Await Future and async call failed: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_async_await_schedule_proof_wavefronts() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn worker_alpha() -> Int {
    return 10
}

async fn worker_beta() -> Int {
    return 20
}

fn main() -> Int {
    let a = await worker_alpha()
    let b = await worker_beta()
    return a + b
}
"#;
    let res = compiler.compile_source(src, "test_proof_async.dtr", None);
    assert!(
        res.success,
        "Schedule proof compilation failed: {:?}",
        res.error
    );

    let proof = res
        .schedule_proof
        .expect("ScheduleProof must be present in compilation result");

    // Must find all three tasks
    let alpha_task = proof
        .find_task("worker_alpha")
        .expect("worker_alpha must exist in schedule proof");
    let beta_task = proof
        .find_task("worker_beta")
        .expect("worker_beta must exist in schedule proof");
    let main_task = proof
        .find_task("main")
        .expect("main must exist in schedule proof");

    assert_eq!(alpha_task.effect_class, ScheduleEffectClass::Pure);
    assert_eq!(beta_task.effect_class, ScheduleEffectClass::Pure);
    assert!(alpha_task.deterministic);
    assert!(beta_task.deterministic);

    // main must depend on worker_alpha and worker_beta
    assert!(
        main_task.deps.contains(&alpha_task.id),
        "main must depend on worker_alpha"
    );
    assert!(
        main_task.deps.contains(&beta_task.id),
        "main must depend on worker_beta"
    );

    // Topological wavefront check: alpha and beta can run concurrently in earlier wave,
    // and main must be in a strictly subsequent wave
    let mut alpha_wave = None;
    let mut beta_wave = None;
    let mut main_wave = None;

    for (w_idx, wave) in proof.waves.iter().enumerate() {
        if wave.contains(&alpha_task.id) {
            alpha_wave = Some(w_idx);
        }
        if wave.contains(&beta_task.id) {
            beta_wave = Some(w_idx);
        }
        if wave.contains(&main_task.id) {
            main_wave = Some(w_idx);
        }
    }

    assert!(
        alpha_wave.is_some() && beta_wave.is_some() && main_wave.is_some(),
        "All tasks must be assigned to topological waves"
    );
    assert_eq!(
        alpha_wave, beta_wave,
        "worker_alpha and worker_beta have no mutual dependencies and must share wavefront level"
    );
    assert!(
        main_wave.unwrap() > alpha_wave.unwrap(),
        "main must execute in a wave strictly after its prerequisite tasks"
    );
}

#[test]
fn test_async_await_end_to_end_execution() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
async fn calculate_part_a() -> Int {
    return 150
}

async fn calculate_part_b() -> Int {
    return 270
}

fn main() -> Int {
    let a = await calculate_part_a()
    let b = await calculate_part_b()
    if (a + b) == 420 {
        return 0
    }
    return 1
}
"#;
    let out_dir = std::env::temp_dir().join("datara_test_async_exec_bin");
    let _ = std::fs::create_dir_all(&out_dir);
    let exe_path = out_dir.join("test_async_await.exe");

    let res = compiler.compile_source(src, "test_async_await_exec.dtr", Some(&exe_path));
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    if let Some(ref path) = res.exe_path {
        let output = std::process::Command::new(path)
            .output()
            .expect("Failed to execute compiled binary");
        assert_eq!(
            output.status.code(),
            Some(0),
            "Expected exit code 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
