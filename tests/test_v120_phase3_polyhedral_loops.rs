//! Phase 3: Polyhedral Loop Engine & Advanced Array Transformations
//!
//! Verifies:
//! 1. Fused Multiply-Add (FMA) pattern matching & lowering.
//! 2. Whole-Array loop fusion (eliminates intermediate buffer allocations).
//! 3. Polyhedral stencil wavefront skewing (t, i) -> (t, i + 2*t).
//! 4. Affine independence analysis and LLVM loop vectorization metadata.
//! 5. Differential consistency between Cranelift and JIT/LLVM backends.

use forgen::driver::ForgenCompiler;

#[test]
fn test_v120_polyhedral_fma_optimization() {
    let source = r#"
fn compute_fma(a: Float, b: Float, c: Float) -> Float {
    let prod = a * b
    let res = prod + c
    return res
}

fn main() {
    let r = compute_fma(3.0, 4.0, 5.0)
    out r
}
"#;

    // 1. Verify compilation and decision trace in optimization report
    let compiler = ForgenCompiler::new("domain").with_llvm(true);
    let res = compiler.compile_source(source, "poly_fma.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let report = res
        .optimization_report
        .expect("Optimization report must be present");
    let has_fma_record = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PolyhedralFMA" && r.decision == "Applied");
    assert!(
        has_fma_record,
        "Decision trace must record PolyhedralFMA Applied"
    );

    // 2. Verify compilation & execution on JIT / LLVM
    let base_temp = std::env::temp_dir().join("datara_polyhedral_fma");
    let _ = std::fs::create_dir_all(&base_temp);
    let jit_compiler = ForgenCompiler::new("jit");
    let jit_exe = base_temp.join("poly_fma_jit.exe");
    let jit_res = jit_compiler.compile_source(source, "poly_fma_jit.dtr", Some(&jit_exe));
    assert!(
        jit_res.success,
        "JIT compilation failed: {:?}",
        jit_res.error
    );
    let output_jit = std::process::Command::new(&jit_exe)
        .output()
        .expect("run jit exe");
    assert_eq!(output_jit.status.code(), Some(0));
    let out_val = String::from_utf8_lossy(&output_jit.stdout)
        .trim()
        .to_string();
    assert_eq!(out_val, "17", "FMA: (3.0 * 4.0) + 5.0 = 17");
}

#[test]
fn test_v120_polyhedral_loop_fusion() {
    let source = r#"
fn array_ops() -> Int {
    mut a = [1, 2, 3, 4]
    mut b = [10, 20, 30, 40]
    mut i = 0
    while i < 4 {
        a[i] = a[i] * 2
        i = i + 1
    }
    mut j = 0
    while j < 4 {
        b[j] = b[j] + a[j]
        j = j + 1
    }
    return b[3]
}

fn main() {
    let r = array_ops()
    out r
}
"#;

    let compiler = ForgenCompiler::new("domain").with_llvm(true);
    let res = compiler.compile_source(source, "poly_fusion.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let report = res
        .optimization_report
        .expect("Optimization report must be present");
    let has_fusion_record = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PolyhedralLoopFusion" && r.decision == "Applied");
    assert!(
        has_fusion_record,
        "Decision trace must record PolyhedralLoopFusion Applied"
    );
}

#[test]
fn test_v120_polyhedral_stencil_wavefront_skewing() {
    let source = r#"
fn jacobi_stencil_2d() -> Int {
    mut t = 0
    mut acc = 0
    while t < 10 {
        mut i = 0
        while i < 10 {
            acc = acc + 1
            i = i + 1
        }
        t = t + 1
    }
    return acc
}

fn main() {
    let r = jacobi_stencil_2d()
    out r
}
"#;

    let compiler = ForgenCompiler::new("domain").with_llvm(true);
    let res = compiler.compile_source(source, "poly_stencil.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let report = res
        .optimization_report
        .expect("Optimization report must be present");
    let has_skew_record = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PolyhedralWavefrontSkewing" && r.decision == "Applied");
    assert!(
        has_skew_record,
        "Decision trace must record PolyhedralWavefrontSkewing Applied"
    );
}

#[test]
fn test_v120_polyhedral_vectorize_metadata() {
    let source = r#"
fn independent_loop(arr: List<Int>, n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + arr[i]
        i = i + 1
    }
    return sum
}

fn main() {
    val arr = [1, 2, 3, 4, 5]
    val s = independent_loop(arr, 5)
    out s
}
"#;

    let compiler = ForgenCompiler::new("domain").with_llvm(true);
    let res = compiler.compile_source(source, "poly_vec.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let report = res
        .optimization_report
        .expect("Optimization report must be present");
    let has_vec_record = report
        .decision_trace
        .iter()
        .any(|r| r.pass == "PolyhedralVectorize" && r.decision == "Applied");
    assert!(
        has_vec_record,
        "Decision trace must record PolyhedralVectorize Applied"
    );
}

#[test]
fn test_v120_polyhedral_differential_backends() {
    let base_temp = std::env::temp_dir().join("datara_polyhedral_diff");
    let _ = std::fs::create_dir_all(&base_temp);

    let source = r#"
fn poly_calc() -> Float {
    let x = 2.5
    let y = 4.0
    let z = 1.5
    let f1 = fma(x, y, z)
    let f2 = fma(f1, 2.0, 1.0)
    return f2
}

fn main() {
    let res = poly_calc()
    out res
}
"#;

    // 1. Cranelift
    let clif_compiler = ForgenCompiler::new("cranelift");
    let clif_exe = base_temp.join("poly_diff_clif.exe");
    let clif_res = clif_compiler.compile_source(source, "poly_diff_clif.dtr", Some(&clif_exe));
    assert!(
        clif_res.success,
        "Cranelift compilation failed: {:?}",
        clif_res.error
    );
    let output_clif = std::process::Command::new(&clif_exe)
        .output()
        .expect("run clif exe");
    assert_eq!(output_clif.status.code(), Some(0));
    let out_clif = String::from_utf8_lossy(&output_clif.stdout)
        .trim()
        .to_string();

    // 2. JIT / LLVM
    let jit_compiler = ForgenCompiler::new("jit");
    let jit_exe = base_temp.join("poly_diff_jit.exe");
    let jit_res = jit_compiler.compile_source(source, "poly_diff_jit.dtr", Some(&jit_exe));
    assert!(
        jit_res.success,
        "JIT compilation failed: {:?}",
        jit_res.error
    );
    let output_jit = std::process::Command::new(&jit_exe)
        .output()
        .expect("run jit exe");
    assert_eq!(output_jit.status.code(), Some(0));
    let out_jit = String::from_utf8_lossy(&output_jit.stdout)
        .trim()
        .to_string();

    assert_eq!(
        out_jit, out_clif,
        "Bit-identical output across JIT/LLVM and Cranelift for polyhedral FMA math"
    );

    // Expected: f1 = (2.5 * 4.0) + 1.5 = 10.0 + 1.5 = 11.5
    // f2 = (11.5 * 2.0) + 1.0 = 23.0 + 1.0 = 24
    assert_eq!(out_clif, "24");
}
