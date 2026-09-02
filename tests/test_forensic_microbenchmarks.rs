use forgen::driver::ForgenCompiler;
use std::time::Instant;

#[test]
fn test_forensic_timing_breakdown_and_microbenchmarks() {
    let compiler = ForgenCompiler::new("release");

    let int_loop_src = r#"
fn compute_sum(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}
fn main() {
    mut res = 0

    res = compute_sum(10000000)
    out res
}
"#;

    println!(
        "\n=========================================================================================="
    );
    println!(
        "     FORENSIC TIMING BREAKDOWN: STAGES OF COMPILATION & EXECUTION                         "
    );
    println!(
        "=========================================================================================="
    );

    // 1. Compile to Native
    let t_compile_start = Instant::now();
    let res = compiler.compile_source_native(int_loop_src, "forensic_breakdown.dtr", None);
    let t_compile_total = t_compile_start.elapsed();
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    println!(" [COMPILATION BREAKDOWN]");
    println!(
        "   - Lexing & Parsing:       {:>8.3} ms",
        res.timings.parse_ms as f64
    );
    println!(
        "   - Type / Scope Resolve:   {:>8.3} ms",
        res.timings.resolve_ms as f64
    );
    println!(
        "   - Typechecking:           {:>8.3} ms",
        res.timings.typecheck_ms as f64
    );
    println!(
        "   - Ownership & Views:      {:>8.3} ms",
        res.timings.ownership_ms as f64
    );
    println!(
        "   - IR Optimizer Passes:    {:>8.3} ms",
        res.timings.optimizer_ms as f64
    );
    println!(
        "   - Cranelift Emit & Link:  {:>8.3} ms",
        res.timings.codegen_ms as f64
    );
    println!(
        "   - Total End-to-End Build: {:>8.3} ms",
        t_compile_total.as_secs_f64() * 1000.0
    );

    // 2. Process Startup vs Warm Repeated Execution
    println!("\n [EXECUTION TIMING BREAKDOWN]");
    let runs = 20;
    let mut total_process_times = Vec::new();
    for _ in 0..runs {
        let t_run = Instant::now();
        let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "49999995000000");
        total_process_times.push(t_run.elapsed().as_secs_f64() * 1000.0);
    }

    let min_time = total_process_times
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let avg_time = total_process_times.iter().sum::<f64>() / runs as f64;
    let max_time = total_process_times
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    println!(
        "   - Cold Run (First Run):   {:>8.3} ms",
        total_process_times[0]
    );
    println!("   - Best Warm Run (Min):    {:>8.3} ms", min_time);
    println!("   - Average Process Run:    {:>8.3} ms", avg_time);
    println!("   - Max Run:                {:>8.3} ms", max_time);

    // 3. Multi-Iteration In-Process Native Benchmark (100M iterations)
    let multi_iter_src = r#"
fn compute_sum_100m(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}
fn main() {
    mut res = 0

    res = compute_sum_100m(100000000)
    out res
}
"#;
    let res_100m = compiler.compile_source_native(multi_iter_src, "forensic_100m.dtr", None);
    assert!(res_100m.success);
    let exe_100m = res_100m.exe_path.unwrap();
    let t_100m_start = Instant::now();
    let (stdout_100m, _, _, _) = compiler.codegen.run_executable(&exe_100m, &[]).unwrap();
    let t_100m_total = t_100m_start.elapsed().as_secs_f64() * 1000.0;
    println!("\n [SCALING TEST: 100 MILLION ITERATIONS]");
    println!(
        "   - 100M Iterations Native Total Time: {:>8.3} ms (Output: {})",
        t_100m_total,
        stdout_100m.trim()
    );
    println!(
        "   - Derived Pure 10M Execution Time:   {:>8.3} ms",
        (t_100m_total - avg_time) / 9.0
    );
    println!(
        "==========================================================================================\n"
    );
}
