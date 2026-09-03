use crate::ast::*;
use crate::lint::diagnostics::LintDiagnostic;
use std::collections::{HashMap, HashSet};

pub fn is_snake_case(s: &str) -> bool {
    let s = s.trim_start_matches('_');
    if s.is_empty() {
        return true;
    }
    s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.contains("__")
}

pub fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let first = s.chars().next().unwrap();
    first.is_ascii_uppercase() && !s.contains('_')
}

pub fn is_screaming_snake_case(s: &str) -> bool {
    let s = s.trim_start_matches('_');
    if s.is_empty() {
        return true;
    }
    s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 && !chars[i - 1].is_ascii_uppercase() && chars[i - 1] != '_' {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn to_pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for c in s.chars() {
        if c == '_' {
            cap_next = true;
        } else if cap_next {
            out.push(c.to_ascii_uppercase());
            cap_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn run_all_rules(program: &Program) -> Vec<LintDiagnostic> {
    let mut diags = Vec::new();
    check_declarations(program, &mut diags);
    diags
}

fn check_declarations(program: &Program, diags: &mut Vec<LintDiagnostic>) {
    for decl in &program.declarations {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                // Function name must be snake_case
                if !is_snake_case(&f.name) {
                    let suggested = to_snake_case(&f.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_snake_case",
                            format!("function `{}` should have a snake_case name", f.name),
                            f.span.clone(),
                        )
                        .with_help(format!("convert to snake_case: `{}`", suggested))
                        .with_note("Datara naming convention enforces snake_case for functions".into()),
                    );
                }

                // Check parameter names
                for p in &f.params {
                    if !is_snake_case(&p.name) {
                        let suggested = to_snake_case(&p.name);
                        diags.push(
                            LintDiagnostic::new(
                                "style::non_snake_case",
                                format!("parameter `{}` should have a snake_case name", p.name),
                                p.span.clone(),
                            )
                            .with_help(format!("convert to snake_case: `{}`", suggested))
                            .with_note("Datara naming convention enforces snake_case for parameters".into()),
                        );
                    }
                }

                // Check statements in function body
                check_body(&f.body, diags);
            }

            Decl::Class(c) => {
                if !is_pascal_case(&c.name) {
                    let suggested = to_pascal_case(&c.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_camel_case_types",
                            format!("class `{}` should have a PascalCase name", c.name),
                            c.span.clone(),
                        )
                        .with_help(format!("convert to PascalCase: `{}`", suggested))
                        .with_note("Datara naming convention enforces PascalCase for types and classes".into()),
                    );
                }
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item {
                        if !is_snake_case(&m.name) {
                            let suggested = to_snake_case(&m.name);
                            diags.push(
                                LintDiagnostic::new(
                                    "style::non_snake_case",
                                    format!("method `{}` should have a snake_case name", m.name),
                                    m.span.clone(),
                                )
                                .with_help(format!("convert to snake_case: `{}`", suggested)),
                            );
                        }
                        if let Some(body) = &m.body {
                            check_body(body, diags);
                        }
                    }
                }
            }

            Decl::Enum(e) => {
                if !is_pascal_case(&e.name) {
                    let suggested = to_pascal_case(&e.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_camel_case_types",
                            format!("enum `{}` should have a PascalCase name", e.name),
                            e.span.clone(),
                        )
                        .with_help(format!("convert to PascalCase: `{}`", suggested)),
                    );
                }
            }

            Decl::Component(c) => {
                if !is_pascal_case(&c.name) {
                    let suggested = to_pascal_case(&c.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_camel_case_types",
                            format!("component `{}` should have a PascalCase name", c.name),
                            c.span.clone(),
                        )
                        .with_help(format!("convert to PascalCase: `{}`", suggested)),
                    );
                }
            }

            Decl::Role(r) => {
                if !is_pascal_case(&r.name) {
                    let suggested = to_pascal_case(&r.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_camel_case_types",
                            format!("role `{}` should have a PascalCase name", r.name),
                            r.span.clone(),
                        )
                        .with_help(format!("convert to PascalCase: `{}`", suggested)),
                    );
                }
            }

            Decl::Packet(p)
                if !is_pascal_case(&p.name) => {
                    let suggested = to_pascal_case(&p.name);
                    diags.push(
                        LintDiagnostic::new(
                            "style::non_camel_case_types",
                            format!("packet `{}` should have a PascalCase name", p.name),
                            p.span.clone(),
                        )
                        .with_help(format!("convert to PascalCase: `{}`", suggested)),
                    );
                }

            _ => {}
        }
    }
}

