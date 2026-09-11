use super::discovery::ProjectLayout;
use crate::ast::*;
use crate::diagnostics::DiagnosticEngine;
use crate::driver::ForgenCompiler;
use crate::lexer::Lexer;
use crate::parser::Parser;
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
    /// Discovers and returns names of all tests without executing them
    pub fn list_tests(layout: &ProjectLayout, filter: Option<&str>) -> Vec<String> {
        let mut test_names = Vec::new();
        let mut candidate_files = layout.test_files.clone();
        if candidate_files.is_empty() {
            if layout
                .entry_point
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("test_"))
                .unwrap_or(false)
            {
                candidate_files.push(layout.entry_point.clone());
            } else {
                for src in &layout.source_files {
                    candidate_files.push(src.clone());
                }
            }
        }

        for test_file in candidate_files {
            let file_content = match std::fs::read_to_string(&test_file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let file_str = test_file.to_str().unwrap_or("test.dtr");
            let mut diag = DiagnosticEngine::new("en");
            let mut lexer = Lexer::new(&file_content, file_str);
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, file_str);
            let program = parser.parse_program();

            let test_functions: Vec<String> = program
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Decl::Function(f) if f.attributes.iter().any(|a| a.name == "test") => {
                        Some(f.name.clone())
                    }
                    _ => None,
                })
                .collect();

            if !test_functions.is_empty() {
                for name in test_functions {
                    if let Some(flt) = filter {
                        if !name.contains(flt) {
                            continue;
                        }
                    }
                    test_names.push(name);
                }
            } else {
                let name = test_file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("test")
                    .to_string();
                if let Some(flt) = filter {
                    if !name.contains(flt) {
                        continue;
                    }
                }
                test_names.push(name);
            }
        }

        test_names.sort();
        test_names.dedup();
        test_names
    }

    /// Executes all tests discovered in the project layout
    pub fn run_tests(layout: &ProjectLayout, compiler: &ForgenCompiler) -> TestReport {
        Self::run_tests_filtered(layout, compiler, None)
    }

    /// Executes tests matching an optional name filter
    pub fn run_tests_filtered(
        layout: &ProjectLayout,
        compiler: &ForgenCompiler,
        filter: Option<&str>,
    ) -> TestReport {
        let total_start = Instant::now();
        let mut report = TestReport::default();

        let mut candidate_files = layout.test_files.clone();
        if candidate_files.is_empty() {
            // Check if entry point is itself a test file
            if layout
                .entry_point
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("test_"))
                .unwrap_or(false)
            {
                candidate_files.push(layout.entry_point.clone());
            } else {
                for src in &layout.source_files {
                    candidate_files.push(src.clone());
                }
            }
        }

        for test_file in candidate_files {
            let file_content = match std::fs::read_to_string(&test_file) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let file_str = test_file.to_str().unwrap_or("test.dtr");
            let mut diag = DiagnosticEngine::new(&compiler.locale);
            let mut lexer = Lexer::new(&file_content, file_str);
            let tokens = lexer.tokenize(&mut diag);
            let mut parser = Parser::new(tokens, &mut diag, file_str);
            let program = parser.parse_program();

            // Check if this file has @test or `test fn` annotated functions
            let test_functions: Vec<FunctionDecl> = program
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Decl::Function(f) if f.attributes.iter().any(|a| a.name == "test") => {
                        Some(f.clone())
                    }
                    _ => None,
                })
                .collect();

            if !test_functions.is_empty() {
                // Function-level tests
                for test_fn in test_functions {
                    if let Some(flt) = filter {
                        if !test_fn.name.contains(flt) {
                            continue;
                        }
                    }

                    let test_start = Instant::now();
                    let mut test_program = program.clone();
                    test_program.declarations.retain(|d| match d {
                        Decl::Function(f) => f.name != "main",
                        _ => true,
                    });

                    let test_call = Expr::Call {
                        callee: Box::new(Expr::Identifier(
                            test_fn.name.clone(),
                            test_fn.span.clone(),
                        )),
                        args: Vec::new(),
                        span: test_fn.span.clone(),
                    };
                    let main_body = Stmt::Block(
                        vec![Stmt::Expr(test_call, test_fn.span.clone())],
                        test_fn.span.clone(),
                    );
                    let main_fn = FunctionDecl {
                        name: "main".to_string(),
                        attributes: Vec::new(),
                        generic_params: Vec::new(),
                        generic_constraints: Vec::new(),
                        params: Vec::new(),
                        return_type: None,
                        requires: Vec::new(),
                        ensures: Vec::new(),
                        decreases: None,
                        body: Box::new(main_body),
                        is_expression_body: false,
                        is_export: false,
                        span: test_fn.span.clone(),
                    };
                    test_program.declarations.push(Decl::Function(main_fn));

                    let mut test_diag = DiagnosticEngine::new(&compiler.locale);
                    let comp_res = compiler.compile_ast(
                        test_program,
                        test_file.to_str().unwrap_or("test.dtr"),
                        None,
                        &mut test_diag,
                    );

                    let duration_ms = test_start.elapsed().as_millis();

                    if !comp_res.success {
                        report.failed += 1;
                        report.results.push(TestResultItem {
                            name: test_fn.name.clone(),
                            path: test_file.clone(),
                            passed: false,
                            duration_ms,
                            output: String::new(),
                            error: comp_res.error.or_else(|| {
                                if test_diag.has_errors() {
                                    Some(test_diag.format_all())
                                } else {
                                    None
                                }
                            }),
                        });
                        continue;
                    }

                    let exe = match comp_res.exe_path {
                        Some(p) => p,
                        None => {
                            report.failed += 1;
                            report.results.push(TestResultItem {
                                name: test_fn.name.clone(),
                                path: test_file.clone(),
                                passed: false,
                                duration_ms,
                                output: String::new(),
                                error: Some(
                                    "Compilation succeeded but produced no executable".to_string(),
                                ),
                            });
                            continue;
                        }
                    };

                    match compiler.codegen.run_executable(&exe, &[]) {
                        Ok((stdout, stderr, code, run_ms)) => {
                            let passed =
                                code == 0 && !stdout.contains("FAIL:") && !stderr.contains("FAIL:");
                            if passed {
                                report.passed += 1;
                            } else {
                                report.failed += 1;
                            }
                            report.results.push(TestResultItem {
                                name: test_fn.name.clone(),
                                path: test_file.clone(),
                                passed,
                                duration_ms: run_ms.max(duration_ms),
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
                                name: test_fn.name.clone(),
                                path: test_file.clone(),
                                passed: false,
                                duration_ms,
                                output: String::new(),
                                error: Some(e),
                            });
                        }
                    }
                }
            } else {
                // File-level test (e.g. test_*.dtr with fn main)
                let has_main = program.declarations.iter().any(|d| match d {
                    Decl::Function(f) => f.name == "main",
                    _ => false,
                });
                let is_explicit_test_file = layout.test_files.contains(&test_file)
                    || test_file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("test_"))
                        .unwrap_or(false);

                if !has_main || !is_explicit_test_file {
                    continue;
                }

                let test_name = test_file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("test")
                    .to_string();

                if let Some(flt) = filter {
                    if !test_name.contains(flt) {
                        continue;
                    }
                }

                let test_start = Instant::now();

                // Build test combining supporting project sources if needed
                let mut compile_paths = vec![test_file.clone()];
                for src in &layout.source_files {
                    if src.file_name().and_then(|n| n.to_str()) != Some("main.dtr")
                        && src != &test_file
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
                        path: test_file.clone(),
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
                            path: test_file.clone(),
                            passed: false,
                            duration_ms,
                            output: String::new(),
                            error: Some(
                                "Compilation succeeded but produced no executable".to_string(),
                            ),
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
                            path: test_file.clone(),
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
                            path: test_file.clone(),
                            passed: false,
                            duration_ms,
                            output: String::new(),
                            error: Some(e),
                        });
                    }
                }
            }
        }

        report.total = report.results.len();
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
