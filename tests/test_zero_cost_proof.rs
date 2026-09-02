use forgen::codegen::cranelift::CraneliftBackend;
use forgen::driver::ForgenCompiler;

#[test]
fn test_zero_cost_oop_point_length() {
    let source = r#"
class Point {
    x: Float
    y: Float
}

behavior Point {
    length_sq() -> Float => this.x * this.x + this.y * this.y
}

fn main() {
    mut p = Point { x: 3.0, y: 4.0 }
    mut res = 0.0

    res = p.length_sq()
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "point_zero_cost.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let dmir = res.dmir_module.as_ref().unwrap();
    let rep = res.optimization_report.as_ref().unwrap();

    // 1. Verify inlining & scalarization facts
    let trace = &rep.decision_trace;
    println!("=== Decision Trace for Point ===");
    for r in trace {
        println!(
            "Pass: {:<10} | Candidate: {:<20} | Decision: {:<10} | Reason: {}",
            r.pass, r.candidate, r.decision, r.reason
        );
    }

    // 2. Codegen Machine Inspection
    let cranelift = CraneliftBackend::for_host();
    let inspection = cranelift.inspect_module(dmir);

    assert_eq!(
        inspection.total_heap_allocations, 0,
        "Zero-cost Point must have 0 heap allocations"
    );

    let clif = res.clif_source.as_ref().unwrap();
    println!("=== Generated Cranelift IR (CLIF) for Point ===");
    println!("{}", clif);

    // Verify CLIF contains direct float multiplication and addition, with no heap malloc or vtable calls
    assert!(
        clif.contains("fmul") || clif.contains("fadd"),
        "CLIF must contain direct float arithmetic"
    );
    assert!(
        !clif.contains("malloc"),
        "CLIF must not contain malloc calls"
    );
    assert!(
        !clif.contains("vtable"),
        "CLIF must not contain vtable indirection"
    );

    // 3. Execution Verification
    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "25");
}

#[test]
fn test_zero_cost_generic_box_suite() {
    let source = r#"
class Box<T> {
    val: T
}

class Inner {
    id: Int
    score: Float
}

fn process_int_box(b: Box<Int>) -> Int => b.val + 10
fn process_float_box(b: Box<Float>) -> Float => b.val * 2.0
fn process_struct_box(b: Box<Inner>) -> Float => b.val.score

fn main() {
    mut b_int = Box<Int> { val: 100 }
    mut b_flt = Box<Float> { val: 5.5 }
    mut inner = Inner { id: 1, score: 99.5 }
    mut b_struct = Box<Inner> { val: inner }

    out process_int_box(b_int)
    out process_float_box(b_flt)
    out process_struct_box(b_struct)
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "generic_box_zero_cost.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let rep = res.optimization_report.as_ref().unwrap();
    let clif = res.clif_source.as_ref().unwrap();
    println!("=== Generic CLIF ===\n{}", clif);

    assert!(
        rep.generic_specializations
            .contains(&"Box<Int>".to_string())
    );
    assert!(
        rep.generic_specializations
            .contains(&"Box<Float>".to_string())
    );
    assert!(
        rep.generic_specializations
            .contains(&"Box<Inner>".to_string())
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    println!("=== FULL STDOUT ===\n{}", stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines[0], "110");
    assert_eq!(lines[1], "11");
    assert_eq!(lines[2], "99.5");
}

#[test]
fn test_zero_cost_struct_creation_in_hot_loop() {
    // The box_generic benchmark shape, shrunk to a verifiable trip count:
    // a struct is created inside the loop every iteration. SROA must
    // eliminate the allocation entirely — the CLIF may contain no heap call
    // at all, inside or outside the loop.
    let source = r#"
class Box<T> {
    val: T
}

fn compute(n: Int) -> Int {
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
    let r = compute(100000)
    out r
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "hot_loop_box_zero_cost.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let clif = res.clif_source.as_ref().unwrap();
    let cranelift = CraneliftBackend::for_host();
    let inspection = cranelift.inspect_module(res.dmir_module.as_ref().unwrap());

    assert_eq!(
        inspection.total_heap_allocations, 0,
        "loop-local Box<Int> must be fully scalarized: got heap allocations"
    );
    assert!(
        !clif.contains("malloc"),
        "CLIF must not contain malloc calls for loop-local struct: {clif}"
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "4999950000");
}
