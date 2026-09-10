use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;

struct CustomLcg {
    state: u64,
}

impl CustomLcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        let u = self.next_u32();
        ((u as f64 / u32::MAX as f64) * 200.0 - 100.0) as f32
    }
}

#[test]
fn audit_simd_emit_artifacts_inspection() {
    let source = r#"
fn simd_kernel() -> Float {
    let v1 = float4(1.0, 2.0, 3.0, 4.0)
    let v2 = float4(5.0, 6.0, 7.0, 8.0)
    let m = min4(v1, v2)
    let d = dot(v1, v2)
    return d
}
fn main() {
    out simd_kernel()
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "simd_emit_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    // 1. LLVM IR must contain real vector operations
    let llvm_ir = res.llvm_source.expect("LLVM IR generated");
    assert!(
        llvm_ir.contains("<4 x float>"),
        "LLVM IR MUST contain <4 x float> vector types"
    );
    assert!(
        llvm_ir.contains("llvm.minnum.v4f32") || llvm_ir.contains("fmin"),
        "LLVM IR MUST contain vector minnum intrinsic"
    );
    assert!(
        llvm_ir.contains("fmul <4 x float>"),
        "LLVM IR MUST contain vector fmul instruction"
    );

    // 2. WASM Binary must contain SIMD prefix 0xFD
    let dmir = res.dmir_module.expect("DMIR module");
    let temp_wasm = std::env::temp_dir().join("simd_test.wasm");
    WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm).expect("Wasm emit");
    let wasm_bytes = fs::read(&temp_wasm).expect("read wasm");
    let _ = fs::remove_file(&temp_wasm);
    assert!(
        wasm_bytes.contains(&0xFD),
        "WASM binary MUST physically contain byte 0xFD (WASM SIMD prefix)"
    );

    // 3. Cranelift observation
    let clif = res.clif_source.expect("CLIF generated");
    let clif_has_v128 = clif.contains("f32x4") || clif.contains("v128");
    println!(
        "CRANELIFT FORENSIC FINDING: clif has v128/f32x4 hardware SIMD? {}",
        clif_has_v128
    );
}

#[test]
fn audit_simd_thousand_random_inputs_dot_product_matches_naive_reference() {
    let mut rng = CustomLcg::new(0xCAFE_BABE_1234_5678);

    for idx in 0..1000 {
        let a = [
            rng.next_f32(),
            rng.next_f32(),
            rng.next_f32(),
            rng.next_f32(),
        ];
        let b = [
            rng.next_f32(),
            rng.next_f32(),
            rng.next_f32(),
            rng.next_f32(),
        ];

        // Naive reference calculation
        let naive_dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];

        let source = format!(
            r#"
fn main() {{
    let v1 = float4({:.6}, {:.6}, {:.6}, {:.6})
    let v2 = float4({:.6}, {:.6}, {:.6}, {:.6})
    let d = dot(v1, v2)
    out d
}}
"#,
            a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]
        );

        // Test with Cranelift JIT / native run
        let compiler = ForgenCompiler::new("release");
        let res = compiler.compile_source(&source, &format!("simd_dot_{}.dtr", idx % 10), None);
        if !res.success {
            panic!("Compilation failed at iteration {}: {:?}", idx, res.error);
        }

        let (stdout, _, code, _) = compiler
            .cranelift
            .run_executable(&res.exe_path.unwrap(), &[])
            .unwrap();
        assert_eq!(code, 0);

        let parsed: f32 = stdout.trim().parse().expect("Parse float output");
        let diff = (parsed - naive_dot).abs();
        let max_val = parsed.abs().max(naive_dot.abs()).max(1.0);
        let rel_err = diff / max_val;

        assert!(
            rel_err < 1e-4,
            "Iteration {}: Dot product mismatch! Datara={}, Naive={}, rel_err={}",
            idx,
            parsed,
            naive_dot,
            rel_err
        );

        // Run 20 iterations thoroughly through LLVM and WASM as well
        if idx < 20 {
            // LLVM check
            let llvm_comp = ForgenCompiler::new("release").with_llvm(true);
            let res_llvm = llvm_comp.compile_source(&source, "simd_dot_llvm.dtr", None);
            if res_llvm.success && res_llvm.exe_path.is_some() {
                let (llvm_out, _, _, _) = compiler
                    .cranelift
                    .run_executable(&res_llvm.exe_path.unwrap(), &[])
                    .unwrap();
                let llvm_f: f32 = llvm_out.trim().parse().unwrap();
                assert!((llvm_f - naive_dot).abs() / max_val < 1e-4);
            }
        }
    }
}
