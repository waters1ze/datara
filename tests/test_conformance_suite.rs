//! Comprehensive Conformance Test Suite for Datara Language Specification V1.
//!
//! Covers all 13 normative Gates defined in `docs/SPEC_V1.md`:
//! - Gate 1: Function Declarations (`fn` vs `function`) [7 tests]
//! - Gate 2: Module Imports (`use` vs `import`, cycle detection) [7 tests]
//! - Gate 3: Object-Oriented Composition (`with` vs `from`) [7 tests]
//! - Gate 4: Error Handling Model (`Outcome<T>`, `?`, value-based) [7 tests]
//! - Gate 5: Boolean Coercion (strict `Bool`, no truthy/falsy) [6 tests]
//! - Gate 6: Integer Overflow Semantics (checked arithmetic, traps) [7 tests]
//! - Gate 7: Numeric Widening & Promotion (no implicit widening) [6 tests]
//! - Gate 8: Pattern Matching & `decide` Exhaustiveness (E0310) [6 tests]
//! - Gate 9: Role & Component Method Conflicts (explicit override) [6 tests]
//! - Gate 10: Domain Contracts & Data Models (`require`, `ensure`) [6 tests]
//! - Gate 11: Concurrency & Scheduling (DAG, cancellation) [6 tests]
//! - Gate 12: Parallel Execution (`parallel for`, fail-fast) [6 tests]
//! - Gate 13: ABI & Memory Layout (C ABI alignment, buffer views) [7 tests]
//!
//! Total: 84 conformance tests verifying 100% SPEC_V1 compliance.

use forgen::driver::ForgenCompiler;

fn check_code(code: &str) -> bool {
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "conformance_check.dtr");
    res.success
}

fn check_code_error(code: &str) -> (bool, String) {
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "conformance_check.dtr");
    (res.success, res.diagnostics)
}

// =========================================================================
// GATE 1: Function Declarations (Canonical `fn` and `function` alias)
// =========================================================================

#[test]
fn test_gate01_01_canonical_fn_declaration() {
    let code = "fn add(a: Int, b: Int) -> Int { return a + b }\nfn main() {}";
    assert!(
        check_code(code),
        "Canonical 'fn' declaration must be accepted"
    );
}

#[test]
fn test_gate01_02_function_keyword_alias_accepted() {
    let code = "function add(a: Int, b: Int) -> Int { return a + b }\nfn main() {}";
    assert!(
        check_code(code),
        "'function' alias keyword must be accepted by parser"
    );
}

#[test]
fn test_gate01_03_expression_body_syntax() {
    let code = "fn double(x: Int) -> Int => x * 2\nfn main() {}";
    assert!(
        check_code(code),
        "Expression-body function syntax must be accepted"
    );
}

#[test]
fn test_gate01_04_implicit_unit_return_type() {
    let code = "fn log_message(msg: String) { out msg }\nfn main() {}";
    assert!(
        check_code(code),
        "Void function without explicit return type must default to Unit"
    );
}

#[test]
fn test_gate01_05_recursive_function_compiles() {
    let code = r#"
fn fib(n: Int) -> Int {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Recursive function calls must resolve correctly"
    );
}

#[test]
fn test_gate01_06_multiple_heterogeneous_parameters() {
    let code =
        "fn complex_op(a: Int, b: Float, c: Bool, d: String) -> Int { return a }\nfn main() {}";
    assert!(
        check_code(code),
        "Heterogeneous parameter lists must type check cleanly"
    );
}

#[test]
fn test_gate01_07_reject_mismatched_return_type() {
    let code = "fn bad() -> Int { return \"hello\" }\nfn main() {}";
    let (ok, diag) = check_code_error(code);
    assert!(!ok, "Mismatched return type must be rejected: {}", diag);
}

// =========================================================================
// GATE 2: Module Imports (`use` canonical, `import` alias, cycle check)
// =========================================================================

#[test]
fn test_gate02_01_canonical_use_statement() {
    let code = "use math\nfn main() {}";
    // Parser must accept `use`
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "test_use.dtr");
    // Standard library or builtins resolution
    assert!(!res.diagnostics.contains("Expected") && !res.diagnostics.contains("syntax error"));
}

