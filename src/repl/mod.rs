//! Datara Interactive REPL Engine (Zero-Latency In-Process JIT Console)

use crate::driver::ForgenCompiler;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Configuration and session state for the interactive REPL
pub struct ReplSession {
    pub history: Vec<String>,
    pub top_level_declarations: Vec<String>,
    pub main_statements: Vec<String>,
    pub variable_names: Vec<String>,
    compiler: ForgenCompiler,
    session_exe: PathBuf,
}

impl Default for ReplSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplSession {
    pub fn new() -> Self {
        let exe_name = if cfg!(windows) {
            format!("datara_repl_{}.exe", std::process::id())
        } else {
            format!("datara_repl_{}", std::process::id())
        };
        let session_exe = std::env::temp_dir().join(exe_name);
        Self {
            history: Vec::new(),
            top_level_declarations: Vec::new(),
            main_statements: Vec::new(),
            variable_names: Vec::new(),
            compiler: ForgenCompiler::new("repl"),
            session_exe,
        }
    }

    /// Evaluates a single line or command in the REPL session
    pub fn eval_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Handle meta-commands
        if trimmed.starts_with(':') {
            return match trimmed {
                ":help" | ":h" => Some(
                    "Datara REPL Commands:\n  :vars    List active session variables\n  :clear   Reset session state\n  :history Show command history\n  :help    Display this help message\n  :exit    Quit the REPL"
                        .to_string(),
                ),
                ":vars" => {
                    if self.variable_names.is_empty() {
                        Some("No active variables in current session.".to_string())
                    } else {
                        Some(format!("Active variables: {}", self.variable_names.join(", ")))
                    }
                }
                ":clear" => {
                    self.top_level_declarations.clear();
                    self.main_statements.clear();
                    self.variable_names.clear();
                    Some("Session state cleared.".to_string())
                }
                ":history" => Some(
                    self.history
                        .iter()
                        .enumerate()
                        .map(|(i, h)| format!("  {}: {}", i + 1, h))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                ":exit" | ":quit" | ":q" => {
                    let _ = std::fs::remove_file(&self.session_exe);
                    let _ = std::fs::remove_file(self.session_exe.with_extension("obj"));
                    std::process::exit(0);
                }
                _ => Some(format!("Unknown command '{}'. Type ':help' for help.", trimmed)),
            };
        }

        // Handle bare function identifiers (introspection & tips)
        match trimmed {
            "input" => {
                return Some(
                    "=> <built-in function input(prompt: String = \"\") -> String>\n   (Чтобы вызвать функцию ввода, используйте круглые скобки: input() или input(\"подсказка\"))"
                        .to_string(),
                );
            }
            "input_int" => {
                return Some(
                    "=> <built-in function input_int(prompt: String = \"\") -> Int>\n   (Используйте круглые скобки для ввода целого числа: input_int(\"Введите число: \"))"
                        .to_string(),
                );
            }
            "input_float" => {
                return Some(
                    "=> <built-in function input_float(prompt: String = \"\") -> Float>\n   (Используйте круглые скобки для ввода дробного числа: input_float(\"Введите баланс: \"))"
                        .to_string(),
                );
            }
            "print" => {
                return Some(
                    "=> <built-in function print(...) -> Unit>\n   (Печать аргументов без переноса строки: print(\"текст\", x))"
                        .to_string(),
                );
            }
            "println" => {
                return Some(
                    "=> <built-in function println(...) -> Unit>\n   (Печать аргументов с переводом строки в конце: println(\"текст\", x))"
                        .to_string(),
                );
            }
            "len" => {
                return Some(
                    "=> <built-in function len(collection) -> Int>\n   (Получение длины строки или списка: len(items))"
                        .to_string(),
                );
            }
            "now" => {
                return Some(
                    "=> <built-in function now() -> Int>\n   (Текущий Unix timestamp в миллисекундах: now())"
                        .to_string(),
                );
            }
            _ => {}
        }

        self.history.push(trimmed.to_string());

        // Top-level declaration: fn, class, use, behavior, enum
        if trimmed.starts_with("fn ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("behavior ")
            || trimmed.starts_with("enum ")
        {
            self.top_level_declarations.push(trimmed.to_string());
            return Some("registered declaration".to_string());
        }

        // Main statement: let, mut, val, assignment
        if trimmed.starts_with("let ") || trimmed.starts_with("mut ") || trimmed.starts_with("val ")
        {
            let res = self.execute_expression(trimmed);
            if res.is_none() || !res.as_ref().unwrap().contains("error[") {
                // Extract variable name
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let var_name = parts[1]
                        .trim_end_matches(':')
                        .split('=')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !var_name.is_empty() && !self.variable_names.contains(&var_name.to_string())
                    {
                        self.variable_names.push(var_name.to_string());
                    }
                }
                self.main_statements.push(trimmed.to_string());
                return Some(format!(
                    "defined {}",
                    self.variable_names
                        .last()
                        .map(|s| s.as_str())
                        .unwrap_or("variable")
                ));
            }
            return res;
        }

        // Free expression or statement to evaluate
        self.execute_expression(trimmed)
    }

