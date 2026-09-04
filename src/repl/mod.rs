//! Datara Interactive REPL Engine (Zero-Latency In-Process JIT Console)

use crate::driver::ForgenCompiler;
use std::io::{self, BufRead, Write};

/// Calculates the delta of unclosed braces in a line, ignoring string literals and comments.
pub fn count_brace_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else {
            // Line comment //: ignore rest of line
            if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                break;
            }
            if ch == '"' {
                in_string = true;
            } else if ch == '{' {
                delta += 1;
            } else if ch == '}' {
                delta -= 1;
            }
        }
        i += 1;
    }
    delta
}

/// Configuration and session state for the interactive REPL
pub struct ReplSession {
    pub history: Vec<String>,
    pub top_level_declarations: Vec<String>,
    pub main_statements: Vec<String>,
    pub variable_names: Vec<String>,
    pub buffer: String,
    pub brace_depth: i32,
    compiler: ForgenCompiler,
}

impl Default for ReplSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplSession {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            top_level_declarations: Vec::new(),
            main_statements: Vec::new(),
            variable_names: Vec::new(),
            buffer: String::new(),
            brace_depth: 0,
            compiler: ForgenCompiler::new("repl"),
        }
    }

    /// Feed a line from user input or paste stream.
    /// Returns Some(output) when a complete command or block has been evaluated,
    /// or None when more lines are needed (e.g. unclosed braces in multi-line block).
    pub fn feed_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();

        // Cancellation meta-command during multi-line input
        if self.brace_depth > 0 {
            if trimmed == ":cancel" || trimmed == ":clear" {
                self.buffer.clear();
                self.brace_depth = 0;
                return Some("Pending multi-line input cancelled.".to_string());
            }
            let delta = count_brace_delta(line);
            self.buffer.push_str(line);
            self.buffer.push('\n');
            self.brace_depth += delta;

            if self.brace_depth <= 0 {
                let complete_block = std::mem::take(&mut self.buffer);
                self.brace_depth = 0;
                return self.eval_block(&complete_block);
            }
            return None;
        }

        if trimmed.is_empty() {
            return None;
        }

        let delta = count_brace_delta(line);
        if delta > 0 {
            self.buffer = line.to_string();
            self.buffer.push('\n');
            self.brace_depth = delta;
            return None;
        }

        self.eval_block(line)
    }

    /// Backward-compatible eval_line that processes a single line or starts/continues a block
    pub fn eval_line(&mut self, line: &str) -> Option<String> {
        self.feed_line(line)
    }

    /// Evaluates a complete, balanced block or meta-command
    pub fn eval_block(&mut self, block: &str) -> Option<String> {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Handle meta-commands
        if trimmed.starts_with(':') {
            return match trimmed {
                ":help" | ":h" => Some(
                    "Datara REPL Commands:\n  :vars    List active session variables\n  :clear   Reset session state\n  :cancel  Cancel current multi-line input\n  :history Show command history\n  :help    Display this help message\n  :exit    Quit the REPL"
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
                    self.buffer.clear();
                    self.brace_depth = 0;
                    Some("Session state cleared.".to_string())
                }
                ":cancel" => {
                    self.buffer.clear();
                    self.brace_depth = 0;
                    Some("No active multi-line input.".to_string())
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

        // Special case: full user-defined `fn main() { ... }`
        // Execute directly including any previous top-level declarations!
        if trimmed.starts_with("fn main") {
            let mut source = String::new();
            for decl in &self.top_level_declarations {
                source.push_str(decl);
                source.push('\n');
            }
            source.push_str(trimmed);
            source.push('\n');

            return match self.compiler.run_source(&source, "repl", &[], true) {
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
                Err(e) => Some(e),
            };
        }

        // Top-level declaration: fn, class, entity, behavior, role, component, packet, enum, use, extern
        if trimmed.starts_with("fn ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("entity ")
            || trimmed.starts_with("behavior ")
            || trimmed.starts_with("role ")
            || trimmed.starts_with("component ")
            || trimmed.starts_with("packet ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("extern ")
        {
            // Verify declaration syntax before saving
            let mut test_source = String::new();
            for decl in &self.top_level_declarations {
                test_source.push_str(decl);
                test_source.push('\n');
            }
            test_source.push_str(trimmed);
            test_source.push_str("\nfn main() {}\n");

            let res = self.compiler.check_source(&test_source, "repl_test");
            if !res.success {
                return res.error;
            }

            let decl_name = trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or("declaration")
                .trim_end_matches('{')
                .trim_end_matches('(')
                .trim_end_matches(':');

            self.top_level_declarations.push(trimmed.to_string());
            return Some(format!("registered declaration: {}", decl_name));
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

        let trimmed = expr.trim();
        let is_direct_stmt = trimmed.starts_with("out ")
            || trimmed.starts_with("println")
            || trimmed.starts_with("print")
            || trimmed.starts_with("eprintln")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("mut ")
            || trimmed.starts_with("val ")
            || trimmed.starts_with("if ")
            || trimmed.starts_with("while ")
            || trimmed.starts_with("for ")
            || trimmed.contains(" = ");

        if !is_direct_stmt {
            // First try evaluating as an expression whose result is printed
            let mut expr_source = source.clone();
            expr_source.push_str("    let __repl_res = ");
            expr_source.push_str(trimmed);
            expr_source.push_str(";\n    println(__repl_res)\n}\n");

            if let Ok((stdout, stderr, code, _)) =
                self.compiler.run_source(&expr_source, "repl", &[], true)
            {
                let out = stdout.trim();
                let err = stderr.trim();
                if code == 0 {
                    return if out.is_empty() {
                        None
                    } else {
                        Some(format!("=> {}", out))
                    };
                } else if !err.is_empty() {
                    return Some(err.to_string());
                } else if !out.is_empty() {
                    return Some(out.to_string());
                }
            }
        }

        // Evaluate directly as statements
        for line in trimmed.lines() {
            source.push_str("    ");
            source.push_str(line);
            source.push('\n');
        }
        source.push_str("}\n");

        let capture = !expr.contains("input");
        match self.compiler.run_source(&source, "repl", &[], capture) {
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
            Err(e) => Some(e),
        }
    }

    /// Starts interactive terminal REPL loop
    pub fn run_interactive() {
        println!(
            "================================================================================"
        );
        println!(
            " Datara Interactive REPL (Zero-Latency In-Memory JIT Console v{})",
            env!("CARGO_PKG_VERSION")
        );
        println!(
            " In-memory JIT execution active: zero disk artifacts, sub-millisecond evaluation."
        );
        println!(" Type ':help' for commands, ':exit' or Ctrl+C to quit.");
        println!(
            "================================================================================"
        );

        let mut session = ReplSession::new();
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            if session.brace_depth > 0 {
                print!(".. ");
            } else {
                print!(">> ");
            }
            let _ = stdout.flush();

            let mut input = String::new();
            if stdin.lock().read_line(&mut input).is_err() || input.is_empty() {
                // EOF or error
                break;
            }

            if let Some(res) = session.feed_line(&input) {
                println!("{}", res);
            }
        }
    }
}
