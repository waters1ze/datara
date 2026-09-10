use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_llvm_ir_emission_arithmetic_and_math() {
    let source = r#"
fn calculate(a: Int, b: Int) -> Int {
    let x = (a + b) * 2 - 5
    x
}

fn main() {
    let res = calculate(10, 20)
    out(res)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "calc.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");

    // Verify LLVM IR structure
    assert!(
        llvm.contains("target triple ="),
        "Must contain target triple"
    );
    assert!(
        llvm.contains("define internal fastcc i64 @calculate(")
            || llvm.contains("define i64 @calculate("),
        "Must declare calculate"
    );
    assert!(
        llvm.contains("add i64") || llvm.contains("checked_add"),
        "Must contain integer add instruction"
    );
    assert!(
        llvm.contains("mul i64") || llvm.contains("checked_mul"),
        "Must contain integer mul instruction"
    );
    assert!(
        llvm.contains("sub i64") || llvm.contains("checked_sub"),
        "Must contain integer sub instruction"
    );
    assert!(llvm.contains("define i32 @main()"), "Main must return i32");
    assert!(
        llvm.contains("call void @datara_rt_out_int("),
        "Must call datara_rt_out_int"
    );
}

#[test]
fn test_llvm_ir_emission_branching_and_loops() {
    let source = r#"
fn count_sum(n: Int) -> Int {
    mut total = 0
    mut i = 1
    while i <= n {
        total = total + i
        i = i + 1
    }
    total
}

fn main() {
    let s = count_sum(10)
    out(s)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "loop.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");

    // Verify control flow in LLVM IR
    assert!(
        llvm.contains("define internal fastcc i64 @count_sum(")
            || llvm.contains("define i64 @count_sum("),
        "Must contain count_sum function"
    );
    assert!(llvm.contains("icmp"), "Must contain loop condition icmp");
    assert!(llvm.contains("br i1"), "Must contain conditional branch");
    assert!(
        llvm.contains("br label"),
        "Must contain unconditional branch"
    );
}

#[test]
fn test_llvm_ir_emission_classes_and_fields() {
    let source = r#"
class Vector2 {
    x: Float
    y: Float
}

fn length_sq(v: Vector2) -> Float {
    v.x * v.x + v.y * v.y
}

fn main() {
    let pt = Vector2 { x: 3.0, y: 4.0 }
    let lsq = length_sq(pt)
    out(lsq)
}
"#;

    let compiler = ForgenCompiler::new("quick").with_llvm(true);
    let res = compiler.compile_source(source, "vec.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR source must be generated");
    println!("VEC LLVM IR:\n{}", llvm);

    // Verify object memory instructions
    assert!(
        llvm.contains("getelementptr inbounds i8, ptr %v"),
        "Must contain GEP for field access"
    );
    assert!(
        llvm.contains("fmul double"),
        "Must contain floating point multiplications"
    );
    assert!(
        llvm.contains("fadd double"),
        "Must contain floating point addition"
    );
    assert!(
        llvm.contains("call void @datara_rt_out_float(double %v"),
        "Must output float"
    );
}

#[test]
fn test_llvm_ir_generated_when_compiling_with_llvm() {
    let temp_dir = std::env::temp_dir().join("forgen_llvm_test");
    let _ = fs::create_dir_all(&temp_dir);
    let src_path = temp_dir.join("test_app.dtr");
    let exe_path = temp_dir.join("test_app.exe");

    fs::write(
        &src_path,
        r#"
fn main() {
    out("Hello from LLVM backend!")
}
"#,
    )
    .unwrap();

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_file(&src_path, Some(&exe_path));

    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    // The LLVM pipeline emits the IR into the CompilationResult. When a
    // Clang/LLC toolchain is available the on-disk `.ll` is an intermediate
    // (written under `.forgen_cache/build/`) and is removed again after the
    // AOT compile consumes it, so the IR is verified through the result
    // rather than through a file that only exists on toolchain-less hosts.
    let llvm = res
        .llvm_source
        .expect("LLVM IR source must be generated when compiling with --llvm");
    assert!(
        llvm.contains("Hello from LLVM backend!"),
        "LLVM IR must contain string literal"
    );
    assert!(
        llvm.contains("datara_rt_out_str"),
        "LLVM IR must call datara_rt_out_str"
    );

    // Cleanup
    let _ = fs::remove_file(&src_path);
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_dir(&temp_dir);
}

#[test]
fn test_llvm_guarded_function_compilation_and_lowering() {
    let source = r#"
fn dynamic_move(flag: Int, v: Int) -> Int {
    if flag > 0 {
        destroy(v)
    }
    out(v)
    return v
}

fn main() {
    let r = dynamic_move(0, 42)
    out(r)
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "guarded_llvm.dtr", None);

    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res
        .llvm_source
        .expect("LLVM IR source must be generated when compiling with --llvm");

    // Verify declarations and calls in LLVM IR
    assert!(
        llvm.contains("declare i64 @datara_rt_own_acquire(i64)"),
        "Must declare datara_rt_own_acquire in LLVM IR"
    );
    assert!(
        llvm.contains("declare void @datara_rt_own_release(i64)"),
        "Must declare datara_rt_own_release in LLVM IR"
    );
    assert!(
        llvm.contains("call i64 @datara_rt_own_acquire"),
        "Must call datara_rt_own_acquire in LLVM IR"
    );
    assert!(
        llvm.contains("call void @datara_rt_own_release"),
        "Must call datara_rt_own_release in LLVM IR"
    );

    // If linker / toolchain is available, compile to executable and test output
    let temp_dir = std::env::temp_dir().join("forgen_llvm_guarded_test");
    let _ = fs::create_dir_all(&temp_dir);
    let src_path = temp_dir.join("guarded_app.dtr");
    let exe_path = temp_dir.join("guarded_app.exe");
    fs::write(&src_path, source).unwrap();

    let exe_res = compiler.compile_file(&src_path, Some(&exe_path));
    if exe_res.success && exe_path.exists() {
        let output = std::process::Command::new(&exe_path)
            .output()
            .expect("llvm binary execution");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "Execution must succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("42"),
            "Output must contain result 42, got: {}",
            stdout
        );
    }

    let _ = fs::remove_file(&src_path);
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_dir(&temp_dir);
}
