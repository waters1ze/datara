use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn audit_modules_private_symbol_not_visible_outside() {
    let temp_dir = std::env::temp_dir().join("audit_mod_priv");
    let _ = fs::create_dir_all(&temp_dir);

    let mod_file = temp_dir.join("submod.dtr");
    let mod_code = r#"
export fn visible_fn() -> Int { return 1 }
fn hidden_fn() -> Int { return 2 }
"#;
    fs::write(&mod_file, mod_code).unwrap();

    let main_file = temp_dir.join("main.dtr");
    let main_code = r#"
use submod.hidden_fn
fn main() {
    out hidden_fn()
}
"#;
    fs::write(&main_file, main_code).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        !res.success,
        "Compiler MUST reject importing unexported/private symbol"
    );
    assert!(
        res.diagnostics.contains("E0804") || res.diagnostics.to_lowercase().contains("private"),
        "Diagnostic must mention private symbol: {}",
        res.diagnostics
    );
}

#[test]
fn audit_traits_dispatch_1_impl_vs_3_impls() {
    let source_1 = r#"
trait SingleWorker {
    fn run(&self) -> Int;
}
class WorkerA { val: Int }
impl SingleWorker for WorkerA {
    fn run(&self) -> Int { return self.val * 10 }
}
fn main() {
    let w = WorkerA { val: 5 }
    out w.run()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res_1 = compiler.compile_source(source_1, "trait_1.dtr", None);
    assert!(res_1.success);
    let clif_1 = res_1.clif_source.expect("CLIF");
    assert!(
        !clif_1.contains("call_indirect"),
        "1 impl must be direct/inlined call"
    );

    let source_3 = r#"
trait MultiWorker {
    fn run(&self) -> Int;
}
class Worker1 { val: Int }
class Worker2 { val: Int }
class Worker3 { val: Int }
impl MultiWorker for Worker1 { fn run(&self) -> Int { return self.val + 1 } }
impl MultiWorker for Worker2 { fn run(&self) -> Int { return self.val + 2 } }
impl MultiWorker for Worker3 { fn run(&self) -> Int { return self.val + 3 } }
fn main() {
    let w1 = Worker1 { val: 10 }
    let w2 = Worker2 { val: 20 }
    let w3 = Worker3 { val: 30 }
    out w1.run()
    out w2.run()
    out w3.run()
}
"#;
    let res_3 = compiler.compile_source(source_3, "trait_3.dtr", None);
    assert!(res_3.success);
    let clif_3 = res_3.clif_source.expect("CLIF");
    let has_indirect = clif_3.contains("call_indirect");
    println!(
        "FORENSIC FACT [Trait 3+]: does CLIF emit call_indirect? {}",
        has_indirect
    );

    let (stdout, _, code, _) = compiler
        .cranelift
        .run_executable(&res_3.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim().lines().collect::<Vec<_>>(),
        vec!["11", "22", "33"]
    );
}

#[test]
fn audit_traits_default_methods_work() {
    let source = r#"
trait Greet {
    fn name(&self) -> String;
    fn greeting(&self) -> String {
        return "Hello, " + self.name()
    }
}

class User {
    user_name: String
}

impl Greet for User {
    fn name(&self) -> String {
        return self.user_name
    }
}

fn main() {
    let u = User { user_name: "Alice" }
    out u.greeting()
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "trait_default.dtr", None);
    assert!(
        res.success,
        "Default method compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .cranelift
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Hello, Alice");
}

#[test]
fn audit_traits_bounds_violation_rejected() {
    let source = r#"
trait Printable {
    fn print(&self) -> String;
}

class NotPrintable {
    val: Int
}

fn display<T: Printable>(item: T) {
    out item.print()
}

fn main() {
    let np = NotPrintable { val: 123 }
    display(np)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "trait_bounds_violation.dtr", None);
    assert!(
        !res.success,
        "Passing a type that does not satisfy trait bound MUST be rejected at compile time"
    );
    assert!(
        res.diagnostics.to_lowercase().contains("bound")
            || res.diagnostics.to_lowercase().contains("trait")
            || res.diagnostics.contains("Printable"),
        "Diagnostic must specify trait bound violation: {}",
        res.diagnostics
    );
}
