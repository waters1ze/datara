use forgen::driver::ForgenCompiler;
use std::path::Path;
use std::time::Instant;

#[test]
fn test_comparative_benchmark_suite() {
    let benchmarks = [
        (
            "Integer Loop 1,000,000",
            "benchmarks/loops/int_loop_1m.dtr",
            "499999500000",
        ),
        (
            "Zero-Cost Point SROA 1,000,000",
            "benchmarks/classes/point_sroa_1m.dtr",
            "500000500000",
        ),
        (
            "Generic Box Specialization 1,000,000",
            "benchmarks/generics/box_specialization.dtr",
            "499999500000",
        ),
    ];

    println!("================================================================================");
    println!("             FORGEN COMPARATIVE PERFORMANCE BENCHMARK SUITE                     ");
    println!("================================================================================");
    println!(
        " {:<35} | {:<12} | {:<12} | {:<10}",
        "Benchmark Workload", "Compile (ms)", "Runtime (ms)", "Status"
    );
    println!("--------------------------------------------------------------------------------");

    for (name, path_str, expected_out) in benchmarks {
        let p = Path::new(path_str);
        if !p.exists() {
            continue;
        }

        let compiler = ForgenCompiler::new("domain");
        let c_start = Instant::now();
        let res = compiler.compile_file(p, None);
        let compile_ms = c_start.elapsed().as_millis();

        assert!(
            res.success,
            "Benchmark compilation failed for {}: {:?}",
            name, res.error
        );

        let exe = res.exe_path.unwrap();
        let (stdout, _, code, run_ms) = compiler.codegen.run_executable(&exe, &[]).unwrap();

        assert_eq!(code, 0, "Execution failed for {}", name);
        assert_eq!(
            stdout.trim(),
            expected_out,
            "Incorrect benchmark output for {}",
            name
        );

        println!(
            " {:<35} | {:>10}ms | {:>10}ms | PASSED",
            name, compile_ms, run_ms
        );
    }

    println!("================================================================================");
}
