use forgen::driver::ForgenCompiler;
use std::time::Instant;

#[test]
fn test_forensic_full_7_workloads_cranelift_native() {
    let compiler_rel = ForgenCompiler::new("release");
    let compiler_dbg = ForgenCompiler::new("debug");

    let workloads = [
        (
            "Integer Loop 10M",
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
            "Float Compute 10M",
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
            "Point 2D SROA 10M",
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
        mut total = 0
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
            "Generic Box 10M",
            r#"
class Box<T> {
    val: T
}
fn compute_boxes(n: Int) -> Int {
    mut total = 0
    mut i = 0
    while i < n {
        mut b = Box<Int> { val: i }
        mut total = 0
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
            "Pipeline Dataflow 5M",
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
            "Array Processing 1M",
            r#"
fn compute_array(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        mut elem = 0
        elem = i * 2
        sum = sum + elem
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
            "String Formatting 200K",
            r#"
fn main() {
    mut len_acc = 0
    mut i = 0
    while i < 200000 {
        let msg = "item_" + i + ": " + (i * 2)
        len_acc = len_acc + 14
        i = i + 1
    }
    out len_acc
}
"#,
        ),
    ];

    println!(
        "\n=========================================================================================="
    );
    println!(
        "     FORENSIC AUDIT: CRANELIFT NATIVE RELEASE (-O3) vs NATIVE DEBUG                      "
    );
    println!(
        "=========================================================================================="
    );
    println!(
        " {:<24} | {:>16} | {:>16} | {:>14}",
        "Workload", "Native Release", "Native Debug", "Speedup"
    );
    println!(
        "------------------------------------------------------------------------------------------"
    );

    for (idx, (name, src)) in workloads.iter().enumerate() {
        // 1. Native Release
        let rel_res =
            compiler_rel.compile_source_native(src, &format!("forensic_rel_{}.dtr", idx), None);
        if !rel_res.success {
            eprintln!(
                "[FORENSIC ERROR for {}]:\n{:?}\n{}",
                name, rel_res.error, rel_res.diagnostics
            );
        }
        let (rel_time, rel_ok) = if rel_res.success {
            let exe = rel_res.exe_path.unwrap();
            let t0 = Instant::now();
            let (_, _, code, _) = compiler_rel.codegen.run_executable(&exe, &[]).unwrap();
            (t0.elapsed().as_secs_f64() * 1000.0, code == 0)
        } else {
            (0.0, false)
        };

        // 2. Native Debug
        let dbg_res = compiler_dbg.compile_source(src, &format!("forensic_dbg_{}.dtr", idx), None);
        let (dbg_time, dbg_ok) = if dbg_res.success {
            let exe = dbg_res.exe_path.unwrap();
            let t0 = Instant::now();
            let (_, _, code, _) = compiler_dbg.codegen.run_executable(&exe, &[]).unwrap();
            (t0.elapsed().as_secs_f64() * 1000.0, code == 0)
        } else {
            (0.0, false)
        };

        let speedup = if rel_time > 0.001 {
            dbg_time / rel_time
        } else {
            0.0
        };

        println!(
            " {:<24} | {:>13.2} ms | {:>13.2} ms | {:>13.1}x faster",
            name, rel_time, dbg_time, speedup
        );
        assert!(rel_ok, "Native release execution failed for {}", name);
        assert!(dbg_ok, "Native debug execution failed for {}", name);
    }
    println!(
        "==========================================================================================\n"
    );
}
