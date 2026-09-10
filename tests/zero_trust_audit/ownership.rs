use forgen::codegen::wasm::WasmEmitter;
use forgen::diagnostics::ErrorCode;
use forgen::driver::ForgenCompiler;
use std::fs;
use wasmparser::{Parser, Payload};

#[test]
fn audit_ownership_definite_use_after_move_rejected_all_backends() {
    let source = r#"
fn bad_uam() -> Int {
    let x = 42
    destroy(x)
    return x + 1
}
fn main() {
    out bad_uam()
}
"#;

    // (a) Cranelift
    let compiler_clif = ForgenCompiler::new("release");
    let res_clif = compiler_clif.compile_source(source, "bad_uam_clif.dtr", None);
    assert!(
        !res_clif.success,
        "Definite use-after-move MUST be rejected by Cranelift"
    );
    assert!(
        res_clif
            .diagnostics
            .contains(ErrorCode::BorrowUseAfterMove.as_str())
            || res_clif.error.as_deref().unwrap_or("").contains("moved"),
        "Diagnostic must state use-after-move"
    );

    // (b) LLVM
    let compiler_llvm = ForgenCompiler::new("release").with_llvm(true);
    let res_llvm = compiler_llvm.compile_source(source, "bad_uam_llvm.dtr", None);
    assert!(
        !res_llvm.success,
        "Definite use-after-move MUST be rejected by LLVM"
    );

    // (c) WASM target
    let compiler_wasm =
        ForgenCompiler::new("release").with_target(Some("wasm32-unknown-unknown".to_string()));
    let res_wasm = compiler_wasm.compile_source(source, "bad_uam_wasm.dtr", None);
    assert!(
        !res_wasm.success,
        "Definite use-after-move MUST be rejected on WASM target"
    );

    // (f) Verification that under NO compiler mode/flag definite move compiles to runnable code
    for mode in ["debug", "quick", "release", "domain"] {
        let comp = ForgenCompiler::new(mode);
        let res = comp.compile_source(source, "bad_uam_modes.dtr", None);
        assert!(
            !res.success,
            "Definite use-after-move MUST NEVER compile under mode '{}'",
            mode
        );
        assert!(
            res.exe_path.is_none(),
            "No executable can be produced for UAM"
        );
    }
}

#[test]
fn audit_ownership_two_live_mut_borrows_rejected() {
    let source = r#"
class Buffer {
    capacity: Int
}
fn main() {
    mut buf = Buffer { capacity: 1024 }
    let v1 = mut_view(buf)
    let v2 = mut_view(buf)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "two_mut_borrows.dtr", None);
    assert!(
        !res.success,
        "Two concurrent mutable views MUST be rejected with BorrowConflict/BorrowMultipleMutableViews"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains(ErrorCode::BorrowMultipleMutableViews.as_str())
            || diag.contains(ErrorCode::BorrowConflictActiveView.as_str())
            || diag.contains(ErrorCode::BorrowConflict.as_str()),
        "Diagnostic must indicate borrow conflict on multiple mutable views, got:\n{}",
        diag
    );
}

#[test]
fn audit_ownership_mut_borrow_plus_read_conflict() {
    let source = r#"
class DataBox {
    val: Int
}
fn main() {
    mut b = DataBox { val: 50 }
    let v = mut_view(b)
    out b.val
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "mut_borrow_read.dtr", None);
    assert!(
        !res.success,
        "Reading variable during active mutable borrow MUST be rejected"
    );
    let diag = res.diagnostics;
    assert!(
        diag.contains(ErrorCode::BorrowConflict.as_str())
            || diag.contains(ErrorCode::BorrowConflictActiveView.as_str())
            || diag.to_lowercase().contains("borrow"),
        "Diagnostic must report BorrowConflict on read during mutable borrow, got:\n{}",
        diag
    );
}

#[test]
fn audit_ownership_guarded_calls_in_clif_llvm_and_wasm() {
    let source = r#"
fn conditional_free(flag: Int, item: Int) -> Int {
    if flag > 0 {
        destroy(item)
    }
    return item
}
fn main() {
    let r = conditional_free(0, 123)
    out r
}
"#;
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "audit_guard_backends.dtr", None);
    assert!(
        res.success,
        "Guarded function must compile: {:?}",
        res.error
    );

    // 1. CLIF IR must contain runtime acquire/release calls
    let clif = res.clif_source.expect("CLIF generated");
    assert!(
        clif.contains("datara_rt_own_acquire") || clif.contains("datara_rt_own_release"),
        "CLIF IR must contain datara_rt_own_acquire / datara_rt_own_release, got:\n{}",
        clif
    );

    // 2. LLVM IR must contain @datara_rt_own_acquire / @datara_rt_own_release
    let llvm = res.llvm_source.expect("LLVM IR generated");
    assert!(
        llvm.contains("@datara_rt_own_acquire") || llvm.contains("@datara_rt_own_release"),
        "LLVM IR must contain datara_rt_own_acquire / datara_rt_own_release, got:\n{}",
        llvm
    );

    // 3. WASM import section must contain datara:rt own_acquire / own_release
    let dmir = res.dmir_module.expect("DMIR module");
    let temp_wasm = std::env::temp_dir().join("audit_guarded_wasm_import.wasm");
    WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm).expect("Wasm emit");
    let wasm_bytes = fs::read(&temp_wasm).expect("Wasm bytes");
    let parser = Parser::new(0);
    let mut imported = Vec::new();
    for payload in parser.parse_all(&wasm_bytes) {
        if let Ok(Payload::ImportSection(sec)) = payload {
            for imp in sec.into_iter().flatten() {
                imported.push((imp.module.to_string(), imp.name.to_string()));
            }
        }
    }
    let _ = fs::remove_file(&temp_wasm);

    let has_rt_own = imported
        .iter()
        .any(|(m, f)| m == "datara:rt" && (f == "own_acquire" || f == "own_release"));
    assert!(
        has_rt_own,
        "WASM binary physically MUST import datara:rt own_acquire or own_release! Found: {:?}",
        imported
    );
}

