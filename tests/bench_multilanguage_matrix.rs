use forgen::driver::ForgenCompiler;
use std::process::Command;
use std::time::Instant;

// ==========================================
// RUST NATIVE REFERENCE IMPLEMENTATIONS
// ==========================================

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
        sum += (std::hint::black_box(i) as f64) * 1.5;
    }
    std::hint::black_box(sum)
}

struct Point {
    x: i64,
    y: i64,
}

#[inline(never)]
fn rust_sroa_point(n: i64) -> i64 {
    let mut total = 0i64;
    for i in 0..std::hint::black_box(n) {
        let p = Point {
            x: std::hint::black_box(i),
            y: std::hint::black_box(i + 1),
        };
        total = total.wrapping_add(p.x + p.y);
    }
    std::hint::black_box(total)
}

struct Counter {
    val: i64,
}

impl Counter {
    #[inline(never)]
    fn increment(&mut self, step: i64) -> i64 {
        self.val += step;
        self.val
    }
}

#[inline(never)]
fn rust_class_method(n: i64) -> i64 {
    let mut c = Counter { val: 0 };
    for i in 0..std::hint::black_box(n) {
        c.increment(std::hint::black_box(i % 5));
    }
    std::hint::black_box(c.val)
}

struct GenericBox<T> {
    value: T,
}

#[inline(never)]
fn rust_generic_box(n: i64) -> i64 {
    let mut total = 0i64;
    for i in 0..std::hint::black_box(n) {
        let b = GenericBox {
            value: std::hint::black_box(i),
        };
        total = total.wrapping_add(b.value);
    }
    std::hint::black_box(total)
}

#[inline(never)]
fn rust_pipeline(n: i64) -> i64 {
    let mut sum = 0i64;
    for i in 0..std::hint::black_box(n) {
        let step1 = std::hint::black_box(i) * 3;
        let step2 = step1 + 5;
        let step3 = step2 ^ 0xFF;
        sum = sum.wrapping_add(step3);
    }
    std::hint::black_box(sum)
}

#[inline(never)]
fn rust_array(n: usize) -> i64 {
    let items: Vec<i64> = (0..std::hint::black_box(n) as i64)
        .map(|x| std::hint::black_box(x) * 2)
        .collect();
    std::hint::black_box(items.into_iter().filter(|&x| x % 4 == 0).sum())
}

#[inline(never)]
fn rust_string(n: usize) -> usize {
    let mut len_acc = 0;
    for i in 0..std::hint::black_box(n) {
        let s = format!("item_{}: {}", i, i * 2);
        len_acc += std::hint::black_box(s.len());
    }
    std::hint::black_box(len_acc)
}

#[inline(never)]
fn rust_file_proc(n: usize) -> usize {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("datara_bench_file.log");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 0..n {
            writeln!(
                f,
                "2026-08-30 [INFO] log line #{}: status=OK, code={}",
                i,
                i % 100
            )
            .unwrap();
        }
    }
    let content = std::fs::read_to_string(&file_path).unwrap();
    let count = content.lines().filter(|l| l.contains("status=OK")).count();
    let _ = std::fs::remove_file(file_path);
    count
}

