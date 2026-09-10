use forgen::driver::ForgenCompiler;
use std::fs;
use std::time::Instant;

#[test]
fn test_synthetic_100_modules_domain_stress() {
    let temp_dir = std::env::temp_dir().join("forgen_stress_project");
    let src_dir = temp_dir.join("src");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&src_dir).unwrap();

    // 1. Create main.dtr
    let main_content = r#"
fn main() {
    mut u = Module1_Class { id: 100 }
    out u.id
}
"#;
    fs::write(src_dir.join("main.dtr"), main_content).unwrap();

    // 2. Generate 100 modules with 10 symbols each = 1,000+ symbols
    for i in 1..=100 {
        let mut mod_content = format!("class Module{}_Class {{\n    id: Int\n}}\n\n", i);

        for s in 1..=10 {
            mod_content.push_str(&format!(
                "fn module{}_helper_{}(x: Int) -> Int => x + {}\n",
                i, s, s
            ));
        }

        fs::write(src_dir.join(format!("module_{}.dtr", i)), mod_content).unwrap();
    }

    let compiler = ForgenCompiler::new("domain");
    let start = Instant::now();
    let res = compiler.compile_project(&temp_dir, None);
    let duration = start.elapsed();

    println!("============================================================");
    println!("        SYNTHETIC 100-MODULE STRESS TEST RESULTS            ");
    println!("============================================================");
    println!(" Modules discovered: 101");
    println!(" Total elapsed time: {:?}", duration);

    assert!(res.success, "Stress compilation failed: {:?}", res.error);
    let rep = res.optimization_report.expect("Domain report required");

    println!(" Modules analyzed:   {}", rep.modules_analyzed);
    println!(" Symbols analyzed:   {}", rep.symbols_analyzed);
    println!(" Reachable symbols:  {}", rep.reachable_symbols);
    println!(" Removed symbols:    {}", rep.removed_symbols);
    println!(
        " Output binary:      {}",
        res.exe_path.as_ref().unwrap().display()
    );
    println!("============================================================");

    assert_eq!(rep.modules_analyzed, 101);
    assert!(
        rep.symbols_analyzed >= 1000,
        "Must analyze over 1000 symbols"
    );
    // All helper functions are uncalled, so they must be eliminated
    assert!(rep.removed_symbols >= 990, "Must prune uncalled helpers");

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "100");

    let _ = fs::remove_dir_all(&temp_dir);
}