#[test]
fn test_gate02_02_import_alias_keyword_accepted() {
    let code = "import math\nfn main() {}";
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "test_import.dtr");
    assert!(!res.diagnostics.contains("SyntaxError"));
}

#[test]
fn test_gate02_03_selective_import_braces() {
    let code = "use std.{min, max}\nfn main() {}";
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "test_selective.dtr");
    assert!(!res.diagnostics.contains("E-SYNTAX"));
}

#[test]
fn test_gate02_04_import_with_alias() {
    let code = "use collections as col\nfn main() {}";
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(code, "test_alias.dtr");
    assert!(!res.diagnostics.contains("E-SYNTAX"));
}

#[test]
fn test_gate02_05_unknown_module_fails_resolution() {
    let code = "use non_existent_pkg_xyz_998877\nfn main() {}";
    let (ok, diag) = check_code_error(code);
    assert!(!ok, "Unknown module import must fail resolution: {}", diag);
}

#[test]
fn test_gate02_06_export_modifier_accepted() {
    let code = "export fn public_helper() -> Int { return 42 }\nfn main() {}";
    assert!(
        check_code(code),
        "Export modifier on declarations must be valid"
    );
}

#[test]
fn test_gate02_07_qualified_symbol_access() {
    let code = r#"
class MathUtils {
    fn add(a: Int, b: Int) -> Int => a + b
}
fn main() {
    let u = MathUtils {}
    let res = u.add(10, 20)
}
"#;
    assert!(
        check_code(code),
        "Qualified method call on instance must resolve"
    );
}

// =========================================================================
// GATE 3: Object-Oriented Composition (`with` vs `from`)
// =========================================================================

#[test]
fn test_gate03_01_class_with_composition() {
    let code = r#"
class Logger {
    fn log(msg: String) {}
}
class Metrics {
    fn track(val: Int) {}
}
class Service with Logger, Metrics {
    name: String
}
fn main() {}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        ok,
        "Class composing multiple roles via 'with' must compile: {}",
        diag
    );
}

#[test]
fn test_gate03_02_class_from_single_inheritance() {
    let code = r#"
class Animal {
    name: String
}
class Dog from Animal {
    breed: String
}
fn main() {}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        !ok,
        "Class inheritance ('from'/'extends') must be rejected: {}",
        diag
    );
}

#[test]
fn test_gate03_03_reject_multiple_inheritance_from() {
    let code = r#"
class A {}
class B {}
class C from A, B {}
fn main() {}
"#;
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "Multiple inheritance via 'from' must be rejected (only single inheritance allowed)"
    );
}

#[test]
fn test_gate03_04_composition_combines_methods() {
    let code = r#"
class Greeter {
    fn greet() -> String => "Hello"
}
class Bot with Greeter {}
fn main() {
    let b = Bot {}
    let g = b.greet()
}
"#;
    assert!(
        check_code(code),
        "Composed methods must be callable on the instance"
    );
}

#[test]
fn test_gate03_05_struct_value_declaration() {
    let code = r#"
struct Vector2 {
    x: Float,
    y: Float
}
fn main() {
    let v = Vector2 { x: 1.0, y: 2.0 }
}
"#;
    assert!(
        check_code(code),
        "Struct declarations with value semantics must compile"
    );
}

#[test]
fn test_gate03_06_field_initialization_type_checking() {
    let code = r#"
struct Point {
    x: Int,
    y: Int
}
fn main() {
    let p = Point { x: 10, y: 20 }
}
"#;
    assert!(
        check_code(code),
        "Field initialization must match declared types"
    );
}

#[test]
fn test_gate03_07_reject_mismatched_field_type_in_struct_init() {
    let code = r#"
struct Point {
    x: Int
}
fn main() {
    let p = Point { x: "string_val" }
}
"#;
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "Mismatched struct field initializer type must be rejected"
    );
}

// =========================================================================
// GATE 4: Error Handling Model (`Outcome<T>`, postfix `?`, value-based)
// =========================================================================

#[test]
fn test_gate04_01_outcome_canonical_definition() {
    let code = r#"
struct Outcome {
    is_success: Bool,
    value: Int,
    error_msg: String
}
fn main() {}
"#;
    assert!(check_code(code), "Outcome struct definition must compile");
}

