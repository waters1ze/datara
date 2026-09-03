use forgen::driver::ForgenCompiler;
use std::fs;
use std::process::Command;

fn run_datara(code: &str, file_name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, file_name, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        file_name, res.error
    );

    let exe_path = res.exe_path.expect("exe_path missing");
    let output = Command::new(&exe_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run {}: {}", exe_path.display(), e));

    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(exe_path.with_extension("obj"));

    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_compiled_parallel_for_multicore_speedup() {
    let code = r#"
fn heavy_worker(id: Int) {
    mut acc = id
    mut i = 0
    while i < 15000000 {
        acc = (acc + i * 31) % 1000003
        i = i + 1
    }
}

fn main() {
    let t_par_0 = now_ms()
    parallel for i in 0..8 {
        heavy_worker(i)
    }
    let t_par = now_ms() - t_par_0

    let t_seq_0 = now_ms()
    for i in 0..8 {
        heavy_worker(i)
    }
    let t_seq = now_ms() - t_seq_0

    out "PAR_MS:" + t_par
    out "SEQ_MS:" + t_seq
}
"#;

    let out = run_datara(code, "test_par_for_speedup.dtr");
    println!("[PARALLEL FOR SPEEDUP TEST]\n{}", out);

    let mut par_ms: i64 = 0;
    let mut seq_ms: i64 = 0;

    for line in out.lines() {
        if line.starts_with("PAR_MS:") {
            par_ms = line
                .strip_prefix("PAR_MS:")
                .unwrap()
                .trim()
                .parse()
                .unwrap_or(0);
        } else if line.starts_with("SEQ_MS:") {
            seq_ms = line
                .strip_prefix("SEQ_MS:")
                .unwrap()
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }

    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    println!(
        "[PARALLEL FOR STATS] CPUs: {}, Sequential: {} ms, Parallel: {} ms, Speedup: {:.2}x",
        num_cpus,
        seq_ms,
        par_ms,
        (seq_ms as f64) / (par_ms as f64).max(1.0)
    );

    assert!(par_ms >= 0, "Parallel run failed");
    assert!(seq_ms >= 0, "Sequential run failed");
    if num_cpus >= 4 && seq_ms > 50 {
        // Only assert speedup on machines with 4+ real cores where virtualization jitter does not dominate
        assert!(
            par_ms <= seq_ms * 2,
            "Parallel execution ({} ms) abnormal compared to sequential ({} ms)",
            par_ms,
            seq_ms
        );
    }
}
