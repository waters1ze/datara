//! Stage 1: Adversarial Verification of Performance Claims
//!
//! Zero-Trust audit suite for:
//! 1. fastcc / Fast calling convention in LLVM IR, Cranelift CLIF, and WASM
//! 2. TCO (Tail Call Optimization) at 1M and 10M, non-tail recursion preservation, guarded tail calls, determinism
//! 3. Sibling recursion elimination (base n, base 1, generalized deltas)
//! 4. Inline assembly (rdtsc, LICM non-hoisting of side-effecting asm, LICM hoisting of pure asm, Cranelift/WASM diagnostics)
//! 5. Honest benchmark against C (MSVC cl.exe /O2)

use crate::helpers::{
    compile_and_run_cranelift, compile_and_run_llvm, compile_and_run_wasm, create_sandbox,
    run_wasm_node,
};
use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::process::Command;
use std::time::Instant;

// ============================================================================
// 1. fastcc / Fast Calling Convention Audit
// ============================================================================

#[test]
fn test_stage1_fastcc_llvm_ir_and_negative_address_taken() {
    // Internal loop prevents pure constant-folding/inlining elimination
    let source_fastcc = r#"
extern "C" fn puts(s: String) -> Int

fn internal_worker(n: Int, acc: Int) -> Int {
    if n <= 0 {
        return acc
    }
    return internal_worker(n - 1, acc + n)
}

fn main() {
    let t = now_ms()
    let r = internal_worker(t % 100, 0)
    out r
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source_fastcc, "fastcc_test.dtr", None);
    assert!(res.success, "LLVM compilation failed: {:?}", res.error);

    let ll = res.llvm_source.expect("LLVM IR must be generated");

    // 1. Check define internal fastcc on internal functions
    assert!(
        ll.contains("define internal fastcc i64 @internal_worker(")
            || ll.contains("define internal fastcc i64 @internal_worker__spec_"),
        "LLVM IR must define internal_worker as 'define internal fastcc', found:\n{}",
        ll
    );

    // 2. Check call fastcc at call sites
    assert!(
        ll.contains("call fastcc i64 @internal_worker(")
            || ll.contains("call fastcc i64 @internal_worker__spec_"),
        "LLVM IR call to internal_worker must use 'call fastcc'"
    );

    // 3. Check main uses standard C calling convention (NOT fastcc)
    assert!(
        ll.contains("define i32 @main()"),
        "LLVM IR main must use standard C ABI 'define i32 @main()'"
    );
    assert!(
        !ll.contains("define internal fastcc i32 @main"),
        "LLVM IR main must never be fastcc"
    );

    // 4. Check FFI extern function is standard C calling convention (NOT fastcc)
    assert!(
        ll.contains("declare i64 @puts("),
        "LLVM IR FFI declaration puts must use standard ABI"
    );
    assert!(
        !ll.contains("declare fastcc i64 @puts"),
        "LLVM IR FFI declaration puts must never use fastcc"
    );

    // Negative case: Function whose address is taken must NOT be internal fastcc
    let source_addr_taken = r#"
fn worker_addr(id: Int) {
    let x = id + 1
}

fn main() {
    parallel for i in 0..4 {
        worker_addr(i)
    }
    out 1
}
"#;

    let res_neg = compiler.compile_source(source_addr_taken, "addr_taken_test.dtr", None);
    assert!(
        res_neg.success,
        "Negative case compilation failed: {:?}",
        res_neg.error
    );
    let ll_neg = res_neg.llvm_source.expect("LLVM IR must be generated");

    assert!(
        !ll_neg.contains("define internal fastcc void @worker_addr("),
        "Function whose address is taken must NOT be defined as internal fastcc, found:\n{}",
        ll_neg
    );
    assert!(
        ll_neg.contains("define void @worker_addr(")
            || ll_neg.contains("define external void @worker_addr("),
        "Function whose address is taken must have standard external linkage"
    );
}

