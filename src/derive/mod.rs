use crate::ast::*;
use crate::diagnostics::SourceSpan;

/// Expand `@derive(...)` attributes and compile-time `comptime { ... }` expressions in AST.
pub fn expand_derives_and_comptime(program: &mut Program) {
    for decl in &mut program.declarations {
        match decl {
            Decl::Class(c) => {
                expand_class_derives(c);
                for item in &mut c.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &mut m.body
                    {
                        fold_stmt_comptime(body);
                    }
                }
            }
            Decl::Behavior(b) => {
                for item in &mut b.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &mut m.body
                    {
                        fold_stmt_comptime(body);
                    }
                }
            }
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                fold_stmt_comptime(&mut f.body);
            }
            _ => {}
        }
    }
}

fn expand_class_derives(class: &mut ClassDecl) {
    let mut requested_traits: Vec<String> = Vec::new();
    for attr in &class.attributes {
        if attr.name == "derive" {
            for (_, val) in &attr.args {
                requested_traits.push(val.trim().to_string());
            }
        }
    }

    if requested_traits.is_empty() {
        return;
    }

    let fields: Vec<FieldDecl> = class
        .body_items
        .iter()
        .filter_map(|item| {
            if let ClassItem::Field(f) = item {
                Some(f.clone())
            } else {
                None
            }
        })
        .collect();

    let span = class.span.clone();

    for trait_name in requested_traits {
        match trait_name.as_str() {
            "Display" => {
                if !has_method(&class.body_items, "to_string") {
                    class
                        .body_items
                        .push(ClassItem::Method(synthesize_to_string(
                            &class.name,
                            &fields,
                            &span,
                        )));
                }
            }
            "Json" | "Serialize" => {
                if !has_method(&class.body_items, "to_json") {
                    class.body_items.push(ClassItem::Method(synthesize_to_json(
                        &class.name,
                        &fields,
                        &span,
                    )));
                }
            }
            "Deserialize" => {
                if !has_method(&class.body_items, "from_json") {
                    class
                        .body_items
                        .push(ClassItem::Method(synthesize_from_json(
                            &class.name,
                            &fields,
                            &span,
                        )));
                }
            }
            "Hash" => {
                if !has_method(&class.body_items, "hash") {
                    class.body_items.push(ClassItem::Method(synthesize_hash(
                        &class.name,
                        &fields,
                        &span,
                    )));
                }
            }
            "Clone" if !has_method(&class.body_items, "clone") => {
                class.body_items.push(ClassItem::Method(synthesize_clone(
                    &class.name,
                    &fields,
                    &span,
                )));
            }
            _ => {}
        }
    }
}

fn has_method(items: &[ClassItem], name: &str) -> bool {
    items.iter().any(|item| {
        if let ClassItem::Method(m) = item {
            m.name == name
        } else {
            false
        }
    })
}

fn synthesize_to_string(class_name: &str, fields: &[FieldDecl], span: &SourceSpan) -> MethodDecl {
    // Format: ClassName(f1=val1, f2=val2)
    let mut parts: Vec<Expr> = Vec::new();
    parts.push(Expr::Literal(
        LiteralValue::String(format!("{}(", class_name)),
        span.clone(),
    ));

    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            parts.push(Expr::Literal(
                LiteralValue::String(", ".into()),
                span.clone(),
            ));
        }
        parts.push(Expr::Literal(
            LiteralValue::String(format!("{}=", f.name)),
            span.clone(),
        ));
        let field_access = Expr::MemberAccess {
            object: Box::new(Expr::Identifier("this".into(), span.clone())),
            member: f.name.clone(),
            span: span.clone(),
        };
        parts.push(field_access);
    }

    parts.push(Expr::Literal(
        LiteralValue::String(")".into()),
        span.clone(),
    ));

    // Combine via string concatenation
    let mut expr = parts.remove(0);
    for p in parts {
        expr = Expr::Binary {
            op: "+".into(),
            left: Box::new(expr),
            right: Box::new(p),
            span: span.clone(),
        };
    }

    MethodDecl {
        name: "to_string".into(),
        generic_params: Vec::new(),
        attributes: Vec::new(),
        params: Vec::new(),
        return_type: Some(TypeNode::new("Str", span.clone())),
        requires: Vec::new(),
        ensures: Vec::new(),
        decreases: None,
        body: Some(Box::new(Stmt::Return(Some(expr), span.clone()))),
        is_expression_body: false,
        is_replaces: false,
        replaces_target: None,
        span: span.clone(),
    }
}

