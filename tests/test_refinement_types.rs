use forgen::driver::ForgenCompiler;

#[test]
fn test_refinement_port_number_valid() {
    let source = r#"
type PortNumber = Int in 1..=65535

fn main() {
    val port: PortNumber = 8080
    out port
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_port_valid.dtr", None);
    assert!(res.success, "Valid port should compile: {:?}", res.error);

    let exe_path = res.exe_path.expect("executable path missing");
    let (out, err, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("execution failed");
    assert_eq!(code, 0, "non-zero exit: {}", err);
    assert_eq!(out.trim(), "8080");
}

#[test]
fn test_refinement_port_number_out_of_range_high() {
    let source = r#"
type PortNumber = Int in 1..=65535

fn main() {
    val port: PortNumber = 70000
    out port
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_port_high.dtr", None);
    assert!(
        !res.success,
        "Out of range port 70000 must fail compilation"
    );
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Refinement type violation") && err_str.contains("70000"),
        "Expected refinement error message, got: {}",
        err_str
    );
}

#[test]
fn test_refinement_port_number_out_of_range_low() {
    let source = r#"
type PortNumber = Int in 1..=65535

fn main() {
    val port: PortNumber = 0
    out port
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_port_low.dtr", None);
    assert!(!res.success, "Port 0 must fail compilation for 1..=65535");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Refinement type violation") && err_str.contains("0"),
        "Expected refinement error message, got: {}",
        err_str
    );
}

#[test]
fn test_refinement_normalized_float_valid() {
    let source = r#"
type NormalizedFloat = Float in 0.0..=1.0

fn main() {
    val ratio: NormalizedFloat = 0.75
    out ratio
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_float_valid.dtr", None);
    assert!(res.success, "Valid float should compile: {:?}", res.error);
}

#[test]
fn test_refinement_normalized_float_out_of_range() {
    let source = r#"
type NormalizedFloat = Float in 0.0..=1.0

fn main() {
    val ratio: NormalizedFloat = 1.5
    out ratio
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_float_invalid.dtr", None);
    assert!(
        !res.success,
        "Float 1.5 must fail compilation for 0.0..=1.0"
    );
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Refinement type violation") && err_str.contains("1.5"),
        "Expected refinement error message, got: {}",
        err_str
    );
}

#[test]
fn test_refinement_non_zero_int_valid() {
    let source = r#"
type NonZeroInt = Int where val != 0

fn main() {
    val x: NonZeroInt = 42
    out x
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_nonzero_valid.dtr", None);
    assert!(res.success, "Non-zero int 42 must compile: {:?}", res.error);
}

#[test]
fn test_refinement_non_zero_int_zero_fails() {
    let source = r#"
type NonZeroInt = Int where val != 0

fn main() {
    val x: NonZeroInt = 0
    out x
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_nonzero_invalid.dtr", None);
    assert!(!res.success, "Value 0 must fail compilation for NonZeroInt");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Refinement type violation") && err_str.contains("0"),
        "Expected refinement error message, got: {}",
        err_str
    );
}

#[test]
fn test_refinement_function_argument_check() {
    let source = r#"
type PortNumber = Int in 1..=65535

fn serve(port: PortNumber) -> Int {
    return port
}

fn main() {
    out serve(99999)
}
"#;
    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "refinement_call_invalid.dtr", None);
    assert!(!res.success, "Calling serve(99999) must fail compilation");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Refinement type violation") && err_str.contains("99999"),
        "Expected refinement error message, got: {}",
        err_str
    );
}