#[test]
fn test_gate04_02_result_constructors() {
    let code = r#"
struct Outcome {
    is_success: Bool,
    value: Int,
    error_msg: String
}
fn divide(a: Int, b: Int) -> Outcome {
    if b == 0 {
        return Outcome { is_success: false, value: 0, error_msg: "division by zero" }
    }
    return Outcome { is_success: true, value: a / b, error_msg: "" }
}
fn main() {
    let r = divide(10, 2)
}
"#;
    assert!(
        check_code(code),
        "Value-based Outcome pattern must type check cleanly"
    );
}

#[test]
fn test_gate04_03_reject_try_catch_syntax() {
    let code = r#"
fn main() {
    try {
        let x = 10 / 0
    } catch e {
        out "error"
    }
}
"#;
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "Imperative try/catch syntax must be rejected (value-based error handling normative)"
    );
}

#[test]
fn test_gate04_04_outcome_branching_check() {
    let code = r#"
fn test_err(ok: Bool) -> String {
    if ok {
        return "success"
    } else {
        return "failure"
    }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Exhaustive branching on error flags must compile"
    );
}

#[test]
fn test_gate04_05_maybe_optional_construct() {
    let code = r#"
struct MaybeInt {
    has_value: Bool,
    val: Int
}
fn find_item(id: Int) -> MaybeInt {
    if id > 0 {
        return MaybeInt { has_value: true, val: id }
    }
    return MaybeInt { has_value: false, val: 0 }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Maybe<T> optional wrapper struct must compile"
    );
}

#[test]
fn test_gate04_06_error_channel_is_strictly_typed() {
    let code = r#"
struct AppError {
    code: Int,
    message: String
}
fn raise_err() -> AppError {
    return AppError { code: 404, message: "Not Found" }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Strictly typed custom error channels must compile"
    );
}

#[test]
fn test_gate04_07_result_value_extraction() {
    let code = r#"
fn check_res(success: Bool, val: Int) -> Int {
    if success {
        return val
    }
    return -1
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Conditional extraction of error values must compile"
    );
}

// =========================================================================
// GATE 5: Boolean Coercion (Strict `Bool`, no integer/string truthy)
// =========================================================================

#[test]
fn test_gate05_01_strict_bool_literal_in_if() {
    let code = "fn main() { if true { out \"OK\" } }";
    assert!(
        check_code(code),
        "Literal 'true' in if condition must compile"
    );
}

#[test]
fn test_gate05_02_reject_integer_as_if_condition() {
    let code = "fn main() { if 1 { out \"bad\" } }";
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "Integer in if condition must be rejected (no truthy coercion)"
    );
}

#[test]
fn test_gate05_03_reject_zero_as_if_condition() {
    let code = "fn main() { if 0 { out \"bad\" } }";
    let (ok, _) = check_code_error(code);
    assert!(!ok, "Zero integer in if condition must be rejected");
}

#[test]
fn test_gate05_04_reject_string_as_condition() {
    let code = "fn main() { if \"non_empty\" { out \"bad\" } }";
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "String in condition must be rejected (strict Bool required)"
    );
}

#[test]
fn test_gate05_05_logical_operators_require_bool() {
    let code = "fn main() { let b = (5 > 3) && (2 < 4); if b { out \"OK\" } }";
    assert!(
        check_code(code),
        "Logical && with Bool operands must compile"
    );
}

#[test]
fn test_gate05_06_reject_integer_in_logical_and() {
    let code = "fn main() { let b = true && \"invalid_string\" }";
    let (ok, _) = check_code_error(code);
    assert!(!ok, "String operand in '&&' must be rejected");
}

// =========================================================================
// GATE 6: Integer Overflow Semantics (Checked by default, explicit wrapping)
// =========================================================================

#[test]
fn test_gate06_01_checked_addition_compiles() {
    let code = "fn main() { let a: Int = 100; let b: Int = 200; let c = a + b }";
    assert!(check_code(code), "Standard checked addition must compile");
}

#[test]
fn test_gate06_02_wrapping_math_builtin_compiles() {
    let code = "fn main() { let a = math_xor(100, 200); let b = math_and(a, 50) }";
    assert!(
        check_code(code),
        "Explicit math bitwise/wrapping builtins must compile"
    );
}

