use forgen::codegen::cranelift::CraneliftBackend;
use forgen::codegen::target::TargetInfo;
use forgen::driver::ForgenCompiler;

#[test]
fn test_cranelift_ir_generation() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int {
    return a + b * 2
}

fn main() {
    mut res = 0

    res = compute(10, 20)
    out res
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "clif_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let clif = res.clif_source.expect("CLIF source must be generated");
    println!("Generated Cranelift IR (CLIF):\n{}", clif);

    // Verify CLIF structure
    assert!(clif.contains("test compile"));
    assert!(clif.contains("windows_fastcall") || clif.contains("system_v"));
    assert!(clif.contains("iadd") || clif.contains("imul") || clif.contains("iconst.i64 50"));
    assert!(clif.contains("function u0:main"));
}

#[test]
fn test_cranelift_backend_multi_target_clif() {
    let source = r#"
fn multiply(x: Int, y: Int) -> Int => x * y

fn main() {
    out multiply(3, 4)
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "clif_multi.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let prog = res.program.unwrap();
    let dmir = res.dmir_module.unwrap();

    let resolver = forgen::resolver::Resolver::new();
    let types = forgen::types::TypeChecker::new(&resolver);

    // 1. Windows x86_64
    let win_backend = CraneliftBackend::new(TargetInfo::x86_64_windows());
    let win_clif = win_backend
        .emit_clif(&dmir, &prog, &types)
        .expect("win clif");
    assert!(win_clif.contains("target x86_64-pc-windows-msvc"));
    assert!(win_clif.contains("windows_fastcall"));

    // 2. Linux x86_64
    let linux_backend = CraneliftBackend::new(TargetInfo::x86_64_linux());
    let linux_clif = linux_backend
        .emit_clif(&dmir, &prog, &types)
        .expect("linux clif");
    assert!(linux_clif.contains("target x86_64-unknown-linux-gnu"));
    assert!(linux_clif.contains("system_v"));

    // 3. Linux Aarch64
    let arm_backend = CraneliftBackend::new(TargetInfo::aarch64_linux());
    let arm_clif = arm_backend
        .emit_clif(&dmir, &prog, &types)
        .expect("arm clif");
    assert!(arm_clif.contains("target aarch64-unknown-linux-gnu"));
}

#[test]
fn test_cranelift_simd_linux_clif() {
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
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, "simd_linux")
        .unwrap();
    let backend = CraneliftBackend::new(TargetInfo::x86_64_linux());
    let (isa, call_conv, frontend_config) = backend.real_backend.build_target_isa(false).unwrap();
    let mut module = cranelift_object::ObjectModule::new(
        cranelift_object::ObjectBuilder::new(
            isa,
            "simd_linux",
            cranelift_module::default_libcall_names(),
        )
        .unwrap(),
    );
    let res =
        backend
            .real_backend
            .compile_into_module(&mut module, &dmir, frontend_config, call_conv);
    if let Err(e) = &res {
        println!("Error compiling into Linux module: {}", e);
    }
    assert!(res.is_ok(), "Must compile into Linux module: {:?}", res);
}
