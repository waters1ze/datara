use forgen::driver::ForgenCompiler;

#[test]
fn test_inspect_all_7_workloads_assembly_and_clif() {
    let compiler = ForgenCompiler::new("release");

    let workloads = [
        (
            "Integer Loop (10M)",
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
            "Float Compute (10M)",
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
            "Point 2D SROA (10M)",
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
            "Generic Box (10M)",
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
            "Pipeline Dataflow (5M)",
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
            "Array Processing (1M)",
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
            "String Formatting (200K)",
            r#"
fn compute_strings(n: Int) -> Int {
    mut total_len = 0
    mut i = 0
    while i < n {
        total_len = total_len + 15
        i = i + 1
    }
    return total_len
}
fn main() {
    mut res = 0

    res = compute_strings(200000)
    out res
}
"#,
        ),
    ];

    println!(
        "\n=========================================================================================="
    );
    println!(
        "          FORGEN PERFORMANCE CLOSURE 2.0: CODEGEN & IR DISASSEMBLY INSPECTION             "
    );
    println!(
        "=========================================================================================="
    );

    for (idx, (name, src)) in workloads.iter().enumerate() {
        let res = compiler.compile_source_native(src, &format!("inspect_{}.dtr", idx), None);
        assert!(
            res.success,
            "Inspection compilation failed for {}: {:?}",
            name, res.error
        );

        println!(
            "\n------------------------------------------------------------------------------------------"
        );
        println!(" WORKLOAD [{}]: {}", idx + 1, name);
        println!(
            "------------------------------------------------------------------------------------------"
        );

        if let Some(rep) = &res.optimization_report {
            println!(" [OPTIMIZER REPORT]");
            println!("   - Constants folded:     {}", rep.constants_folded);
            println!(
                "   - Dead code removed:    {}",
                rep.dead_instructions_removed
            );
            println!("   - Functions inlined:    {}", rep.functions_inlined);
            println!("   - SROA allocs saved:    {}", rep.allocations_eliminated);
            println!("   - Decisions recorded:   {}", rep.decision_trace.len());
        }

        if let Some(clif) = &res.clif_source {
            println!(" [GENERATED CRANELIFT CLIF IR]");
            for line in clif.lines().take(35) {
                println!("   {}", line);
            }
            if clif.lines().count() > 35 {
                println!(
                    "   ... [truncated {} more lines]",
                    clif.lines().count() - 35
                );
            }
        }
    }
    println!(
        "\n==========================================================================================\n"
    );
}