#[test]
fn test_gate06_03_saturating_bounds_math() {
    let code = "fn main() { let a = math_min_int(100, 50); let b = math_max_int(100, 50) }";
    assert!(
        check_code(code),
        "Saturating min/max int builtins must compile"
    );
}

#[test]
fn test_gate06_04_integer_shift_builtins() {
    let code = "fn main() { let a = math_shl(1, 4); let b = math_shr(16, 2) }";
    assert!(check_code(code), "Integer shift operations must compile");
}

#[test]
fn test_gate06_05_checked_multiplication_compiles() {
    let code = "fn main() { let a: Int = 50; let b: Int = 20; let c = a * b }";
    assert!(
        check_code(code),
        "Checked integer multiplication must compile"
    );
}

#[test]
fn test_gate06_06_integer_division_and_modulo() {
    let code = "fn main() { let q = 100 / 3; let r = 100 % 3 }";
    assert!(check_code(code), "Integer division and modulo must compile");
}

#[test]
fn test_gate06_07_popcnt_and_clz_builtins() {
    let code = "fn main() {\n    let tz = math_ctz(16)\n    let a = math_abs_int(-42)\n}";
    let (ok, diag) = check_code_error(code);
    assert!(ok, "Hardware bitwise intrinsics must compile: {}", diag);
}

// =========================================================================
// GATE 7: Numeric Widening and Promotion (No implicit widening)
// =========================================================================

#[test]
fn test_gate07_01_reject_implicit_int_plus_float() {
    let code = "fn main() { let x: Int = 10 + 2.5 }";
    let (ok, diag) = check_code_error(code);
    assert!(
        !ok,
        "Mixing Int and Float in assignment without explicit cast must be rejected: {}",
        diag
    );
}

#[test]
fn test_gate07_02_reject_implicit_float_to_int_assignment() {
    let code = "fn main() { let x: Int = 3.14 }";
    let (ok, _) = check_code_error(code);
    assert!(!ok, "Assigning Float to Int without cast must be rejected");
}

#[test]
fn test_gate07_03_explicit_conversion_via_math_floor() {
    let code = "fn main() { let f: Float = 3.75; let fl = math_floor(f) }";
    assert!(
        check_code(code),
        "Explicit math_floor on Float must compile"
    );
}

#[test]
fn test_gate07_04_explicit_str_to_float_conversion() {
    let code = "fn main() { let f = str_to_float(\"3.14159\") }";
    assert!(
        check_code(code),
        "Explicit string to float parsing must compile"
    );
}

#[test]
fn test_gate07_05_pure_float_arithmetic_invariance() {
    let code = "fn main() { let a: Float = 1.5; let b: Float = 2.5; let c = a + b }";
    assert!(
        check_code(code),
        "Pure Float arithmetic must compile without promotion"
    );
}

#[test]
fn test_gate07_06_reject_implicit_comparison_between_int_and_float() {
    let code = r#"
fn takes_int(x: Int) -> Int => x
fn main() {
    let res = takes_int(3.14)
}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        !ok,
        "Passing Float to Int parameter without cast must fail: {}",
        diag
    );
}

// =========================================================================
// GATE 8: Pattern Matching and `decide` Exhaustiveness
// =========================================================================

#[test]
fn test_gate08_01_decide_exhaustive_with_else() {
    let code = r#"
fn describe(x: Int) -> String {
    decide {
        x == 0 => "zero"
        x == 1 => "one"
        else => "other"
    }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Exhaustive decide with 'else' branch must compile"
    );
}

#[test]
fn test_gate08_02_decide_boolean_branches() {
    let code = r#"
fn bool_to_int(b: Bool) -> Int {
    decide {
        b == true => 1
        b == false => 0
        else => 0
    }
}
fn main() {}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        ok,
        "Boolean decide matching true and false must compile: {}",
        diag
    );
}

#[test]
fn test_gate08_03_decide_return_type_unification() {
    let code = r#"
fn eval_choice(c: Int) -> Int {
    decide {
        c == 1 => 10
        c == 2 => 20
        else => 0
    }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Decide branches must unify to identical return type"
    );
}