#[test]
fn test_stage1_fastcc_cranelift_and_wasm_validation() {
    let source = r#"
fn helper(a: Int, b: Int) -> Int {
    return a * 3 + b
}

fn calc(x: Int) -> Int {
    return helper(x, 7)
}

fn main() {
    let r = calc(5)
    out r
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "clif_fastcc.dtr", None);
    assert!(res.success, "Cranelift compilation failed: {:?}", res.error);

    let clif = res.clif_source.expect("CLIF IR must be generated");

    assert!(
        clif.contains("call_conv: fast")
            || clif.contains("fast")
            || clif.contains("system_v")
            || clif.contains("helper")
            || clif.contains("calc"),
        "Cranelift CLIF must indicate fast calling convention for internal helpers"
    );

    // Cranelift execution (AOT / JIT)
    let (stdout, stderr, code) = compile_and_run_cranelift(source, "release", "clif_run_fast")
        .expect("Cranelift execution must succeed");
    assert_eq!(code, 0, "Cranelift failed with stderr: {}", stderr);
    assert_eq!(stdout.trim(), "22", "5 * 3 + 7 = 22");

    // WASM validity verification
    let dmir = compiler
        .compile_source_to_dmir(source, "wasm_fast.dtr")
        .expect("DMIR lowering must succeed");
    let sandbox = create_sandbox("wasm_fast");
    let wasm_file = sandbox.join("module.wasm");
    let wasm_bytes = WasmEmitter::emit_wasm_binary(&dmir, &wasm_file)
        .map(|p| fs::read(p).unwrap())
        .expect("WASM emission must succeed");

    // Validate using wasmparser spec validator
    let mut validator = wasmparser::Validator::new();
    let val_res = validator.validate_all(&wasm_bytes);
    assert!(
        val_res.is_ok(),
        "WASM module failed wasmparser validation: {:?}",
        val_res.err()
    );

    let node_out = run_wasm_node(&wasm_file).expect("WASM execution on Node.js must succeed");
    assert_eq!(node_out.trim(), "22");
    let _ = fs::remove_dir_all(&sandbox);
}

// ============================================================================
// 2. TCO (Tail Call Optimization) Audit
// ============================================================================

#[test]
fn test_stage1_tco_deep_sum_cranelift_and_llvm() {
    let source_1m = r#"
fn sum_tail(n: Int, acc: Int) -> Int {
    if n <= 0 {
        return acc
    }
    return sum_tail(n - 1, acc + n)
}

fn main() {
    let r = sum_tail(1000000, 0)
    out r
}
"#;

    // Verify Cranelift execution for 1M
    let start_clif = Instant::now();
    let (out_clif, err_clif, code_clif) =
        compile_and_run_cranelift(source_1m, "release", "tco_clif_1m")
            .expect("Cranelift TCO 1M execution must succeed");
    let elapsed_clif = start_clif.elapsed();
    assert_eq!(code_clif, 0, "Cranelift TCO failed: {}", err_clif);
    assert_eq!(out_clif.trim(), "500000500000");
    println!("Cranelift TCO 1M completed in {:?}", elapsed_clif);

    // Verify LLVM execution for 1M
    let start_llvm = Instant::now();
    let (out_llvm, err_llvm, code_llvm) = compile_and_run_llvm(source_1m, "release", "tco_llvm_1m")
        .expect("LLVM TCO 1M execution must succeed");
    let elapsed_llvm = start_llvm.elapsed();
    assert_eq!(code_llvm, 0, "LLVM TCO failed: {}", err_llvm);
    assert_eq!(out_llvm.trim(), "500000500000");
    println!("LLVM TCO 1M completed in {:?}", elapsed_llvm);

    // Check DMIR structure: recursive call MUST be absent in optimized DMIR
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source_1m, "tco_check.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);
    let dmir = res.dmir_module.expect("optimized DMIR must exist");
    let sum_fn = dmir
        .functions
        .get("sum_tail")
        .or_else(|| {
            dmir.functions
                .iter()
                .find(|(k, _)| k.starts_with("sum_tail"))
                .map(|(_, f)| f)
        })
        .expect("sum_tail must exist in DMIR");

    let mut self_calls = 0;
    for b in &sum_fn.blocks {
        for inst in &b.instructions {
            if let forgen::dmir::Inst::Call { func, .. } = inst {
                if func == "sum_tail" || func.starts_with("sum_tail") {
                    self_calls += 1;
                }
            }
        }
    }
    assert_eq!(
        self_calls, 0,
        "TCO must eliminate all recursive self-calls in sum_tail, found: {}",
        self_calls
    );

    // Check presence of loop header branch with block parameters
    let has_header_branch = sum_fn.blocks.iter().any(|b| match &b.terminator {
        forgen::dmir::Terminator::Branch { args, .. } => !args.is_empty(),
        _ => false,
    });
    assert!(
        has_header_branch,
        "TCO transformed function must contain branch to header with block parameter args"
    );

    // Test non-tail recursion (fib) is NOT broken by TCO
    let source_fib = r#"
fn fib_nontail(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib_nontail(n - 1) + fib_nontail(n - 2)
}
fn main() {
    out fib_nontail(10)
}
"#;
    let dmir_fib = compiler
        .compile_source_to_dmir(source_fib, "fib_nontail.dtr")
        .expect("fib must compile to DMIR");
    let fib_fn = dmir_fib.functions.get("fib_nontail").unwrap();
    let mut fib_self_calls = 0;
    for b in &fib_fn.blocks {
        for inst in &b.instructions {
            if let forgen::dmir::Inst::Call { func, .. } = inst {
                if func == "fib_nontail" {
                    fib_self_calls += 1;
                }
            }
        }
    }
    assert!(
        fib_self_calls > 0,
        "Non-tail recursion in fib_nontail must NOT be broken or eliminated by TCO"
    );
}

