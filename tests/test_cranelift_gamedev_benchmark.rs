use forgen::driver::ForgenCompiler;

#[test]
fn test_cranelift_gamedev_particle_physics_simd() {
    let source = r#"
fn main() {
    mut pos = float4(0.0, 0.0, 0.0, 0.0)
    let vel = float4(1.5, 2.5, 3.5, 0.0)
    let dt = float4(0.016, 0.016, 0.016, 0.0)
    
    let vel_dt = f32x4_mul(vel, dt)
    
    // Simulate 1000 frames of physics integration
    mut frame = 0
    while frame < 1000 {
        pos = f32x4_add(pos, vel_dt)
        frame = frame + 1
    }
    
    let final_x = f32x4_dot(pos, float4(1.0, 0.0, 0.0, 0.0))
    let final_y = f32x4_dot(pos, float4(0.0, 1.0, 0.0, 0.0))
    let final_z = f32x4_dot(pos, float4(0.0, 0.0, 1.0, 0.0))
    
    println(fmt"FINAL_X: {final_x}")
    println(fmt"FINAL_Y: {final_y}")
    println(fmt"FINAL_Z: {final_z}")
}
"#;
    let compiler = ForgenCompiler::new("release");
    let (stdout, stderr, code, duration_ms) = compiler
        .run_source(source, "particle_physics", &[], true)
        .expect("Must run particle physics simulation");
    assert_eq!(code, 0, "Error: stderr={}", stderr);

    // 1.5 * 0.016 * 1000 = 24.0
    // 2.5 * 0.016 * 1000 = 40.0
    // 3.5 * 0.016 * 1000 = 56.0
    assert!(stdout.contains("FINAL_X: 24"), "Output: {}", stdout);
    assert!(stdout.contains("FINAL_Y: 40"), "Output: {}", stdout);
    assert!(
        stdout.contains("FINAL_Z: 55.999") || stdout.contains("FINAL_Z: 56"),
        "Output: {}",
        stdout
    );
    println!(
        "Physics simulation 1000 frames completed in {} ms",
        duration_ms
    );
}