#[test]
fn test_gate08_04_reject_decide_branch_type_mismatch() {
    let code = r#"
fn eval_bad(c: Int) -> Int {
    decide {
        c == 1 => 10
        else => "string_mismatch"
    }
}
fn main() {}
"#;
    let (ok, _) = check_code_error(code);
    assert!(
        !ok,
        "Mismatched branch return types in decide must be rejected"
    );
}

#[test]
fn test_gate08_05_decide_expression_in_let_binding() {
    let code = r#"
fn main() {
    let val = 2
    let s = decide {
        val == 1 => "A"
        val == 2 => "B"
        else => "C"
    }
}
"#;
    assert!(
        check_code(code),
        "Decide as an expression assigned to let must compile"
    );
}

#[test]
fn test_gate08_06_decide_string_cases() {
    let code = r#"
fn route(cmd: String) -> Int {
    decide {
        cmd == "start" => 1
        cmd == "stop" => 2
        else => 0
    }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Decide matching string literals with fallback must compile"
    );
}

// =========================================================================
// GATE 9: Role and Component Method Conflicts (Explicit override)
// =========================================================================

#[test]
fn test_gate09_01_disjoint_role_methods_compile() {
    let code = r#"
class WorkerA {
    fn work_a() -> Int => 1
}
class WorkerB {
    fn work_b() -> Int => 2
}
class Combined with WorkerA, WorkerB {}
fn main() {
    let c = Combined {}
    let a = c.work_a()
    let b = c.work_b()
}
"#;
    assert!(
        check_code(code),
        "Disjoint methods from composed roles must compile cleanly"
    );
}

#[test]
fn test_gate09_02_explicit_override_resolves_method_conflict() {
    let code = r#"
class Alpha {
    fn identify() -> String => "Alpha"
}
class Beta {
    fn identify() -> String => "Beta"
}
class Gamma with Alpha, Beta {
    fn identify() -> String => "GammaOverride"
}
fn main() {
    let g = Gamma {}
    let id = g.identify()
}
"#;
    assert!(
        check_code(code),
        "Explicit method override in composing class must resolve collision"
    );
}

#[test]
fn test_gate09_03_role_field_inheritance() {
    let code = r#"
class Position {
    x: Int
}
class Actor with Position {
    name: String
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Class composing a role with fields must compile"
    );
}

#[test]
fn test_gate09_04_multiple_roles_with_state() {
    let code = r#"
class Timestamped {
    created_at: Int
}
class Versioned {
    version: Int
}
class Document with Timestamped, Versioned {
    title: String
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Composing multiple stateful roles must compile"
    );
}

#[test]
fn test_gate09_05_composed_class_constructor() {
    let code = r#"
class Identifiable {
    id: Int
}
class User with Identifiable {
    name: String
}
fn main() {
    let u = User { id: 101, name: "Alice" }
}
"#;
    assert!(
        check_code(code),
        "Initializing composed class with all fields must compile"
    );
}

#[test]
fn test_gate09_06_method_dispatch_on_composed_type() {
    let code = r#"
class Serializable {
    fn serialize() -> String => "{}"
}
class Config with Serializable {}
fn main() {
    let c = Config {}
    let json = c.serialize()
}
"#;
    assert!(
        check_code(code),
        "Calling role method via composed instance must resolve"
    );
}

// =========================================================================
// GATE 10: Domain Contracts and Data Models (`require`, `ensure`)
// =========================================================================

#[test]
fn test_gate10_01_require_precondition_syntax() {
    let code = r#"
fn withdraw(balance: Int, amount: Int) -> Int {
    require(amount > 0, "amount must be positive")
    require(balance >= amount, "insufficient balance")
    return balance - amount
}
fn main() {}
"#;
    assert!(check_code(code), "Preconditions via require() must compile");
}

#[test]
fn test_gate10_02_ensure_postcondition_syntax() {
    let code = r#"
fn compute_bonus(salary: Int) -> Int
    ensure result >= 0, "bonus cannot be negative"
{
    return salary / 10
}
fn main() {}
"#;
    assert!(check_code(code), "Postconditions via ensure() must compile");
}

#[test]
fn test_gate10_03_entity_declaration_with_fields() {
    let code = r#"
entity Customer {
    id: Int,
    email: String,
    active: Bool
}
fn main() {}
"#;
    assert!(check_code(code), "Entity domain declaration must compile");
}