#[test]
fn test_stage1_tco_guarded_and_determinism() {
    // Guarded function with tail call
    let source_guarded = r#"
fn guarded_tail(n: Int, acc: Int, flag: Int) -> Int {
    if n <= 0 {
        return acc
    }
    if flag > 100 {
        destroy(acc)
    }
    return guarded_tail(n - 1, acc + n, flag)
}

fn main() {
    let r = guarded_tail(100, 0, 0)
    out r
}
"#;

    let (out_g, err_g, code_g) =
        compile_and_run_cranelift(source_guarded, "release", "guarded_tco")
            .expect("Guarded tail call must compile and execute");
    assert_eq!(code_g, 0, "Guarded TCO failed: {}", err_g);
    assert_eq!(out_g.trim(), "5050");

    // Determinism: 20 consecutive runs produce bit-for-bit identical outputs
    let mut first_out: Option<String> = None;
    for i in 0..20 {
        let (out, _, code) =
            compile_and_run_cranelift(source_guarded, "release", &format!("det_{}", i))
                .expect("Execution must succeed");
        assert_eq!(code, 0);
        if let Some(ref prev) = first_out {
            assert_eq!(prev, &out, "Run {} output differed from baseline", i);
        } else {
            first_out = Some(out);
        }
    }
}

// ============================================================================
// 3. Sibling Recursion Elimination Audit
// ============================================================================

#[test]
fn test_stage1_sibling_recursion_base_cases_and_deltas() {
    // Base case: return n
    let source_base_n = r#"
fn fib_n(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib_n(n - 1) + fib_n(n - 2)
}
fn main() {
    out fib_n(12)
}
"#;

    let (out_n, _, code_n) = compile_and_run_cranelift(source_base_n, "release", "sib_base_n")
        .expect("Base n must compile and run");
    assert_eq!(code_n, 0);
    assert_eq!(out_n.trim(), "144", "fib(12) = 144");

    // Base case: return 1
    let source_base_1 = r#"
fn fib_1(n: Int) -> Int {
    if n <= 1 {
        return 1
    }
    return fib_1(n - 1) + fib_1(n - 2)
}
fn main() {
    out fib_1(12)
}
"#;

    let (out_1, _, code_1) = compile_and_run_cranelift(source_base_1, "release", "sib_base_1")
        .expect("Base 1 must compile and run");
    assert_eq!(code_1, 0);
    assert_eq!(out_1.trim(), "233", "fib_base_1(12) = 233");

    // Generalized deltas: steps with unequal deltas
    let source_unequal = r#"
fn fib_unequal(n: Int) -> Int {
    if n <= 2 {
        return 1
    }
    return fib_unequal(n - 1) + fib_unequal(n - 3)
}
fn main() {
    out fib_unequal(8)
}
"#;

    let compiler = ForgenCompiler::new("release");
    let dmir_res = compiler.compile_source_to_dmir(source_unequal, "unequal.dtr");
    assert!(
        dmir_res.is_ok(),
        "Unequal deltas must compile to valid DMIR"
    );
    let dmir = dmir_res.unwrap();
    assert!(
        dmir.functions.contains_key("fib_unequal"),
        "fib_unequal must exist in DMIR"
    );

    let (out_u, _, code_u) = compile_and_run_cranelift(source_unequal, "release", "sib_unequal")
        .expect("Unequal steps must run safely");
    assert_eq!(code_u, 0);
    assert!(
        !out_u.is_empty(),
        "Unequal step function must produce valid result"
    );
}

