use forgen::driver::ForgenCompiler;
use forgen::runtime::parallel::ParallelRuntime;
use std::time::Instant;

fn heavy_compute(n: u64) -> u64 {
    let mut sum = 0u64;
    for i in 0..n {
        sum = sum.wrapping_add((i.wrapping_mul(31)).wrapping_add(i ^ 0x55555555));
    }
    sum
}

#[test]
fn test_parallel_runtime_real_multithread_speedup() {
    let runtime = ParallelRuntime::new(4);
    let n = 25_000_000u64;

    // 1. Sequential execution baseline
    let t_seq_start = Instant::now();
    let res1_seq = heavy_compute(n);
    let res2_seq = heavy_compute(n);
    let seq_duration = t_seq_start.elapsed();

    // 2. Parallel execution
    let t_par_start = Instant::now();
    let (res1_par, res2_par) =
        runtime.run_parallel(move || heavy_compute(n), move || heavy_compute(n));
    let par_duration = t_par_start.elapsed();

    assert_eq!(res1_seq, res1_par);
    assert_eq!(res2_seq, res2_par);

    println!(
        "[PARALLEL TEST] Sequential: {:?}, Parallel: {:?}, Speedup: {:.2}x",
        seq_duration,
        par_duration,
        seq_duration.as_secs_f64() / par_duration.as_secs_f64().max(0.0001)
    );

    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // On shared CI runner virtual machines with 2 vCPUs, noisy neighbor scheduling can skew wall-clock duration.
    // We only enforce strict speedup when running on dedicated local hardware (> 2 cores) outside CI.
    if cpu_count > 2 && std::env::var("CI").is_err() {
        assert!(
            par_duration < seq_duration,
            "Parallel duration ({:?}) should be faster than sequential ({:?})",
            par_duration,
            seq_duration
        );
    }
}

#[test]
fn test_parallel_batch_map_execution() {
    let runtime = ParallelRuntime::new(4);
    let items: Vec<u64> = (0..8).map(|i| (i + 1) * 2_000_000).collect();

    let t_start = Instant::now();
    let results = runtime.par_map(items.clone(), heavy_compute);
    let duration = t_start.elapsed();

    assert_eq!(results.len(), items.len());
    for (i, &n) in items.iter().enumerate() {
        assert_eq!(results[i], heavy_compute(n));
    }

    println!("[PARALLEL BATCH] 8 chunks computed in {:?}", duration);
}

#[test]
fn test_compiled_parallel_block_execution() {
    let source = r#"
fn compute_worker(seed: Int) -> Int {
    mut acc = seed
    mut i = 0
    while i < 100000 {
        acc = acc + i * 2
        i = i + 1
    }
    return acc
}

fn main() {
    mut r1 = 0
    mut r2 = 0
    parallel {
        r1 = compute_worker(10)
        r2 = compute_worker(20)
    }
    out fmt"Result 1: {r1}"
    out fmt"Result 2: {r2}"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_parallel_block.dtr", None);
    assert!(
        res.success,
        "Parallel block compilation failed: {:?}",
        res.error
    );

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Result 1: 9999900010"));
    assert!(stdout.contains("Result 2: 9999900020"));
}

#[test]
fn test_compiled_parallel_invoke_fork_join() {
    let source = r#"
fn worker_a() {
    mut i = 0
    while i < 5000000 {
        i = i + 1
    }
    out "WORKER_A_DONE"
}

fn worker_b() {
    mut i = 0
    while i < 5000000 {
        i = i + 1
    }
    out "WORKER_B_DONE"
}

fn main() {
    parallel {
        worker_a()
        worker_b()
    }
    out "ALL_DONE"
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_parallel_invoke_fj.dtr", None);
    assert!(
        res.success,
        "Parallel invoke compilation failed: {:?}",
        res.error
    );

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("WORKER_A_DONE"));
    assert!(stdout.contains("WORKER_B_DONE"));
    assert!(stdout.contains("ALL_DONE"));
}
