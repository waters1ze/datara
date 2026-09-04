use super::discovery::ProjectLayout;
use crate::driver::ForgenCompiler;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TestResultItem {
    pub name: String,
    pub path: PathBuf,
    pub passed: bool,
    pub duration_ms: u128,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TestReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_duration_ms: u128,
    pub results: Vec<TestResultItem>,
}

pub struct ProjectRunner;

impl ProjectRunner {
    /// Executes all tests discovered in the project layout
    pub fn run_tests(layout: &ProjectLayout, compiler: &ForgenCompiler) -> TestReport {
        let total_start = Instant::now();
        let mut report = TestReport::default();

        let mut tests_to_run = layout.test_files.clone();
        if tests_to_run.is_empty() {
            // Check if entry point is itself a test file
            if layout
                .entry_point
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("test_"))
                .unwrap_or(false)
            {
                tests_to_run.push(layout.entry_point.clone());
            }
        }

        report.total = tests_to_run.len();

        for test_file in tests_to_run {
            let test_name = test_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("test")
                .to_string();
            let test_start = Instant::now();

            // Build test combining supporting project sources if needed
            let mut compile_paths = vec![test_file.clone()];
            for src in &layout.source_files {
                if src.file_name().and_then(|n| n.to_str()) != Some("main.dtr") && src != &test_file
                {
                    compile_paths.push(src.clone());
                }
            }

            let comp_res = if compile_paths.len() == 1 {
                compiler.compile_file(&test_file, None)
            } else {
                compiler.compile_files(&compile_paths, None)
            };

            let duration_ms = test_start.elapsed().as_millis();

            if !comp_res.success {
                report.failed += 1;
                report.results.push(TestResultItem {
                    name: test_name,
                    path: test_file,
                    passed: false,
                    duration_ms,
                    output: String::new(),
                    error: comp_res.error,
                });
                continue;
            }

            let exe = match comp_res.exe_path {
                Some(p) => p,
                None => {
                    report.failed += 1;
                    report.results.push(TestResultItem {
                        name: test_name,
                        path: test_file,
                        passed: false,
                        duration_ms,
                        output: String::new(),
                        error: Some("Compilation succeeded but produced no executable".to_string()),
                    });
                    continue;
                }
            };
            match compiler.codegen.run_executable(&exe, &[]) {
                Ok((stdout, stderr, code, _)) => {
                    let passed =
                        code == 0 && !stdout.contains("FAIL:") && !stderr.contains("FAIL:");
                    if passed {
                        report.passed += 1;
                    } else {
                        report.failed += 1;
                    }
                    report.results.push(TestResultItem {
                        name: test_name,
                        path: test_file,
                        passed,
                        duration_ms,
                        output: stdout,
                        error: if !stderr.is_empty() {
                            Some(stderr)
                        } else {
                            None
                        },
                    });
                }
                Err(e) => {
                    report.failed += 1;
                    report.results.push(TestResultItem {
                        name: test_name,
                        path: test_file,
                        passed: false,
                        duration_ms,
                        output: String::new(),
                        error: Some(e),
                    });
                }
            }
        }

        report.total_duration_ms = total_start.elapsed().as_millis();
        report
    }

    /// Executes project benchmarks
    pub fn run_benches(layout: &ProjectLayout, compiler: &ForgenCompiler) -> Result<(), String> {
        let benches = &layout.bench_files;
        if benches.is_empty() {
            println!(
                "No benchmark files found in 'benches/'. Running default project throughput benchmark..."
            );
            let start = Instant::now();
            let res = compiler.compile_files(&layout.source_files, None);
            if !res.success {
                return Err(res.error.unwrap_or_else(|| "Compilation failed".into()));
            }
            let exe = match res.exe_path {
                Some(p) => p,
                None => {
                    return Err("Compilation succeeded but produced no executable".to_string());
                }
            };
            let (_, _, code, run_ms) = compiler.codegen.run_executable(&exe, &[])?;
            let total_ms = start.elapsed().as_millis();
            println!(
                "Benchmark Result: exit_code={}, execution_time={}ms, total_turnaround={}ms",
                code, run_ms, total_ms
            );
            return Ok(());
        }

        println!(
            "Running {} benchmarks in '{}'...",
            benches.len(),
            layout.name
        );
        for bench_file in benches {
            let name = bench_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("bench");

            let mut compile_paths = vec![bench_file.clone()];
            for src in &layout.source_files {
                if src.file_name().and_then(|n| n.to_str()) != Some("main.dtr") && src != bench_file
                {
                    compile_paths.push(src.clone());
                }
            }

            let res = if compile_paths.len() == 1 {
                compiler.compile_file(bench_file, None)
            } else {
                compiler.compile_files(&compile_paths, None)
            };

            if !res.success {
                eprintln!("Bench '{}' failed to compile: {:?}", name, res.error);
                continue;
            }
            let exe = match res.exe_path {
                Some(p) => p,
                None => {
                    eprintln!("Bench '{}' failed: no executable produced", name);
                    continue;
                }
            };
            let (_, _, code, run_ms) = compiler.codegen.run_executable(&exe, &[])?;
            println!("  bench {} ... {} ms (exit {})", name, run_ms, code);
        }
        Ok(())
    }
}