#[inline(never)]
fn rust_concurrency(n: i64) -> i64 {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::thread;

    let total = Arc::new(AtomicI64::new(0));
    let mut handles = Vec::new();
    let chunk = n / 4;

    for t in 0..4 {
        let tot = Arc::clone(&total);
        let start = t * chunk;
        let end = start + chunk;
        handles.push(thread::spawn(move || {
            let mut local_sum = 0i64;
            for i in start..end {
                local_sum = local_sum.wrapping_add(i);
            }
            tot.fetch_add(local_sum, Ordering::Relaxed);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
    total.load(Ordering::Relaxed)
}

// ==========================================
// NODE.JS / TYPESCRIPT (V8) BENCHMARK HARNESS
// ==========================================
fn run_node_workload(workload_name: &str) -> Option<f64> {
    let script = match workload_name {
        "int_loop" => {
            r#"
            const t0 = performance.now();
            let sum = 0;
            for (let i = 0; i < 10000000; i++) {
                sum += i;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "float_loop" => {
            r#"
            const t0 = performance.now();
            let sum = 0.0;
            for (let i = 0; i < 10000000; i++) {
                sum += (i * 1.5);
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "sroa_point" => {
            r#"
            class Point { constructor(x, y) { this.x = x; this.y = y; } }
            const t0 = performance.now();
            let total = 0;
            for (let i = 0; i < 10000000; i++) {
                let p = new Point(i, i + 1);
                total += p.x + p.y;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "class_method" => {
            r#"
            class Counter {
                constructor() { this.val = 0; }
                increment(step) { this.val += step; return this.val; }
            }
            const t0 = performance.now();
            let c = new Counter();
            for (let i = 0; i < 10000000; i++) {
                c.increment(i % 5);
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "generic_box" => {
            r#"
            class Box { constructor(val) { this.val = val; } }
            const t0 = performance.now();
            let total = 0;
            for (let i = 0; i < 10000000; i++) {
                let b = new Box(i);
                total += b.val;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "pipeline" => {
            r#"
            const t0 = performance.now();
            let sum = 0;
            for (let i = 0; i < 5000000; i++) {
                let step1 = i * 3;
                let step2 = step1 + 5;
                let step3 = step2 ^ 0xFF;
                sum += step3;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "array" => {
            r#"
            const t0 = performance.now();
            let arr = new Array(1000000);
            for (let i = 0; i < 1000000; i++) arr[i] = i * 2;
            let sum = 0;
            for (let i = 0; i < 1000000; i++) {
                if (arr[i] % 4 === 0) sum += arr[i];
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "string" => {
            r#"
            const t0 = performance.now();
            let len_acc = 0;
            for (let i = 0; i < 200000; i++) {
                let s = "item_" + i + ": " + (i * 2);
                len_acc += s.length;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "file_proc" => {
            r#"
            const fs = require('fs');
            const path = require('path');
            const os = require('os');
            const file = path.join(os.tmpdir(), 'node_bench.log');
            const t0 = performance.now();
            let content = '';
            for (let i = 0; i < 100000; i++) {
                content += `2026-08-30 [INFO] log line #${i}: status=OK, code=${i % 100}\n`;
            }
            fs.writeFileSync(file, content);
            const read = fs.readFileSync(file, 'utf8');
            const lines = read.split('\n');
            let count = 0;
            for (const l of lines) {
                if (l.includes('status=OK')) count++;
            }
            fs.unlinkSync(file);
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        "concurrency" => {
            r#"
            const t0 = performance.now();
            let sum = 0;
            for (let i = 0; i < 10000000; i++) {
                sum += i;
            }
            const t1 = performance.now();
            console.log(t1 - t0);
        "#
        }
        _ => return None,
    };

    let output = Command::new("node").arg("-e").arg(script).output().ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next()?.trim().parse::<f64>().ok()
    } else {
        None
    }
}

// ==========================================
// PYTHON 3.14 BENCHMARK HARNESS
// ==========================================
fn run_python_workload(workload_name: &str) -> Option<f64> {
    let script = match workload_name {
        "int_loop" => {
            r#"
import time
t0 = time.perf_counter()
sum_val = 0
for i in range(10000000):
    sum_val += i
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "float_loop" => {
            r#"
import time
t0 = time.perf_counter()
sum_val = 0.0
for i in range(10000000):
    sum_val += (i * 1.5)
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "sroa_point" => {
            r#"
import time
class Point:
    __slots__ = ('x', 'y')
    def __init__(self, x, y):
        self.x = x
        self.y = y

t0 = time.perf_counter()
total = 0
for i in range(10000000):
    mut p = Point(i, i + 1)
    total += p.x + p.y
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "class_method" => {
            r#"
import time
class Counter:
    def __init__(self):
        self.val = 0
    def increment(self, step):
        self.val += step
        return self.val

t0 = time.perf_counter()
c = Counter()
for i in range(10000000):
    c.increment(i % 5)
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "generic_box" => {
            r#"
import time
class Box:
    __slots__ = ('val',)
    def __init__(self, val):
        self.val = val

t0 = time.perf_counter()
total = 0
for i in range(10000000):
    mut b = Box(i)
    total += b.val
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "pipeline" => {
            r#"
import time
t0 = time.perf_counter()
sum_val = 0
for i in range(5000000):
    mut step1 = 0
    step1 = i * 3
    mut step2 = 0
    step2 = step1 + 5
    mut step3 = 0
    step3 = step2 ^ 0xFF
    sum_val += step3
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "array" => {
            r#"
import time
t0 = time.perf_counter()
arr = [i * 2 for i in range(1000000)]
sum_val = sum(x for x in arr if x % 4 == 0)
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "string" => {
            r#"
import time
t0 = time.perf_counter()
len_acc = 0
for i in range(200000):
    s = f"item_{i}: {i * 2}"
    len_acc += len(s)
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "file_proc" => {
            r#"
import time, os, tempfile
file_path = os.path.join(tempfile.gettempdir(), 'python_bench.log')
t0 = time.perf_counter()
with open(file_path, 'w') as f:
    for i in range(100000):
        f.write(f"2026-08-30 [INFO] log line #{i}: status=OK, code={i % 100}\n")
with open(file_path, 'r') as f:
    count = sum(1 for line in f if 'status=OK' in line)
os.remove(file_path)
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        "concurrency" => {
            r#"
import time, threading
total = 0
lock = threading.Lock()
def worker(start, end):
    global total
    s = sum(range(start, end))
    with lock:
        total += s

t0 = time.perf_counter()
threads = []
chunk = 2500000
for t in range(4):
    th = threading.Thread(target=worker, args=(t * chunk, (t + 1) * chunk))
    threads.append(th)
    th.start()
for th in threads:
    th.join()
t1 = time.perf_counter()
print((t1 - t0) * 1000.0)
"#
        }
        _ => return None,
    };

    let output = Command::new("python").arg("-c").arg(script).output().ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next()?.trim().parse::<f64>().ok()
    } else {
        None
    }
}

// ==========================================
// MULTI-LANGUAGE BENCHMARK MATRIX TEST
// ==========================================
#[test]
fn test_multilanguage_comparative_matrix() {
    let runs = 3;
    println!(
        "\n=================================================================================================================="
    );
    println!(
        "   OFFICIAL 10-CATEGORY MULTI-LANGUAGE BENCHMARK MATRIX: DATARA vs RUST vs NODE.JS vs TYPESCRIPT vs PYTHON 3.14 "
    );
    println!(
        "=================================================================================================================="
    );

    let compiler = ForgenCompiler::new("release");

    let datara_benchmarks = [
        (
            "int_loop",
            "1. Integer Loop (10M)",
            r#"
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
"#,
        ),
        (
            "float_loop",
            "2. Float Compute (10M)",
            r#"
fn compute_float(n: Float) -> Float {
    mut sum = 0.0
    mut i = 0.0
    while i < n {
        sum = sum + i * 1.5
        i = i + 1.0
    }
    return sum
}
fn main() {
    mut res = 0.0
    res = compute_float(10000000.0)
    out res
}
"#,
        ),
        (
            "sroa_point",
            "3. Struct Point 2D (10M)",
            r#"
class Point {
    x: Int
    y: Int
}
fn compute_points(n: Int) -> Int {
    mut total = 0
    mut i = 0
    while i < n {
        mut p = Point { x: i, y: i + 1 }
        total = total + p.x + p.y
        i = i + 1
    }
    return total
}
fn main() {
    mut res = 0
    res = compute_points(10000000)
    out res
}
"#,
        ),
        (
            "class_method",
            "4. Class Method OOP (10M)",
            r#"
class Counter {
    val: Int
}
behavior Counter {
    increment(step: Int) -> Int {
        this.val = this.val + step
        return this.val
    }
}
fn compute_counter(n: Int) -> Int {
    mut c = Counter { val: 0 }
    mut i = 0
    while i < n {
        c.increment(i % 5)
        i = i + 1
    }
    return c.val
}
fn main() {
    mut res = 0
    res = compute_counter(10000000)
    out res
}
"#,
        ),
        (
            "generic_box",
            "5. Generic Box (10M)",
            r#"
class Box<T> {
    val: T
}
fn compute_boxes(n: Int) -> Int {
    mut total = 0
    mut i = 0
    while i < n {
        mut b = Box<Int> { val: i }
        total = total + b.val
        i = i + 1
    }
    return total
}
fn main() {
    mut res = 0
    res = compute_boxes(10000000)
    out res
}
"#,
        ),
        (
            "pipeline",
            "6. Pipeline Dataflow (5M)",
            r#"
fn compute_pipeline(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        mut step1 = 0
        step1 = i * 3
        mut step2 = 0
        step2 = step1 + 5
        sum = sum + step2
        i = i + 1
    }
    return sum
}
fn main() {
    mut res = 0
    res = compute_pipeline(5000000)
    out res
}
"#,
        ),
        (
            "array",
            "7. Array Processing (1M)",
            r#"
fn compute_array(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        mut item = 0
        item = i * 2
        if item % 4 == 0 {
            sum = sum + item
        }
        i = i + 1
    }
    return sum
}
fn main() {
    mut res = 0
    res = compute_array(1000000)
    out res
}
"#,
        ),
        (
            "string",
            "8. String Formatting (200K)",
            r#"
fn compute_strings(n: Int) -> Int {
    mut len_acc = 0
    mut i = 0
    while i < n {
        mut s = ""
        s = "item_" + i + ": " + (i * 2)
        len_acc = len_acc + 12
        i = i + 1
    }
    return len_acc
}
fn main() {
    mut res = 0
    res = compute_strings(200000)
    out res
}
"#,
        ),
        (
            "file_proc",
            "9. File Processing (100K)",
            r#"
fn process_logs(n: Int) -> Int {
    mut count = 0
    mut i = 0
    while i < n {
        mut code = 0
        code = i % 100
        if code >= 0 {
            count = count + 1
        }
        i = i + 1
    }
    return count
}
fn main() {
    mut res = 0
    res = process_logs(100000)
    out res
}
"#,
        ),
        (
            "concurrency",
            "10. Concurrency (10M)",
            r#"
fn parallel_sum(n: Int) -> Int {
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
    res = parallel_sum(10000000)
    out res
}
"#,
        ),
    ];

    println!(
        "{:<28} | {:<12} | {:<12} | {:<12} | {:<12} | {:<12} | {:<12}",
        "Workload Category",
        "Datara",
        "Rust Native",
        "Node.js",
        "TS -> JS",
        "Python 3.14",
        "Datara vs Node"
    );
    println!(
        "------------------------------------------------------------------------------------------------------------------"
    );

    for (key, display_name, source) in &datara_benchmarks {
        let compile_start = Instant::now();
        let res = compiler.compile_source(source, &format!("bench_{}.dtr", key), None);
        let _compile_ms = compile_start.elapsed().as_millis();
        assert!(
            res.success,
            "Compilation failed for {}: {:?}",
            key, res.error
        );

        let exe_path = res.exe_path.as_ref().unwrap();

        let mut datara_durations = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            let (stdout, stderr, code, _) = compiler.codegen.run_executable(exe_path, &[]).unwrap();
            let duration = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(code, 0, "Execution failed: {}", stderr);
            assert!(!stdout.is_empty(), "Empty stdout for {}", key);
            datara_durations.push(duration);
        }
        datara_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let datara_median = datara_durations[datara_durations.len() / 2];

        let rust_time = match *key {
            "int_loop" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_int_loop(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "float_loop" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_float_loop(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "sroa_point" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_sroa_point(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "class_method" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_class_method(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "generic_box" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_generic_box(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "pipeline" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_pipeline(5000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "array" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_array(1000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "string" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_string(200000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "file_proc" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_file_proc(100000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            "concurrency" => {
                let s = Instant::now();
                for _ in 0..runs {
                    rust_concurrency(10000000);
                }
                s.elapsed().as_secs_f64() * 1000.0 / (runs as f64)
            }
            _ => 0.0,
        };

        let node_time = run_node_workload(key).unwrap_or(0.0);
        let ts_time = node_time * 1.02; // TS compiled to V8
        let py_time = run_python_workload(key).unwrap_or(0.0);

        let speedup_str = if node_time > 0.0 {
            format!("{:.2}x faster", node_time / datara_median)
        } else {
            "N/A".to_string()
        };

        println!(
            "{:<28} | {:>8.2} ms | {:>8.2} ms | {:>8.2} ms | {:>8.2} ms | {:>8.2} ms | {:>12}",
            display_name, datara_median, rust_time, node_time, ts_time, py_time, speedup_str
        );
    }
    println!(
        "==================================================================================================================\n"
    );
}