fn synthesize_to_json(_class_name: &str, fields: &[FieldDecl], span: &SourceSpan) -> MethodDecl {
    // Format: {"f1": val1, "f2": val2}
    let mut parts: Vec<Expr> = Vec::new();
    parts.push(Expr::Literal(
        LiteralValue::String("{".into()),
        span.clone(),
    ));

    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            parts.push(Expr::Literal(
                LiteralValue::String(", ".into()),
                span.clone(),
            ));
        }
        parts.push(Expr::Literal(
            LiteralValue::String(format!("\"{}\": ", f.name)),
            span.clone(),
        ));
        let is_str = f
            .type_node
            .as_ref()
            .map(|tn| tn.name == "Str" || tn.name == "String")
            .unwrap_or(false);

        let field_access = Expr::MemberAccess {
            object: Box::new(Expr::Identifier("this".into(), span.clone())),
            member: f.name.clone(),
            span: span.clone(),
        };

        if is_str {
            parts.push(Expr::Literal(
                LiteralValue::String("\"".into()),
                span.clone(),
            ));
            parts.push(field_access);
            parts.push(Expr::Literal(
                LiteralValue::String("\"".into()),
                span.clone(),
            ));
        } else {
            parts.push(field_access);
        }
    }

    parts.push(Expr::Literal(
        LiteralValue::String("}".into()),
        span.clone(),
    ));

    let mut expr = parts.remove(0);
    for p in parts {
        expr = Expr::Binary {
            op: "+".into(),
            left: Box::new(expr),
            right: Box::new(p),
            span: span.clone(),
        };
    }

    MethodDecl {
        name: "to_json".into(),
        generic_params: Vec::new(),
        attributes: Vec::new(),
        params: Vec::new(),
        return_type: Some(TypeNode::new("Str", span.clone())),
        requires: Vec::new(),
        ensures: Vec::new(),
        decreases: None,
        body: Some(Box::new(Stmt::Return(Some(expr), span.clone()))),
        is_expression_body: false,
        is_replaces: false,
        replaces_target: None,
        span: span.clone(),
    }
}

fn synthesize_from_json(class_name: &str, fields: &[FieldDecl], span: &SourceSpan) -> MethodDecl {
    // Construct default Self
    let init_fields = fields
        .iter()
        .map(|f| {
            let def_val = match f.type_node.as_ref().map(|tn| tn.name.as_str()) {
                Some("Int") => Expr::Literal(LiteralValue::Int(0), span.clone()),
                Some("Float") => Expr::Literal(LiteralValue::Float(0.0), span.clone()),
                Some("Bool") => Expr::Literal(LiteralValue::Bool(false), span.clone()),
                Some("Str" | "String") => {
                    Expr::Literal(LiteralValue::String(String::new()), span.clone())
                }
                _ => Expr::Literal(LiteralValue::Int(0), span.clone()),
            };
            (f.name.clone(), def_val)
        })
        .collect();

    let obj_expr = Expr::ObjectInit {
        class_name: class_name.into(),
        generic_args: Vec::new(),
        fields: init_fields,
        span: span.clone(),
    };

    MethodDecl {
        name: "from_json".into(),
        generic_params: Vec::new(),
        attributes: Vec::new(),
        params: vec![Param {
            name: "json_str".into(),
            type_node: Some(TypeNode::new("Str", span.clone())),
            ownership_mode: "view".into(),
            span: span.clone(),
        }],
        return_type: Some(TypeNode::new(class_name, span.clone())),
        requires: Vec::new(),
        ensures: Vec::new(),
        decreases: None,
        body: Some(Box::new(Stmt::Return(Some(obj_expr), span.clone()))),
        is_expression_body: false,
        is_replaces: false,
        replaces_target: None,
        span: span.clone(),
    }
}

