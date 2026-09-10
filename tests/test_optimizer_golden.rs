use forgen::dmir::Inst;
use forgen::driver::ForgenCompiler;

#[test]
fn test_golden_constant_folding_ir() {
    let source = r#"
fn compute() -> Int {
    mut a = 0
    a = 10
    mut b = 0
    b = 20
    mut c = 0
    c = a * b
    return c
}

fn main() {
    mut res = 0

    res = compute()
    out res
}
"#;

    let compiler_debug = ForgenCompiler::new("debug");
    let res_debug = compiler_debug.compile_source(source, "debug.dtr", None);
    assert!(res_debug.success);
    let dmir_before = res_debug.dmir_module.unwrap();
    let compute_debug = dmir_before.functions.get("compute").unwrap();

    // In debug mode (no opt), there are ConstInt(10), ConstInt(20), BinOp("*")
    let has_binop_before = compute_debug.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, Inst::BinOp { .. }));
    assert!(has_binop_before, "Debug IR must contain runtime BinOp");

    // In release mode, optimizer must fold 10 * 20 into 200
    let compiler_release = ForgenCompiler::new("release");
    let res_release = compiler_release.compile_source(source, "release.dtr", None);
    assert!(res_release.success);

    let rep = res_release.optimization_report.unwrap();
    println!(
        "Constant folding report: constants_folded = {}",
        rep.constants_folded
    );
    assert!(rep.constants_folded >= 1, "Must fold constants");

    let (stdout, _, code, _) = compiler_release
        .codegen
        .run_executable(&res_release.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "200");
}

#[test]
fn test_golden_inlining_pure_leaf_function() {
    let source = r#"
fn add(a: Int, b: Int) -> Int => a + b

fn main() {
    mut res = 0

    res = add(100, 200)
    out res
}
"#;

    let compiler_domain = ForgenCompiler::new("domain");
    let res_domain = compiler_domain.compile_source(source, "inlining.dtr", None);
    assert!(res_domain.success);

    let rep = res_domain.optimization_report.unwrap();
    println!(
        "Inlining report: functions_inlined = {}",
        rep.functions_inlined
    );
    assert!(
        rep.functions_inlined >= 1,
        "Pure leaf function 'add' must be inlined into main"
    );

    let dmir = res_domain.dmir_module.unwrap();
    let main_fn = dmir.functions.get("main").unwrap();

    // Main should no longer contain a call to add
    let has_call_to_add = main_fn.blocks[0].instructions.iter().any(|i| match i {
        Inst::Call { func, .. } => func == "add",
        _ => false,
    });
    assert!(
        !has_call_to_add,
        "Call to pure function 'add' must be eliminated by inliner"
    );

    let (stdout, _, code, _) = compiler_domain
        .codegen
        .run_executable(&res_domain.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "300");
}

#[test]
fn test_golden_sroa_stack_scalarization() {
    let source = r#"
class Point {
    x: Int
    y: Int
}

fn main() {
    mut p = Point { x: 15, y: 25 }
    let sum = p.x + p.y
    out sum
}
"#;

    let compiler_domain = ForgenCompiler::new("domain");
    let res_domain = compiler_domain.compile_source(source, "sroa.dtr", None);
    assert!(res_domain.success);

    let rep = res_domain.optimization_report.unwrap();
    println!(
        "SROA report: allocations_eliminated = {}",
        rep.allocations_eliminated
    );
    assert!(
        rep.allocations_eliminated >= 1,
        "Non-escaping local struct Point must be scalarized (SROA)"
    );

    let dmir = res_domain.dmir_module.unwrap();
    let main_fn = dmir.functions.get("main").unwrap();

    // StructInit must be eliminated from main
    let has_struct_init = main_fn.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, Inst::StructInit { .. }));
    assert!(
        !has_struct_init,
        "StructInit must be replaced by scalar values under SROA"
    );

    let (stdout, _, code, _) = compiler_domain
        .codegen
        .run_executable(&res_domain.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "40");
}