#[test]
fn audit_ownership_five_programs_exact_proven_guarded_percentages() {
    let compiler = ForgenCompiler::new("release");

    // 1. Pure scalar arithmetic -> 100% proven (0% guarded)
    let p1 = r#"
fn pure_calc(x: Int) -> Int {
    let a = x * 2
    let b = a + 10
    return b
}
fn main() { out pure_calc(5) }
"#;
    let res1 = compiler.compile_source(p1, "p1_scalar.dtr", None);
    assert!(res1.success);
    let opt1 = res1
        .semantic_graph
        .unwrap()
        .inspect_optimization("pure_calc")
        .unwrap()
        .to_string();
    assert!(
        opt1.contains("0% guarded") || opt1.contains("0.0% guarded"),
        "P1 must be 0% guarded: {}",
        opt1
    );

    // 2. Conditional destroy -> Guarded (>0% guarded)
    let p2 = r#"
fn cond_destroy(f: Int, val: Int) -> Int {
    if f > 0 {
        destroy(val)
    }
    return val
}
fn main() { out cond_destroy(0, 10) }
"#;
    let res2 = compiler.compile_source(p2, "p2_guarded.dtr", None);
    assert!(res2.success);
    let opt2 = res2
        .semantic_graph
        .unwrap()
        .inspect_optimization("cond_destroy")
        .unwrap()
        .to_string();
    assert!(
        opt2.contains("guarded") && !opt2.contains("0% guarded") && !opt2.contains("0.0% guarded"),
        "P2 must have guarded ops: {}",
        opt2
    );

    // 3. Loop with reinitialization -> Proven by fixpoint (0% guarded)
    let p3 = r#"
fn loop_proven(n: Int) -> Int {
    mut i = 0
    mut sum = 0
    while i < n {
        mut item = i * 2
        destroy(item)
        item = 10
        sum = sum + item
        i = i + 1
    }
    return sum
}
fn main() { out loop_proven(3) }
"#;
    let res3 = compiler.compile_source(p3, "p3_loop.dtr", None);
    assert!(res3.success);
    let opt3 = res3
        .semantic_graph
        .unwrap()
        .inspect_optimization("loop_proven")
        .unwrap()
        .to_string();
    assert!(
        opt3.contains("0% guarded") || opt3.contains("0.0% guarded"),
        "P3 loop must be proven (0% guarded): {}",
        opt3
    );

    // 4. Branch with conditional consume in one path -> Guarded
    let p4 = r#"
fn branch_consume(c: Int, v: Int) -> Int {
    if c == 1 {
        destroy(v)
    }
    out v
    return v
}
fn main() { out branch_consume(0, 77) }
"#;
    let res4 = compiler.compile_source(p4, "p4_branch.dtr", None);
    assert!(res4.success);
    let opt4 = res4
        .semantic_graph
        .unwrap()
        .inspect_optimization("branch_consume")
        .unwrap()
        .to_string();
    assert!(
        opt4.contains("guarded") && !opt4.contains("0% guarded") && !opt4.contains("0.0% guarded"),
        "P4 must be guarded: {}",
        opt4
    );

    // 5. Linear flow with immutable view -> 100% proven (0% guarded)
    let p5 = r#"
class Item {
    num: Int
}
fn view_proven(it: Item) -> Int {
    let v = view(it)
    return it.num
}
fn main() {
    let it = Item { num: 99 }
    out view_proven(it)
}
"#;
    let res5 = compiler.compile_source(p5, "p5_view.dtr", None);
    assert!(res5.success);
    let opt5 = res5
        .semantic_graph
        .unwrap()
        .inspect_optimization("view_proven")
        .unwrap()
        .to_string();
    assert!(
        opt5.contains("0% guarded") || opt5.contains("0.0% guarded"),
        "P5 view must be proven (0% guarded): {}",
        opt5
    );
}