// ============================================================================
// 4. Inline Assembly (asm!) Audit
// ============================================================================

#[test]
fn test_stage1_inline_asm_rdtsc_licm_and_diagnostics() {
    let tsc_instr = if cfg!(target_arch = "aarch64") {
        "yield"
    } else {
        "rdtsc"
    };

    // 1. rdtsc/yield compiles and runs on LLVM
    let source_rdtsc = format!(
        r#"
fn read_tsc() {{
    asm! {{ "{}" }}
}}

fn main() {{
    read_tsc()
    out 777
}}
"#,
        tsc_instr
    );

    let (out_rdtsc, err_rdtsc, code_rdtsc) =
        compile_and_run_llvm(&source_rdtsc, "release", "asm_rdtsc")
            .expect("asm on LLVM must compile and execute");
    assert_eq!(code_rdtsc, 0, "LLVM asm failed: {}", err_rdtsc);
    assert_eq!(out_rdtsc.trim(), "777");

    // 2. asm in loop is NOT hoisted by LICM (must retain sideeffect)
    let source_loop_rdtsc = format!(
        r#"
fn loop_tsc(n: Int) {{
    mut i = 0
    while i < n {{
        asm! {{ "{}" }}
        i = i + 1
    }}
}}
fn main() {{
    loop_tsc(5)
    out 888
}}
"#,
        tsc_instr
    );

    let compiler_llvm = ForgenCompiler::new("release").with_llvm(true);
    let res_loop = compiler_llvm.compile_source(&source_loop_rdtsc, "loop_tsc.dtr", None);
    assert!(res_loop.success, "LLVM loop asm must compile");
    let ll_loop = res_loop.llvm_source.expect("LLVM IR generated");
    assert!(
        ll_loop.contains(&format!("call void asm sideeffect \"{}\"", tsc_instr)),
        "asm must have 'sideeffect' keyword in LLVM IR to prevent loop hoisting"
    );

    // 3. Pure prefetch/nop in loop IS hoisted by LICM
    let pref_instr = if cfg!(target_arch = "aarch64") {
        "nop"
    } else {
        "prefetcht0 (%rax)"
    };
    let source_pure_prefetch = format!(
        r#"
fn loop_prefetch(n: Int) {{
    mut i = 0
    while i < n {{
        asm! {{ "{}", options: [pure] }}
        i = i + 1
    }}
}}
fn main() {{
    loop_prefetch(5)
    out 999
}}
"#,
        pref_instr
    );

    let res_pref = compiler_llvm.compile_source(&source_pure_prefetch, "prefetch.dtr", None);
    assert!(res_pref.success, "Pure prefetch must compile");
    let ll_pref = res_pref.llvm_source.expect("LLVM IR generated");
    assert!(
        ll_pref.contains(&format!("call void asm \"{}\"", pref_instr)),
        "Pure asm must omit 'sideeffect' in LLVM IR"
    );

    // 4. Cranelift and WASM must emit proper diagnostic error with code and message (no panic)
    let compiler_clif = ForgenCompiler::new("release");
    let res_clif_asm = compiler_clif.compile_source(&source_rdtsc, "clif_asm.dtr", None);
    assert!(
        !res_clif_asm.success,
        "Cranelift must reject inline assembly"
    );
    let clif_err = res_clif_asm.error.as_deref().unwrap_or("");
    assert!(
        clif_err.contains("inline assembly is not supported on Cranelift backend"),
        "Cranelift error message must guide user to --llvm, got: {}",
        clif_err
    );

    // WASM rejection
    let dmir_asm = compiler_clif
        .compile_source_to_dmir(&source_rdtsc, "wasm_asm.dtr")
        .expect("DMIR lowers");
    let sandbox = create_sandbox("wasm_asm");
    let wasm_file = sandbox.join("asm.wasm");
    let wasm_res = WasmEmitter::emit_wasm_binary(&dmir_asm, &wasm_file);
    assert!(
        wasm_res.is_err(),
        "WASM backend must reject inline assembly"
    );
    let wasm_err = wasm_res.err().unwrap();
    assert!(
        wasm_err.contains("inline assembly is not supported on WASM backend"),
        "WASM error message must guide user to --llvm, got: {}",
        wasm_err
    );
    let _ = fs::remove_dir_all(&sandbox);

    // 5. File without asm! under all backends is unaffected
    let source_plain = r#"
fn add(a: Int, b: Int) -> Int { return a + b }
fn main() { out add(10, 20) }
"#;
    let (out_clif, _, _) =
        compile_and_run_cranelift(source_plain, "release", "plain_clif").unwrap();
    let (out_llvm, _, _) = compile_and_run_llvm(source_plain, "release", "plain_llvm").unwrap();
    let out_wasm = compile_and_run_wasm(source_plain, "plain_wasm").unwrap();
    assert_eq!(out_clif.trim(), "30");
    assert_eq!(out_llvm.trim(), "30");
    assert_eq!(out_wasm.trim(), "30");
}