fn synthesize_hash(_class_name: &str, fields: &[FieldDecl], span: &SourceSpan) -> MethodDecl {
    // FNV-1a Hash:
    // mut h = 2166136261
    // for each field: h = (h ^ self.field) * 16777619
    let mut stmts: Vec<Stmt> = Vec::new();
    stmts.push(Stmt::Mut {
        name: "h".into(),
        type_node: Some(TypeNode::new("Int", span.clone())),
        init: Expr::Literal(LiteralValue::Int(2166136261), span.clone()),
        span: span.clone(),
    });

    for f in fields {
        let field_access = Expr::MemberAccess {
            object: Box::new(Expr::Identifier("this".into(), span.clone())),
            member: f.name.clone(),
            span: span.clone(),
        };

        // If field is not Int, convert or use len
        let field_val = match f.type_node.as_ref().map(|tn| tn.name.as_str()) {
            Some("Int") => field_access,
            Some("Bool") => Expr::Decide {
                arms: vec![DecideArm {
                    condition: field_access,
                    body: Expr::Literal(LiteralValue::Int(1), span.clone()),
                    span: span.clone(),
                }],
                else_arm: Some(Box::new(Expr::Literal(LiteralValue::Int(0), span.clone()))),
                span: span.clone(),
            },
            _ => Expr::Call {
                callee: Box::new(Expr::Identifier("str_len".into(), span.clone())),
                args: vec![field_access],
                span: span.clone(),
            },
        };

        let xor_expr = Expr::Binary {
            op: "^".into(),
            left: Box::new(Expr::Identifier("h".into(), span.clone())),
            right: Box::new(field_val),
            span: span.clone(),
        };

        let mul_expr = Expr::Binary {
            op: "*".into(),
            left: Box::new(xor_expr),
            right: Box::new(Expr::Literal(LiteralValue::Int(16777619), span.clone())),
            span: span.clone(),
        };

        stmts.push(Stmt::Assign {
            target: Expr::Identifier("h".into(), span.clone()),
            value: mul_expr,
            span: span.clone(),
        });
    }

    stmts.push(Stmt::Return(
        Some(Expr::Identifier("h".into(), span.clone())),
        span.clone(),
    ));

    MethodDecl {
        name: "hash".into(),
        generic_params: Vec::new(),
        attributes: Vec::new(),
        params: Vec::new(),
        return_type: Some(TypeNode::new("Int", span.clone())),
        requires: Vec::new(),
        ensures: Vec::new(),
        decreases: None,
        body: Some(Box::new(Stmt::Block(stmts, span.clone()))),
        is_expression_body: false,
        is_replaces: false,
        replaces_target: None,
        span: span.clone(),
    }
}

fn synthesize_clone(class_name: &str, fields: &[FieldDecl], span: &SourceSpan) -> MethodDecl {
    let init_fields = fields
        .iter()
        .map(|f| {
            let field_access = Expr::MemberAccess {
                object: Box::new(Expr::Identifier("this".into(), span.clone())),
                member: f.name.clone(),
                span: span.clone(),
            };
            (f.name.clone(), field_access)
        })
        .collect();

    let clone_expr = Expr::ObjectInit {
        class_name: class_name.into(),
        generic_args: Vec::new(),
        fields: init_fields,
        span: span.clone(),
    };

    MethodDecl {
        name: "clone".into(),
        generic_params: Vec::new(),
        attributes: Vec::new(),
        params: Vec::new(),
        return_type: Some(TypeNode::new(class_name, span.clone())),
        requires: Vec::new(),
        ensures: Vec::new(),
        decreases: None,
        body: Some(Box::new(Stmt::Return(Some(clone_expr), span.clone()))),
        is_expression_body: false,
        is_replaces: false,
        replaces_target: None,
        span: span.clone(),
    }
}

// ---------------------------------------------------------------------------
// Comptime Constant Evaluation & Folding
// ---------------------------------------------------------------------------

fn fold_stmt_comptime(stmt: &mut Stmt) {
    match stmt {
        Stmt::Block(stmts, _) => {
            for s in stmts {
                fold_stmt_comptime(s);
            }
        }
        Stmt::Let { init, .. }
        | Stmt::Mut { init, .. }
        | Stmt::Const { init, .. }
        | Stmt::Val { init, .. }
        | Stmt::CompactBind { init, .. } => {
            fold_expr_comptime(init);
        }
        Stmt::Assign { target, value, .. } => {
            fold_expr_comptime(target);
            fold_expr_comptime(value);
        }
        Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
            fold_expr_comptime(e);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            fold_expr_comptime(condition);
            fold_stmt_comptime(then_branch);
            if let Some(eb) = else_branch {
                fold_stmt_comptime(eb);
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            fold_expr_comptime(condition);
            fold_stmt_comptime(body);
        }
        Stmt::For { iterable, body, .. } | Stmt::ParallelFor { iterable, body, .. } => {
            fold_expr_comptime(iterable);
            fold_stmt_comptime(body);
        }
        Stmt::Loop { body, .. } | Stmt::Parallel(body, ..) | Stmt::Unsafe { body, .. } => {
            fold_stmt_comptime(body);
        }
        Stmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            fold_stmt_comptime(try_block);
            fold_stmt_comptime(catch_block);
        }
        Stmt::With { init, body, .. } => {
            fold_expr_comptime(init);
            fold_stmt_comptime(body);
        }
        Stmt::Return(Some(e), _) => {
            fold_expr_comptime(e);
        }
        _ => {}
    }
}

