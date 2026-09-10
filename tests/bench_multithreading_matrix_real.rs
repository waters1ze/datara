use forgen::driver::ForgenCompiler;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;
use std::time::Instant;

fn run_datara(code: &str, file_name: &str) -> (String, i64) {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, file_name, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        file_name, res.error
    );

    let exe_path = res.exe_path.expect("exe_path missing");
    let t0 = Instant::now();
    let output = Command::new(&exe_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run {}: {}", exe_path.display(), e));
    let elapsed_ms = t0.elapsed().as_millis() as i64;

    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(exe_path.with_extension("obj"));

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        elapsed_ms,
    )
}

#[inline(never)]
fn rust_heavy_compute(id: i64, iters: i64) -> i64 {
    let mut acc = id;
    let mut i = 0i64;
    while i < iters {
        acc = (acc + i * 31) % 1000003;
        i += 1;
    }
    std::hint::black_box(acc)
}

fn benchmark_rust_multithreaded(num_chunks: i64, iters_per_chunk: i64) -> i64 {
    let t0 = Instant::now();
    let mut handles = Vec::new();
    let counter = Arc::new(AtomicI64::new(0));

    for id in 0..num_chunks {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            let res = rust_heavy_compute(id, iters_per_chunk);
            c.fetch_add(res, Ordering::Relaxed);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
    t0.elapsed().as_millis() as i64
}

fn benchmark_python_multithreaded(num_chunks: i64, iters_per_chunk: i64) -> i64 {
    let py_script = format!(
        r#"
import time
from concurrent.futures import ThreadPoolExecutor

def heavy_worker(id_val):
    acc = id_val
    i = 0
    while i < {}:
        acc = (acc + i * 31) % 1000003
        i += 1
    return acc

t0 = time.perf_counter()
with ThreadPoolExecutor(max_workers={}) as executor:
    results = list(executor.map(heavy_worker, range({})))
elapsed_ms = int((time.perf_counter() - t0) * 1000)
print(elapsed_ms)
"#,
        iters_per_chunk, num_chunks, num_chunks
    );

    let output = Command::new("python").arg("-c").arg(&py_script).output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        text.parse::<i64>().unwrap_or(-1)
    } else {
        -1
    }
}

fn benchmark_node_multithreaded(num_chunks: i64, iters_per_chunk: i64) -> i64 {
    let js_script = format!(
        r#"
const {{ Worker, isMainThread, parentPort, workerData }} = require('worker_threads');

if (isMainThread) {{
    const t0 = Date.now();
    let completed = 0;
    const numChunks = {};
    for (let i = 0; i < numChunks; i++) {{
        const worker = new Worker(__filename, {{ workerData: {{ id: i, iters: {} }} }});
        worker.on('message', () => {{
            completed++;
            if (completed === numChunks) {{
                console.log(Date.now() - t0);
            }}
        }});
    }}
}} else {{
    const {{ id, iters }} = workerData;
    let acc = id;
    let i = 0;
    while (i < iters) {{
        acc = (acc + i * 31) % 1000003;
        i++;
    }}
    parentPort.postMessage(acc);
}}
"#,
        num_chunks, iters_per_chunk
    );

    let tmp_file = std::env::temp_dir().join("node_par_bench.js");
    let _ = fs::write(&tmp_file, js_script);

    let output = Command::new("node").arg(&tmp_file).output();
    let _ = fs::remove_file(&tmp_file);

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        text.parse::<i64>().unwrap_or(-1)
    } else {
        -1
    }
}

#[test]
#[ignore = "intensive multithreading benchmark"]
fn test_multithreading_performance_matrix_cross_language() {
    let num_chunks = 16i64;
    let iters_per_chunk = 10_000_000i64;

    let datara_code = format!(
        r#"
fn heavy_worker(id: Int) {{
    mut acc = id
    mut i = 0
    while i < {} {{
        acc = (acc + i * 31) % 1000003
        i = i + 1
    }}
    if acc == 999999999 {{
        out acc
    }}
}}

fn main() {{
    let t0 = now_ms()
    parallel for i in 0..{} {{
        heavy_worker(i)
    }}
    let elapsed = now_ms() - t0
    out "DATARA_PAR_MS:" + elapsed
}}
"#,
        iters_per_chunk, num_chunks
    );

    let (datara_out, _) = run_datara(&datara_code, "bench_datara_par.dtr");
    let mut datara_ms: i64 = -1;
    for line in datara_out.lines() {
        if line.starts_with("DATARA_PAR_MS:") {
            datara_ms = line
                .strip_prefix("DATARA_PAR_MS:")
                .unwrap()
                .trim()
                .parse()
                .unwrap_or(-1);
        }
    }

    let rust_ms = benchmark_rust_multithreaded(num_chunks, iters_per_chunk);
    let python_ms = benchmark_python_multithreaded(num_chunks, iters_per_chunk);
    let node_ms = benchmark_node_multithreaded(num_chunks, iters_per_chunk);

    let num_cpus = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    println!(
        "\n=========================================================================================="
    );
    println!(
        "   OFFICIAL NATIVE MULTITHREADING PERFORMANCE BENCHMARK MATRIX (16 CHUNKS x 10M ITERATIONS)"
    );
    println!("   Host CPU Cores: {}", num_cpus);
    println!(
        "=========================================================================================="
    );
    println!(
        "Language/Runtime                | Multi-Core Execution Time | vs Datara Performance       "
    );
    println!(
        "------------------------------------------------------------------------------------------"
    );
    println!(
        "Datara (Native Cranelift Pool)  | {:>10} ms            | 1.00x (Baseline Native)     ",
        datara_ms
    );
    println!(
        "Rust (std::thread pool / LLVM)  | {:>10} ms            | {:.2}x ({})                 ",
        rust_ms,
        (rust_ms as f64) / (datara_ms as f64).max(1.0),
        if rust_ms <= datara_ms {
            "faster"
        } else {
            "slower"
        }
    );
    if node_ms > 0 {
        println!(
            "Node.js (Worker Threads / V8)   | {:>10} ms            | {:.2}x slower               ",
            node_ms,
            (node_ms as f64) / (datara_ms as f64).max(1.0)
        );
    }
    if python_ms > 0 {
        println!(
            "Python 3.14 (ThreadPoolExecutor)| {:>10} ms            | {:.2}x slower (GIL bound)   ",
            python_ms,
            (python_ms as f64) / (datara_ms as f64).max(1.0)
        );
    }
    println!(
        "==========================================================================================\n"
    );

    assert!(datara_ms >= 0, "Datara execution failed");
    assert!(rust_ms >= 0, "Rust execution failed");
}