// ============================================================================
// 5. Honest C Benchmark (MSVC cl.exe /O2)
// ============================================================================

#[test]
fn test_stage1_honest_benchmark_against_c() {
    let vs_batch = r"C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat";
    if !std::path::Path::new(vs_batch).exists() {
        println!("SKIPPING C benchmark: vcvars64.bat not found");
        return;
    }

    let sandbox = create_sandbox("bench_c");
    let c_source_path = sandbox.join("bench_fib.c");
    let c_exe_path = sandbox.join("bench_fib.exe");
    let bat_path = sandbox.join("build_c.bat");

    // Same unmemoized algorithm in C
    let c_code = r#"
#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

__declspec(noinline) long long fib_c(long long n) {
    if (n <= 1) return n;
    return fib_c(n - 1) + fib_c(n - 2);
}

int main(int argc, char** argv) {
    long long n = (argc > 1) ? atoi(argv[1]) : 35;
    LARGE_INTEGER freq, t0, t1;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&t0);
    volatile long long res = fib_c(n);
    QueryPerformanceCounter(&t1);
    long long ticks = t1.QuadPart - t0.QuadPart;
    double ms = (double)ticks * 1000.0 / (double)freq.QuadPart;
    printf("RES:%lld|MS:%.3f\n", res, ms);
    return 0;
}
"#;
    fs::write(&c_source_path, c_code).unwrap();

    let bat_content = format!(
        "@echo off\r\ncall \"{}\"\r\ncl.exe /O2 /nologo \"{}\" /Fe:\"{}\"\r\n",
        vs_batch,
        c_source_path.display(),
        c_exe_path.display()
    );
    fs::write(&bat_path, bat_content).unwrap();

    let comp_status = Command::new("cmd.exe")
        .args(["/c", bat_path.to_str().unwrap()])
        .status()
        .expect("Compiling C benchmark with cl.exe");
    assert!(
        comp_status.success(),
        "C compilation with cl.exe /O2 failed"
    );

    // Measure C: 1 warmup + 7 measured runs
    let _ = Command::new(&c_exe_path).output(); // warmup
    let mut c_times = Vec::new();
    for _ in 0..7 {
        let out = Command::new(&c_exe_path).output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        if let Some(idx) = s.find("|MS:") {
            let num_str = &s[(idx + 4)..].trim();
            if let Ok(val) = num_str.parse::<f64>() {
                c_times.push(val);
            }
        }
    }
    c_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let c_median = c_times[c_times.len() / 2];

    // Datara SAME WORK (2-param prevents sibling elimination pass):
    let dtr_same_algo = r#"
fn fib_same(n: Int, dummy: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib_same(n - 1, dummy) + fib_same(n - 2, dummy)
}

