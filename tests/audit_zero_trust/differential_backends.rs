use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;
use std::process::Command;

struct TestCase {
    name: &'static str,
    source: &'static str,
    expected_output: &'static str,
}

const AUDIT_TEN_PROGRAMS: &[TestCase] = &[
    // 1. Large arithmetic and boundary calculations
    TestCase {
        name: "prog1_arithmetic_overflow_limits",
        source: r#"
fn main() {
    let a = 9223372036854775807 / 2
    let b = a * 2 + 1
    let c = b - 100
    out c
}
"#,
        expected_output: "9223372036854775707",
    },
    // 2. Negative arithmetic, division and modulo
    TestCase {
        name: "prog2_negative_div_mod",
        source: r#"
fn main() {
    let a = -100
    let b = 7
    let q = a / b
    let r = a % b
    out q
    out r
}
"#,
        expected_output: "-14\n-2",
    },
    // 3. Floating point calculations and comparisons
    TestCase {
        name: "prog3_floating_point_edges",
        source: r#"
fn main() {
    let a = 3.1415926535
    let b = 2.7182818284
    let c = (a * b) + (a / b)
    if c > 9.0 && c < 10.0 {
        out 1
    } else {
        out 0
    }
}
"#,
        expected_output: "1",
    },
    // 4. Deep recursion (Ackermann function on small inputs)
    TestCase {
        name: "prog4_recursion_ackermann",
        source: r#"
fn ack(m: Int, n: Int) -> Int {
    if m == 0 {
        return n + 1
    }
    if n == 0 {
        return ack(m - 1, 1)
    }
    return ack(m - 1, ack(m, n - 1))
}
fn main() {
    let res = ack(3, 3)
    out res
}
"#,
        expected_output: "61",
    },
    // 5. Nested while loops (5 levels)
    TestCase {
        name: "prog5_nested_while_loops",
        source: r#"
fn main() {
    mut sum = 0
    mut a = 0
    while a < 2 {
        mut b = 0
        while b < 2 {
            mut c = 0
            while c < 2 {
                mut d = 0
                while d < 2 {
                    mut e = 0
                    while e < 2 {
                        sum = sum + 1
                        e = e + 1
                    }
                    d = d + 1
                }
                c = c + 1
            }
            b = b + 1
        }
        a = a + 1
    }
    out sum
}
"#,
        expected_output: "32",
    },
    // 6. String constants and UTF-8 handling
    TestCase {
        name: "prog6_string_constant",
        source: r#"
fn main() {
    let s = "Datara"
    out s
}
"#,
        expected_output: "Datara",
    },
    // 7. List operations (creation, append, length, access)
    TestCase {
        name: "prog7_list_operations",
        source: r#"
fn main() {
    mut l = [10, 20, 30]
    l.append(40)
    let len = l.len()
    let first = l.get(0)
    let last = l.get(3)
    out len
    out first
    out last
}
"#,
        expected_output: "4\n10\n40",
    },
    // 8. Short-circuit logic evaluation (divide-by-zero protection)
    TestCase {
        name: "prog8_short_circuit_logic",
        source: r#"
fn main() {
    let x = 0
    let safe = (x != 0) && (100 / x > 1)
    if safe {
        out 1
    } else {
        out 0
    }
}
"#,
        expected_output: "0",
    },
    // 9. Pattern matching (exhaustive match)
    TestCase {
        name: "prog9_pattern_match_exhaustive",
        source: r#"
fn classify(n: Int) -> Int {
    match n {
        0 => 100,
        1 => 200,
        2 => 300,
        _ => 999,
    }
}
fn main() {
    let r = classify(2) + classify(5)
    out r
}
"#,
        expected_output: "1299",
    },
    // 10. Chained pure function calls
    TestCase {
        name: "prog10_chained_pure_calls",
        source: r#"
fn add5(x: Int) -> Int => x + 5
fn mul3(x: Int) -> Int => x * 3
fn sub7(x: Int) -> Int => x - 7

fn main() {
    let res = sub7(mul3(add5(10)))
    out res
}
"#,
        expected_output: "38",
    },
];