fn check_body(stmt: &Stmt, diags: &mut Vec<LintDiagnostic>) {
    let mut tracker = VariableUsageTracker::new();
    tracker.analyze_stmt(stmt);

    // 1. Check unnecessary mut: declared mut, but never reassigned
    for (name, (span, is_mut)) in &tracker.declared {
        if *is_mut && !tracker.mutated.contains(name) {
            diags.push(
                LintDiagnostic::new(
                    "perf::unnecessary_mut",
                    format!("variable `{}` does not need to be mutable", name),
                    span.clone(),
                )
                .with_help(format!("remove `mut` to declare an immutable variable: `let {}`", name))
                .with_note(format!("`{}` is never reassigned after initialization", name))
                .with_fix(format!("let {}", name)),
            );
        }

        // 2. Check unused variables: declared, but never read
        if !tracker.read.contains(name) && !name.starts_with('_') {
            diags.push(
                LintDiagnostic::new(
                    "style::unused_variable",
                    format!("unused variable `{}`", name),
                    span.clone(),
                )
                .with_help(format!("if this is intentional, prefix with an underscore: `_{}`", name))
                .with_note(format!("`{}` is defined but its value is never evaluated", name)),
            );
        }

        // 3. Check variable naming
        if !is_snake_case(name) {
            let suggested = to_snake_case(name);
            diags.push(
                LintDiagnostic::new(
                    "style::non_snake_case",
                    format!("variable `{}` should have a snake_case name", name),
                    span.clone(),
                )
                .with_help(format!("convert to snake_case: `{}`", suggested))
                .with_note("Datara naming convention enforces snake_case for local variables".into()),
            );
        }
    }

    // 4. Recursive structural checks for loops and expressions
    check_stmt_structure(stmt, diags);
}

fn check_stmt_structure(stmt: &Stmt, diags: &mut Vec<LintDiagnostic>) {
    match stmt {
        Stmt::Block(stmts, _) => {
            for s in stmts {
                check_stmt_structure(s, diags);
            }
        }

        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            check_expr_idioms(condition, diags);
            check_stmt_structure(then_branch, diags);
            if let Some(eb) = else_branch {
                check_stmt_structure(eb, diags);
            }
        }

        Stmt::While {
            condition,
            body,
            span,
        } => {
            check_expr_idioms(condition, diags);
            // Check if this while loop is an index loop increment: while i < 100 { ... i = i + 1 }
            if let Expr::Binary { op, left, .. } = condition
                && (op == "<" || op == "<=")
                && let Expr::Identifier(var_name, _) = left.as_ref()
                && body_increments_var(body, var_name)
            {
                diags.push(
                    LintDiagnostic::new(
                        "style::prefer_for_loop",
                        format!("manual while loop index increment for `{}` detected", var_name),
                        span.clone(),
                    )
                    .with_help(format!("use an idiomatic range for-loop: `for {} in 0..N {{ ... }}`", var_name))
                    .with_note("range for-loops are optimized into zero-cost vector loops by Evidence Gate".into()),
                );
            }
            check_stmt_structure(body, diags);
        }

        Stmt::For {
            var_name,
            iterable,
            body,
            span,
        } => {
            if !is_snake_case(var_name) {
                diags.push(
                    LintDiagnostic::new(
                        "style::non_snake_case",
                        format!("loop variable `{}` should have a snake_case name", var_name),
                        span.clone(),
                    )
                    .with_help(format!("convert to snake_case: `{}`", to_snake_case(var_name))),
                );
            }
            check_expr_idioms(iterable, diags);
            check_stmt_structure(body, diags);
        }

        Stmt::Assign { target: _, value, .. } => {
            check_expr_idioms(value, diags);
        }

        Stmt::Let { init, .. } | Stmt::Mut { init, .. } | Stmt::Val { init, .. } => {
            check_expr_idioms(init, diags);
        }

        Stmt::Expr(expr, _) | Stmt::Out(expr, _) | Stmt::Err(expr, _) => {
            check_expr_idioms(expr, diags);
        }

        Stmt::Return(Some(expr), _) => {
            check_expr_idioms(expr, diags);
        }

        _ => {}
    }
}

