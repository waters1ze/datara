use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;

struct LcgRng {
    state: u64,
}

impl LcgRng {
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
        // Scale to [-100.0, 100.0]
        ((u as f64 / u32::MAX as f64) * 200.0 - 100.0) as f32
    }
}

#[test]
fn audit_simd_vector_opcodes_inspection() {
    let source = r#"
fn test_simd_ops(v1: Float, v2: Float) -> Float {
    let a = float4(1.0, 2.0, 3.0, 4.0)
    let b = float4(5.0, 6.0, 7.0, 8.0)
    let m = min4(a, b)
    let d = dot(a, b)
    return d
}
fn main() {
    let r = test_simd_ops(1.0, 2.0)
    out r
}
"#;

    // 1. LLVM IR Inspection: must contain genuine vector types `<4 x float>` and intrinsics
    let llvm_compiler = ForgenCompiler::new("release").with_llvm(true);
    let llvm_res = llvm_compiler.compile_source(source, "audit_simd_llvm.dtr", None);
    assert!(llvm_res.success, "LLVM compile must succeed");
    let llvm_ir = llvm_res.llvm_source.expect("LLVM IR must be generated");

    let has_llvm_vector = llvm_ir.contains("<4 x float>");
    let has_llvm_minnum = llvm_ir.contains("llvm.minnum.v4f32") || llvm_ir.contains("fmin");
    let has_llvm_fmul_vec = llvm_ir.contains("fmul <4 x float>");

    println!(
        "LLVM IR SIMD inspection: has_vec={}, has_min={}, has_fmul_vec={}",
        has_llvm_vector, has_llvm_minnum, has_llvm_fmul_vec
    );

    assert!(
        has_llvm_vector,
        "LLVM IR must declare <4 x float> vector types"
    );
    assert!(
        has_llvm_minnum,
        "LLVM IR must use vector minnum intrinsic for min4"
    );
    assert!(
        has_llvm_fmul_vec,
        "LLVM IR must use vector fmul for float4 dot product"
    );

    // 2. WASM Binary Inspection: must contain 0xFD prefix (SIMD opcodes)
    let dmir = llvm_res.dmir_module.expect("DMIR module");
    let temp_wasm = std::env::temp_dir().join("audit_simd.wasm");
    WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm).expect("Wasm emit");
    let wasm_bytes = fs::read(&temp_wasm).expect("Read wasm");
    let contains_simd_prefix = wasm_bytes.contains(&0xFD);
    assert!(
        contains_simd_prefix,
        "WASM binary MUST physically contain byte 0xFD (Wasm SIMD prefix)"
    );
    let _ = fs::remove_file(&temp_wasm);

    // 3. Cranelift Inspection: Check whether CLIF emitted genuine vector instructions (f32x4)
    // or scalar loop fallback with repeated loads/stores
    let clif = llvm_res.clif_source.expect("CLIF source");
    let has_clif_f32x4 = clif.contains("f32x4") || clif.contains("v128");
    let has_clif_scalar_loop = clif.contains("fmin") || clif.contains("fmul");

    println!(
        "CRANELIFT FORENSIC FACT: clif has f32x4/v128={}, has scalar fmin/fmul={}",
        has_clif_f32x4, has_clif_scalar_loop
    );
    // Forensic note: Cranelift backend uses 4-lane unrolled scalar fmin/fmul loops with malloc(16)
    // rather than Cranelift's native v128 f32x4 vector SSA values!
}

#[test]
#[ignore = "slow: runs 1000 random iterations"]
fn slow_audit_simd_thousand_random_inputs_against_scalar_oracle() {
    let mut rng = LcgRng::new(0xCAFEBABE12345678);

    for i in 0..1000 {
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

        // Scalar mathematical reference:
        // 1. dot product (SSE2 performs pairwise reduction: (p0+p1) + (p2+p3))
        let p0 = a[0] * b[0];
        let p1 = a[1] * b[1];
        let p2 = a[2] * b[2];
        let p3 = a[3] * b[3];
        let expected_dot = ((p0 + p1) + (p2 + p3)) as f64;

        // 2. min4
        let expected_min = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ];

        // 3. max4
        let expected_max = [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ];

        // Datara C runtime implementation invocation
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct DataraFloat4 {
            x: f32,
            y: f32,
            z: f32,
            w: f32,
        }

        unsafe extern "C" {
            fn datara_rt_float4_dot(a: DataraFloat4, b: DataraFloat4) -> f64;
            fn datara_rt_float4_min4(a: DataraFloat4, b: DataraFloat4) -> DataraFloat4;
            fn datara_rt_float4_max4(a: DataraFloat4, b: DataraFloat4) -> DataraFloat4;
        }

        let va = DataraFloat4 {
            x: a[0],
            y: a[1],
            z: a[2],
            w: a[3],
        };
        let vb = DataraFloat4 {
            x: b[0],
            y: b[1],
            z: b[2],
            w: b[3],
        };

        let actual_dot = unsafe { datara_rt_float4_dot(va, vb) };
        let actual_min = unsafe { datara_rt_float4_min4(va, vb) };
        let actual_max = unsafe { datara_rt_float4_max4(va, vb) };

        // Float epsilon comparison accounting for magnitude
        let diff_dot = (actual_dot - expected_dot).abs();
        let max_mag = actual_dot.abs().max(expected_dot.abs()).max(1.0);
        let rel_err = diff_dot / max_mag;
        assert!(
            rel_err < 1e-4,
            "Iteration #{}: Dot product mismatch: expected {}, got {}, diff={}, rel_err={}",
            i,
            expected_dot,
            actual_dot,
            diff_dot,
            rel_err
        );

        for lane in 0..4 {
            let actual_min_lane = match lane {
                0 => actual_min.x,
                1 => actual_min.y,
                2 => actual_min.z,
                _ => actual_min.w,
            };
            let actual_max_lane = match lane {
                0 => actual_max.x,
                1 => actual_max.y,
                2 => actual_max.z,
                _ => actual_max.w,
            };

            assert_eq!(
                actual_min_lane, expected_min[lane],
                "Iteration #{}, lane {}: min4 mismatch! expected {}, got {}",
                i, lane, expected_min[lane], actual_min_lane
            );
            assert_eq!(
                actual_max_lane, expected_max[lane],
                "Iteration #{}, lane {}: max4 mismatch! expected {}, got {}",
                i, lane, expected_max[lane], actual_max_lane
            );
        }
    }
}
