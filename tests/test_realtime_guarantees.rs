use forgen::driver::ForgenCompiler;

#[test]
fn test_no_alloc_static_gate_rejects_heap_allocation() {
    let source = r#"
class Task {
    id: Int
}

behavior Task {
    @no_alloc
    fn run_bad() -> Int {
        let items = [1, 2, 3]
        return 0
    }
}

fn main() -> Int {
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "test_no_alloc_bad.dtr");
    assert!(
        !res.success,
        "@no_alloc must reject heap allocation in list literal"
    );
    assert!(
        res.diagnostics.contains("E0950") || res.diagnostics.contains("Allocation Violation"),
        "Diagnostics must contain E0950: {}",
        res.diagnostics
    );
}

#[test]
fn test_no_alloc_static_gate_accepts_zero_alloc_code() {
    let source = r#"
class Sensor {
    pin: Int
    last_value: Int
}

behavior Sensor {
    @no_alloc
    fn read_pin(self: Sensor, factor: Int) -> Int {
        let raw = self.pin * factor
        return raw + 10
    }
}

fn main() -> Int {
    let s = Sensor { pin: 4, last_value: 0 }
    return s.read_pin(2)
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "test_no_alloc_good.dtr");
    assert!(
        res.success,
        "@no_alloc must pass for pure stack/arithmetic code: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_no_panic_static_gate_rejects_unproven_panic() {
    let source = r#"
class Controller {
    mode: Int
}

behavior Controller {
    @no_panic
    fn trigger_fail() -> Void {
        panic("unexpected failure")
    }
}

fn main() -> Int {
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "test_no_panic_bad.dtr");
    assert!(!res.success, "@no_panic must reject panic() call");
    assert!(
        res.diagnostics.contains("E0951") || res.diagnostics.contains("Panic Violation"),
        "Diagnostics must contain E0951: {}",
        res.diagnostics
    );
}

#[test]
fn test_no_panic_static_gate_accepts_safe_code() {
    let source = r#"
class SafeMath {
    offset: Int
}

behavior SafeMath {
    @no_panic
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}

fn main() -> Int {
    return SafeMath.add(10, 20)
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "test_no_panic_good.dtr");
    assert!(
        res.success,
        "@no_panic must pass for safe arithmetic operations: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_stack_arena_allocation_guarantee() {
    let source = r#"
use sys.arena

fn main() -> Int {
    let arena = StackArena.stack(4096)
    let reset_arena = arena.reset()
    return reset_arena.used
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "test_arena.dtr");
    assert!(
        res.success,
        "StackArena must pass compilation: {:?}",
        res.diagnostics
    );
}
