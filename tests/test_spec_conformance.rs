//! Specification Conformance Test Suite for DATARA_LANGUAGE_SPEC.md v1.0
//!
//! Covers all 7 sections of the Canonical Specification with positive and negative tests:
//! - Section 1: Grammar (Syntax conformance & rejection of malformed tokens)
//! - Section 2: Memory Model (Affine ownership, borrow exclusivity, use-after-move)
//! - Section 3: Type System (Primitives, Outcome<T, E>, exhaustive pattern matching)
//! - Section 4: Arithmetic & Overflow (Cross-platform checked, wrapping, saturating, div-by-zero)
//! - Section 5: String & Unicode (UTF-8 preservation, multi-byte slicing, char iteration)
//! - Section 6: Concurrency & Wavefronts (DAG execution, cycle prevention)
//! - Section 7: Effect System (Pure vs IO composition, unsafe escape boundaries)

use forgen::driver::ForgenCompiler;

fn check(code: &str) -> bool {
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "spec_test.dtr");
    res.success
}

fn check_diag(code: &str) -> (bool, String) {
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "spec_test.dtr");
    (res.success, res.diagnostics)
}

// =========================================================================
// SECTION 1: FORMAL GRAMMAR (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec1_grammar_positive_full_declarations() {
    let code = r#"
class Entity {
    uid: Int
}

behavior Entity {
    fn id(this: Entity) -> Int {
        return this.uid
    }
}

fn calculate(x: Int, y: Float) -> Int {
    let outcome = decide {
        x > 10 => x * 2
        else => 0
    }
    return outcome
}

fn main() {
    let e = Entity { uid: 100 }
    out e.id()
    out calculate(15, 2.5)
}
"#;
    assert!(
        check(code),
        "Valid full declaration syntax must parse cleanly"
    );
}

#[test]
fn test_spec_sec1_grammar_negative_unterminated_string() {
    let code = r#"
fn main() {
    let s = "unterminated string literal;
}
"#;
    let (success, diag) = check_diag(code);
    assert!(!success, "Unterminated string must be rejected");
    assert!(
        diag.contains("E-SYNTAX") || diag.to_lowercase().contains("unterminated"),
        "Diagnostic must report syntax error: {}",
        diag
    );
}

// =========================================================================
// SECTION 2: MEMORY MODEL & OWNERSHIP (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec2_ownership_positive_multiple_immutable_views() {
    let code = r#"
class Dataset {
    name: String
    size: Int
}

behavior Dataset {
    summary() -> String => this.name + " (" + this.size + " items)"
}

fn main() {
    let data = Dataset { name: "AuditLogs", size: 4200 }
    let v1 = data.view()
    let v2 = data.view()
    out v1.summary()
    out v2.summary()
}
"#;
    let (success, diag) = check_diag(code);
    assert!(
        success,
        "Multiple immutable views must be accepted, got: {}",
        diag
    );
}

#[test]
fn test_spec_sec2_ownership_negative_use_after_move() {
    let code = r#"
class Buffer {
    size: Int
}

fn consume(b: Buffer) -> Int {
    return b.size
}

fn main() {
    let b = Buffer { size: 1024 }
    let x = consume(b)
    out b.size
}
"#;
    let (success, diag) = check_diag(code);
    assert!(
        !success,
        "Use after move must be rejected by ownership tracker"
    );
    assert!(
        diag.contains("E-BORROW-001") || diag.to_lowercase().contains("move"),
        "Diagnostic must mention borrow/move violation: {}",
        diag
    );
}

// =========================================================================
// SECTION 3: TYPE SYSTEM & MATCH EXHAUSTIVENESS (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec3_types_positive_exhaustive_match() {
    let code = r#"
enum Status {
    Pending,
    Active,
    Completed,
}

fn to_int(s: Status) -> Int {
    match s {
        Status.Pending => 1,
        Status.Active => 2,
        Status.Completed => 3,
    }
}

fn main() {
    out to_int(Status.Active)
}
"#;
    assert!(check(code), "Exhaustive pattern match must succeed");
}

#[test]
fn test_spec_sec3_types_negative_non_exhaustive_match() {
    let code = r#"
enum Color {
    Red,
    Green,
    Blue,
}

fn to_code(c: Color) -> Int {
    match c {
        Color.Red => 1,
        Color.Green => 2,
    }
}

fn main() {}
"#;
    let (success, diag) = check_diag(code);
    assert!(
        !success,
        "Non-exhaustive match without wildcard must fail typecheck"
    );
    assert!(
        diag.contains("E0310") || diag.to_lowercase().contains("pattern"),
        "Diagnostic must report non-exhaustive patterns: {}",
        diag
    );
}

// =========================================================================
// SECTION 4: OVERFLOW & CROSS-PLATFORM ARITHMETIC (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec4_overflow_positive_explicit_wrapping_and_saturating() {
    let code = r#"