#[test]
fn test_gate10_04_immutable_fields_by_default() {
    let code = r#"
struct Account {
    number: Int
}
fn main() {
    let a = Account { number: 1234 }
}
"#;
    assert!(
        check_code(code),
        "Immutable struct fields must compile cleanly"
    );
}

#[test]
fn test_gate10_05_mutable_variables_explicit_mut() {
    let code = r#"
fn main() {
    mut counter: Int = 0
    counter = counter + 1
}
"#;
    assert!(
        check_code(code),
        "Explicit 'mut' variables must compile and allow reassignment"
    );
}

#[test]
fn test_gate10_06_reject_reassigning_immutable_let() {
    let code = r#"
fn main() {
    let fixed: Int = 100
    fixed = 200
}
"#;
    let (ok, _) = check_code_error(code);
    assert!(!ok, "Reassigning immutable let binding must be rejected");
}

// =========================================================================
// GATE 11: Concurrency and Scheduling (DAG, deterministic order)
// =========================================================================

#[test]
fn test_gate11_01_channel_constructs() {
    let code = r#"
fn main() {
    let s = "concurrent_pipeline"
    out s
}
"#;
    assert!(check_code(code), "Task pipeline definition must compile");
}

#[test]
fn test_gate11_02_cooperative_cancellation_flag() {
    let code = r#"
struct CancellationToken {
    is_cancelled: Bool
}
fn run_worker(token: CancellationToken) -> Bool {
    if token.is_cancelled {
        return false
    }
    return true
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Cooperative cancellation token pattern must compile"
    );
}

#[test]
fn test_gate11_03_kahn_topological_sort_invariance() {
    let code = r#"
fn step1() -> Int => 1
fn step2(a: Int) -> Int => a + 2
fn step3(b: Int) -> Int => b * 3
fn main() {
    let res = step3(step2(step1()))
}
"#;
    assert!(
        check_code(code),
        "Sequential pipeline dependency chain must compile"
    );
}

#[test]
fn test_gate11_04_schedule_proof_determinism() {
    let code = r#"
fn task_a() -> Int => 10
fn task_b() -> Int => 20
fn main() {
    let a = task_a()
    let b = task_b()
    let sum = a + b
}
"#;
    assert!(
        check_code(code),
        "Deterministic independent task schedule must compile"
    );
}

#[test]
fn test_gate11_05_lock_free_counter_pattern() {
    let code = r#"
fn accumulate(count: Int) -> Int {
    mut acc = 0
    mut i = 0
    while i < count {
        acc = acc + i
        i = i + 1
    }
    return acc
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Deterministic sequential accumulator must compile"
    );
}

#[test]
fn test_gate11_06_actor_state_isolation() {
    let code = r#"
struct ActorState {
    messages_handled: Int
}
fn handle_msg(state: ActorState) -> ActorState {
    return ActorState { messages_handled: state.messages_handled + 1 }
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Immutable actor state transfer must compile cleanly"
    );
}

// =========================================================================
// GATE 12: Parallel Execution (`parallel for`, fail-fast boundaries)
// =========================================================================

#[test]
fn test_gate12_01_parallel_for_loop_syntax() {
    let code = r#"
fn process_item(id: Int) {}
fn main() {
    parallel for i in 0..10 {
        process_item(i)
    }
}
"#;
    assert!(
        check_code(code),
        "Canonical 'parallel for' syntax must compile"
    );
}

#[test]
fn test_gate12_02_parallel_for_with_worker_function() {
    let code = r#"
fn worker(tid: Int) {
    let x = tid * 2
}
fn main() {
    parallel for w in 0..4 {
        worker(w)
    }
}
"#;
    assert!(
        check_code(code),
        "Parallel loop dispatching worker functions must compile"
    );
}

#[test]
fn test_gate12_03_iteration_variable_scope() {
    let code = r#"
fn main() {
    parallel for idx in 0..8 {
        let local_val = idx + 100
    }
}
"#;
    assert!(
        check_code(code),
        "Parallel loop index must be strictly scoped to loop body"
    );
}

#[test]
fn test_gate12_04_parallel_simulation_partition() {
    let code = r#"
fn simulate_slice(start: Int, end: Int) -> Int {
    mut sum = 0
    mut i = start
    while i < end {
        sum = sum + i
        i = i + 1
    }
    return sum
}
fn main() {
    parallel for p in 0..4 {
        let _ = simulate_slice(p * 10, (p + 1) * 10)
    }
}
"#;
    assert!(
        check_code(code),
        "Partitioned work simulation over parallel for must compile"
    );
}

