use forgen::driver::ForgenCompiler;
use std::time::Instant;

#[test]
fn benchmark_workloads() {
    let int_loop_source = r#"
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

    res = compute_sum(1000000)
    out res
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(int_loop_source, "bench_loop.dtr", None);
    assert!(res.success, "Benchmark compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let start = Instant::now();
    let (stdout, _, code, duration_ms) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    let wall_time = start.elapsed().as_millis();

    println!("------------------------------------------------------------");
    println!("             FORGEN COMPILER BENCHMARK RESULTS              ");
    println!("------------------------------------------------------------");
    println!(" Benchmark: 1,000,000 iteration integer loop");
    println!(" Exit code: {}", code);
    println!(" Result:    {}", stdout.trim());
    println!(" Runtime:   {}ms (wall: {}ms)", duration_ms, wall_time);
    println!("------------------------------------------------------------");

    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "499999500000");
}

#[test]
fn benchmark_zero_cost_oop_point() {
    let point_source = r#"
class Point {
    x: Int
    y: Int
}

fn compute_points(n: Int) -> Int {
    mut total = 0
    mut i = 0
    while i < n {
        mut p = Point { x: i, y: 1 }
        total = total + p.x + p.y
        i = i + 1
    }
    return total
}

fn main() {
    mut res = 0
    res = compute_points(1000000)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(point_source, "bench_point.dtr", None);
    assert!(res.success, "Benchmark compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let start = Instant::now();
    let (stdout, _, code, duration_ms) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    let wall_time = start.elapsed().as_millis();

    println!("------------------------------------------------------------");
    println!("        FORGEN ZERO-COST OOP SROA BENCHMARK RESULTS         ");
    println!("------------------------------------------------------------");
    println!(" Benchmark: 1,000,000 Point object instantiations + sums");
    println!(" Exit code: {}", code);
    println!(" Result:    {}", stdout.trim());
    println!(" Runtime:   {}ms (wall: {}ms)", duration_ms, wall_time);
    println!(
        " SROA Allocations Eliminated: {}",
        res.optimization_report.unwrap().allocations_eliminated
    );
    println!("------------------------------------------------------------");

    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "500000500000");
}
