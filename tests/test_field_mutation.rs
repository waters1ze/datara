//! Regression tests for mutating a class field from inside a method.
//!
//! Before this fix, `this.field = value` was silently dropped during lowering:
//! `Inst::SetField` existed in the DMIR enum and was handled by the verifier,
//! the optimizer and both backends, but **no lowering path ever constructed
//! it**. Every mutating method compiled to a no-op, so `counter.increment(5)`
//! left the field at its initial value and printed `0`.
//!
//! The second half of the bug was in SROA: an object passed to a method was
//! scalarized into registers, and the scalarized start value was forwarded
//! across the call, so even a correctly-emitted store was undone. SROA must
//! refuse to scalarize any object that reaches a method call.
//!
//! These tests pin down all three shapes: a single mutation, mutation
//! accumulated across a loop of calls, and the boundary where a *pure* struct
//! must still be scalarized while a *mutated* one must not.

use forgen::driver::ForgenCompiler;

/// Compile and run a Datara program, returning trimmed stdout with CRLF normalised.
fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, None);
    assert!(
        res.success,
        "compilation failed for {}: {:?}",
        name, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", name, code);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_field_mutation_accumulates_across_calls() {
    let out = run_datara(
        r#"
class Counter {
    val: Int
}

behavior Counter {
    increment(step: Int) -> Int {
        this.val = this.val + step
        return this.val
    }
}

fn main() {
    mut c = Counter { val: 0 }
    c.increment(5)
    out c.val
}
"#,
        "test_field_mut.dtr",
    );

    assert_eq!(
        out, "5",
        "increment(5) must mutate the field; a no-op lowering prints 0"
    );
}

#[test]
fn test_field_mutation_survives_method_call() {
    let out = run_datara(
        r#"
class Counter {
    val: Int
}

behavior Counter {
    increment(step: Int) -> Int {
        this.val = this.val + step
        return this.val
    }
}

fn compute_counter(n: Int) -> Int {
    mut c = Counter { val: 0 }
    mut i = 0
    while i < n {
        c.increment(i % 5)
        i = i + 1
    }
    return c.val
}

fn main() {
    out compute_counter(10)
}
"#,
        "test_field_mut_loop.dtr",
    );

    // Sum of i % 5 for i in 0..10 == 0+1+2+3+4+0+1+2+3+4 == 20.
    assert_eq!(
        out, "20",
        "mutation must accumulate across ten calls, not reset each iteration"
    );
}

#[test]
fn test_scalarization_respects_mutation_boundary() {
    let out = run_datara(
        r#"
class Point {
    x: Int
    y: Int
}

class Counter {
    val: Int
}

behavior Counter {
    bump() -> Int {
        this.val = this.val + 1
        return this.val
    }
}

fn pure_geometry() -> Int {
    mut p = Point { x: 10, y: 20 }
    return p.x + p.y
}

fn mutated_counter() -> Int {
    mut c = Counter { val: 0 }
    c.bump()
    c.bump()
    c.bump()
    return c.val
}

fn main() {
    out pure_geometry()
    out mutated_counter()
}
"#,
        "test_scalar_mut.dtr",
    );

    // 10 + 20, then three bumps. The pure Point may be scalarized; the
    // mutated Counter must not, because SROA would forward the start value
    // through the calls and undo every store.
    assert_eq!(
        out, "30\n3",
        "pure struct must still optimize, mutated struct must keep its state"
    );
}
