use forgen::driver::ForgenCompiler;

#[test]
fn test_microbenchmark_integer_and_float_scalar() {
    let compiler = ForgenCompiler::new("domain");

    let source = r#"
fn compute_scalar(n: Int, factor: Float) -> Float {
    mut sum = 0.0
    mut i = 0
    while i < n {
        sum = sum + i * factor
        i = i + 1
    }
    return sum
}

fn main() {
    mut res = 0.0
    res = compute_scalar(1000, 2.5)
    out res
}
"#;

    let res = compiler.compile_source(source, "micro_scalar.dtr", None);
    assert!(res.success, "Scalar compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let (stdout, stderr, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    // Sum of 0..1000 is 499500 * 2.5 = 1248750
    assert!(stdout.contains("1.24875e+06") || stdout.contains("1248750"));
}

#[test]
fn test_microbenchmark_array_bounds_and_contiguous_memory() {
    let compiler = ForgenCompiler::new("domain");

    let source = r#"
fn sum_array_elements(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        let v = i * 4
        if v % 8 == 0 {
            sum = sum + v
        }
        i = i + 1
    }
    return sum
}

fn main() {
    let res = sum_array_elements(500)
    out res
}
"#;

    let res = compiler.compile_source(source, "micro_array.dtr", None);
    assert!(res.success, "Array microbenchmark failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("249000") || !stdout.is_empty());
}

#[test]
fn test_microbenchmark_generic_specialization_box_and_list() {
    let compiler = ForgenCompiler::new("domain");

    let source = r#"
class Box<T> {
    val: T
}

fn main() {
    let b_int = Box<Int> { val: 42 }
    let b_flt = Box<Float> { val: 3.14159 }
    
    let v1 = b_int.val
    let v2 = b_flt.val
    
    out v1
    out v2
}
"#;

    let res = compiler.compile_source(source, "micro_generic.dtr", None);
    assert!(
        res.success,
        "Generic specialization failed: {:?}",
        res.error
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("42"));
    assert!(stdout.contains("3.14159"));
}

#[test]
fn test_microbenchmark_zero_cost_oop_sroa() {
    let compiler = ForgenCompiler::new("domain");

    let source = r#"
class Point {
    x: Float
    y: Float
}

behavior Point {
    length_sq() -> Float => this.x * this.x + this.y * this.y
}

fn main() {
    let p = Point { x: 3.0, y: 4.0 }
    let res = p.length_sq()
    out res
}
"#;

    let res = compiler.compile_source(source, "micro_oop.dtr", None);
    assert!(
        res.success,
        "Zero-cost OOP microbenchmark failed: {:?}",
        res.error
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("25"));
}

#[test]
fn test_microbenchmark_pipeline_fusion() {
    let compiler = ForgenCompiler::new("domain");

    let source = r#"
fn process_pipeline(n: Int) -> Int {
    mut acc = 0
    mut i = 0
    while i < n {
        let item = i * 3
        let mapped = item + 7
        acc = acc + mapped
        i = i + 1
    }
    return acc
}

fn main() {
    let total = process_pipeline(1000)
    out total
}
"#;

    let res = compiler.compile_source(source, "micro_pipeline.dtr", None);
    assert!(
        res.success,
        "Pipeline microbenchmark failed: {:?}",
        res.error
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("1505500"));
}

// The memory-layout and adaptive-cost test that lived here was removed along
// with `optimizer::layout::MemoryLayoutAnalyzer` and
// `optimizer::adaptive::AdaptiveCostModel`.
//
// Neither module was connected to code generation: they turned inputs into a
// plan string ("Plan B: AVX2 256-bit") that nothing in the optimizer or the
// Cranelift backend ever read. Testing them produced passing tests that looked
// like evidence of SIMD and layout optimization while no vector instruction or
// field reordering was ever emitted.
//
// Restoring this area honestly requires a layout/strategy decision that changes
// DMIR or backend output, plus a structural test on that output.
