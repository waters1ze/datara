use super::*;
use crate::ast::*;
use crate::types::DataraType;
use std::collections::{HashMap, HashSet};

impl<'a> SecurityVerifier<'a> {
    pub(crate) fn expr_has_capability(&self, expr: &Expr, req_cap: &str, ctx: &FnContext) -> bool {
        if let Expr::Identifier(name, _) = expr
            && let Some(ty) = ctx.symbols.get(name)
        {
            return matches_capability(ty, req_cap);
        }
        false
    }
}

pub(crate) fn is_proven_non_zero_cond(cond: &Expr, var_name: &str) -> bool {
    match cond {
        Expr::Binary {
            op, left, right, ..
        } => {
            if op == "!=" {
                if is_var(left, var_name) && is_zero(right) {
                    return true;
                }
                if is_var(right, var_name) && is_zero(left) {
                    return true;
                }
            } else if op == ">" {
                if is_var(left, var_name) && (is_zero(right) || is_positive(right)) {
                    return true;
                }
            } else if op == ">=" {
                if is_var(left, var_name) && is_strictly_positive(right) {
                    return true;
                }
            } else if op == "<" {
                if is_var(left, var_name) && (is_zero(right) || is_negative(right)) {
                    return true;
                }
            } else if op == "<=" {
                if is_var(left, var_name) && is_strictly_negative(right) {
                    return true;
                }
            } else if op == "&&" {
                return is_proven_non_zero_cond(left, var_name)
                    || is_proven_non_zero_cond(right, var_name);
            }
            false
        }
        _ => false,
    }
}

pub(crate) fn is_var(expr: &Expr, var_name: &str) -> bool {
    if let Expr::Identifier(name, _) = expr {
        name == var_name
    } else {
        false
    }
}

pub(crate) fn is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(0), _) => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f == 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_positive(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n >= 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f >= 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_strictly_positive(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n >= 1 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f > 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_negative(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n <= 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f <= 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_strictly_negative(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n <= -1 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f < 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_non_zero_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(LiteralValue::Int(n), _) if *n != 0 => true,
        Expr::Literal(LiteralValue::Float(f), _) if *f != 0.0 => true,
        _ => false,
    }
}

pub(crate) fn is_equality_zero_cond(cond: &Expr, var_name: &str) -> bool {
    match cond {
        Expr::Binary {
            op, left, right, ..
        } if op == "==" => {
            (is_var(left, var_name) && is_zero(right)) || (is_var(right, var_name) && is_zero(left))
        }
        _ => false,
    }
}

pub(crate) fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_, _) => true,
        Stmt::Block(stmts, _) => stmts.iter().any(stmt_always_returns),
        _ => false,
    }
}

pub(crate) fn required_capability_for_op(callee: &str) -> Option<&'static str> {
    match callee {
        "fs_open" | "fs_read" | "read_file" | "file_read" => Some("Capability<FileRead>"),
        "fs_write" | "file_write" | "write_file" | "file_append" => Some("Capability<FileWrite>"),
        "net_connect" | "socket_connect" => Some("Capability<NetworkConnect>"),
        "net_listen" | "socket_listen" | "socket_bind" => Some("Capability<NetworkListen>"),
        "proc_spawn" | "process_run" | "system" | "exec" | "process_output" => {
            Some("Capability<ProcessExec>")
        }
        _ => None,
    }
}

pub(crate) fn has_capability(symbols: &HashMap<String, DataraType>, req_cap: &str) -> bool {
    for ty in symbols.values() {
        if matches_capability(ty, req_cap) {
            return true;
        }
    }
    false
}

pub(crate) fn matches_capability(ty: &DataraType, req_cap: &str) -> bool {
    match ty {
        DataraType::GenericInstance { name, args } if name == "Capability" => {
            if let Some(first_arg) = args.first() {
                let inner = match first_arg {
                    DataraType::Class(c) => c.as_str(),
                    _ => "",
                };
                let formatted = format!("Capability<{}>", inner);
                if formatted == req_cap {
                    return true;
                }
            }
        }
        DataraType::Class(c) if c == req_cap || c == "SystemCapabilities" => {
            return true;
        }
        _ => {}
    }
    false
}

