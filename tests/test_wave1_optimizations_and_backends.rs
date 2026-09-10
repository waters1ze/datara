use forgen::driver::ForgenCompiler;

#[test]
fn test_wave1_tail_call_optimization() {
    let source = r#"
fn sum(n: Int, acc: Int) -> Int {
    if n <= 0 {
        return acc
    }
    return sum(n - 1, acc + n)
}

fn main() {
    let res = sum(100, 0)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "tco_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let trace = res
        .optimization_report
        .as_ref()
        .map(|r| &r.decision_trace)
        .expect("Optimization report must exist");

    let tco_records: Vec<_> = trace
        .iter()
        .filter(|d| d.pass == "TailRecursionElimination" && d.decision == "Applied")
        .collect();
    assert!(
        !tco_records.is_empty(),
        "TailRecursionElimination pass must be applied for tail-recursive sum"
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "5050");
}

#[test]
fn test_wave1_tail_call_multiple_accumulators() {
    let source = r#"
fn tail_multi(n: Int, a: Int, b: Int) -> Int {
    if n <= 0 {
        return a + b
    }
    return tail_multi(n - 1, a + 1, b + 2)
}

fn main() {
    let res = tail_multi(10, 0, 0)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "tco_multi_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "30");
}

#[test]
fn test_wave1_sibling_recursion_constant_base() {
    let source = r#"
fn fib_const(n: Int) -> Int {
    if n <= 1 {
        return 1
    }
    return fib_const(n - 1) + fib_const(n - 2)
}

fn main() {
    let res = fib_const(6)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "sibling_const_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "13");
}

#[test]
fn test_wave1_llvm_fastcc_and_internal_linkage() {
    let source = r#"
fn helper_calc(a: Int, b: Int) -> Int {
    return a * b + 42
}

fn main() {
    let ans = helper_calc(10, 5)
    out ans
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "llvm_fastcc_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let llvm = res.llvm_source.expect("LLVM IR must be generated");

    assert!(
        llvm.contains("define internal fastcc i64 @helper_calc(")
            || llvm.contains("define internal fastcc"),
        "helper_calc must use define internal fastcc: {}",
        llvm
    );
    assert!(
        llvm.contains("call fastcc i64 @helper_calc(") || llvm.contains("call fastcc"),
        "Call site for helper_calc must use call fastcc: {}",
        llvm
    );

    assert!(
        llvm.contains("define i32 @main()"),
        "main must use standard define i32 @main(): {}",
        llvm
    );
}

#[test]
fn test_wave1_inline_asm_llvm() {
    let pause_instr = if cfg!(target_arch = "aarch64") {
        "yield"
    } else {
        "pause"
    };
    let source = format!(
        r#"
fn run_asm() {{
    asm! {{
        "nop",
        options: [pure]
    }}
    asm! {{
        "{}"
    }}
}}

fn main() {{
    run_asm()
    out 1
}}
"#,
        pause_instr
    );

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(&source, "inline_asm_llvm.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let llvm = res.llvm_source.expect("LLVM IR must be generated");

    assert!(
        llvm.contains("call void asm \"nop\"") || llvm.contains("asm \"nop\""),
        "Pure asm must not emit sideeffect: {}",
        llvm
    );
    assert!(
        llvm.contains(&format!("sideeffect \"{}\"", pause_instr)),
        "Default asm must emit sideeffect: {}",
        llvm
    );
}

#[test]
fn test_wave1_inline_asm_diagnostics() {
    let source = r#"
fn run_asm() {
    asm! {
        "nop"
    }
}

fn main() {
    run_asm()
}
"#;

    let clif_compiler = ForgenCompiler::new("quick");
    let clif_res = clif_compiler.compile_source(source, "clif_asm.dtr", None);
    assert!(!clif_res.success, "Cranelift must reject inline assembly");
    let clif_err = clif_res.error.unwrap_or_default();
    assert!(
        clif_err.contains("--llvm"),
        "Cranelift error must mention --llvm, got: {}",
        clif_err
    );

    let dmir = clif_compiler
        .compile_source_to_dmir(source, "wasm_asm.dtr")
        .expect("DMIR compilation should succeed");
    let temp_wasm = std::env::temp_dir().join("test_wasm_asm.wasm");
    let wasm_res = forgen::codegen::wasm::WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm);
    assert!(wasm_res.is_err(), "WASM must reject inline assembly");
    let wasm_err = wasm_res.unwrap_err();
    assert!(
        wasm_err.contains("--llvm"),
        "WASM error must mention --llvm, got: {}",
        wasm_err
    );
}
