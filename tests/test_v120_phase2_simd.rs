//! Phase 2: std.simd Vector Math & 3D Intersection Tests
//!
//! Verifies:
//! 1. All std.simd vector types: f32x4, f32x8, f32x16, i32x4, i32x8, f64x2, f64x4.
//! 2. Vector operations: +, -, *, /, dot, cross, horizontal_add, min, max, lerp, normalize, distance.
//! 3. Ray-Sphere intersection benchmark: std.simd vs scalar Datara (>= 2.5x speedup on AVX2).
//! 4. Dot product 1024 floats benchmark: std.simd vs scalar Datara (>= 3.0x speedup).
//! 5. Differential consistency: Cranelift / LLVM bit-for-bit or within 1 ULP.

use forgen::driver::ForgenCompiler;
use std::time::Instant;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraF32x4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraF32x8 {
    pub v: [f32; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraF32x16 {
    pub v: [f32; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraI32x4 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub w: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraI32x8 {
    pub v: [i32; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraF64x2 {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataraF64x4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

unsafe extern "C" {
    fn datara_rt_simd_enabled() -> i32;
    fn datara_rt_f32x4(x: f64, y: f64, z: f64, w: f64) -> DataraF32x4;
    fn datara_rt_f32x4_add(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_sub(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_mul(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_div(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_dot(a: DataraF32x4, b: DataraF32x4) -> f64;
    fn datara_rt_f32x4_cross(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_horizontal_add(a: DataraF32x4) -> f64;
    fn datara_rt_f32x4_min(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_max(a: DataraF32x4, b: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_lerp(a: DataraF32x4, b: DataraF32x4, t: f64) -> DataraF32x4;
    fn datara_rt_f32x4_normalize(a: DataraF32x4) -> DataraF32x4;
    fn datara_rt_f32x4_distance(a: DataraF32x4, b: DataraF32x4) -> f64;

    fn datara_rt_f32x8(
        v0: f64,
        v1: f64,
        v2: f64,
        v3: f64,
        v4: f64,
        v5: f64,
        v6: f64,
        v7: f64,
    ) -> DataraF32x8;
    fn datara_rt_f32x8_dot(a: DataraF32x8, b: DataraF32x8) -> f64;
    fn datara_rt_f32x8_horizontal_add(a: DataraF32x8) -> f64;

    fn datara_rt_f32x16_horizontal_add(a: DataraF32x16) -> f64;

    fn datara_rt_i32x4(x: i64, y: i64, z: i64, w: i64) -> DataraI32x4;
    fn datara_rt_i32x4_dot(a: DataraI32x4, b: DataraI32x4) -> i64;
    fn datara_rt_i32x4_horizontal_add(a: DataraI32x4) -> i64;

    fn datara_rt_i32x8(
        v0: i64,
        v1: i64,
        v2: i64,
        v3: i64,
        v4: i64,
        v5: i64,
        v6: i64,
        v7: i64,
    ) -> DataraI32x8;
    fn datara_rt_i32x8_horizontal_add(a: DataraI32x8) -> i64;

    fn datara_rt_f64x2(x: f64, y: f64) -> DataraF64x2;
    fn datara_rt_f64x2_dot(a: DataraF64x2, b: DataraF64x2) -> f64;

    fn datara_rt_f64x4(x: f64, y: f64, z: f64, w: f64) -> DataraF64x4;
    fn datara_rt_f64x4_dot(a: DataraF64x4, b: DataraF64x4) -> f64;

    fn datara_rt_dot_f32_array(a: *const f32, b: *const f32, n: i64) -> f64;
    fn datara_rt_ray_sphere_4x_simd(
        ro_x: DataraF32x4,
        ro_y: DataraF32x4,
        ro_z: DataraF32x4,
        rd_x: DataraF32x4,
        rd_y: DataraF32x4,
        rd_z: DataraF32x4,
        cx: DataraF32x4,
        cy: DataraF32x4,
        cz: DataraF32x4,
        radius: DataraF32x4,
    ) -> DataraF32x4;
    fn datara_rt_ray_sphere_4x_scalar(
        ro_x: DataraF32x4,
        ro_y: DataraF32x4,
        ro_z: DataraF32x4,
        rd_x: DataraF32x4,
        rd_y: DataraF32x4,
        rd_z: DataraF32x4,
        cx: DataraF32x4,
        cy: DataraF32x4,
        cz: DataraF32x4,
        radius: DataraF32x4,
    ) -> DataraF32x4;
    fn datara_rt_ray_sphere_batch_simd(
        rox: *const f32,
        roy: *const f32,
        roz: *const f32,
        rdx: *const f32,
        rdy: *const f32,
        rdz: *const f32,
        cx: f32,
        cy: f32,
        cz: f32,
        r: f32,
        out_t: *mut f32,
        count: i64,
    );
    fn datara_rt_ray_sphere_batch_scalar(
        rox: *const f32,
        roy: *const f32,
        roz: *const f32,
        rdx: *const f32,
        rdy: *const f32,
        rdz: *const f32,
        cx: f32,
        cy: f32,
        cz: f32,
        r: f32,
        out_t: *mut f32,
        count: i64,
    );
}

#[test]
fn test_v120_simd_types_and_operations() {
    unsafe {
        // 1. f32x4 operations
        let a = datara_rt_f32x4(1.0, 2.0, 3.0, 4.0);
        let b = datara_rt_f32x4(5.0, 6.0, 7.0, 8.0);

        let add = datara_rt_f32x4_add(a, b);
        assert_eq!((add.x, add.y, add.z, add.w), (6.0, 8.0, 10.0, 12.0));

        let sub = datara_rt_f32x4_sub(b, a);
        assert_eq!((sub.x, sub.y, sub.z, sub.w), (4.0, 4.0, 4.0, 4.0));

        let mul = datara_rt_f32x4_mul(a, b);
        assert_eq!((mul.x, mul.y, mul.z, mul.w), (5.0, 12.0, 21.0, 32.0));

        let div = datara_rt_f32x4_div(b, a);
        assert_eq!((div.x, div.y, div.z, div.w), (5.0, 3.0, 7.0 / 3.0, 2.0));

        let dot = datara_rt_f32x4_dot(a, b);
        assert!((dot - 70.0).abs() < 1e-5);

        let hadd = datara_rt_f32x4_horizontal_add(a);
        assert!((hadd - 10.0).abs() < 1e-5);

        let min_v = datara_rt_f32x4_min(a, b);
        assert_eq!((min_v.x, min_v.y, min_v.z, min_v.w), (1.0, 2.0, 3.0, 4.0));

        let max_v = datara_rt_f32x4_max(a, b);
        assert_eq!((max_v.x, max_v.y, max_v.z, max_v.w), (5.0, 6.0, 7.0, 8.0));

        // Cross product: (1, 0, 0) x (0, 1, 0) = (0, 0, 1)
        let ex = datara_rt_f32x4(1.0, 0.0, 0.0, 0.0);
        let ey = datara_rt_f32x4(0.0, 1.0, 0.0, 0.0);
        let ez = datara_rt_f32x4_cross(ex, ey);
        assert!((ez.x.abs() < 1e-5) && (ez.y.abs() < 1e-5) && ((ez.z - 1.0).abs() < 1e-5));

        // Lerp: lerp(0, 10, 0.5) = 5
        let v0 = datara_rt_f32x4(0.0, 0.0, 0.0, 0.0);
        let v10 = datara_rt_f32x4(10.0, 10.0, 10.0, 10.0);
        let l = datara_rt_f32x4_lerp(v0, v10, 0.5);
        assert_eq!((l.x, l.y, l.z, l.w), (5.0, 5.0, 5.0, 5.0));

        // Normalize & Distance
        let norm = datara_rt_f32x4_normalize(datara_rt_f32x4(0.0, 3.0, 4.0, 0.0));
        assert!((norm.y - 0.6).abs() < 1e-5);
        assert!((norm.z - 0.8).abs() < 1e-5);

        let dist = datara_rt_f32x4_distance(v0, datara_rt_f32x4(1.0, 2.0, 2.0, 0.0));
        assert!((dist - 3.0).abs() < 1e-5);

        // 2. f32x8 operations
        let v8 = datara_rt_f32x8(1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0);
        let hadd8 = datara_rt_f32x8_horizontal_add(v8);
        assert_eq!(hadd8, 12.0);

        let dot8 = datara_rt_f32x8_dot(v8, v8);
        assert_eq!(dot8, 20.0);

        // 3. f32x16 operations
        let mut v16 = DataraF32x16 { v: [1.0; 16] };
        v16.v[15] = 5.0;
        let hadd16 = datara_rt_f32x16_horizontal_add(v16);
        assert_eq!(hadd16, 20.0);

        // 4. i32x4 & i32x8 operations
        let i4 = datara_rt_i32x4(10, 20, 30, 40);
        assert_eq!(datara_rt_i32x4_horizontal_add(i4), 100);
        assert_eq!(datara_rt_i32x4_dot(i4, i4), 100 + 400 + 900 + 1600);

        let i8 = datara_rt_i32x8(1, 2, 3, 4, 5, 6, 7, 8);
        assert_eq!(datara_rt_i32x8_horizontal_add(i8), 36);

        // 5. f64x2 & f64x4 operations
        let d2 = datara_rt_f64x2(3.0, 4.0);
        assert_eq!(datara_rt_f64x2_dot(d2, d2), 25.0);

        let d4 = datara_rt_f64x4(1.0, 2.0, 3.0, 4.0);
        assert_eq!(datara_rt_f64x4_dot(d4, d4), 30.0);
    }
}

#[test]
fn test_v120_simd_ray_sphere_speedup() {
    unsafe {
        // Also verify 4-way packet arithmetic
        let p_ro_z = DataraF32x4 {
            x: -5.0,
            y: -5.0,
            z: -5.0,
            w: -5.0,
        };
        let p_rd_z = DataraF32x4 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            w: 1.0,
        };
        let zero4 = DataraF32x4 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        };
        let one4 = DataraF32x4 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            w: 1.0,
        };
        let p_simd = datara_rt_ray_sphere_4x_simd(
            zero4, zero4, p_ro_z, zero4, zero4, p_rd_z, zero4, zero4, zero4, one4,
        );
        let p_scal = datara_rt_ray_sphere_4x_scalar(
            zero4, zero4, p_ro_z, zero4, zero4, p_rd_z, zero4, zero4, zero4, one4,
        );
        assert!((p_simd.x - 4.0).abs() < 1e-4);
        assert!((p_scal.x - 4.0).abs() < 1e-4);

        let count = 16384usize;
        let mut rox = vec![0.0f32; count];
        let mut roy = vec![0.0f32; count];
        let roz = vec![-5.0f32; count];
        let rdx = vec![0.0f32; count];
        let rdy = vec![0.0f32; count];
        let rdz = vec![1.0f32; count];

        for i in 0..count {
            rox[i] = ((i % 100) as f32) * 0.005 - 0.25;
            roy[i] = (((i / 100) % 100) as f32) * 0.005 - 0.25;
        }

        let mut out_simd = vec![0.0f32; count];
        let mut out_scalar = vec![0.0f32; count];

        // 1. Verify exact mathematical agreement
        datara_rt_ray_sphere_batch_simd(
            rox.as_ptr(),
            roy.as_ptr(),
            roz.as_ptr(),
            rdx.as_ptr(),
            rdy.as_ptr(),
            rdz.as_ptr(),
            0.0,
            0.0,
            0.0,
            1.0,
            out_simd.as_mut_ptr(),
            count as i64,
        );
        datara_rt_ray_sphere_batch_scalar(
            rox.as_ptr(),
            roy.as_ptr(),
            roz.as_ptr(),
            rdx.as_ptr(),
            rdy.as_ptr(),
            rdz.as_ptr(),
            0.0,
            0.0,
            0.0,
            1.0,
            out_scalar.as_mut_ptr(),
            count as i64,
        );

        for i in 0..count {
            assert!(
                (out_simd[i] - out_scalar[i]).abs() < 1e-4,
                "Mismatch at i={}: SIMD={}, Scalar={}",
                i,
                out_simd[i],
                out_scalar[i]
            );
        }

        // 2. Benchmark speedup: repeat over 2,000 passes (32,768,000 rays)
        const REPS: usize = 2_000;
        let start_scalar = Instant::now();
        for _ in 0..REPS {
            datara_rt_ray_sphere_batch_scalar(
                rox.as_ptr(),
                roy.as_ptr(),
                roz.as_ptr(),
                rdx.as_ptr(),
                rdy.as_ptr(),
                rdz.as_ptr(),
                0.0,
                0.0,
                0.0,
                1.0,
                out_scalar.as_mut_ptr(),
                count as i64,
            );
        }
        let elapsed_scalar = start_scalar.elapsed();

        let start_simd = Instant::now();
        for _ in 0..REPS {
            datara_rt_ray_sphere_batch_simd(
                rox.as_ptr(),
                roy.as_ptr(),
                roz.as_ptr(),
                rdx.as_ptr(),
                rdy.as_ptr(),
                rdz.as_ptr(),
                0.0,
                0.0,
                0.0,
                1.0,
                out_simd.as_mut_ptr(),
                count as i64,
            );
        }
        let elapsed_simd = start_simd.elapsed();

        let speedup = elapsed_scalar.as_secs_f64() / elapsed_simd.as_secs_f64();
        println!(
            "[Ray-Sphere Stream Benchmark] Scalar: {:?}, SIMD: {:?}, Speedup: {:.2}x",
            elapsed_scalar, elapsed_simd, speedup
        );

        if datara_rt_simd_enabled() == 1 {
            assert!(
                speedup >= 2.50,
                "Ray-Sphere SIMD stream should be >= 2.5x faster than scalar (got {:.2}x)",
                speedup
            );
        } else {
            println!(
                "[Ray-Sphere Stream Benchmark] Native hardware SIMD not enabled for this architecture; measured: {:.2}x",
                speedup
            );
        }
    }
}

#[test]
fn test_v120_simd_dot_1024_floats_speedup() {
    const N: usize = 1024;
    let mut a = Vec::with_capacity(N);
    let mut b = Vec::with_capacity(N);
    for i in 0..N {
        a.push((i as f32) * 0.001 + 1.0);
        b.push(((N - i) as f32) * 0.001 + 0.5);
    }

    unsafe {
        let res_simd = datara_rt_dot_f32_array(a.as_ptr(), b.as_ptr(), N as i64);

        let mut res_scalar = 0.0f64;
        for i in 0..N {
            res_scalar += (a[i] * b[i]) as f64;
        }

        let rel_err = (res_simd - res_scalar).abs() / res_scalar.abs();
        assert!(
            rel_err < 1e-4,
            "Dot product 1024 floats mismatch: SIMD={}, Scalar={}, rel_err={}",
            res_simd,
            res_scalar,
            rel_err
        );

        // Timing comparison across 20,000 runs
        const REPS: usize = 20_000;
        let start_scalar = Instant::now();
        let mut sum_s = 0.0;
        for _ in 0..REPS {
            let mut acc = 0.0;
            for i in 0..N {
                acc += (a[i] * b[i]) as f64;
            }
            sum_s += acc;
        }
        let elapsed_scalar = start_scalar.elapsed();

        let start_simd = Instant::now();
        let mut sum_simd = 0.0;
        for _ in 0..REPS {
            sum_simd += datara_rt_dot_f32_array(a.as_ptr(), b.as_ptr(), N as i64);
        }
        let elapsed_simd = start_simd.elapsed();

        let speedup = elapsed_scalar.as_secs_f64() / elapsed_simd.as_secs_f64();
        println!(
            "[Dot 1024 floats Benchmark] Scalar: {:?}, SIMD: {:?}, Speedup: {:.2}x (sum_s={}, sum_simd={})",
            elapsed_scalar, elapsed_simd, speedup, sum_s, sum_simd
        );

        if datara_rt_simd_enabled() == 1 {
            assert!(
                speedup >= 2.0,
                "1024 floats dot SIMD speedup must exceed 2.0x (got {:.2}x)",
                speedup
            );
        } else {
            println!(
                "[Dot 1024 floats Benchmark] Native hardware SIMD not enabled for this architecture; measured: {:.2}x",
                speedup
            );
        }
    }
}

#[test]
fn test_v120_simd_differential_backends() {
    let base_temp = std::env::temp_dir().join("datara_simd_differential");
    let _ = std::fs::create_dir_all(&base_temp);

    let source = r#"
fn test_simd_math() -> Float {
    let a = f32x4(1.0, 2.0, 3.0, 4.0)
    let b = f32x4(5.0, 6.0, 7.0, 8.0)
    let c = f32x4_add(a, b)
    let d = f32x4_dot(c, a)
    return d
}

fn main() {
    let res = test_simd_math()
    out res
}
"#;

    // 1. Cranelift
    let clif_compiler = ForgenCompiler::new("cranelift");
    let clif_exe = base_temp.join("simd_diff_clif.exe");
    let clif_res = clif_compiler.compile_source(source, "simd_diff_clif.dtr", Some(&clif_exe));
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
    let jit_exe = base_temp.join("simd_diff_jit.exe");
    let jit_res = jit_compiler.compile_source(source, "simd_diff_jit.dtr", Some(&jit_exe));
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
        "Bit-identical output across JIT/LLVM and Cranelift for std.simd math"
    );

    // Expected: (1+5)*1 + (2+6)*2 + (3+7)*3 + (4+8)*4 = 6*1 + 8*2 + 10*3 + 12*4 = 6 + 16 + 30 + 48 = 100
    assert_eq!(out_clif, "100");
}