fn check_expr_idioms(expr: &Expr, diags: &mut Vec<LintDiagnostic>) {
    match expr {
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => {
            // Check bool comparisons like x == true or x == false
            if op == "=="
                && let Expr::Literal(LiteralValue::Bool(b), _) = right.as_ref() {
                    if *b {
                        diags.push(
                            LintDiagnostic::new(
                                "style::bool_comparison",
                                "redundant comparison with `true`".into(),
                                span.clone(),
                            )
                            .with_help("simplify condition to evaluate boolean expression directly".into()),
                        );
                    } else {
                        diags.push(
                            LintDiagnostic::new(
                                "style::bool_comparison",
                                "comparison with `false` can be inverted".into(),
                                span.clone(),
                            )
                            .with_help("simplify condition by prefixing with negation `!`".into()),
                        );
                    }
                }

            check_expr_idioms(left, diags);
            check_expr_idioms(right, diags);
        }

        Expr::Unary { expr, .. } => {
            check_expr_idioms(expr, diags);
        }

        Expr::Call { callee, args, .. } => {
            check_expr_idioms(callee, diags);
            for a in args {
                check_expr_idioms(a, diags);
            }
        }

        _ => {}
    }
}

fn body_increments_var(stmt: &Stmt, var: &str) -> bool {
    match stmt {
        Stmt::Block(stmts, _) => stmts.iter().any(|s| body_increments_var(s, var)),
        Stmt::Assign { target, value, .. } => {
            if let Expr::Identifier(name, _) = target
                && name == var
                    && let Expr::Binary { op, left, .. } = value
                        && op == "+"
                            && let Expr::Identifier(l_name, _) = left.as_ref() {
                                return l_name == var;
                            }
            false
        }
        _ => false,
    }
}

struct VariableUsageTracker {
    declared: HashMap<String, (crate::diagnostics::SourceSpan, bool)>, // (span, is_mut)
    mutated: HashSet<String>,
    read: HashSet<String>,
}

impl VariableUsageTracker {
    fn new() -> Self {
        Self {
            declared: HashMap::new(),
            mutated: HashSet::new(),
            read: HashSet::new(),
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, init, span, .. } => {
                self.declared.insert(name.clone(), (span.clone(), false));
                self.analyze_expr(init);
            }
            Stmt::Mut { name, init, span, .. } => {
                self.declared.insert(name.clone(), (span.clone(), true));
                self.analyze_expr(init);
            }
            Stmt::Val { name, init, is_mut, span, .. } => {
                self.declared.insert(name.clone(), (span.clone(), *is_mut));
                self.analyze_expr(init);
            }
            Stmt::Assign { target, value, .. } => {
                if let Expr::Identifier(name, _) = target {
                    self.mutated.insert(name.clone());
                } else {
                    self.analyze_expr(target);
                }
                self.analyze_expr(value);
            }
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    self.analyze_stmt(s);
                }
            }
            Stmt::If { condition, then_branch, else_branch, .. } => {
                self.analyze_expr(condition);
                self.analyze_stmt(then_branch);
                if let Some(eb) = else_branch {
                    self.analyze_stmt(eb);
                }
            }
            Stmt::While { condition, body, .. } => {
                self.analyze_expr(condition);
                self.analyze_stmt(body);
            }
            Stmt::For { var_name, iterable, body, span, .. } => {
                self.declared.insert(var_name.clone(), (span.clone(), false));
                self.analyze_expr(iterable);
                self.analyze_stmt(body);
            }
            Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.analyze_expr(e);
            }
            Stmt::Return(Some(e), _) => {
                self.analyze_expr(e);
            }
            _ => {}
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Identifier(name, _) => {
                self.read.insert(name.clone());
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr);
            }
            Expr::Call { callee, args, .. } => {
                self.analyze_expr(callee);
                for a in args {
                    self.analyze_expr(a);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.analyze_expr(object);
            }
            Expr::IndexAccess { object, index, .. } => {
                self.analyze_expr(object);
                self.analyze_expr(index);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.analyze_expr(e);
                }
            }
            _ => {}
        }
    }
}