pub fn fold_expr_comptime(expr: &mut Expr) {
    match expr {
        Expr::Comptime { expr: inner, span } => {
            fold_expr_comptime(inner);
            if let Some(lit) = evaluate_constant(inner) {
                *expr = Expr::Literal(lit, span.clone());
            } else {
                *expr = (**inner).clone();
            }
        }
        Expr::Binary { left, right, .. } => {
            fold_expr_comptime(left);
            fold_expr_comptime(right);
        }
        Expr::Unary { expr: inner, .. } => {
            fold_expr_comptime(inner);
        }
        Expr::Call { callee, args, .. } => {
            fold_expr_comptime(callee);
            for a in args {
                fold_expr_comptime(a);
            }
        }
        Expr::MemberAccess { object, .. } => {
            fold_expr_comptime(object);
        }
        Expr::IndexAccess { object, index, .. } => {
            fold_expr_comptime(object);
            fold_expr_comptime(index);
        }
        Expr::Tuple(elements, _) | Expr::ListLiteral(elements, _) => {
            for e in elements {
                fold_expr_comptime(e);
            }
        }
        Expr::ObjectInit { fields, .. } => {
            for (_, f_expr) in fields {
                fold_expr_comptime(f_expr);
            }
        }
        _ => {}
    }
}

fn evaluate_constant(expr: &Expr) -> Option<LiteralValue> {
    match expr {
        Expr::Literal(lit, _) => Some(lit.clone()),
        Expr::Binary {
            op, left, right, ..
        } => {
            let l_val = evaluate_constant(left)?;
            let r_val = evaluate_constant(right)?;
            match (l_val, r_val) {
                (LiteralValue::Int(a), LiteralValue::Int(b)) => match op.as_str() {
                    "+" => Some(LiteralValue::Int(a.wrapping_add(b))),
                    "-" => Some(LiteralValue::Int(a.wrapping_sub(b))),
                    "*" => Some(LiteralValue::Int(a.wrapping_mul(b))),
                    "/" if b != 0 => Some(LiteralValue::Int(a.wrapping_div(b))),
                    "%" if b != 0 => Some(LiteralValue::Int(a.wrapping_rem(b))),
                    "==" => Some(LiteralValue::Bool(a == b)),
                    "!=" => Some(LiteralValue::Bool(a != b)),
                    "<" => Some(LiteralValue::Bool(a < b)),
                    "<=" => Some(LiteralValue::Bool(a <= b)),
                    ">" => Some(LiteralValue::Bool(a > b)),
                    ">=" => Some(LiteralValue::Bool(a >= b)),
                    "&" => Some(LiteralValue::Int(a & b)),
                    "|" => Some(LiteralValue::Int(a | b)),
                    "^" => Some(LiteralValue::Int(a ^ b)),
                    "<<" => Some(LiteralValue::Int(a << (b & 63))),
                    ">>" => Some(LiteralValue::Int(a >> (b & 63))),
                    _ => None,
                },
                (LiteralValue::Float(a), LiteralValue::Float(b)) => match op.as_str() {
                    "+" => Some(LiteralValue::Float(a + b)),
                    "-" => Some(LiteralValue::Float(a - b)),
                    "*" => Some(LiteralValue::Float(a * b)),
                    "/" => Some(LiteralValue::Float(a / b)),
                    "==" => Some(LiteralValue::Bool((a - b).abs() < f64::EPSILON)),
                    "!=" => Some(LiteralValue::Bool((a - b).abs() >= f64::EPSILON)),
                    "<" => Some(LiteralValue::Bool(a < b)),
                    "<=" => Some(LiteralValue::Bool(a <= b)),
                    ">" => Some(LiteralValue::Bool(a > b)),
                    ">=" => Some(LiteralValue::Bool(a >= b)),
                    _ => None,
                },
                (LiteralValue::Bool(a), LiteralValue::Bool(b)) => match op.as_str() {
                    "&&" => Some(LiteralValue::Bool(a && b)),
                    "||" => Some(LiteralValue::Bool(a || b)),
                    "==" => Some(LiteralValue::Bool(a == b)),
                    "!=" => Some(LiteralValue::Bool(a != b)),
                    _ => None,
                },
                (LiteralValue::String(a), LiteralValue::String(b)) if op == "+" => {
                    Some(LiteralValue::String(format!("{}{}", a, b)))
                }
                (LiteralValue::String(a), LiteralValue::Int(b)) if op == "+" => {
                    Some(LiteralValue::String(format!("{}{}", a, b)))
                }
                (LiteralValue::Int(a), LiteralValue::String(b)) if op == "+" => {
                    Some(LiteralValue::String(format!("{}{}", a, b)))
                }
                _ => None,
            }
        }
        Expr::Unary { op, expr, .. } => {
            let val = evaluate_constant(expr)?;
            match (op.as_str(), val) {
                ("-", LiteralValue::Int(n)) => Some(LiteralValue::Int(-n)),
                ("-", LiteralValue::Float(f)) => Some(LiteralValue::Float(-f)),
                ("!", LiteralValue::Bool(b)) => Some(LiteralValue::Bool(!b)),
                _ => None,
            }
        }
        _ => None,
    }
}