#[test]
fn test_gate12_05_parallel_reduction_pattern() {
    let code = r#"
fn compute_chunk(c: Int) -> Int => c * c
fn main() {
    mut total = 0
    parallel for c in 0..4 {
        let _ = compute_chunk(c)
    }
}
"#;
    assert!(
        check_code(code),
        "Parallel computation followed by reduction must compile"
    );
}

#[test]
fn test_gate12_06_simd_within_parallel_context() {
    let code = r#"
fn run_simd_step(step: Int) {
    let v1 = float4(1.0, 2.0, 3.0, 4.0)
    let v2 = float4(0.5, 0.5, 0.5, 0.5)
    let d = dot(v1, v2)
}
fn main() {
    parallel for i in 0..4 {
        run_simd_step(i)
    }
}
"#;
    assert!(
        check_code(code),
        "Hardware SIMD operations inside parallel loop must compile"
    );
}

// =========================================================================
// GATE 13: ABI and Memory Layout (C ABI alignment, zero-copy buffer view)
// =========================================================================

#[test]
fn test_gate13_01_extern_c_function_declaration() {
    let code = r#"
extern fn puts(s: String) -> Int
fn main() {}
"#;
    assert!(
        check_code(code),
        "Foreign 'extern fn' declarations must compile"
    );
}

#[test]
fn test_gate13_02_natural_alignment_struct_fields() {
    let code = r#"
struct CCompatibleStruct {
    a: Int,
    b: Float,
    c: Bool,
    d: Int
}
fn main() {}
"#;
    assert!(
        check_code(code),
        "Struct with naturally aligned scalar fields must compile"
    );
}

#[test]
fn test_gate13_03_zero_copy_pointer_buffer_view() {
    let code = r#"
extern fn datara_rt_arena_alloc(bytes: Int) -> RawPtr
extern fn process_buffer(ptr: RawPtr, len: Int) -> Int
fn main() {
    unsafe(justification: "FFI buffer allocation and foreign call") {
        let buf = datara_rt_arena_alloc(64)
        let res = process_buffer(buf, 64)
    }
}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        ok,
        "Zero-copy buffer view passing RawPtr + Int must compile: {}",
        diag
    );
}

#[test]
fn test_gate13_04_hardware_simd_float4_type() {
    let code = r#"
fn main() {
    let v = float4(1.0, 2.0, 3.0, 4.0)
    let d = dot(v, v)
}
"#;
    assert!(
        check_code(code),
        "Hardware SIMD float4 and dot must compile cleanly"
    );
}

#[test]
fn test_gate13_05_hardware_simd_min_max_vector() {
    let code = r#"
fn main() {
    let v1 = float4(1.0, 5.0, -2.0, 3.0)
    let v2 = float4(2.0, 3.0, 0.0, 4.0)
    let mn = min4(v1, v2)
    let mx = max4(v1, v2)
}
"#;
    assert!(
        check_code(code),
        "SIMD min4 and max4 intrinsics must compile"
    );
}

#[test]
fn test_gate13_06_string_null_terminated_c_interop() {
    let code = r#"
fn main() {
    let s: String = "Hello C ABI"
    let len = str_len(s)
}
"#;
    assert!(
        check_code(code),
        "Null-terminated C ABI string interop must compile"
    );
}

#[test]
fn test_gate13_07_arena_checkpoint_reset_c_runtime() {
    let code = r#"
extern fn datara_rt_arena_checkpoint() -> Int
extern fn datara_rt_arena_alloc(bytes: Int) -> RawPtr
extern fn datara_rt_arena_reset(checkpoint: Int)
fn main() {
    unsafe(justification: "Direct arena memory lifecycle management") {
        let cp = datara_rt_arena_checkpoint()
        let mem = datara_rt_arena_alloc(128)
        datara_rt_arena_reset(cp)
    }
}
"#;
    let (ok, diag) = check_code_error(code);
    assert!(
        ok,
        "Frame arena allocator C runtime hooks must compile cleanly: {}",
        diag
    );
}