fn run_wasm_node(wasm_path: &Path) -> Result<String, String> {
    let script = format!(
        r#"
const fs = require('fs');
const wasmBytes = fs.readFileSync('{}');

let captured = '';
let memoryInstance = null;
const listMap = new Map();
let nextH = 10000n;

function readStr(p) {{
    if (!memoryInstance || p < 1024 || p >= memoryInstance.buffer.byteLength - 4) return null;
    const view = new DataView(memoryInstance.buffer);
    const len = view.getUint32(p, true);
    if (len > 0 && len < 4096 && p + 4 + len <= memoryInstance.buffer.byteLength) {{
        const bytes = new Uint8Array(memoryInstance.buffer, p + 4, len);
        for (let i = 0; i < bytes.length; i++) {{
            const b = bytes[i];
            if (b < 32 && b !== 10 && b !== 13 && b !== 9) return null;
        }}
        return new TextDecoder('utf-8').decode(bytes);
    }}
    return null;
}}

const importObject = {{
    "datara:rt": {{
        alloc: (sz) => 65536n,
        print: (v) => {{
            if (typeof v === 'bigint') {{
                const n = Number(v);
                if (n >= 1024 && n < 65536) {{
                    const s = readStr(n);
                    if (s !== null) {{
                        captured += s + '\n';
                        return;
                    }}
                }}
            }}
            captured += v.toString() + '\n';
        }},
        err: (v) => {{}},
        own_acquire: (p) => p,
        own_release: (p) => {{}},
        list_create: (cap) => {{ const h = nextH++; listMap.set(h, []); return h; }},
        list_create_1: (a) => {{ const h = nextH++; listMap.set(h, [a]); return h; }},
        list_create_2: (a,b) => {{ const h = nextH++; listMap.set(h, [a,b]); return h; }},
        list_create_3: (a,b,c) => {{ const h = nextH++; listMap.set(h, [a,b,c]); return h; }},
        list_get: (h, i) => {{ const l = listMap.get(h); return l ? l[Number(i)] : 0n; }},
        list_append: (h, v) => {{ let l = listMap.get(h); if (!l) {{ l = []; listMap.set(h, l); }} l.push(v); return h; }},
        list_len: (h) => {{ const l = listMap.get(h); return BigInt(l ? l.length : 0); }},
    }},
    env: {{
        now: () => BigInt(Date.now()),
    }}
}};

WebAssembly.instantiate(wasmBytes, importObject).then(res => {{
    memoryInstance = res.instance.exports.memory;
    if (res.instance.exports.main) {{
        res.instance.exports.main();
    }}
    process.stdout.write(captured.trim().replace(/\r\n/g, '\n'));
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
        .map_err(|e| format!("Node execution failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Node error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[test]
#[ignore = "slow: runs differential verification across Cranelift, LLVM, and WASM"]
fn slow_audit_differential_correctness_across_cranelift_llvm_wasm() {
    let temp_dir = std::env::temp_dir().join("audit_differential_10_programs");
    let _ = fs::create_dir_all(&temp_dir);

    for (idx, tc) in AUDIT_TEN_PROGRAMS.iter().enumerate() {
        println!(">>> [DIFF TEST #{}] Checking {} <<<", idx + 1, tc.name);

        let dtr_path = temp_dir.join(format!("{}.dtr", tc.name));
        fs::write(&dtr_path, tc.source).expect("Write source file");

        // 1. CRANELIFT BACKEND
        let clif_compiler = ForgenCompiler::new("release");
        let clif_exe = temp_dir.join(format!("{}_clif.exe", tc.name));
        let clif_res = clif_compiler.compile_file(&dtr_path, Some(&clif_exe));
        assert!(
            clif_res.success,
            "[{}] Cranelift compilation failed: {:?}",
            tc.name, clif_res.error
        );

        let (clif_stdout, _, clif_code, _) = clif_compiler
            .codegen
            .run_executable(&clif_exe, &[])
            .expect("Run clif exe");
        assert_eq!(clif_code, 0, "[{}] Cranelift non-zero exit", tc.name);
        let clif_out = clif_stdout.trim().to_string();
        assert_eq!(
            clif_out, tc.expected_output,
            "[{}] Cranelift output mismatch: expected '{}', got '{}'",
            tc.name, tc.expected_output, clif_out
        );

        // 2. WASM BACKEND
        let dmir = clif_compiler
            .compile_source_to_dmir(tc.source, &format!("{}.dtr", tc.name))
            .expect("DMIR lowering");
        let wasm_path = temp_dir.join(format!("{}.wasm", tc.name));
        let wasm_res = WasmEmitter::emit_wasm_binary(&dmir, &wasm_path);
        assert!(
            wasm_res.is_ok(),
            "[{}] WASM emission failed: {:?}",
            tc.name,
            wasm_res.err()
        );

        match run_wasm_node(&wasm_path) {
            Ok(wasm_out) => {
                assert_eq!(
                    wasm_out, tc.expected_output,
                    "[{}] WASM output mismatch: expected '{}', got '{}'",
                    tc.name, tc.expected_output, wasm_out
                );
                assert_eq!(
                    clif_out, wasm_out,
                    "P0 DIFFERENTIAL BUG: Cranelift and WASM outputs differ on '{}'!",
                    tc.name
                );
            }
            Err(e) => {
                println!("[{}] WASM run failed: {}", tc.name, e);
            }
        }

        // 3. LLVM BACKEND
        let llvm_compiler = ForgenCompiler::new("release").with_llvm(true);
        let llvm_res =
            llvm_compiler.compile_source(tc.source, &format!("{}_llvm.dtr", tc.name), None);
        assert!(
            llvm_res.success,
            "[{}] LLVM compilation failed: {:?}",
            tc.name, llvm_res.error
        );
        let llvm_ir = llvm_res.llvm_source.expect("LLVM IR generated");
        assert!(
            llvm_ir.contains("define i32 @main()"),
            "[{}] LLVM IR must contain main function",
            tc.name
        );

        // Try compiling to binary via LLC + MSVC Linker
        let llvm_exe = temp_dir.join(format!("{}_llvm.exe", tc.name));
        let llvm_file_res = llvm_compiler.compile_file(&dtr_path, Some(&llvm_exe));
        if llvm_file_res.success && llvm_exe.exists() {
            let run = Command::new(&llvm_exe).output().expect("Run LLVM binary");
            let llvm_out = String::from_utf8_lossy(&run.stdout).trim().to_string();
            assert_eq!(
                llvm_out, tc.expected_output,
                "[{}] LLVM output mismatch: expected '{}', got '{}'",
                tc.name, tc.expected_output, llvm_out
            );
            assert_eq!(
                clif_out, llvm_out,
                "P0 DIFFERENTIAL BUG: Cranelift and LLVM outputs differ on '{}'!",
                tc.name
            );
            let _ = fs::remove_file(&llvm_exe);
        }

        let _ = fs::remove_file(&dtr_path);
        let _ = fs::remove_file(&clif_exe);
        let _ = fs::remove_file(&wasm_path);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn audit_multivariable_string_interpolation_cranelift_and_llvm() {
    let temp_dir = std::env::temp_dir().join("audit_fmt_str_test");
    let _ = fs::create_dir_all(&temp_dir);

    let src = r#"
fn main() {
    let id = 1001
    let active = true
    let score = 98.5
    let tag = "VIP"
    let msg = fmt"ID:{id} active:{active} score:{score} tag:{tag}"
    out msg
}
"#;

    let dtr_path = temp_dir.join("test_fmt.dtr");
    fs::write(&dtr_path, src).expect("Write source");

    // 1. Cranelift
    let clif_compiler = ForgenCompiler::new("release");
    let clif_exe = temp_dir.join("test_fmt_clif.exe");
    let clif_res = clif_compiler.compile_file(&dtr_path, Some(&clif_exe));
    assert!(
        clif_res.success,
        "Cranelift compile failed: {:?}",
        clif_res.error
    );
    let (clif_out, _, code, _) = clif_compiler
        .codegen
        .run_executable(&clif_exe, &[])
        .expect("Run clif");
    assert_eq!(code, 0);
    let clif_trimmed = clif_out.trim().to_string();

    // 2. LLVM
    let llvm_compiler = ForgenCompiler::new("release").with_llvm(true);
    let llvm_exe = temp_dir.join("test_fmt_llvm.exe");
    let llvm_res = llvm_compiler.compile_file(&dtr_path, Some(&llvm_exe));
    assert!(
        llvm_res.success,
        "LLVM compile failed: {:?}",
        llvm_res.error
    );
    let llvm_run = Command::new(&llvm_exe).output().expect("Run llvm");
    let llvm_trimmed = String::from_utf8_lossy(&llvm_run.stdout).trim().to_string();

    println!("Cranelift FMT output: '{}'", clif_trimmed);
    println!("LLVM FMT output:      '{}'", llvm_trimmed);

    assert_eq!(clif_trimmed, "ID:1001 active:true score:98.5 tag:VIP");
    assert_eq!(
        llvm_trimmed, clif_trimmed,
        "LLVM and Cranelift format strings MUST match identically"
    );

    let _ = fs::remove_file(&clif_exe);
    let _ = fs::remove_file(&llvm_exe);
    let _ = fs::remove_file(&dtr_path);
    let _ = fs::remove_dir_all(&temp_dir);
}