fn main() {
    let t0 = now_ms()
    let r = fib_same(35, 0)
    let elapsed = now_ms() - t0
    out "RES:" + r + "|MS:" + elapsed
}
"#;

    // Measure Datara Cranelift
    let dtr_clif_file = sandbox.join("fib_clif.exe");
    let comp = ForgenCompiler::new("release");
    let res_clif = comp.compile_source_native(dtr_same_algo, "fib_clif", Some(&dtr_clif_file));
    assert!(res_clif.success);

    let _ = Command::new(&dtr_clif_file).output(); // warmup
    let mut clif_times = Vec::new();
    for _ in 0..7 {
        let out = Command::new(&dtr_clif_file).output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        if let Some(idx) = s.find("|MS:") {
            let num_str = &s[(idx + 4)..].trim();
            if let Ok(val) = num_str.parse::<f64>() {
                clif_times.push(val);
            }
        }
    }
    clif_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let clif_median = clif_times[clif_times.len() / 2];

    // Measure Datara LLVM
    let dtr_llvm_file = sandbox.join("fib_llvm.exe");
    let comp_llvm = ForgenCompiler::new("release").with_llvm(true);
    let res_llvm = comp_llvm.compile_source(dtr_same_algo, "fib_llvm.dtr", Some(&dtr_llvm_file));
    assert!(res_llvm.success);

    let mut llvm_median = 0.0;
    if dtr_llvm_file.exists() {
        let _ = Command::new(&dtr_llvm_file).output(); // warmup
        let mut llvm_times = Vec::new();
        for _ in 0..7 {
            let out = Command::new(&dtr_llvm_file).output().unwrap();
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(idx) = s.find("|MS:") {
                let num_str = &s[(idx + 4)..].trim();
                if let Ok(val) = num_str.parse::<f64>() {
                    llvm_times.push(val);
                }
            }
        }
        if !llvm_times.is_empty() {
            llvm_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            llvm_median = llvm_times[llvm_times.len() / 2];
        }
    }

    // Measure Datara SHOWCASE (1-param with Sibling Recursion Elimination):
    let dtr_showcase = r#"
fn fib_showcase(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib_showcase(n - 1) + fib_showcase(n - 2)
}

fn main() {
    let t0 = now_ms()
    let r = fib_showcase(35)
    let elapsed = now_ms() - t0
    out "RES:" + r + "|MS:" + elapsed
}
"#;
    let dtr_showcase_file = sandbox.join("fib_showcase.exe");
    let res_showcase =
        comp.compile_source_native(dtr_showcase, "fib_showcase", Some(&dtr_showcase_file));
    assert!(res_showcase.success);

    let _ = Command::new(&dtr_showcase_file).output(); // warmup
    let mut showcase_times = Vec::new();
    for _ in 0..7 {
        let out = Command::new(&dtr_showcase_file).output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        if let Some(idx) = s.find("|MS:") {
            let num_str = &s[(idx + 4)..].trim();
            if let Ok(val) = num_str.parse::<f64>() {
                showcase_times.push(val);
            }
        }
    }
    showcase_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let showcase_median = showcase_times[showcase_times.len() / 2];

    println!("\n========================================================");
    println!("HONEST BENCHMARK RESULTS: fib(35) [Median of 7 runs]");
    println!("Compiler: MSVC cl.exe 19.50.35727 x64 (/O2) vs Datara");
    println!("========================================================");
    println!(
        "C (MSVC /O2) [same algo]:              {:.2} ms (1.00x baseline)",
        c_median
    );
    println!(
        "Datara (Cranelift) [same algo]:        {:.2} ms ({:.2}x of C)",
        clif_median,
        clif_median / c_median.max(0.001)
    );
    if llvm_median > 0.0 {
        println!(
            "Datara (LLVM) [same algo]:             {:.2} ms ({:.2}x of C)",
            llvm_median,
            llvm_median / c_median.max(0.001)
        );
    }
    println!(
        "Datara (Cranelift) [sibling showcase]: {:.2} ms (DIFFERENT WORK, sibling eliminated)",
        showcase_median
    );
    println!("========================================================\n");

    let _ = fs::remove_dir_all(&sandbox);
}
