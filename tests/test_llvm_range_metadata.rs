use forgen::driver::ForgenCompiler;

#[test]
fn test_llvm_fvrp_range_metadata_and_assumes() {
    let source = r#"
fn process_byte(val: Int<0..255>) -> Int {
    let bounded: Int<0..255> = val
    bounded
}

fn main() {
    let b = process_byte(42)
    out(b)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "range_test.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");

    // Verify LLVM IR declarations and metadata
    assert!(
        llvm.contains("declare void @llvm.assume(i1)"),
        "Must declare @llvm.assume intrinsic in LLVM IR"
    );
    assert!(
        llvm.contains("@llvm.assume"),
        "Must call @llvm.assume for range bound verification"
    );
    assert!(
        llvm.contains("!range"),
        "Must emit !range metadata on load instruction"
    );
    assert!(
        llvm.contains("!{i64 0, i64 255}"),
        "Must emit half-open interval metadata node for Int<0..255>"
    );
}

#[test]
fn test_llvm_unit_of_measure_lowering_to_double() {
    let source = r#"
fn compute_velocity(v: Float<m/s>) -> Float {
    v * 2.0
}

fn main() {
    let speed: Float<m/s> = 12.5
    let doubled = compute_velocity(speed)
    out(doubled)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "unit_test.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");

    // Float<m/s> must be lowered to native IEEE-754 double, not ptr
    assert!(
        llvm.contains("double @compute_velocity(double %v"),
        "Float<m/s> parameter must lower to native 'double' in LLVM IR"
    );
    assert!(
        llvm.contains("fmul double"),
        "Must use native floating point multiplication instruction 'fmul double'"
    );
}

#[test]
fn test_llvm_fvrp_bounds_check_elimination_assume() {
    let source = r#"
fn read_first(items: List<Int>) -> Int {
    items[0]
}

fn main() {
    let arr = [10, 20, 30]
    let first = read_first(arr)
    out(first)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "bce_test.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");

    // Verify BCE assumption is injected
    assert!(
        llvm.contains("@llvm.assume"),
        "Must inject @llvm.assume for proved array bounds"
    );
}
