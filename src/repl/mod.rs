//! Datara Interactive REPL Engine (Zero-Latency JIT Console)

use std::io::{self, BufRead, Write};
use std::process::Command;

/// Configuration and session state for the interactive REPL
pub struct ReplSession {
    pub history: Vec<String>,
    pub accumulated_declarations: Vec<String>,
    pub variable_names: Vec<String>,
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
            accumulated_declarations: Vec::new(),
            variable_names: Vec::new(),
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
                    self.accumulated_declarations.clear();
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
                    std::process::exit(0);
                }
                _ => Some(format!("Unknown command '{}'. Type ':help' for help.", trimmed)),
            };
        }

        self.history.push(trimmed.to_string());

        // Check if line is a declaration (let, mut, val, fn, class, use)
        if trimmed.starts_with("let ")
            || trimmed.starts_with("mut ")
            || trimmed.starts_with("val ")
        {
            // Extract variable name
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let var_name = parts[1].trim_end_matches(':').split('=').next().unwrap_or("").trim();
                if !var_name.is_empty() && !self.variable_names.contains(&var_name.to_string()) {
                    self.variable_names.push(var_name.to_string());
                }
            }
            self.accumulated_declarations.push(trimmed.to_string());
            Some(format!("defined {}", parts.get(1).unwrap_or(&"variable")))
        } else if trimmed.starts_with("fn ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("behavior ")
        {
            self.accumulated_declarations.push(trimmed.to_string());
            Some("registered declaration".to_string())
        } else {
            // Free expression or statement to evaluate
            self.execute_expression(trimmed)
        }
    }

    /// Wraps expression in a synthesized program and executes it via forgen
    fn execute_expression(&self, expr: &str) -> Option<String> {
        let mut source = String::new();
        for decl in &self.accumulated_declarations {
            source.push_str(decl);
            source.push('\n');
        }

        source.push_str("fn main() {\n");
        // If expression looks like a statement (contains '=' or starts with 'out ', 'print'), execute as is
        if expr.starts_with("out ") || expr.starts_with("println") || expr.starts_with("print") || expr.contains(" = ") {
            source.push_str("    ");
            source.push_str(expr);
            source.push('\n');
        } else {
            // Print expression value directly
            source.push_str("    let __repl_res = ");
            source.push_str(expr);
            source.push('\n');
            source.push_str("    out __repl_res\n");
        }
        source.push_str("}\n");

        let temp_dir = std::env::temp_dir();
        let file_name = format!("repl_{}.dtr", std::process::id());
        let temp_file = temp_dir.join(&file_name);

        if std::fs::write(&temp_file, &source).is_err() {
            return Some("Error: Failed to write temporary REPL source file.".to_string());
        }

        // Run through current executable
        let current_exe = std::env::current_exe().unwrap_or_else(|_| "forgen".into());
        let output = Command::new(&current_exe)
            .arg("run")
            .arg(&temp_file)
            .output();

        let _ = std::fs::remove_file(&temp_file);

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if out.status.success() {
                    if stdout.is_empty() {
                        None
                    } else {
                        Some(format!("=> {}", stdout))
                    }
                } else {
                    let err = if !stderr.is_empty() { stderr } else { stdout };
                    Some(err)
                }
            }
            Err(e) => Some(format!("Execution failed: {}", e)),
        }
    }

    /// Starts interactive terminal REPL loop
    pub fn run_interactive() {
        println!("================================================================================");
        println!(" Datara Interactive REPL (Zero-Latency JIT Console v0.1.0)");
        println!(" Type ':help' for commands, ':exit' or Ctrl+C to quit.");
        println!("================================================================================");

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
