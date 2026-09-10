use forgen::driver::ForgenCompiler;

#[test]
fn test_advanced_cse_optimization() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int {
    mut x = 0
    x = a + b
    mut y = 0
    y = a + b
    return x + y
}

fn main() {
    mut res = 0

    res = compute(10, 20)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "cse_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let trace = res
        .optimization_report
        .as_ref()
        .map(|r| &r.decision_trace)
        .unwrap();
    let cse_records: Vec<_> = trace.iter().filter(|d| d.pass == "CSE").collect();
    assert!(
        !cse_records.is_empty(),
        "CSE optimization pass must record decision traces"
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "60");
}

#[test]
fn test_advanced_licm_loop_optimization() {
    let source = r#"
fn loop_with_invariant(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        mut inv = 0
        inv = 100 * 2
        sum = sum + inv
        i = i + 1
    }
    return sum
}

fn main() {
    mut res = 0

    res = loop_with_invariant(10)
    out res
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "licm_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let trace = res
        .optimization_report
        .as_ref()
        .map(|r| &r.decision_trace)
        .unwrap();
    let licm_records: Vec<_> = trace.iter().filter(|d| d.pass == "LICM").collect();
    assert!(
        !licm_records.is_empty(),
        "LICM optimization pass must record decision traces"
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "2000");
}

#[test]
fn test_global_cse_across_blocks() {
    // The second `a * b` is in a block dominated by entry, where the first
    // `a * b` lives. Local (per-block) CSE cannot see this pair — only
    // dominance-based global CSE can. The record must say Applied, and the
    // result must be exact.
    let source = r#"
fn pick(a: Int, b: Int) -> Int {
    mut t = 0
    t = a * b
    if a > 0 {
        return t
    }
    return t + a * b
}

fn main() {
    out pick(3, 4)
    out pick(-3, 4)
}
"#;

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_source(source, "global_cse_test.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let trace = res
        .optimization_report
        .as_ref()
        .map(|r| &r.decision_trace)
        .unwrap();
    let cse_records: Vec<_> = trace
        .iter()
        .filter(|d| d.pass == "CSE" && d.decision == "Applied")
        .collect();
    assert!(
        !cse_records.is_empty(),
        "global CSE must fire on the cross-block duplicate a*b"
    );

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    // pick(3, 4): a > 0 -> return t = 12. pick(-3, 4): t + a*b = -12 + -12 = -24.
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0].trim(), "12");
    assert_eq!(lines[1].trim(), "-24");
}
