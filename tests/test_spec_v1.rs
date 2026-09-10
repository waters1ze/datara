use forgen::driver::ForgenCompiler;

#[test]
fn test_spec_fn_and_function_keyword_equivalence() {
    let src1 = "fn calc(x: Int) -> Int { return x + 10 }\nfn main() { out calc(5) }";
    let src2 = "function calc(x: Int) -> Int { return x + 10 }\nfunction main() { out calc(5) }";

    let compiler = ForgenCompiler::new("release");
    let res1 = compiler.compile_source(src1, "spec_fn.dtr", None);
    let res2 = compiler.compile_source(src2, "spec_function.dtr", None);

    assert!(res1.success, "fn keyword failed: {:?}", res1.error);
    assert!(res2.success, "function keyword failed: {:?}", res2.error);

    let (out1, _, _, _) = compiler
        .cranelift
        .run_executable(&res1.exe_path.unwrap(), &[])
        .unwrap();
    let (out2, _, _, _) = compiler
        .cranelift
        .run_executable(&res2.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(out1.trim(), "15");
    assert_eq!(out2.trim(), "15");
}

#[test]
fn test_spec_strict_bool_coercion_fails_on_int() {
    let src = r#"
fn main() {
    if 1 {
        out "invalid truthy"
    }
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.compile_source(src, "spec_strict_bool.dtr", None);
    assert!(
        !res.success,
        "Compiler must reject non-Bool condition in if statement"
    );
    let diag = res.diagnostics.to_lowercase();
    assert!(
        diag.contains("bool"),
        "Diagnostic must state condition must be Bool, got: {}",
        res.diagnostics
    );
}

#[test]
fn test_spec_integer_wrapping_arithmetic() {
    // wrapping(9223372036854775807 + 1) should wrap to -9223372036854775808 (i64::MIN)
    let src = r#"
fn main() {
    mut max = 9223372036854775807
    mut wrapped = wrapping(max + 1)
    out wrapped
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_wrap.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (out, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        out.trim(),
        "-9223372036854775808",
        "Integer wrapping arithmetic must wrap to i64::MIN"
    );
}

#[test]
fn test_spec_integer_default_overflow_traps() {
    // 9223372036854775807 + 1 without wrapping must trap with non-zero exit code
    let src = r#"
fn main() {
    mut max = 9223372036854775807
    mut overflowed = max + 1
    out overflowed
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_ovf.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (_, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_ne!(
        code, 0,
        "Default integer addition overflow must trap with non-zero exit code"
    );
}

#[test]
fn test_spec_integer_saturating_arithmetic() {
    // saturating(9223372036854775807 + 10) should clamp to 9223372036854775808 - 1
    // saturating(-9223372036854775807 - 1 - 10) should clamp to -9223372036854775808
    let src = r#"
fn main() {
    mut max = 9223372036854775807
    mut sat_max = saturating(max + 10)
    out sat_max

    mut min = wrapping(-9223372036854775807 - 1)
    mut sat_min = saturating(min - 10)
    out sat_min
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_sat.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (out, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().map(|s| s.trim()).collect();
    assert_eq!(lines, vec!["9223372036854775807", "-9223372036854775808"]);
}

#[test]
fn test_spec_with_composition() {
    let src = r#"
role Printable {
    label() -> String
}

class Counter with Printable {
    count: Int
    label() -> String {
        return "Count"
    }
}

fn main() {
    mut c = Counter { count: 42 }
    out c.label()
    out c.count
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(src, "spec_with.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (out, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().map(|s| s.trim()).collect();
    assert_eq!(lines, vec!["Count", "42"]);
}

#[test]
fn test_spec_wasm_integer_overflow_policies_emit_valid_wasm() {
    let source = r#"
fn calc(a: Int, b: Int) -> Int {
    mut sum = a + b
    mut wrap = wrapping(a + b)
    mut sat = saturating(a + b)
    return sum + wrap + sat
}
fn main() {
    out calc(10, 20)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "spec_wasm_ovf.dtr")
        .expect("DMIR lowering must succeed");

    let temp_dir = std::env::temp_dir().join("spec_wasm_ovf_test");
    let _ = std::fs::create_dir_all(&temp_dir);
    let wasm_file = temp_dir.join("spec_wasm_ovf.wasm");

    let res = forgen::codegen::wasm::WasmEmitter::emit_wasm_binary(&dmir, &wasm_file);
    assert!(res.is_ok(), "WASM emission must succeed: {:?}", res.err());

    let wasm_bytes = std::fs::read(&wasm_file).expect("WASM file must exist");
    let mut validator = wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all());
    assert!(
        validator.validate_all(&wasm_bytes).is_ok(),
        "WASM binary containing overflow checks must be valid according to Wasm spec"
    );
}