    /// Wraps expression in a synthesized program and executes it via in-process compiler
    fn execute_expression(&self, expr: &str) -> Option<String> {
        let mut source = String::new();
        for decl in &self.top_level_declarations {
            source.push_str(decl);
            source.push('\n');
        }

        source.push_str("fn main() {\n");
        for stmt in &self.main_statements {
            source.push_str("    ");
            source.push_str(stmt);
            source.push('\n');
        }

        // If expression looks like a statement, execute directly
        if expr.starts_with("out ")
            || expr.starts_with("println")
            || expr.starts_with("print")
            || expr.starts_with("eprintln")
            || expr.starts_with("let ")
            || expr.starts_with("mut ")
            || expr.starts_with("val ")
            || expr.contains(" = ")
        {
            source.push_str("    ");
            source.push_str(expr);
            source.push('\n');
        } else {
            // Expression to evaluate and print
            source.push_str("    println(");
            source.push_str(expr);
            source.push_str(")\n");
        }
        source.push_str("}\n");

        let res = self
            .compiler
            .compile_source(&source, "repl", Some(&self.session_exe));
        if !res.success {
            return res.error;
        }

        if expr.contains("input") {
            let _ = std::process::Command::new(&self.session_exe)
                .stdin(std::process::Stdio::inherit())
                .status();
            return None;
        }

        match self.compiler.codegen.run_executable(&self.session_exe, &[]) {
            Ok((stdout, stderr, code, _)) => {
                let out = stdout.trim();
                let err = stderr.trim();
                if code == 0 {
                    if out.is_empty() {
                        None
                    } else {
                        Some(format!("=> {}", out))
                    }
                } else {
                    Some(if !err.is_empty() {
                        err.to_string()
                    } else {
                        out.to_string()
                    })
                }
            }
            Err(e) => Some(format!("Execution failed: {}", e)),
        }
    }

    /// Starts interactive terminal REPL loop
    pub fn run_interactive() {
        println!(
            "================================================================================"
        );
        println!(" Datara Interactive REPL (Zero-Latency JIT Console v0.1.0)");
        println!(" Type ':help' for commands, ':exit' or Ctrl+C to quit.");
        println!(
            "================================================================================"
        );

        let mut session = ReplSession::new();
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!(">> ");
            let _ = stdout.flush();

            let mut input = String::new();
            if stdin.lock().read_line(&mut input).is_err() || input.is_empty() {
                // EOF or error
                break;
            }

            if let Some(res) = session.eval_line(&input) {
                println!("{}", res);
            }
        }
    }
}

impl Drop for ReplSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.session_exe);
        let _ = std::fs::remove_file(self.session_exe.with_extension("obj"));
    }
}
