use forgen::codegen::cranelift::CraneliftBackend;
use forgen::driver::ForgenCompiler;

#[test]
fn test_native_backend_execution() {
    let test_cases = [
        (
            "Arithmetic",
            r#"
fn compute(a: Int, b: Int) -> Int => (a + b) * 2 - a / 2
fn main() {
    out compute(10, 20)
}
"#,
            "55",
        ),
        (
            "Branches and Decide",
            r#"
fn classify(score: Int) -> String {
    return decide {
        score >= 90 => "A"
        score >= 80 => "B"
        else => "C"
    }
}
fn main() {
    out classify(85)
}
"#,
            "B",
        ),
        (
            "Loop Accumulator",
            r#"
fn sum_to_n(n: Int) -> Int {
    mut sum = 0
    mut i = 1
    while i <= n {
        sum = sum + i
        i = i + 1
    }
    return sum
}
fn main() {
    out sum_to_n(10)
}
"#,
            "55",
        ),
        (
            "Classes and Methods",
            r#"
class Point {
    x: Int
    y: Int
}
behavior Point {
    manhattan() -> Int => this.x + this.y
}
fn main() {
    mut p = Point { x: 15, y: 25 }
    out p.manhattan()
}
"#,
            "40",
        ),
        (
            "Generic Box Monomorphization",
            r#"
class Box<T> {
    val: T
}
fn main() {
    mut b1 = Box<Int> { val: 42 }
    out b1.val
}
"#,
            "42",
        ),
    ];

    let compiler = ForgenCompiler::new("release");

    for (name, source, expected_stdout) in test_cases {
        // 1. Compile and run with Native Cranelift Compiler
        let res = compiler.compile_source(source, &format!("{}_native.dtr", name), None);
        assert!(
            res.success,
            "[{}] Native compilation failed: {:?}",
            name, res.error
        );
        println!(
            "[{}] CLIF:\n{}",
            name,
            res.clif_source.as_deref().unwrap_or("")
        );
        let exe = res.exe_path.unwrap();
        let (stdout, stderr, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
        println!("[{}] STDOUT: [{}], STDERR: [{}]", name, stdout, stderr);

        let normalized = stdout.replace("\r\n", "\n");
        let normalized_expected = expected_stdout.replace("\r\n", "\n");
        assert_eq!(code, 0, "[{}] Native return code non-zero", name);
        assert_eq!(
            normalized.trim(),
            normalized_expected.trim(),
            "[{}] Native stdout mismatch",
            name
        );
        assert!(
            stderr.is_empty(),
            "[{}] Native stderr not empty: {}",
            name,
            stderr
        );

        // 2. Cranelift Backend Code Emission Verification
        let dmir = res.dmir_module.as_ref().unwrap();
        let prog = res.program.as_ref().unwrap();
        let resolver = forgen::resolver::Resolver::new();
        let types = forgen::types::TypeChecker::new(&resolver);

        let cranelift_backend = CraneliftBackend::for_host();
        let clif = cranelift_backend.emit_clif(dmir, prog, &types);
        assert!(
            clif.contains("function u0:main"),
            "[{}] Cranelift IR missing main function",
            name
        );

        // 3. Artifact Inspection
        let inspect = cranelift_backend.inspect_module(dmir);
        assert!(
            inspect.total_functions > 0,
            "[{}] Cranelift inspection found 0 functions",
            name
        );
    }
}
