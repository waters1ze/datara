use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;
use std::process::Command;

struct DifferentialTestCase {
    name: &'static str,
    source: &'static str,
    expected_output: &'static str,
}

const DIFFERENTIAL_PROGRAMS: &[DifferentialTestCase] = &[
    // 1. Basic arithmetic and operator precedence
    DifferentialTestCase {
        name: "arithmetic_precedence",
        source: r#"
fn main() {
    let a = 10
    let b = 20
    let c = 3
    let res = (a + b) * c - 40 / 4
    out res
}
"#,
        expected_output: "80",
    },
    // 2. While loop accumulation
    DifferentialTestCase {
        name: "while_loop_sum",
        source: r#"
fn main() {
    mut total = 0
    mut i = 1
    while i <= 10 {
        total = total + i
        i = i + 1
    }
    out total
}
"#,
        expected_output: "55",
    },
    // 3. Recursive Fibonacci
    DifferentialTestCase {
        name: "recursive_fibonacci",
        source: r#"
fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let res = fib(8)
    out res
}
"#,
        expected_output: "21",
    },
    // 4. Branching and comparison
    DifferentialTestCase {
        name: "branching_and_comparison",
        source: r#"
fn branch_test(x: Int, y: Int) -> Int {
    if x > y {
        if y > 10 {
            return 42
        } else {
            return 100
        }
    } else {
        return 0
    }
}

fn main() {
    let res = branch_test(50, 20)
    out res
}
"#,
        expected_output: "42",
    },
    // 5. Multi-function call chain
    DifferentialTestCase {
        name: "multi_function_chain",
        source: r#"
fn step3(z: Int) -> Int {
    return z * 2
}

fn step2(y: Int) -> Int {
    return step3(y + 10)
}

fn step1(x: Int) -> Int {
    return step2(x * 2)
}

fn main() {
    let res = step1(20)
    out res
}
"#,
        expected_output: "100",
    },
    // 6. Power of two multiplication loop
    DifferentialTestCase {
        name: "power_of_two_loop",
        source: r#"
fn power_of_two(exp: Int) -> Int {
    mut res = 1
    mut i = 0
    while i < exp {
        res = res * 2
        i = i + 1
    }
    return res
}

fn main() {
    let shifted = power_of_two(6)
    out shifted
}
"#,
        expected_output: "64",
    },
    // 7. Counter accumulator (factorial)
    DifferentialTestCase {
        name: "factorial_loop",
        source: r#"
fn main() {
    mut fact = 1
    mut n = 5
    while n > 0 {
        fact = fact * n
        n = n - 1
    }
    out fact
}
"#,
        expected_output: "120",
    },
    // 8. Modulo and prime factor sum
    DifferentialTestCase {
        name: "modulo_divisors",
        source: r#"
fn sum_divisors(n: Int) -> Int {
    mut sum = 0
    mut d = 1
    while d < n {
        if n % d == 0 {
            sum = sum + d
        }
        d = d + 1
    }
    return sum
}

fn main() {
    let res = sum_divisors(16)
    out res
}
"#,
        expected_output: "15",
    },
    // 9. Nested while loops (2D grid count)
    DifferentialTestCase {
        name: "nested_loops",
        source: r#"
fn main() {
    mut count = 0
    mut i = 0
    while i < 5 {
        mut j = 0
        while j < 5 {
            count = count + 1
            j = j + 1
        }
        i = i + 1
    }
    out count
}
"#,
        expected_output: "25",
    },
    // 10. Complex expression evaluation with multiple arguments
    DifferentialTestCase {
        name: "complex_eval",
        source: r#"
fn eval_poly(a: Int, b: Int, c: Int, x: Int) -> Int {
    return a * (x * x) + b * x + c
}

fn main() {
    let res = eval_poly(2, 3, 4, 5)
    out (res + 30)
}
"#,
        expected_output: "99",
    },
];

