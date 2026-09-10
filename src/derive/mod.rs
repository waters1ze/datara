use crate::ast::*;
use crate::diagnostics::SourceSpan;

/// Expand `@derive(...)` attributes and compile-time `comptime { ... }` expressions in AST.
pub fn expand_derives_and_comptime(program: &mut Program) {
    expand_trait_defaults(program);
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
            Decl::Impl(i) => {
                for m in &mut i.methods {
                    fold_stmt_comptime(&mut m.body);
                }
            }
            _ => {}
        }
    }
}

fn expand_class_derives(class: &mut ClassDecl) {
    let mut requested_traits: Vec<String> = Vec::new();
    for attr in &class.attributes {
        if attr.name == "derive" {
            for (key, val) in &attr.args {
                let name = if !val.trim().is_empty() {
                    val.trim()
                } else {
                    key.trim()
                };
                if !name.is_empty() {
                    requested_traits.push(name.to_string());
                }
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

        let mul_expr = Expr::Wrapping(
            Box::new(Expr::Binary {
                op: "*".into(),
                left: Box::new(xor_expr),
                right: Box::new(Expr::Literal(LiteralValue::Int(16777619), span.clone())),
                span: span.clone(),
            }),
            span.clone(),
        );

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
        Expr::Wrapping(inner, _) | Expr::Saturating(inner, _) => {
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
        Expr::Wrapping(inner, _) => {
            if let Expr::Binary {
                op, left, right, ..
            } = inner.as_ref()
            {
                let l_val = evaluate_constant(left)?;
                let r_val = evaluate_constant(right)?;
                if let (LiteralValue::Int(a), LiteralValue::Int(b)) = (l_val, r_val) {
                    return match op.as_str() {
                        "+" => Some(LiteralValue::Int(a.wrapping_add(b))),
                        "-" => Some(LiteralValue::Int(a.wrapping_sub(b))),
                        "*" => Some(LiteralValue::Int(a.wrapping_mul(b))),
                        _ => None,
                    };
                }
            }
            evaluate_constant(inner)
        }
        Expr::Saturating(inner, _) => {
            if let Expr::Binary {
                op, left, right, ..
            } = inner.as_ref()
            {
                let l_val = evaluate_constant(left)?;
                let r_val = evaluate_constant(right)?;
                if let (LiteralValue::Int(a), LiteralValue::Int(b)) = (l_val, r_val) {
                    return match op.as_str() {
                        "+" => Some(LiteralValue::Int(a.saturating_add(b))),
                        "-" => Some(LiteralValue::Int(a.saturating_sub(b))),
                        "*" => Some(LiteralValue::Int(a.saturating_mul(b))),
                        _ => None,
                    };
                }
            }
            evaluate_constant(inner)
        }
        Expr::Binary {
            op, left, right, ..
        } => {
            let l_val = evaluate_constant(left)?;
            let r_val = evaluate_constant(right)?;
            match (l_val, r_val) {
                (LiteralValue::Int(a), LiteralValue::Int(b)) => match op.as_str() {
                    "+" => a.checked_add(b).map(LiteralValue::Int),
                    "-" => a.checked_sub(b).map(LiteralValue::Int),
                    "*" => a.checked_mul(b).map(LiteralValue::Int),
                    "/" if b != 0 => a.checked_div(b).map(LiteralValue::Int),
                    "%" if b != 0 => a.checked_rem(b).map(LiteralValue::Int),
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

/// Expand trait default method implementations into `impl` blocks that omit them.
pub fn expand_trait_defaults(program: &mut Program) {
    let mut traits: std::collections::HashMap<String, TraitDef> = std::collections::HashMap::new();
    for decl in &program.declarations {
        if let Decl::Trait(t) = decl {
            traits.insert(t.name.clone(), t.clone());
        }
    }

    fn collect_trait_methods(
        tr_name: &str,
        traits: &std::collections::HashMap<String, TraitDef>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Vec<TraitMethodSignature> {
        if !visited.insert(tr_name.to_string()) {
            return Vec::new();
        }
        let mut result = Vec::new();
        if let Some(t_def) = traits.get(tr_name) {
            for super_tr in &t_def.super_traits {
                result.extend(collect_trait_methods(super_tr, traits, visited));
            }
            for m in &t_def.methods {
                if let Some(idx) = result
                    .iter()
                    .position(|existing: &TraitMethodSignature| existing.name == m.name)
                {
                    result[idx] = m.clone();
                } else {
                    result.push(m.clone());
                }
            }
        }
        result
    }

    // Synthesize missing super-trait impl blocks
    let mut synth_impls: Vec<Decl> = Vec::new();
    for decl in &program.declarations {
        if let Decl::Impl(i) = decl {
            if let Some(tr_name) = &i.trait_name {
                fn collect_super_traits(
                    t: &str,
                    traits: &std::collections::HashMap<String, TraitDef>,
                    visited: &mut std::collections::HashSet<String>,
                ) -> Vec<String> {
                    if !visited.insert(t.to_string()) {
                        return Vec::new();
                    }
                    let mut res = Vec::new();
                    if let Some(tdef) = traits.get(t) {
                        for st in &tdef.super_traits {
                            res.push(st.clone());
                            res.extend(collect_super_traits(st, traits, visited));
                        }
                    }
                    res
                }
                let mut visited = std::collections::HashSet::new();
                let super_traits = collect_super_traits(tr_name, &traits, &mut visited);
                for st in super_traits {
                    let already_exists =
                        program
                            .declarations
                            .iter()
                            .chain(synth_impls.iter())
                            .any(|d| {
                                if let Decl::Impl(other) = d {
                                    other.trait_name.as_deref() == Some(&st)
                                        && other.target_type == i.target_type
                                } else {
                                    false
                                }
                            });
                    if !already_exists {
                        synth_impls.push(Decl::Impl(ImplBlock {
                            trait_name: Some(st),
                            target_type: i.target_type.clone(),
                            target_type_args: i.target_type_args.clone(),
                            methods: Vec::new(),
                            span: i.span.clone(),
                        }));
                    }
                }
            }
        }
    }
    program.declarations.extend(synth_impls);

    for decl in &mut program.declarations {
        if let Decl::Impl(i) = decl {
            if let Some(tr_name) = &i.trait_name {
                let mut visited = std::collections::HashSet::new();
                let all_methods = collect_trait_methods(tr_name, &traits, &mut visited);
                for tm in all_methods {
                    if let Some(default_body) = &tm.default_body {
                        if !i.methods.iter().any(|m| m.name == tm.name) {
                            let mut synth_body = default_body.clone();
                            substitute_self_in_stmt(&mut synth_body, &i.target_type);
                            let mut synth_params = tm.params.clone();
                            for p in &mut synth_params {
                                if let Some(tn) = &mut p.type_node {
                                    substitute_self_in_type_node(tn, &i.target_type);
                                }
                            }
                            let mut synth_ret = tm.return_type.clone();
                            if let Some(tn) = &mut synth_ret {
                                substitute_self_in_type_node(tn, &i.target_type);
                            }
                            i.methods.push(FunctionDecl {
                                name: tm.name.clone(),
                                attributes: Vec::new(),
                                generic_params: tm.generic_params.clone(),
                                generic_constraints: Vec::new(),
                                params: synth_params,
                                return_type: synth_ret,
                                requires: Vec::new(),
                                ensures: Vec::new(),
                                decreases: None,
                                body: synth_body,
                                is_expression_body: false,
                                is_export: true,
                                span: tm.span.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    for decl in &mut program.declarations {
        match decl {
            Decl::Class(c) => {
                for item in &mut c.body_items {
                    if let ClassItem::Method(m) = item {
                        for p in &mut m.params {
                            if let Some(tn) = &mut p.type_node {
                                substitute_self_in_type_node(tn, &c.name);
                            }
                        }
                        if let Some(tn) = &mut m.return_type {
                            substitute_self_in_type_node(tn, &c.name);
                        }
                        if let Some(b) = &mut m.body {
                            substitute_self_in_stmt(b, &c.name);
                        }
                    }
                }
            }
            Decl::Behavior(b) => {
                for item in &mut b.body_items {
                    if let ClassItem::Method(m) = item {
                        for p in &mut m.params {
                            if let Some(tn) = &mut p.type_node {
                                substitute_self_in_type_node(tn, &b.target_type);
                            }
                        }
                        if let Some(tn) = &mut m.return_type {
                            substitute_self_in_type_node(tn, &b.target_type);
                        }
                        if let Some(body) = &mut m.body {
                            substitute_self_in_stmt(body, &b.target_type);
                        }
                    }
                }
            }
            Decl::Impl(i) => {
                for m in &mut i.methods {
                    for p in &mut m.params {
                        if let Some(tn) = &mut p.type_node {
                            substitute_self_in_type_node(tn, &i.target_type);
                        }
                    }
                    if let Some(tn) = &mut m.return_type {
                        substitute_self_in_type_node(tn, &i.target_type);
                    }
                    substitute_self_in_stmt(&mut m.body, &i.target_type);
                }
            }
            _ => {}
        }
    }
}

pub fn substitute_self_in_type_node(tn: &mut TypeNode, target: &str) {
    if tn.name == "Self" {
        tn.name = target.to_string();
    }
    for arg in &mut tn.generic_args {
        substitute_self_in_type_node(arg, target);
    }
    if let Some(err) = &mut tn.error_type {
        substitute_self_in_type_node(err, target);
    }
}

pub fn substitute_self_in_expr(expr: &mut Expr, target: &str) {
    match expr {
        Expr::Identifier(name, _) => {
            if name == "Self" {
                *name = target.to_string();
            }
        }
        Expr::ObjectInit {
            class_name,
            generic_args,
            fields,
            ..
        } => {
            if class_name == "Self" {
                *class_name = target.to_string();
            }
            for g in generic_args {
                substitute_self_in_type_node(g, target);
            }
            for (_, fval) in fields {
                substitute_self_in_expr(fval, target);
            }
        }
        Expr::Binary { left, right, .. } => {
            substitute_self_in_expr(left, target);
            substitute_self_in_expr(right, target);
        }
        Expr::Unary { expr, .. } => {
            substitute_self_in_expr(expr, target);
        }
        Expr::Call { callee, args, .. } => {
            substitute_self_in_expr(callee, target);
            for a in args {
                substitute_self_in_expr(a, target);
            }
        }
        Expr::MemberAccess { object, .. } => {
            substitute_self_in_expr(object, target);
        }
        Expr::IndexAccess { object, index, .. } => {
            substitute_self_in_expr(object, target);
            substitute_self_in_expr(index, target);
        }
        Expr::Range { start, end, .. } => {
            substitute_self_in_expr(start, target);
            substitute_self_in_expr(end, target);
        }
        Expr::Tuple(exprs, _) | Expr::ListLiteral(exprs, _) => {
            for e in exprs {
                substitute_self_in_expr(e, target);
            }
        }
        Expr::MapLiteral(entries, _) => {
            for (k, v) in entries {
                substitute_self_in_expr(k, target);
                substitute_self_in_expr(v, target);
            }
        }
        Expr::InterpolatedString { expressions, .. } => {
            for e in expressions {
                substitute_self_in_expr(e, target);
            }
        }
        Expr::Pipeline { stages, .. } => {
            for s in stages {
                substitute_self_in_expr(s, target);
            }
        }
        Expr::Decide { arms, else_arm, .. } => {
            for arm in arms {
                substitute_self_in_expr(&mut arm.condition, target);
                substitute_self_in_expr(&mut arm.body, target);
            }
            if let Some(el) = else_arm {
                substitute_self_in_expr(el, target);
            }
        }
        Expr::Match { value, arms, .. } => {
            substitute_self_in_expr(value, target);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    substitute_self_in_expr(g, target);
                }
                substitute_self_in_expr(&mut arm.body, target);
            }
        }
        Expr::Select { arms, else_arm, .. } => {
            for arm in arms {
                substitute_self_in_expr(&mut arm.condition, target);
                substitute_self_in_expr(&mut arm.body, target);
            }
            if let Some(el) = else_arm {
                substitute_self_in_expr(el, target);
            }
        }
        Expr::Lambda { params, body, .. } => {
            for p in params {
                if let Some(tn) = &mut p.type_node {
                    substitute_self_in_type_node(tn, target);
                }
            }
            substitute_self_in_expr(body, target);
        }
        Expr::ErrorPropagate(inner, _)
        | Expr::Wrapping(inner, _)
        | Expr::Saturating(inner, _)
        | Expr::Comptime { expr: inner, .. } => {
            substitute_self_in_expr(inner, target);
        }
        Expr::OrRecovery {
            expr: inner, arms, ..
        } => {
            substitute_self_in_expr(inner, target);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    substitute_self_in_expr(g, target);
                }
                substitute_self_in_expr(&mut arm.body, target);
            }
        }
        Expr::ArrayRepeatLiteral { elem, .. } => {
            substitute_self_in_expr(elem, target);
        }
        Expr::Block(stmts, trailing, ..) => {
            for s in stmts {
                substitute_self_in_stmt(s, target);
            }
            if let Some(t) = trailing {
                substitute_self_in_expr(t, target);
            }
        }
        Expr::Literal(..) => {}
    }
}

pub fn substitute_self_in_stmt(stmt: &mut Stmt, target: &str) {
    match stmt {
        Stmt::Block(stmts, _) => {
            for s in stmts {
                substitute_self_in_stmt(s, target);
            }
        }
        Stmt::Let {
            type_node, init, ..
        }
        | Stmt::Mut {
            type_node, init, ..
        }
        | Stmt::Const {
            type_node, init, ..
        }
        | Stmt::Val {
            type_node, init, ..
        } => {
            if let Some(tn) = type_node {
                substitute_self_in_type_node(tn, target);
            }
            substitute_self_in_expr(init, target);
        }
        Stmt::CompactBind { init, .. } => {
            substitute_self_in_expr(init, target);
        }
        Stmt::Assign {
            target: lhs, value, ..
        } => {
            substitute_self_in_expr(lhs, target);
            substitute_self_in_expr(value, target);
        }
        Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
            substitute_self_in_expr(e, target);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            substitute_self_in_expr(condition, target);
            substitute_self_in_stmt(then_branch, target);
            if let Some(eb) = else_branch {
                substitute_self_in_stmt(eb, target);
            }
        }
        Stmt::For { iterable, body, .. } => {
            substitute_self_in_expr(iterable, target);
            substitute_self_in_stmt(body, target);
        }
        Stmt::While {
            condition, body, ..
        } => {
            substitute_self_in_expr(condition, target);
            substitute_self_in_stmt(body, target);
        }
        Stmt::Loop { body, .. } => {
            substitute_self_in_stmt(body, target);
        }
        Stmt::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            substitute_self_in_stmt(try_block, target);
            substitute_self_in_stmt(catch_block, target);
        }
        Stmt::Parallel(body, _) | Stmt::Unsafe { body, .. } => {
            substitute_self_in_stmt(body, target);
        }
        Stmt::ParallelFor { iterable, body, .. } => {
            substitute_self_in_expr(iterable, target);
            substitute_self_in_stmt(body, target);
        }
        Stmt::With { init, body, .. } => {
            substitute_self_in_expr(init, target);
            substitute_self_in_stmt(body, target);
        }
        Stmt::Return(opt_e, _) => {
            if let Some(e) = opt_e {
                substitute_self_in_expr(e, target);
            }
        }
        Stmt::Asm { .. } => {}
    }
}