pub(crate) fn collect_expr_reads(
    expr: &Expr,
    outer_vars: &HashSet<String>,
    local_declared: &HashSet<String>,
    reads: &mut HashSet<String>,
) {
    match expr {
        Expr::Identifier(name, _) => {
            if outer_vars.contains(name) && !local_declared.contains(name) {
                reads.insert(name.clone());
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_reads(left, outer_vars, local_declared, reads);
            collect_expr_reads(right, outer_vars, local_declared, reads);
        }
        Expr::Unary { expr, .. } => {
            collect_expr_reads(expr, outer_vars, local_declared, reads);
        }
        Expr::Call { callee, args, .. } => {
            collect_expr_reads(callee, outer_vars, local_declared, reads);
            for a in args {
                collect_expr_reads(a, outer_vars, local_declared, reads);
            }
        }
        Expr::MemberAccess { object, .. } => {
            collect_expr_reads(object, outer_vars, local_declared, reads);
        }
        Expr::IndexAccess { object, index, .. } => {
            collect_expr_reads(object, outer_vars, local_declared, reads);
            collect_expr_reads(index, outer_vars, local_declared, reads);
        }
        Expr::Tuple(exprs, _) | Expr::ListLiteral(exprs, _) => {
            for e in exprs {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
        }
        Expr::MapLiteral(entries, _) => {
            for (k, v) in entries {
                collect_expr_reads(k, outer_vars, local_declared, reads);
                collect_expr_reads(v, outer_vars, local_declared, reads);
            }
        }
        Expr::InterpolatedString { expressions, .. } => {
            for e in expressions {
                collect_expr_reads(e, outer_vars, local_declared, reads);
            }
        }
        Expr::Match { value, arms, .. } => {
            collect_expr_reads(value, outer_vars, local_declared, reads);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_expr_reads(g, outer_vars, local_declared, reads);
                }
                collect_expr_reads(&arm.body, outer_vars, local_declared, reads);
            }
        }
        Expr::Decide { arms, else_arm, .. } => {
            for arm in arms {
                collect_expr_reads(&arm.condition, outer_vars, local_declared, reads);
                collect_expr_reads(&arm.body, outer_vars, local_declared, reads);
            }
            if let Some(eb) = else_arm {
                collect_expr_reads(eb, outer_vars, local_declared, reads);
            }
        }
        Expr::Select { arms, else_arm, .. } => {
            for arm in arms {
                collect_expr_reads(&arm.condition, outer_vars, local_declared, reads);
                collect_expr_reads(&arm.body, outer_vars, local_declared, reads);
            }
            if let Some(eb) = else_arm {
                collect_expr_reads(eb, outer_vars, local_declared, reads);
            }
        }
        Expr::Pipeline { stages, .. } => {
            for s in stages {
                collect_expr_reads(s, outer_vars, local_declared, reads);
            }
        }
        Expr::Range { start, end, .. } => {
            collect_expr_reads(start, outer_vars, local_declared, reads);
            collect_expr_reads(end, outer_vars, local_declared, reads);
        }
        Expr::ObjectInit { fields, .. } => {
            for (_, f_expr) in fields {
                collect_expr_reads(f_expr, outer_vars, local_declared, reads);
            }
        }
        Expr::ErrorPropagate(inner, _) => {
            collect_expr_reads(inner, outer_vars, local_declared, reads);
        }
        Expr::Lambda { body, .. } => {
            collect_expr_reads(body, outer_vars, local_declared, reads);
        }
        Expr::Block(stmts, value, _) => {
            for s in stmts {
                match s {
                    Stmt::Expr(e, _)
                    | Stmt::Out(e, _)
                    | Stmt::Err(e, _)
                    | Stmt::Let { init: e, .. }
                    | Stmt::Mut { init: e, .. }
                    | Stmt::Val { init: e, .. }
                    | Stmt::Const { init: e, .. }
                    | Stmt::CompactBind { init: e, .. } => {
                        collect_expr_reads(e, outer_vars, local_declared, reads);
                    }
                    Stmt::Assign {
                        target, value: v, ..
                    } => {
                        collect_expr_reads(target, outer_vars, local_declared, reads);
                        collect_expr_reads(v, outer_vars, local_declared, reads);
                    }
                    Stmt::Return(Some(e), _) => {
                        collect_expr_reads(e, outer_vars, local_declared, reads);
                    }
                    _ => {}
                }
            }
            if let Some(v) = value {
                collect_expr_reads(v, outer_vars, local_declared, reads);
            }
        }
        _ => {}
    }
}
