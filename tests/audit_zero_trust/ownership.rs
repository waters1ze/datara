use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use wasmparser::{Parser, Payload};

#[test]
fn audit_ownership_five_programs_classification() {
    let compiler = ForgenCompiler::new("release");

    // Program 1: Pure scalar -> 100% proven (0% guarded)
    let src_scalar = r#"
fn pure_scalar(a: Int, b: Int) -> Int {
    let x = a + b
    let y = x * 2
    return y
}
fn main() {
    let r = pure_scalar(10, 20)
    out r
}
"#;
    let res_scalar = compiler.compile_source(src_scalar, "audit_scalar.dtr", None);
    assert!(res_scalar.success, "Scalar program must compile");
    let graph = res_scalar.semantic_graph.expect("Semantic graph present");
    let opt = graph
        .inspect_optimization("pure_scalar")
        .expect("inspect_optimization must return facts");
    let opt_str = opt.to_string();
    assert!(
        opt_str.contains("0% guarded") || opt_str.contains("0.0% guarded"),
        "Pure scalar function must have 0% guarded, got:\n{}",
        opt_str
    );

    // Program 2: Conditional destroy + use -> known guarded ratio
    let src_cond = r#"
fn cond_destroy_use(flag: Int, v: Int) -> Int {
    if flag > 0 {
        destroy(v)
    }
    out v
    return v
}
fn main() {
    let r = cond_destroy_use(0, 42)
    out r
}
"#;
    let res_cond = compiler.compile_source(src_cond, "audit_cond.dtr", None);
    assert!(
        res_cond.success,
        "Conditional destroy must compile with runtime guard"
    );
    let graph_cond = res_cond.semantic_graph.expect("Semantic graph present");
    let opt_cond = graph_cond
        .inspect_optimization("cond_destroy_use")
        .expect("inspect_optimization on cond_destroy_use");
    let opt_cond_str = opt_cond.to_string();
    assert!(
        opt_cond_str.contains("guarded") && !opt_cond_str.contains("0% guarded"),
        "Conditional destroy + use must have >0% guarded ratio, got:\n{}",
        opt_cond_str
    );

    // Program 3: Definite use-after-move -> REJECTED across ALL three paths
    let src_uam = r#"
fn bad_uam() -> Int {
    let x = 100
    destroy(x)
    let y = x + 1
    return y
}
fn main() {
    let r = bad_uam()
    out r
}
"#;
    // (a) Default Cranelift path
    let res_uam_clif = compiler.compile_source(src_uam, "audit_uam_clif.dtr", None);
    assert!(
        !res_uam_clif.success,
        "Definite use-after-move MUST be rejected by default Cranelift compiler"
    );
    let err_clif = res_uam_clif.error.unwrap_or_default();
    assert!(
        err_clif.contains("E0301") || err_clif.to_lowercase().contains("move"),
        "Error must specify use-after-move (E0301): {}",
        err_clif
    );

    // (b) LLVM path
    let llvm_compiler = ForgenCompiler::new("release").with_llvm(true);
    let res_uam_llvm = llvm_compiler.compile_source(src_uam, "audit_uam_llvm.dtr", None);
    assert!(
        !res_uam_llvm.success,
        "Definite use-after-move MUST be rejected by LLVM path"
    );

    // (c) WASM target path
    let wasm_compiler =
        ForgenCompiler::new("release").with_target(Some("wasm32-unknown-unknown".to_string()));
    let res_uam_wasm = wasm_compiler.compile_source(src_uam, "audit_uam_wasm.dtr", None);
    assert!(
        !res_uam_wasm.success,
        "Definite use-after-move MUST be rejected on WASM target"
    );

    // Program 4: Loop-carried reinitialization -> Proven by fixpoint
    let src_loop = r#"
fn loop_reinit(n: Int) -> Int {
    mut i = 0
    mut sum = 0
    while i < n {
        mut item = 50 + i
        destroy(item)
        item = i * 2
        sum = sum + item
        i = i + 1
    }
    return sum
}
fn main() {
    let r = loop_reinit(5)
    out r
}
"#;
    let res_loop = compiler.compile_source(src_loop, "audit_loop.dtr", None);
    assert!(
        res_loop.success,
        "Loop with reinitialization must converge in fixpoint and compile: {:?}",
        res_loop.error
    );
    let graph_loop = res_loop.semantic_graph.expect("Semantic graph present");
    let opt_loop = graph_loop
        .inspect_optimization("loop_reinit")
        .expect("inspect_optimization for loop_reinit");
    let opt_loop_str = opt_loop.to_string();
    assert!(
        opt_loop_str.contains("0% guarded") || opt_loop_str.contains("0.0% guarded"),
        "Loop-carried reinit must be proven (0% guarded), got:\n{}",
        opt_loop_str
    );

    // Program 5: Partial move in branch + use -> Guarded and executes correctly
    let src_partial = r#"
fn partial_move(flag: Int, val: Int) -> Int {
    if flag == 1 {
        destroy(val)
    }
    return val
}
fn main() {
    let r = partial_move(0, 99)
    out r
}
"#;
    let res_partial = compiler.compile_source(src_partial, "audit_partial.dtr", None);
    assert!(
        res_partial.success,
        "Partial move in branch must compile with runtime guard"
    );
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res_partial.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "99",
        "Guarded execution must succeed when branch is not taken"
    );
}

