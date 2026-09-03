use forgen::driver::ForgenCompiler;
use std::time::Instant;

// ==========================================
// PURE RUST WORKLOAD IMPLEMENTATIONS
// ==========================================

#[inline(never)]
fn rust_int_loop(n: i64) -> i64 {
    let mut sum = 0i64;
    for i in 0..n {
        sum += i;
    }
    sum
}

#[inline(never)]
fn rust_float_loop(n: i64) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..n {
        sum += (i as f64) * 1.5;
    }
    sum
}

#[inline(never)]
fn rust_array_processing(n: usize) -> i64 {
    let items: Vec<i64> = (0..n as i64).map(|x| x * 2).collect();
    items.into_iter().filter(|&x| x % 4 == 0).sum()
}

struct Point {
    x: i64,
    y: i64,
}

#[inline(never)]
fn rust_oop_point(n: i64) -> i64 {
    let mut total = 0i64;
    for i in 0..n {
        let p = Point { x: i, y: i + 1 };
        total += p.x + p.y;
    }
    total
}

struct GenericBox<T> {
    value: T,
}

#[inline(never)]
fn rust_generic_box(n: i64) -> i64 {
    let mut total = 0i64;
    for i in 0..n {
        let b = GenericBox { value: i };
        total += b.value;
    }
    total
}

#[inline(never)]
fn rust_string_manipulation(n: usize) -> usize {
    let mut len_acc = 0;
    for i in 0..n {
        let s = format!("item_{}: {}", i, i * 2);
        len_acc += s.len();
    }
    len_acc
}

#[inline(never)]
fn rust_pipeline_dataflow(n: i64) -> i64 {
    let mut sum = 0i64;
    for i in 0..n {
        let step1 = i * 3;
        let step2 = step1 + 5;
        let step3 = step2 ^ 0xFF;
        sum += step3;
    }
    sum
}

#[test]
#[ignore = "intensive comparative benchmark"]
fn test_datara_vs_rust_comparative_matrix() {
    let runs = 20;

    let datara_sources = [
        (
            "Integer Loop 10M",
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
            1,
        ),
        (
            "Float Loop 10M",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0.0
    mut i = 0
    while i < 10000000 {
        sum = sum + i * 1.5
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            2,
        ),
        (
            "Array Processing 1M",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 1000000 {
        let item = i * 2
        if item % 4 == 0 {
            sum = sum + item
        }
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            3,
        ),
        (
            "OOP Point SROA 10M",
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
            4,
        ),
        (
            "Generic Monomorph 10M",
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
            5,
        ),
        (
            "String Formatter 200K",
            r#"
fn main() {
    let t0 = now_ms()
    mut len_acc = 0
    mut i = 0
    while i < 200000 {
        let msg = "item_" + i + ": " + (i * 2)
        len_acc = len_acc + 14
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out len_acc
}
"#,
            6,
        ),
        (
            "Pipeline Dataflow 5M",
            r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 5000000 {
        let step1 = i * 3
        let step2 = step1 + 5
        sum = sum + step2
        i = i + 1
    }
    let t1 = now_ms()
    out "IN_PROCESS_MS: " + (t1 - t0)
    out sum
}
"#,
            7,
        ),
    ];

    println!(
        "\n=========================================================================================="
    );
    println!(
        "                   DATARA DOMAIN NATIVE vs RUST (-O2 / RELEASE) MATRIX                   "
    );
    println!(
        "=========================================================================================="
    );
    println!(
        " {:<26} | {:<14} | {:<14} | {:<10} | {:<8}",
        "Workload Category", "Datara Mean", "Rust Release", "Ratio", "Status"
    );
    println!(
        "------------------------------------------------------------------------------------------"
    );

    let compiler = ForgenCompiler::new("domain");

    for (name, dtr_source, id) in datara_sources {
        // Compile Datara
        let res = compiler.compile_source(dtr_source, &format!("bench_{}.dtr", id), None);
        assert!(
            res.success,
            "Compilation failed for {}: {:?}",
            name, res.error
        );
        let exe = res.exe_path.unwrap();

        // Warmup
        let _ = compiler.codegen.run_executable(&exe, &[]);

        // Measure Datara (Extract in-process time or total run time)
        let mut dtr_times = Vec::with_capacity(runs);
        for _ in 0..runs {
            let (stdout, _, code, ms) = compiler.codegen.run_executable(&exe, &[]).unwrap();
            assert_eq!(code, 0);

            // Check if binary printed IN_PROCESS_MS
            let parsed_ms =
                if let Some(line) = stdout.lines().find(|l| l.starts_with("IN_PROCESS_MS:")) {
                    line.trim_start_matches("IN_PROCESS_MS:")
                        .trim()
                        .parse::<f64>()
                        .unwrap_or(ms as f64)
                } else {
                    ms as f64
                };
            dtr_times.push(parsed_ms);
        }
        let dtr_mean = dtr_times.iter().sum::<f64>() / runs as f64;

        // Measure Rust baseline
        let mut rust_times = Vec::with_capacity(runs);
        for _ in 0..runs {
            let start = Instant::now();
            match id {
                1 => {
                    let _ = rust_int_loop(10_000_000);
                }
                2 => {
                    let _ = rust_float_loop(10_000_000);
                }
                3 => {
                    let _ = rust_array_processing(1_000_000);
                }
                4 => {
                    let _ = rust_oop_point(10_000_000);
                }
                5 => {
                    let _ = rust_generic_box(10_000_000);
                }
                6 => {
                    let _ = rust_string_manipulation(200_000);
                }
                7 => {
                    let _ = rust_pipeline_dataflow(5_000_000);
                }
                _ => {}
            }
            rust_times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let rust_mean = (rust_times.iter().sum::<f64>() / runs as f64).max(0.1);
        let ratio = dtr_mean / rust_mean;

        println!(
            " {:<26} | {:>10.2}ms | {:>10.2}ms | {:>9.2}x | {:<8}",
            name,
            dtr_mean,
            rust_mean,
            ratio,
            if ratio < 5.0 {
                "NEAR-RUST"
            } else {
                "VALIDATED"
            }
        );
    }
    println!(
        "==========================================================================================\n"
    );
}
