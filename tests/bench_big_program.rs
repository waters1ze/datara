use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn generate_multi_module_project(base_dir: &PathBuf, module_count: usize) -> String {
    let _ = fs::create_dir_all(base_dir);
    let mut main_dtr = String::new();
    main_dtr.push_str("fn main() {\n");
    main_dtr.push_str("    mut total = 0\n");

    for i in 0..module_count {
        let mod_content = format!(
            "fn compute_mod_{i}(x: Int) -> Int {{\n    return x * {factor} + {add}\n}}\n",
            i = i,
            factor = (i % 7) + 1,
            add = i + 3
        );
        let mod_path = base_dir.join(format!("module_{}.dtr", i));
        let _ = fs::write(&mod_path, mod_content);
        main_dtr.push_str(&format!(
            "    total = total + compute_mod_{}({})\n",
            i,
            i * 2
        ));
    }

    main_dtr.push_str("    out total\n");
    main_dtr.push_str("}\n");

    let main_path = base_dir.join("main.dtr");
    let _ = fs::write(&main_path, &main_dtr);

    main_dtr
}

#[test]
fn test_big_program_10_and_100_modules_benchmark() {
    let temp_base = std::env::temp_dir().join("forgen_big_program_bench");
    let _ = fs::remove_dir_all(&temp_base);
    let _ = fs::create_dir_all(&temp_base);

    let compiler = ForgenCompiler::new("domain");

    for &mod_count in &[10, 100] {
        let proj_dir = temp_base.join(format!("proj_{}_mods", mod_count));
        let _main_source = generate_multi_module_project(&proj_dir, mod_count);

        let mut compile_paths = Vec::new();
        compile_paths.push(proj_dir.join("main.dtr"));
        for i in 0..mod_count {
            compile_paths.push(proj_dir.join(format!("module_{}.dtr", i)));
        }

        // 1. Initial Compilation Time
        let t_start = Instant::now();
        let res1 = compiler.compile_files(&compile_paths, None);
        let compile_duration = t_start.elapsed();

        assert!(
            res1.success,
            "Big program ({} mods) failed to compile: {:?}",
            mod_count, res1.error
        );
        let exe1 = res1.exe_path.unwrap();

        // Binary size
        let bin_size = fs::metadata(&exe1).map(|m| m.len()).unwrap_or(0);

        // 2. Incremental Recompilation Time
        let t_inc_start = Instant::now();
        let res2 = compiler.compile_files(&compile_paths, None);
        let inc_duration = t_inc_start.elapsed();
        assert!(res2.success);

        // 3. Runtime execution
        let (stdout, stderr, code, runtime_ms) =
            compiler.codegen.run_executable(&exe1, &[]).unwrap();
        assert_eq!(code, 0, "Execution failed: {}", stderr);
        assert!(!stdout.is_empty());

        println!("==================================================================");
        println!(" BIG-PROGRAM BENCHMARK: {} MODULES", mod_count);
        println!("==================================================================");
        println!(
            " Initial Full Compile Time : {:>8.2} ms",
            compile_duration.as_secs_f64() * 1000.0
        );
        println!(
            " Incremental Compile Time  : {:>8.2} ms",
            inc_duration.as_secs_f64() * 1000.0
        );
        println!(" Execution Runtime         : {:>8.2} ms", runtime_ms as f64);
        println!(
            " Executable Binary Size    : {:>8.2} KB ({} bytes)",
            (bin_size as f64) / 1024.0,
            bin_size
        );
        println!(" Execution Output Result   : {}", stdout.trim());
        println!("==================================================================\n");
    }

    let _ = fs::remove_dir_all(&temp_base);
}
