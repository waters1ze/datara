use forgen::driver::ForgenCompiler;

#[test]
fn test_cranelift_simd_vector_math() {
    let source = r#"
fn main() {
    let v1 = float4(10.0, 20.0, 30.0, 40.0)
    let v2 = float4(1.0, 2.0, 3.0, 4.0)
    
    let v_add = f32x4_add(v1, v2)
    let sum_add = f32x4_horizontal_add(v_add)
    println(fmt"ADD_SUM: {sum_add}")
    
    let v_sub = f32x4_sub(v1, v2)
    let sum_sub = f32x4_horizontal_add(v_sub)
    println(fmt"SUB_SUM: {sum_sub}")

    let v_mul = f32x4_mul(v1, v2)
    let sum_mul = f32x4_horizontal_add(v_mul)
    println(fmt"MUL_SUM: {sum_mul}")

    let v_div = f32x4_div(v1, v2)
    let sum_div = f32x4_horizontal_add(v_div)
    println(fmt"DIV_SUM: {sum_div}")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let (stdout, stderr, code, _) = compiler
        .run_source(source, "simd_math_test", &[], true)
        .expect("JIT execution must succeed");
    assert_eq!(code, 0, "Error: stderr={}", stderr);
    assert!(
        stdout.contains("ADD_SUM: 110"),
        "Expected ADD_SUM: 110, got: {}",
        stdout
    );
    assert!(
        stdout.contains("SUB_SUM: 90"),
        "Expected SUB_SUM: 90, got: {}",
        stdout
    );
    assert!(
        stdout.contains("MUL_SUM: 300"),
        "Expected MUL_SUM: 300, got: {}",
        stdout
    );
    // 10/1 + 20/2 + 30/3 + 40/4 = 10 + 10 + 10 + 10 = 40
    assert!(
        stdout.contains("DIV_SUM: 40"),
        "Expected DIV_SUM: 40, got: {}",
        stdout
    );
}

#[test]
fn test_cranelift_simd_dot_and_cross() {
    let source = r#"
fn main() {
    let a = float4(1.0, 2.0, 3.0, 0.0)
    let b = float4(4.0, 5.0, 6.0, 0.0)
    
    // dot: 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
    let d = f32x4_dot(a, b)
    println(fmt"DOT: {d}")
    
    // cross product of (1, 0, 0) and (0, 1, 0) should be (0, 0, 1)
    let x_axis = float4(1.0, 0.0, 0.0, 0.0)
    let y_axis = float4(0.0, 1.0, 0.0, 0.0)
    let z_axis = f32x4_cross(x_axis, y_axis)
    let z_sum = f32x4_horizontal_add(z_axis)
    println(fmt"CROSS_Z: {z_sum}")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let (stdout, stderr, code, _) = compiler
        .run_source(source, "simd_dot_test", &[], true)
        .expect("JIT execution must succeed");
    assert_eq!(code, 0, "Error: stderr={}", stderr);
    assert!(
        stdout.contains("DOT: 32"),
        "Expected DOT: 32, got: {}",
        stdout
    );
    assert!(
        stdout.contains("CROSS_Z: 1"),
        "Expected CROSS_Z: 1, got: {}",
        stdout
    );
}

#[test]
fn test_cranelift_simd_aabb_intersection() {
    let source = r#"
fn main() {
    // Box 1: [0, 0, 0] to [10, 10, 10]
    let min1 = float4(0.0, 0.0, 0.0, 0.0)
    let max1 = float4(10.0, 10.0, 10.0, 0.0)
    
    // Box 2 (overlapping): [5, 5, 5] to [15, 15, 15]
    let min2 = float4(5.0, 5.0, 5.0, 0.0)
    let max2 = float4(15.0, 15.0, 15.0, 0.0)
    
    // Box 3 (separated): [20, 20, 20] to [30, 30, 30]
    let min3 = float4(20.0, 20.0, 20.0, 0.0)
    let max3 = float4(30.0, 30.0, 30.0, 0.0)
    
    let hit_overlap = aabb_intersects(min1, max1, min2, max2)
    let hit_separated = aabb_intersects(min1, max1, min3, max3)
    
    println(fmt"OVERLAP: {hit_overlap}")
    println(fmt"SEPARATED: {hit_separated}")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let (stdout, stderr, code, _) = compiler
        .run_source(source, "simd_aabb_test", &[], true)
        .expect("JIT execution must succeed");
    assert_eq!(code, 0, "Error: stderr={}", stderr);
    assert!(
        stdout.contains("OVERLAP: 1"),
        "Expected OVERLAP: 1, got: {}",
        stdout
    );
    assert!(
        stdout.contains("SEPARATED: 0"),
        "Expected SEPARATED: 0, got: {}",
        stdout
    );
}