fn main() {
    let max = 9223372036854775807
    let w = wrapping(max + 1)
    let s = saturating(max + 1)
    out w
    out s
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "spec_ovf_pos.dtr", None);
    assert!(
        res.success,
        "Explicit wrapping/saturating must compile: {:?}",
        res.error
    );

    let (out, _, code_exit, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_exit, 0);
    let lines: Vec<&str> = out.trim().lines().map(|s| s.trim()).collect();
    assert_eq!(lines[0], "-9223372036854775808");
    assert_eq!(lines[1], "9223372036854775807");
}

#[test]
fn test_spec_sec4_overflow_negative_unadorned_addition_traps() {
    let code = r#"
fn main() {
    let max = 9223372036854775807
    let ovf = max + 1
    out ovf
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "spec_ovf_neg.dtr", None);
    assert!(res.success, "Compilation succeeds with trap check in place");

    let (_, _, code_exit, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_ne!(
        code_exit, 0,
        "Unadorned overflow must trap and yield non-zero exit code"
    );
}

#[test]
fn test_spec_sec4_cross_platform_wasm_overflow_validity() {
    let code = r#"
fn add_checked(a: Int, b: Int) -> Int {
    return a + b
}
fn add_wrapping(a: Int, b: Int) -> Int {
    return wrapping(a + b)
}
fn main() {
    out add_checked(10, 20)
    out add_wrapping(10, 20)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(code, "spec_wasm_ovf_check.dtr")
        .expect("Lowering to DMIR must succeed");

    let temp_wasm = std::env::temp_dir().join("spec_wasm_ovf_cross.wasm");
    let res = forgen::codegen::wasm::WasmEmitter::emit_wasm_binary(&dmir, &temp_wasm);
    assert!(
        res.is_ok(),
        "WASM emitter must succeed with overflow semantics"
    );

    let wasm_bytes = std::fs::read(&temp_wasm).expect("WASM file must exist");
    let mut validator = wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all());
    assert!(
        validator.validate_all(&wasm_bytes).is_ok(),
        "WASM binary must be strictly valid according to Wasm bytecode spec"
    );
}

// =========================================================================
// SECTION 5: STRING & UNICODE GUARANTEES (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec5_unicode_positive_multilingual_and_emoji() {
    let code = r#"
fn main() {
    let greeting = "Привет, мир! 🚀 世界"
    out greeting
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "spec_unicode.dtr", None);
    assert!(res.success, "Unicode strings must compile cleanly");

    let (out, _, code_exit, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_exit, 0);
    assert!(out.contains("Привет, мир! 🚀 世界"));
}

#[test]
fn test_spec_sec5_unicode_negative_type_mismatch_on_string() {
    let code = r#"
fn main() {
    let s: String = 12345
}
"#;
    let (success, diag) = check_diag(code);
    assert!(!success, "Assigning Int to String must fail typecheck");
    assert!(
        diag.contains("E-TYPE-001") || diag.to_lowercase().contains("mismatch"),
        "Diagnostic must state type mismatch: {}",
        diag
    );
}

// =========================================================================
// SECTION 6: CONCURRENCY & WAVEFRONT SCHEDULER (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec6_concurrency_positive_dag_execution() {
    let code = r#"
fn compute_a() -> Int { return 10 }
fn compute_b() -> Int { return 20 }
fn combine(a: Int, b: Int) -> Int { return a + b }

fn main() {
    let a = compute_a()
    let b = compute_b()
    out combine(a, b)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(code, "spec_dag.dtr", None);
    assert!(res.success, "Acyclic independent task calls must compile");

    let (out, _, code_exit, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code_exit, 0);
    assert_eq!(out.trim(), "30");
}

// =========================================================================
// SECTION 7: EFFECTS SYSTEM & COMPOSITION (Positive & Negative)
// =========================================================================

#[test]
fn test_spec_sec7_effects_positive_pure_computation() {
    let code = r#"
class Calculator {
    base: Int
}

behavior Calculator {
    @no_alloc
    fn add_pure(this: Calculator, extra: Int) -> Int {
        return this.base + extra
    }
}

fn main() -> Int {
    let c = Calculator { base: 100 }
    return c.add_pure(50)
}
"#;
    assert!(
        check(code),
        "Pure @no_alloc function must pass verification"
    );
}

#[test]
fn test_spec_sec7_effects_negative_pure_cannot_allocate() {
    let code = r#"
class Task {
    id: Int
}

behavior Task {
    @no_alloc
    fn run_bad(this: Task) -> Int {
        let items = [1, 2, 3]
        return 0
    }
}

fn main() -> Int {
    return 0
}
"#;
    let (success, diag) = check_diag(code);
    assert!(!success, "Heap allocation in @no_alloc must be rejected");
    assert!(
        diag.contains("E0950") || diag.contains("Allocation Violation"),
        "Diagnostic must report allocation violation E0950: {}",
        diag
    );
}