fn run_wasm_node(wasm_path: &Path) -> Result<String, String> {
    let script = format!(
        r#"
const fs = require('fs');
const wasmBytes = fs.readFileSync('{}');

let capturedOutput = '';
const importObject = {{
    "datara:rt": {{
        alloc: (sz) => 65536n,
        print: (v) => {{ capturedOutput += v.toString() + '\n'; }},
        err: (v) => console.error(v),
        own_acquire: (ptr) => ptr,
        own_release: (ptr) => {{}},
        list_create: (cap) => 1000n,
        list_create_1: (a) => 1001n,
        list_create_2: (a, b) => 1002n,
        list_create_3: (a, b, c) => 1003n,
        list_get: (l, i) => 0n,
        list_append: (l, v) => l,
        list_len: (l) => 0n,
    }},
    env: {{
        now: () => BigInt(Date.now()),
    }}
}};

WebAssembly.instantiate(wasmBytes, importObject).then(res => {{
    res.instance.exports.main();
    process.stdout.write(capturedOutput.trim());
}}).catch(err => {{
    console.error(err);
    process.exit(1);
}});
"#,
        wasm_path.to_string_lossy().replace('\\', "/")
    );

    let output = Command::new("node")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Node command failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Node execution failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[test]
#[ignore = "slow: runs full matrix across Cranelift, LLVM, and WASM"]
fn slow_test_differential_all_ten_programs() {
    let base_temp = std::env::temp_dir().join("datara_differential_tests");
    let _ = fs::create_dir_all(&base_temp);

    for (idx, tc) in DIFFERENTIAL_PROGRAMS.iter().enumerate() {
        let test_name = format!("{}_{}", idx + 1, tc.name);
        println!(
            "=== Running Differential Test #{}: {} ===",
            idx + 1,
            test_name
        );

        let src_file = base_temp.join(format!("{}.dtr", test_name));
        fs::write(&src_file, tc.source).expect("write dtr source");

        // ----------------------------------------------------
        // 1. Cranelift Execution
        // ----------------------------------------------------
        let clif_compiler = ForgenCompiler::new("release");
        let clif_exe = base_temp.join(format!("{}_clif.exe", test_name));
        let clif_res = clif_compiler.compile_file(&src_file, Some(&clif_exe));
        assert!(
            clif_res.success,
            "[{}] Cranelift compilation failed: {:?}",
            test_name, clif_res.error
        );
        assert!(clif_exe.exists(), "[{}] Cranelift exe missing", test_name);

        let (clif_stdout, clif_stderr, clif_code, _) = clif_compiler
            .codegen
            .run_executable(&clif_exe, &[])
            .expect("run clif exe");
        assert_eq!(
            clif_code, 0,
            "[{}] Cranelift execution non-zero exit: {}",
            test_name, clif_stderr
        );
        let clif_out = clif_stdout.trim().to_string();
        assert_eq!(
            clif_out, tc.expected_output,
            "[{}] Cranelift output mismatch. Expected {}, got {}",
            test_name, tc.expected_output, clif_out
        );

        // ----------------------------------------------------
        // 2. WASM Execution (via Node.js)
        // ----------------------------------------------------
        let dmir = clif_compiler
            .compile_source_to_dmir(tc.source, &format!("{}.dtr", test_name))
            .expect("lowering to DMIR for wasm");
        let wasm_file = base_temp.join(format!("{}.wasm", test_name));
        let wasm_res = WasmEmitter::emit_wasm_binary(&dmir, &wasm_file);
        assert!(
            wasm_res.is_ok(),
            "[{}] WASM emission failed: {:?}",
            test_name,
            wasm_res.err()
        );
        assert!(wasm_file.exists(), "[{}] WASM file missing", test_name);

        let wasm_out = run_wasm_node(&wasm_file).expect("run wasm via node");
        assert_eq!(
            wasm_out, tc.expected_output,
            "[{}] WASM output mismatch. Expected {}, got {}",
            test_name, tc.expected_output, wasm_out
        );

        // Differential assertion: Cranelift output must match WASM output bit-for-bit
        assert_eq!(
            clif_out, wasm_out,
            "[{}] Backend differential mismatch between Cranelift and WASM!",
            test_name
        );

        // ----------------------------------------------------
        // 3. LLVM Backend Verification
        // ----------------------------------------------------
        let llvm_compiler = ForgenCompiler::new("release").with_llvm(true);
        let llvm_res =
            llvm_compiler.compile_source(tc.source, &format!("{}_llvm.dtr", test_name), None);
        assert!(
            llvm_res.success,
            "[{}] LLVM compilation failed: {:?}",
            test_name, llvm_res.error
        );
        let llvm_ir = llvm_res.llvm_source.expect("LLVM IR generated");
        assert!(
            llvm_ir.contains("define i32 @main()"),
            "[{}] LLVM IR must contain main entry point",
            test_name
        );

        // If Clang is available, execute LLVM binary as well
        let llvm_exe = base_temp.join(format!("{}_llvm.exe", test_name));
        let llvm_file_res = llvm_compiler.compile_file(&src_file, Some(&llvm_exe));
        if llvm_file_res.success && llvm_exe.exists() {
            let llvm_run = Command::new(&llvm_exe)
                .output()
                .expect("execute llvm binary");
            let llvm_stdout = String::from_utf8_lossy(&llvm_run.stdout).trim().to_string();
            assert_eq!(
                llvm_stdout, tc.expected_output,
                "[{}] LLVM execution mismatch! Expected {}, got {}",
                test_name, tc.expected_output, llvm_stdout
            );
            assert_eq!(
                clif_out, llvm_stdout,
                "[{}] Differential mismatch between Cranelift and LLVM!",
                test_name
            );
            let _ = fs::remove_file(&llvm_exe);
        }

        // Cleanup test-specific artifacts
        let _ = fs::remove_file(&src_file);
        let _ = fs::remove_file(&clif_exe);
        let _ = fs::remove_file(&wasm_file);
    }

    let _ = fs::remove_dir_all(&base_temp);
}
