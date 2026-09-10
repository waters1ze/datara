use super::helpers::*;

struct DiffProgram {
    name: &'static str,
    source: &'static str,
    expected: &'static str,
}

const TWELVE_PROGRAMS: &[DiffProgram] = &[
    // 1. Arithmetic precedence, unary negation, large division
    DiffProgram {
        name: "p01_arith_precedence",
        source: r#"
fn main() {
    let a = 100
    let b = 25
    let c = 4
    let r = (a / b) * (c + 6) - (-10)
    out r
}
"#,
        expected: "50",
    },
    // 2. Recursive Fibonacci
    DiffProgram {
        name: "p02_fibonacci",
        source: r#"
fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}
fn main() {
    out fib(10)
}
"#,
        expected: "55",
    },
    // 3. Factorial with while loop
    DiffProgram {
        name: "p03_factorial",
        source: r#"
fn main() {
    mut n = 7
    mut fact = 1
    while n > 1 {
        fact = fact * n
        n = n - 1
    }
    out fact
}
"#,
        expected: "5040",
    },
    // 4. Floating point math & comparison
    DiffProgram {
        name: "p04_float_cmp",
        source: r#"
fn main() {
    let pi = 3.14159
    let r = 2.0
    let area = pi * (r * r)
    if area > 12.5 && area < 12.6 {
        out 1
    } else {
        out 0
    }
}
"#,
        expected: "1",
    },
    // 5. Short-circuit logic
    DiffProgram {
        name: "p05_short_circuit",
        source: r#"
fn main() {
    let a = 1
    let b = 0
    let res = (a == 1 || b == 1) && (a != 0)
    if res {
        out 42
    } else {
        out 0
    }
}
"#,
        expected: "42",
    },
    // 6. Function chaining
    DiffProgram {
        name: "p06_fn_chain",
        source: r#"
fn inc(x: Int) -> Int => x + 1
fn double(x: Int) -> Int => x * 2
fn square(x: Int) -> Int => x * x
fn main() {
    out square(double(inc(3)))
}
"#,
        expected: "64",
    },
    // 7. Nested while loops
    DiffProgram {
        name: "p07_nested_loops",
        source: r#"
fn main() {
    mut i = 0
    mut total = 0
    while i < 5 {
        mut j = 0
        while j < 4 {
            total = total + (i * j)
            j = j + 1
        }
        i = i + 1
    }
    out total
}
"#,
        expected: "60",
    },
    // 8. Bitwise shift & mask operations
    DiffProgram {
        name: "p08_bitwise",
        source: r#"
fn custom_shl(x: Int, n: Int) -> Int {
    mut res = x
    mut i = 0
    while i < n {
        res = res * 2
        i = i + 1
    }
    return res
}

fn custom_shr(x: Int, n: Int) -> Int {
    mut res = x
    mut i = 0
    while i < n {
        res = res / 2
        i = i + 1
    }
    return res
}

fn bit_mask(x: Int, mask: Int) -> Int
    require mask != 0
{
    return x % mask
}

fn main() {
    let a = 15
    let b = 51
    let shl_v = custom_shl(a, 2)
    let shr_v = custom_shr(b, 1)
    let mask_v = bit_mask(b, 16)
    out shl_v + shr_v + mask_v
}
"#,
        expected: "88",
    },
    // 9. Pattern matching (exhaustive match)
    DiffProgram {
        name: "p09_match_exhaustive",
        source: r#"
fn score(tier: Int) -> Int {
    match tier {
        1 => 100,
        2 => 250,
        3 => 500,
        _ => 0,
    }
}
fn main() {
    out score(1) + score(2) + score(3) + score(9)
}
"#,
        expected: "850",
    },
    // 10. String literal output
    DiffProgram {
        name: "p10_strings",
        source: r#"
fn main() {
    let full = "Datara Compiler"
    out full
}
"#,
        expected: "Datara Compiler",
    },
    // 11. List creation and summation
    DiffProgram {
        name: "p11_list_operations",
        source: r#"
fn main() {
    let nums = [10, 20, 30, 40, 50]
    mut sum = 0
    mut i = 0
    while i < nums.len() {
        sum = sum + nums[i]
        i = i + 1
    }
    out sum
}
"#,
        expected: "150",
    },
    // 12. Decide branching
    DiffProgram {
        name: "p12_decide_branching",
        source: r#"
fn grade(points: Int) -> Int {
    decide {
        points >= 90 => 5,
        points >= 75 => 4,
        points >= 50 => 3,
        else => 2,
    }
}
fn main() {
    out grade(95) + grade(80) + grade(60) + grade(40)
}
"#,
        expected: "14",
    },
];

#[test]
fn audit_differential_correctness_12_programs_across_clif_llvm_wasm() {
    for (i, p) in TWELVE_PROGRAMS.iter().enumerate() {
        println!(">>> [DIFF-12 #{}]: {} <<<", i + 1, p.name);

        // 1. Cranelift
        let (clif_out, clif_err, clif_code) =
            compile_and_run_cranelift(p.source, "release", &format!("clif_{}", p.name))
                .expect("Cranelift execution");
        assert_eq!(
            clif_code, 0,
            "[{}] Cranelift exit non-zero: {}",
            p.name, clif_err
        );
        assert_eq!(
            clif_out, p.expected,
            "[{}] Cranelift output mismatch",
            p.name
        );

        // 2. LLVM
        let (llvm_out, llvm_err, llvm_code) =
            compile_and_run_llvm(p.source, "release", &format!("llvm_{}", p.name))
                .expect("LLVM execution");
        assert_eq!(
            llvm_code, 0,
            "[{}] LLVM exit non-zero: {}",
            p.name, llvm_err
        );
        assert_eq!(llvm_out, p.expected, "[{}] LLVM output mismatch", p.name);

        // 3. WASM
        let wasm_out =
            compile_and_run_wasm(p.source, &format!("wasm_{}", p.name)).expect("WASM execution");
        assert_eq!(wasm_out, p.expected, "[{}] WASM output mismatch", p.name);

        // Strict cross-backend parity: Cranelift == LLVM == WASM
        assert_eq!(
            clif_out, llvm_out,
            "CRITICAL P0: Cranelift and LLVM outputs differ on '{}'!",
            p.name
        );
        assert_eq!(
            clif_out, wasm_out,
            "CRITICAL P0: Cranelift and WASM outputs differ on '{}'!",
            p.name
        );
    }
}