#[test]
fn audit_ownership_guarded_path_across_all_three_backends() {
    let guarded_source = r#"
fn dynamic_guarded_fn(flag: Int, val: Int) -> Int {
    if flag > 0 {
        destroy(val)
    }
    return val
}
fn main() {
    let res = dynamic_guarded_fn(0, 777)
    out res
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(guarded_source, "audit_guarded_all_backends.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    // 1. CLIF inspection: must declare/call datara_rt_own_acquire / own_release
    let clif = res.clif_source.expect("CLIF source generated");
    assert!(
        clif.contains("datara_rt_own_acquire") || clif.contains("datara_rt_own_release"),
        "CLIF backend must contain ownership guard calls on guarded path:\n{}",
        clif
    );

    // 2. LLVM IR inspection: must declare/call @datara_rt_own_acquire / @datara_rt_own_release
    let llvm = res.llvm_source.expect("LLVM IR generated");
    assert!(
        llvm.contains("@datara_rt_own_acquire") || llvm.contains("@datara_rt_own_release"),
        "LLVM IR must contain @datara_rt_own_acquire / @datara_rt_own_release on guarded path:\n{}",
        llvm
    );

    // 3. WASM inspection: must import datara:rt/own_acquire and own_release
    let dmir = res.dmir_module.expect("DMIR module must be present");
    let temp_wasm = std::env::temp_dir().join("audit_guarded_test.wasm");
    let wasm_emit = WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm);
    assert!(
        wasm_emit.is_ok(),
        "WASM emission must succeed for guarded function"
    );

    let wasm_bytes = fs::read(&temp_wasm).expect("WASM file must exist");
    let parser = Parser::new(0);
    let mut imported_functions = Vec::new();

    for payload in parser.parse_all(&wasm_bytes) {
        if let Ok(Payload::ImportSection(import_sec)) = payload {
            for imp in import_sec.into_iter().flatten() {
                imported_functions.push((imp.module.to_string(), imp.name.to_string()));
            }
        }
    }

    println!(
        "WASM imported functions for guarded module: {:?}",
        imported_functions
    );

    let has_own_acquire = imported_functions
        .iter()
        .any(|(m, f)| m == "datara:rt" && f == "own_acquire");
    let has_own_release = imported_functions
        .iter()
        .any(|(m, f)| m == "datara:rt" && f == "own_release");

    assert!(
        has_own_acquire || has_own_release,
        "WASM binary MUST physically contain import of datara:rt own_acquire/own_release on guarded path! Found imports: {:?}",
        imported_functions
    );

    let _ = fs::remove_file(&temp_wasm);
}
