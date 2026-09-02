use forgen::driver::ForgenCompiler;
use std::path::PathBuf;

#[test]
fn test_stdlib_all_modules_compilation_and_execution() {
    let compiler = ForgenCompiler::new("release");
    let files = vec![
        PathBuf::from("examples/stdlib_test/main.dtr"),
        PathBuf::from("stdlib/io/fs.dtr"),
        PathBuf::from("stdlib/io/args.dtr"),
        PathBuf::from("stdlib/text/string.dtr"),
        PathBuf::from("stdlib/text/format.dtr"),
        PathBuf::from("stdlib/collections/list.dtr"),
        PathBuf::from("stdlib/collections/map.dtr"),
        PathBuf::from("stdlib/result/result.dtr"),
        PathBuf::from("stdlib/result/option.dtr"),
        PathBuf::from("stdlib/time/clock.dtr"),
        PathBuf::from("stdlib/json/types.dtr"),
        PathBuf::from("stdlib/json/parser.dtr"),
    ];

    let res = compiler.compile_files(&files, None);
    assert!(
        res.success,
        "Stdlib compilation failed:\n{}\n{:?}",
        res.diagnostics, res.error
    );
    if let Some(dmir) = &res.dmir_module {
        println!("[STDLIB DMIR FUNCTIONS]:");
        for (name, func) in &dmir.functions {
            println!(
                "FUNCTION: {} (params: {:?}, ret: {})",
                name, func.params, func.return_type
            );
            for b in &func.blocks {
                println!("  BLOCK {}:", b.id.0);
                for inst in &b.instructions {
                    println!("    {:?}", inst);
                }
                println!("    TERMINATOR: {:?}", b.terminator);
            }
        }
    }

    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    println!(
        "[STDLIB OUTPUT]:\nCODE: {}, STDOUT: [{}], STDERR: [{}]",
        code, stdout, stderr
    );
    assert_eq!(code, 0, "Execution failed with stderr: {}", stderr);

    println!("[STDLIB TEST OUTPUT]:\n{}", stdout);

    assert!(stdout.contains("Quoted: 'file.txt'"));
    assert!(stdout.contains("  item"));
    assert!(stdout.contains("Option: 42"));
    assert!(stdout.contains("Result: OK"));
    assert!(stdout.contains("Clock valid: true"));
    assert!(stdout.contains("JSON Service: datara"));
    assert!(stdout.contains("JSON Port: 8080"));
}
