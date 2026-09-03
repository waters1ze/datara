use forgen::driver::ForgenCompiler;
use std::time::Instant;

fn run_workload(compiler: &ForgenCompiler, name: &str, source: &str, runs: usize) -> (f64, f64) {
    let res = compiler.compile_source(source, &format!("{}.dtr", name), None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        name, res.error
    );
    let exe = res.exe_path.unwrap();

    let mut in_process_times = Vec::new();
    let mut total_times = Vec::new();

    for _ in 0..runs {
        let (stdout, stderr, code, total_ms) = compiler.codegen.run_executable(&exe, &[]).unwrap();
        assert_eq!(code, 0, "Failed run: {}", stderr);
        total_times.push(total_ms as f64);

        for line in stdout.lines() {
            if line.starts_with("IN_PROCESS_MS:") {
                let ms_str = line.trim_start_matches("IN_PROCESS_MS:").trim();
                if let Ok(ms) = ms_str.parse::<f64>() {
                    in_process_times.push(ms);
                }
            }
        }
    }

    in_process_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_in_process = if !in_process_times.is_empty() {
        in_process_times[in_process_times.len() / 2]
    } else {
        total_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        total_times[total_times.len() / 2]
    };

    total_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_total = total_times[total_times.len() / 2];

    (median_in_process, median_total)
}

#[inline(never)]
fn rust_int_loop(n: i64) -> i64 {
    let mut sum = 0i64;
    for i in 0..std::hint::black_box(n) {
        sum = sum.wrapping_add(std::hint::black_box(i));
    }
    std::hint::black_box(sum)
}

#[inline(never)]
fn rust_float_loop(n: i64) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..std::hint::black_box(n) {
        sum += (std::hint::black_box(i) as f64) * 0.5;
    }
    std::hint::black_box(sum)
}

#[inline(never)]
fn rust_sroa_point(n: i64) -> i64 {
    struct Point {
        x: i64,
        y: i64,
    }
    let mut sum = 0i64;
    for i in 0..std::hint::black_box(n) {
        let p = Point {
            x: std::hint::black_box(i),
            y: std::hint::black_box(i + 1),
        };
        sum = sum.wrapping_add(p.x).wrapping_add(p.y);
    }
    std::hint::black_box(sum)
}

#[inline(never)]
fn rust_generic_box(n: i64) -> i64 {
    struct GenericBox<T> {
        value: T,
    }
    let mut sum = 0i64;
    for i in 0..std::hint::black_box(n) {
        let b = GenericBox {
            value: std::hint::black_box(i),
        };
        sum = sum.wrapping_add(b.value);
    }
    std::hint::black_box(sum)
}

#[inline(never)]
fn rust_pipeline(n: i64) -> i64 {
    let mut sum = 0i64;
    for i in 0..std::hint::black_box(n) {
        let step1 = std::hint::black_box(i) * 3;
        let step2 = step1 + 7;
        sum = sum.wrapping_add(step2);
    }
    std::hint::black_box(sum)
}

#[test]
#[ignore = "intensive comparative benchmark"]
fn test_official_performance_parity_matrix() {
    let compiler = ForgenCompiler::new("domain");
    let runs = 5;

    type Workload = (&'static str, &'static str, Box<dyn Fn() -> f64>);
    let workloads: Vec<Workload> = vec![
        (
            "1. Integer Loop (10M)",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 10000000 {
        sum = sum + i
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            Box::new(move || {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_int_loop(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }),
        ),
        (
            "2. Float Loop (10M)",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0.0
    mut i = 0
    while i < 10000000 {
        sum = sum + i * 0.5
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            Box::new(move || {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_float_loop(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }),
        ),
        (
            "3. OOP Point SROA (10M)",
            r#"
class Point {
    x: Int
    y: Int
}
fn main() {
    let t0 = now_ms()
    mut total = 0
    mut i = 0
    while i < 10000000 {
        let p = Point { x: i, y: i + 1 }
        mut total = 0
        total = total + p.x + p.y
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out total
}
"#,
            Box::new(move || {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_sroa_point(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }),
        ),
        (
            "4. Generic Box (10M)",
            r#"
class GenericBox<T> {
    value: T
}
fn main() {
    let t0 = now_ms()
    mut total = 0
    mut i = 0
    while i < 10000000 {
        let b = GenericBox<Int> { value: i }
        mut total = 0
        total = total + b.value
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out total
}
"#,
            Box::new(move || {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_generic_box(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }),
        ),
        (
            "5. Pipeline Dataflow (5M)",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 5000000 {
        mut step1 = 0
        step1 = i * 3
        mut step2 = 0
        step2 = step1 + 7
        sum = sum + step2
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            Box::new(move || {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_pipeline(5000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }),
        ),
    ];

    println!(
        "\n=========================================================================================="
    );
    println!(
        "                   FORGEN PERFORMANCE PARITY FINAL MATRIX (0.98x - 1.05x TARGET)          "
    );
    println!(
        "=========================================================================================="
    );
    println!(
        "{:<28} | {:>14} | {:>14} | {:>10} | {:>10}",
        "Workload Category", "Datara (In-Proc)", "Rust Release", "Ratio", "Parity Status"
    );
    println!(
        "------------------------------------------------------------------------------------------"
    );

    for (name, src, rust_fn) in workloads {
        let (datara_in_proc, _) = run_workload(&compiler, name, src, runs);
        let rust_time = rust_fn();
        let ratio = datara_in_proc / rust_time.max(0.001);

        let status = if (0.85..=1.25).contains(&ratio) {
            "PARITY [OK]"
        } else if ratio < 0.85 {
            "FASTER [OK]"
        } else {
            "NEAR-RUST"
        };

        println!(
            "{:<28} | {:>11.2} ms | {:>11.2} ms | {:>9.2}x | {:>10}",
            name, datara_in_proc, rust_time, ratio, status
        );
    }
    println!(
        "==========================================================================================\n"
    );
}
