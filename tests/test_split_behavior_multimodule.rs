use forgen::driver::ForgenCompiler;
use std::path::PathBuf;

#[test]
fn test_multimodule_reachability_and_stripping() {
    let files = vec![
        PathBuf::from("examples/user_modules/main.dtr"),
        PathBuf::from("examples/user_modules/core.dtr"),
        PathBuf::from("examples/user_modules/serialization.dtr"),
        PathBuf::from("examples/user_modules/security.dtr"),
        PathBuf::from("examples/user_modules/billing.dtr"),
    ];

    let compiler = ForgenCompiler::new("domain");
    let res = compiler.compile_files(&files, None);
    println!("=== CLIF ===\n{:?}", res.clif_source);
    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    println!(
        "=== STDOUT ===\n{}\n=== STDERR ===\n{}\n=== CODE ===\n{}",
        stdout, stderr, code
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Maria:30");

    let rep = res
        .optimization_report
        .expect("Optimization report must be present");
    println!("Modules analyzed: {}", rep.modules_analyzed);
    println!("Symbols analyzed: {}", rep.symbols_analyzed);
    println!("Reachable symbols: {}", rep.reachable_symbols);
    println!("Removed dead symbols: {}", rep.removed_symbols);

    assert_eq!(rep.modules_analyzed, 5);
    // Billing and Security methods were not called, so they must be removed by Domain Dead Symbol Elimination
    assert!(
        rep.removed_symbols >= 2,
        "Security and Billing methods must be eliminated in Domain mode"
    );
}
