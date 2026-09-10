//! Stage 7: Release Performance Benchmarks
//!
//! Benchmarks required by forensic audit:
//! 1. Recursive fib(35)
//! 2. Loop sum 1e8 (both LoopFold O(1) and raw non-foldable loop)
//! 3. SIMD dot product 4M float4

use forgen::driver::ForgenCompiler;
use std::time::Instant;

fn compile_and_run_release(source: &str, name: &str) -> (String, std::time::Duration) {
    let temp_dir = std::env::temp_dir().join(format!("datara_bench_{}", name));
    let _ = std::fs::create_dir_all(&temp_dir);
    let exe_path = temp_dir.join(format!("{}.exe", name));

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, Some(&exe_path));
    assert!(
        res.success,
        "Release compilation failed for {}: {:?}",
        name, res.error
    );

    let start = Instant::now();
    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("must execute compiled release binary");
    let elapsed = start.elapsed();

    assert_eq!(code, 0, "Binary {} failed: {}", name, stderr);

    let _ = std::fs::remove_file(&exe_path);
    let _ = std::fs::remove_file(exe_path.with_extension("obj"));
    let _ = std::fs::remove_file(exe_path.with_extension("pdb"));
    let _ = std::fs::remove_dir_all(&temp_dir);

    (stdout.trim().replace("\r\n", "\n"), elapsed)
}

#[inline(never)]
fn rust_fib(n: i64) -> i64 {
    if n <= 1 {
        n
    } else {
        rust_fib(n - 1) + rust_fib(n - 2)
    }
}

#[inline(never)]
fn rust_loop_raw(n: i64) -> i64 {
    let mut sum = 0i64;
    let mut i = 0i64;
    while i < n {
        if i < 50_000_000 {
            sum += 1;
        } else {
            sum += 2;
        }
        i += 1;
    }
    sum
}

#[inline(never)]
fn rust_dot_4m() -> f64 {
    let mut sum = 0.0f64;
    for _ in 0..1_000_000 {
        let a = [1.0f64, 2.0, 3.0, 4.0];
        let b = [0.5f64, 0.5, 0.5, 0.5];
        let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
        sum += d;
    }
    sum
}

#[test]
fn bench_01_recursive_fib_35() {
    let src = r#"
fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let t0 = now_ms()
    let res = fib(35)
    let elapsed = now_ms() - t0
    out "INTERNAL_MS:" + elapsed
    out res
}
"#;
    let (dtr_out, dtr_time) = compile_and_run_release(src, "bench_fib35");
    assert!(
        dtr_out.contains("9227465"),
        "fib(35) must equal 9227465, got: {}",
        dtr_out
    );

    let t0 = Instant::now();
    let rust_res = rust_fib(35);
    let rust_time = t0.elapsed();
    assert_eq!(rust_res, 9227465);

    let internal_ms: &str = dtr_out
        .lines()
        .find(|l| l.starts_with("INTERNAL_MS:"))
        .map(|l| l.strip_prefix("INTERNAL_MS:").unwrap().trim())
        .unwrap_or("?");

    println!(
        "\n[BENCHMARK 1] fib(35) Recursive:\n  - Datara In-Process: {} ms (Wall clock: {:.2} ms)\n  - Rust Release:      {:.2} ms",
        internal_ms,
        dtr_time.as_secs_f64() * 1000.0,
        rust_time.as_secs_f64() * 1000.0
    );
}

#[test]
fn bench_02_loop_sum_1e8_folded() {
    let src = r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 100000000 {
        sum = sum + i
        i = i + 1
    }
    let elapsed = now_ms() - t0
    out "INTERNAL_MS:" + elapsed
    out sum
}
"#;
    let (dtr_out, dtr_time) = compile_and_run_release(src, "bench_loop_folded");
    assert!(
        dtr_out.contains("4999999950000000"),
        "Gauss sum must match closed form, got: {}",
        dtr_out
    );

    let internal_ms: &str = dtr_out
        .lines()
        .find(|l| l.starts_with("INTERNAL_MS:"))
        .map(|l| l.strip_prefix("INTERNAL_MS:").unwrap().trim())
        .unwrap_or("?");

    println!(
        "\n[BENCHMARK 2A] Loop sum 1e8 Folded (LoopFold O(1)):\n  - Datara In-Process: {} ms (Wall clock: {:.2} ms)",
        internal_ms,
        dtr_time.as_secs_f64() * 1000.0
    );
}

#[test]
fn bench_02_loop_sum_1e8_raw() {
    let src = r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0
    mut i = 0
    while i < 100000000 {
        if i < 50000000 {
            sum = sum + 1
        } else {
            sum = sum + 2
        }
        i = i + 1
    }
    let elapsed = now_ms() - t0
    out "INTERNAL_MS:" + elapsed
    out sum
}
"#;
    let (dtr_out, dtr_time) = compile_and_run_release(src, "bench_loop_raw");

    let t0 = Instant::now();
    let rust_res = rust_loop_raw(100_000_000);
    let rust_time = t0.elapsed();

    assert!(
        dtr_out.contains(&format!("{}", rust_res)),
        "Raw loop outputs must match bit-for-bit, got: {}",
        dtr_out
    );

    let internal_ms: &str = dtr_out
        .lines()
        .find(|l| l.starts_with("INTERNAL_MS:"))
        .map(|l| l.strip_prefix("INTERNAL_MS:").unwrap().trim())
        .unwrap_or("?");

    println!(
        "\n[BENCHMARK 2B] Loop sum 1e8 Raw (100M iter with branch):\n  - Datara In-Process: {} ms (Wall clock: {:.2} ms)\n  - Rust Release:      {:.2} ms",
        internal_ms,
        dtr_time.as_secs_f64() * 1000.0,
        rust_time.as_secs_f64() * 1000.0
    );
}

#[test]
fn bench_03_simd_dot_product_4m_floats() {
    let src = r#"
fn main() {
    let t0 = now_ms()
    mut sum = 0.0
    mut i = 0
    while i < 1000000 {
        let a = float4(1.0, 2.0, 3.0, 4.0)
        let b = float4(0.5, 0.5, 0.5, 0.5)
        let d = dot(a, b)
        sum = sum + d
        i = i + 1
    }
    let elapsed = now_ms() - t0
    out "INTERNAL_MS:" + elapsed
    out sum
}
"#;
    let (dtr_out, dtr_time) = compile_and_run_release(src, "bench_simd_dot");

    let t0 = Instant::now();
    let rust_res = rust_dot_4m();
    let rust_time = t0.elapsed();

    assert!(
        dtr_out.contains("5000000"),
        "SIMD dot sum must match, got: {}",
        dtr_out
    );
    assert!((rust_res - 5000000.0).abs() < 1e-5);

    let internal_ms: &str = dtr_out
        .lines()
        .find(|l| l.starts_with("INTERNAL_MS:"))
        .map(|l| l.strip_prefix("INTERNAL_MS:").unwrap().trim())
        .unwrap_or("?");

    println!(
        "\n[BENCHMARK 3] SIMD Dot 4M float4 (1M x 4 floats):\n  - Datara In-Process: {} ms (Wall clock: {:.2} ms)\n  - Rust Reference:    {:.2} ms",
        internal_ms,
        dtr_time.as_secs_f64() * 1000.0,
        rust_time.as_secs_f64() * 1000.0
    );
}
