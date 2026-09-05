use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::types::{DataraType, PropagationKind, PropagationSite, TypeChecker};
use std::collections::HashMap;

impl<'a> TypeChecker<'a> {
    pub fn check_expr(&mut self, expr: &Expr, diag: &mut DiagnosticEngine) -> DataraType {
        match expr {
            Expr::Literal(lit, _) => match lit {
                LiteralValue::Int(_) => DataraType::Int,
                LiteralValue::Float(_) => DataraType::Float,
                LiteralValue::String(_) => DataraType::String,
                LiteralValue::Bool(_) => DataraType::Bool,
                LiteralValue::Char(_) => DataraType::Char,
                LiteralValue::None => DataraType::Option(Box::new(DataraType::Unit)),
            },
            Expr::Identifier(name, _) => {
                if let Some(t) = self.symbol_types.get(name) {
                    return t.clone();
                }
                if self.resolver.classes.contains_key(name) {
                    return DataraType::Class(name.clone());
                }
                // Genuinely unknown identifier. The resolver has already
                // reported an undefined-symbol error for it, so do not spam a
                // second diagnostic here; `Unit` fails loudly downstream
                // (e.g. in arithmetic/conditions) instead of silently
                // pretending to be an `Int`.
                DataraType::Unit
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.check_expr(e, diag);
                }
                DataraType::String
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let lt = self.check_expr(left, diag);
                let rt = self.check_expr(right, diag);

                // --- Units of Measure Dimensional Analysis ---
                if let (
                    DataraType::Measure { base: b1, unit: u1 },
                    DataraType::Measure { base: b2, unit: u2 },
                ) = (&lt, &rt)
                {
                    let base = if **b1 == DataraType::Float || **b2 == DataraType::Float {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    match op.as_str() {
                        "+" | "-" => {
                            if u1 != u2 {
                                diag.error(
                                    ErrorCode::DimensionMismatch,
                                    format!(
                                        "Cannot perform '{}' on incompatible units of measure '{}' and '{}'",
                                        op, u1, u2
                                    ),
                                    Some(span.clone()),
                                );
                            }
                            return DataraType::Measure {
                                base: Box::new(base),
                                unit: u1.clone(),
                            };
                        }
                        "*" => {
                            let unit = format!("{}*{}", u1, u2);
                            return DataraType::Measure {
                                base: Box::new(base),
                                unit,
                            };
                        }
                        "/" => {
                            if u1 == u2 {
                                return base;
                            } else {
                                let unit = format!("{}/{}", u1, u2);
                                return DataraType::Measure {
                                    base: Box::new(base),
                                    unit,
                                };
                            }
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            if u1 != u2 {
                                diag.error(
                                    ErrorCode::DimensionMismatch,
                                    format!(
                                        "Cannot compare incompatible units of measure '{}' and '{}'",
                                        u1, u2
                                    ),
                                    Some(span.clone()),
                                );
                            }
                            return DataraType::Bool;
                        }
                        _ => {}
                    }
                } else if let DataraType::Measure { base, unit } = &lt {
                    match op.as_str() {
                        "*" => {
                            let b = if **base == DataraType::Float || rt == DataraType::Float {
                                DataraType::Float
                            } else {
                                DataraType::Int
                            };
                            return DataraType::Measure {
                                base: Box::new(b),
                                unit: unit.clone(),
                            };
                        }
                        "/" => {
                            let b = if **base == DataraType::Float || rt == DataraType::Float {
                                DataraType::Float
                            } else {
                                DataraType::Int
                            };
                            return DataraType::Measure {
                                base: Box::new(b),
                                unit: unit.clone(),
                            };
                        }
                        "+" | "-" => {
                            diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot perform '{}' between unit of measure '{}' and dimensionless quantity",
                                    op, unit
                                ),
                                Some(span.clone()),
                            );
                            return DataraType::Measure {
                                base: base.clone(),
                                unit: unit.clone(),
                            };
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            if matches!(
                                &**right,
                                Expr::Literal(LiteralValue::Int(_) | LiteralValue::Float(_), _)
                            ) {
                                return DataraType::Bool;
                            }
                            diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot compare unit of measure '{}' with dimensionless quantity",
                                    unit
                                ),
                                Some(span.clone()),
                            );
                            return DataraType::Bool;
                        }
                        _ => {}
                    }
                } else if let DataraType::Measure { base, unit } = &rt {
                    match op.as_str() {
                        "*" => {
                            let b = if lt == DataraType::Float || **base == DataraType::Float {
                                DataraType::Float
                            } else {
                                DataraType::Int
                            };
                            return DataraType::Measure {
                                base: Box::new(b),
                                unit: unit.clone(),
                            };
                        }
                        "+" | "-" => {
                            diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot perform '{}' between dimensionless quantity and unit of measure '{}'",
                                    op, unit
                                ),
                                Some(span.clone()),
                            );
                            return DataraType::Measure {
                                base: base.clone(),
                                unit: unit.clone(),
                            };
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                            if matches!(
                                &**left,
                                Expr::Literal(LiteralValue::Int(_) | LiteralValue::Float(_), _)
                            ) {
                                return DataraType::Bool;
                            }
                            diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot compare dimensionless quantity with unit of measure '{}'",
                                    unit
                                ),
                                Some(span.clone()),
                            );
                            return DataraType::Bool;
                        }
                        _ => {}
                    }
                }

                // --- Range Interval Arithmetic ---
                if let (
                    DataraType::Range {
                        base: b1,
                        min: min1,
                        max: max1,
                    },
                    DataraType::Range {
                        base: b2,
                        min: min2,
                        max: max2,
                    },
                ) = (&lt, &rt)
                {
                    let base = if **b1 == DataraType::Float || **b2 == DataraType::Float {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    match op.as_str() {
                        "+" => {
                            return DataraType::Range {
                                base: Box::new(base),
                                min: min1.saturating_add(*min2),
                                max: max1.saturating_add(*max2),
                            };
                        }
                        "-" => {
                            return DataraType::Range {
                                base: Box::new(base),
                                min: min1.saturating_sub(*max2),
                                max: max1.saturating_sub(*min2),
                            };
                        }
                        "*" => {
                            let p1 = min1.saturating_mul(*min2);
                            let p2 = min1.saturating_mul(*max2);
                            let p3 = max1.saturating_mul(*min2);
                            let p4 = max1.saturating_mul(*max2);
                            let min = p1.min(p2).min(p3).min(p4);
                            let max = p1.max(p2).max(p3).max(p4);
                            return DataraType::Range {
                                base: Box::new(base),
                                min,
                                max,
                            };
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                            return DataraType::Bool;
                        }
                        _ => {}
                    }
                } else if let DataraType::Range { base, min, max } = &lt {
                    if let Expr::Literal(LiteralValue::Int(n), _) = &**right {
                        let val = *n as i128;
                        match op.as_str() {
                            "+" => {
                                return DataraType::Range {
                                    base: base.clone(),
                                    min: min.saturating_add(val),
                                    max: max.saturating_add(val),
                                };
                            }
                            "-" => {
                                return DataraType::Range {
                                    base: base.clone(),
                                    min: min.saturating_sub(val),
                                    max: max.saturating_sub(val),
                                };
                            }
                            "*" => {
                                let p1 = min.saturating_mul(val);
                                let p2 = max.saturating_mul(val);
                                return DataraType::Range {
                                    base: base.clone(),
                                    min: p1.min(p2),
                                    max: p1.max(p2),
                                };
                            }
                            "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                                return DataraType::Bool;
                            }
                            _ => {}
                        }
                    }
                } else if let DataraType::Range { base, min, max } = &rt
                    && let Expr::Literal(LiteralValue::Int(n), _) = &**left
                {
                    let val = *n as i128;
                    match op.as_str() {
                        "+" => {
                            return DataraType::Range {
                                base: base.clone(),
                                min: val.saturating_add(*min),
                                max: val.saturating_add(*max),
                            };
                        }
                        "-" => {
                            return DataraType::Range {
                                base: base.clone(),
                                min: val.saturating_sub(*max),
                                max: val.saturating_sub(*min),
                            };
                        }
                        "*" => {
                            let p1 = val.saturating_mul(*min);
                            let p2 = val.saturating_mul(*max);
                            return DataraType::Range {
                                base: base.clone(),
                                min: p1.min(p2),
                                max: p1.max(p2),
                            };
                        }
                        "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                            return DataraType::Bool;
                        }
                        _ => {}
                    }
                }

                // --- Operator operand validation ---
                // `Val` (the dynamic type), `TypeParam`s, `Measure`/`Range`
                // quantities and `Never` are intentionally left permissive:
                // they carry their own rules (dimensional analysis and
                // interval arithmetic above) or are resolved dynamically.
                let is_open = |t: &DataraType| {
                    matches!(
                        t,
                        DataraType::Val
                            | DataraType::TypeParam(_)
                            | DataraType::Never
                            | DataraType::Measure { .. }
                            | DataraType::Range { .. }
                    )
                };
                let is_numeric = |t: &DataraType| {
                    matches!(
                        t,
                        DataraType::Int
                            | DataraType::Float
                            | DataraType::Dec64
                            | DataraType::Dec128
                    )
                };
                let is_orderable = |t: &DataraType| {
                    matches!(
                        t,
                        DataraType::Int
                            | DataraType::Float
                            | DataraType::Dec64
                            | DataraType::Dec128
                            | DataraType::String
                            | DataraType::Char
                    )
                };
                let report_bad_operands = |diag: &mut DiagnosticEngine| {
                    diag.error(
                        ErrorCode::TypeInvalidBinaryOp,
                        format!(
                            "Operator '{}' cannot be applied to operands of type '{}' and '{}'",
                            op, lt, rt
                        ),
                        Some(span.clone()),
                    );
                };

                if !is_open(&lt) && !is_open(&rt) {
                    match op.as_str() {
                        "+" | "-" | "*" | "/" | "%" => {
                            // Str concatenation via `+` is an intended
                            // language feature and stays permissive.
                            let concat =
                                op == "+" && (lt == DataraType::String || rt == DataraType::String);
                            if !concat && (!is_numeric(&lt) || !is_numeric(&rt)) {
                                report_bad_operands(diag);
                            }
                        }
                        "==" | "!=" => {
                            // Bool/Int cross-comparisons are intended
                            // dynamic behavior in Datara (truthy scalars).
                            let is_truthy_scalar = |t: &DataraType| {
                                matches!(
                                    t,
                                    DataraType::Int
                                        | DataraType::Float
                                        | DataraType::Dec64
                                        | DataraType::Dec128
                                        | DataraType::Bool
                                )
                            };
                            if !(is_truthy_scalar(&lt) && is_truthy_scalar(&rt))
                                && !lt
                                    .is_compatible_with_refined_with_args(&rt, Some(self.resolver))
                                && !rt
                                    .is_compatible_with_refined_with_args(&lt, Some(self.resolver))
                            {
                                report_bad_operands(diag);
                            }
                        }
                        "<" | "<=" | ">" | ">=" => {
                            if !is_orderable(&lt) || !is_orderable(&rt) {
                                report_bad_operands(diag);
                            }
                        }
                        "&&" | "||" => {
                            // Ints participate in logical ops as truthy
                            // values (C-style); that is intended behavior.
                            let is_logical =
                                |t: &DataraType| matches!(t, DataraType::Bool | DataraType::Int);
                            if !is_logical(&lt) || !is_logical(&rt) {
                                report_bad_operands(diag);
                            }
                        }
                        _ => {}
                    }
                }

                match op.as_str() {
                    "+" if lt == DataraType::String || rt == DataraType::String => {
                        DataraType::String
                    }
                    "+" | "-" | "*" | "/" | "%" => {
                        if lt == DataraType::Float || rt == DataraType::Float {
                            DataraType::Float
                        } else {
                            DataraType::Int
                        }
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => DataraType::Bool,
                    _ => lt,
                }
            }
            Expr::Unary { op, expr, .. } => {
                let inner = self.check_expr(expr, diag);
                if op == "!" { DataraType::Bool } else { inner }
            }
            Expr::Call { callee, args, span } => {
                let mut arg_types = Vec::new();
                for a in args {
                    arg_types.push(self.check_expr(a, diag));
                }

                if let Expr::Identifier(fn_name, _) = &**callee {
                    if fn_name == "view" || fn_name == "mut_view" || fn_name == "mutView" {
                        return arg_types.first().cloned().unwrap_or(DataraType::Unit);
                    }
                    if fn_name == "destroy" || fn_name == "unsafe_op" {
                        return DataraType::Unit;
                    }
                    if fn_name == "println" || fn_name == "print" || fn_name == "eprintln" {
                        return DataraType::Unit;
                    }
                    if fn_name == "input_int" {
                        return DataraType::Int;
                    }
                    if fn_name == "input_float" {
                        return DataraType::Float;
                    }
                    if fn_name == "len" || fn_name == "now" {
                        return DataraType::Int;
                    }
                    if fn_name == "panic" || fn_name == "exit" {
                        return DataraType::Never;
                    }
                    if fn_name == "assert" || fn_name == "require" {
                        return DataraType::Unit;
                    }
                    if fn_name == "input" || fn_name == "read_line" {
                        return DataraType::String;
                    }
                    if fn_name == "str_to_float" {
                        return DataraType::Float;
                    }

                    if let Some(param_nodes) = self.function_param_nodes.get(fn_name).cloned() {
                        for (arg, p_node) in args.iter().zip(param_nodes.iter()) {
                            if let Some(tn) = p_node {
                                self.check_refinement(tn, arg, arg.span(), diag);
                            }
                        }
                    }

                    if let Some((param_types, ret_type, _gen_params)) =
                        self.function_signatures.get(fn_name).cloned()
                    {
                        let mut type_bindings: HashMap<String, DataraType> = HashMap::new();

                        for (idx, (p_ty, a_ty)) in
                            param_types.iter().zip(arg_types.iter()).enumerate()
                        {
                            if let DataraType::TypeParam(param_name) = p_ty {
                                if let Some(existing_bound) = type_bindings.get(param_name) {
                                    if !existing_bound.is_compatible_with_refined_with_args(
                                        a_ty,
                                        Some(self.resolver),
                                    ) {
                                        diag.error(
                                            ErrorCode::TypeMismatch,
                                            format!(
                                                "Generic type parameter '{}' bound to '{}' but argument {} received '{}'",
                                                param_name, existing_bound, idx + 1, a_ty
                                            ),
                                            Some(span.clone()),
                                        );
                                    }
                                } else {
                                    type_bindings.insert(param_name.clone(), a_ty.clone());
                                }
                            } else if !a_ty
                                .is_compatible_with_refined_with_args(p_ty, Some(self.resolver))
                            {
                                diag.error(
                                    ErrorCode::TypeMismatch,
                                    format!(
                                        "Type mismatch for argument {}: expected '{}', got '{}'",
                                        idx + 1,
                                        p_ty,
                                        a_ty
                                    ),
                                    Some(span.clone()),
                                );
                            }
                        }

                        if let DataraType::TypeParam(p) = &ret_type
                            && let Some(bound) = type_bindings.get(p)
                        {
                            return bound.clone();
                        }
                        return ret_type;
                    }
                }

                if let Expr::MemberAccess { object, member, .. } = &**callee {
                    let obj_type = self.check_expr(object, diag);
                    if let DataraType::GenericInstance { name, args } = &obj_type
                        && name == "Capability"
                    {
                        let cap_kind = args.first().map(|a| a.to_string()).unwrap_or_default();
                        if cap_kind == "FileRead" {
                            if member == "open" {
                                return DataraType::Class("FileHandle".into());
                            }
                            if member == "read_all" {
                                return DataraType::String;
                            }
                        } else if cap_kind == "FileWrite" {
                            if member == "open" {
                                return DataraType::Class("FileWriteHandle".into());
                            }
                            if member == "write" || member == "write_all" {
                                return DataraType::Int;
                            }
                        } else if cap_kind == "NetworkConnect" && member == "connect" {
                            return DataraType::Int;
                        }
                    }
                    // Parametric collection methods: the receiver's element
                    // types flow into the result (e.g. `Map<Str, Int>.get`
                    // returns `Int`, not a hardcoded `Int` for every map).
                    match &obj_type {
                        DataraType::List(elem) => match member.as_str() {
                            "length" | "count" | "len" => return DataraType::Int,
                            "get" | "pop" => return (**elem).clone(),
                            "set" | "push" | "append" => return DataraType::List(elem.clone()),
                            _ => {}
                        },
                        DataraType::Map(key, val) => match member.as_str() {
                            "get" => return (**val).clone(),
                            "insert" => {
                                return DataraType::Map(key.clone(), val.clone());
                            }
                            "length" | "count" | "len" => return DataraType::Int,
                            _ => {}
                        },
                        _ => {}
                    }
                    if let DataraType::Class(cls) = &obj_type {
                        let full_name = format!("{}.{}", cls, member);
                        if let Some(t) = self.symbol_types.get(&full_name) {
                            return t.clone();
                        }
                        if cls == "List" {
                            match member.as_str() {
                                "length" | "count" | "len" => return DataraType::Int,
                                "get" | "pop" => {
                                    // Element type recorded from the initializer
                                    // when the receiver is a named variable;
                                    // otherwise the dynamic type, never Int.
                                    if let Expr::Identifier(name, _) = &**object
                                        && let Some(elem) = self.var_element_types.get(name)
                                    {
                                        return elem.clone();
                                    }
                                    return DataraType::Val;
                                }
                                "set" | "push" | "append" => {
                                    return DataraType::Class("List".into());
                                }
                                _ => {}
                            }
                        }
                        if cls == "Map" {
                            match member.as_str() {
                                "get" => {
                                    if let Expr::Identifier(name, _) = &**object
                                        && let Some(elem) = self.var_element_types.get(name)
                                    {
                                        return elem.clone();
                                    }
                                    return DataraType::Val;
                                }
                                "insert" => {
                                    return DataraType::Class("Map".into());
                                }
                                "length" | "count" | "len" => return DataraType::Int,
                                _ => {}
                            }
                        }
                        if let Some(m_type) =
                            self.class_methods.get(cls).and_then(|m| m.get(member))
                        {
                            return m_type.clone();
                        }
                        let specialized = format!("{}_{}", cls, member);
                        if let Some((_, ret_ty, _)) = self.function_signatures.get(&specialized) {
                            return ret_ty.clone();
                        }
                    }
                    // Method-call fallback: only prelude builtins may be
                    // resolved as methods on arbitrary receivers. Resolving
                    // user-defined global functions here would let any
                    // `obj.foo()` silently call the global `foo`.
                    if !self.resolver.functions.contains_key(member)
                        && !self.resolver.extern_functions.contains_key(member)
                        && let Some((_, ret_ty, _)) = self.function_signatures.get(member)
                    {
                        return ret_ty.clone();
                    }
                }

                let callee_ty = self.check_expr(callee, diag);
                if let DataraType::Function { return_type, .. } = callee_ty {
                    return *return_type;
                }
                DataraType::Unit
            }
            Expr::MemberAccess {
                object,
                member,
                span,
            } => {
                let obj_type = self.check_expr(object, diag);
                match &obj_type {
                    DataraType::Class(cls_name) => {
                        let full_name = format!("{}.{}", cls_name, member);
                        if let Some(t) = self.symbol_types.get(&full_name) {
                            return t.clone();
                        }
                        if cls_name == "SystemCapabilities" {
                            if member == "files" {
                                return DataraType::Class("FileCapabilityProvider".into());
                            }
                            if member == "net" {
                                return DataraType::Class("NetCapabilityProvider".into());
                            }
                            if member == "proc" {
                                return DataraType::Class("ProcessCapabilityProvider".into());
                            }
                        }
                        if member == "view" || member == "clone" || member == "mut_view" {
                            return DataraType::Class(cls_name.clone());
                        }
                        if let Some(pkt) = self.resolver.packets.get(cls_name)
                            && pkt.fields.iter().any(|f| &f.name == member)
                        {
                            return DataraType::Int;
                        }
                        if let Some(fields) = self.class_fields.get(cls_name)
                            && let Some(f_type) = fields.get(member)
                        {
                            return f_type.clone();
                        }
                        if let Some((_, t_fields)) = self.generic_templates.get(cls_name)
                            && let Some(f_type) = t_fields.get(member)
                        {
                            return f_type.clone();
                        }
                        if let Some(methods) = self.class_methods.get(cls_name)
                            && methods.contains_key(member)
                        {
                            return DataraType::Unit;
                        }
                        let mut known_fields: Vec<&str> = Vec::new();
                        if let Some(fields) = self.class_fields.get(cls_name) {
                            known_fields.extend(fields.keys().map(|s| s.as_str()));
                        }
                        if let Some((_, t_fields)) = self.generic_templates.get(cls_name) {
                            known_fields.extend(t_fields.keys().map(|s| s.as_str()));
                        }
                        if let Some(methods) = self.class_methods.get(cls_name) {
                            known_fields.extend(methods.keys().map(|s| s.as_str()));
                        }
                        known_fields.sort_unstable();
                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(member, known_fields)
                        {
                            format!(
                                "class '{}' has a field or method with a similar name: '{}'",
                                cls_name, similar
                            )
                        } else {
                            format!(
                                "class '{}' does not define field or method '{}'",
                                cls_name, member
                            )
                        };
                        diag.error_with_help(
                            ErrorCode::TypeInvalidMemberAccess,
                            format!(
                                "Class '{}' has no field or method named '{}'",
                                cls_name, member
                            ),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                    DataraType::GenericInstance { name, args } => {
                        if member == "view" || member == "clone" || member == "mut_view" {
                            return DataraType::GenericInstance {
                                name: name.clone(),
                                args: args.clone(),
                            };
                        }
                        if let Some((params, t_fields)) = self.generic_templates.get(name)
                            && let Some(field_type) = t_fields.get(member)
                        {
                            if let DataraType::TypeParam(p) = field_type
                                && let Some(idx) = params.iter().position(|param| param == p)
                                && idx < args.len()
                            {
                                return args[idx].clone();
                            }
                            return field_type.clone();
                        }
                    }
                    _ => {}
                }
                // MemberAccess on a non-class value has no meaningful static
                // type; `Unit` fails loudly downstream instead of silently
                // pretending to be a `Str`.
                DataraType::Unit
            }
            Expr::ObjectInit {
                class_name,
                generic_args,
                fields,
                ..
            } => {
                let mut inferred_args = Vec::new();
                for g in generic_args {
                    inferred_args.push(self.resolve_type_node(g, diag));
                }

                let mut field_types = HashMap::new();
                for (fname, val) in fields {
                    let ft = self.check_expr(val, diag);
                    field_types.insert(fname.clone(), ft);
                }

                if let Some((params, t_fields)) = self.generic_templates.get(class_name) {
                    if inferred_args.is_empty() && !params.is_empty() {
                        // Infer type parameters deterministically: bind each
                        // template type parameter from the initializer's
                        // field values, walking the initializer's fields in
                        // declaration order (never arbitrary HashMap order).
                        let param_index: HashMap<&String, usize> =
                            params.iter().enumerate().map(|(i, p)| (p, i)).collect();
                        let mut bindings: Vec<Option<DataraType>> = vec![None; params.len()];
                        for (fname, _) in fields {
                            if let (Some(DataraType::TypeParam(p)), Some(v_ty)) =
                                (t_fields.get(fname), field_types.get(fname))
                                && let Some(&i) = param_index.get(p)
                                && bindings[i].is_none()
                            {
                                bindings[i] = Some(v_ty.clone());
                            }
                        }
                        if bindings.iter().all(Option::is_some) {
                            for b in bindings.into_iter().flatten() {
                                inferred_args.push(b);
                            }
                        } else if params.len() == 1 {
                            // Fallback for a single parameter with no
                            // directly-matching template field: use the first
                            // initializer value in declaration order.
                            if let Some((fname, _)) = fields.first()
                                && let Some(v_ty) = field_types.get(fname)
                            {
                                inferred_args.push(v_ty.clone());
                            }
                        }
                    }

                    if !inferred_args.is_empty() {
                        self.generic_specializations
                            .entry(class_name.clone())
                            .or_default()
                            .insert(inferred_args.clone());

                        return DataraType::GenericInstance {
                            name: class_name.clone(),
                            args: inferred_args,
                        };
                    }
                }

                DataraType::Class(class_name.clone())
            }
            Expr::Pipeline { stages, .. } => {
                let mut current = DataraType::Int;
                for s in stages {
                    current = self.check_expr(s, diag);
                }
                current
            }
            Expr::Decide { arms, else_arm, .. } => {
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    self.check_expr(&arm.condition, diag);
                    let body_ty = self.check_expr(&arm.body, diag);
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible_with_args(u, Some(self.resolver))
                            && !u.is_compatible_with_args(&body_ty, Some(self.resolver))
                        {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in decide arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                if let Some(eb) = else_arm {
                    let else_ty = self.check_expr(eb, diag);
                    if let Some(ref u) = unified {
                        if !else_ty.is_compatible_with_args(u, Some(self.resolver))
                            && !u.is_compatible_with_args(&else_ty, Some(self.resolver))
                        {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in decide else branch: '{}' and '{}'",
                                    u, else_ty
                                ),
                                Some(eb.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(else_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Match { value, arms, span } => {
                let val_ty = self.check_expr(value, diag);
                self.check_match_exhaustiveness(&val_ty, arms, span, diag);
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    let mut bound = Vec::new();
                    match &arm.pattern {
                        Pattern::Identifier(name, _) if name != "_" => {
                            let prev = self.symbol_types.insert(name.clone(), val_ty.clone());
                            bound.push((name.clone(), prev));
                        }
                        Pattern::Variant {
                            variant_name,
                            bindings,
                            ..
                        } => {
                            for (i, b) in bindings.iter().enumerate() {
                                let bind_ty = match &val_ty {
                                    DataraType::Option(inner)
                                        if (variant_name == "Some") && i == 0 =>
                                    {
                                        (**inner).clone()
                                    }
                                    DataraType::Result(ok, _)
                                        if (variant_name == "Ok") && i == 0 =>
                                    {
                                        (**ok).clone()
                                    }
                                    DataraType::Result(_, err)
                                        if (variant_name == "Err") && i == 0 =>
                                    {
                                        (**err).clone()
                                    }
                                    DataraType::Class(cls) => {
                                        if let Some(e) = self.resolver.enums.get(cls) {
                                            if let Some(v) = e
                                                .variants
                                                .iter()
                                                .find(|var| &var.name == variant_name)
                                            {
                                                if let Some(f_tn) = v.fields.get(i) {
                                                    self.resolve_type_node(f_tn, diag)
                                                } else {
                                                    val_ty.clone()
                                                }
                                            } else {
                                                val_ty.clone()
                                            }
                                        } else {
                                            val_ty.clone()
                                        }
                                    }
                                    _ => val_ty.clone(),
                                };
                                let prev = self.symbol_types.insert(b.clone(), bind_ty);
                                bound.push((b.clone(), prev));
                            }
                        }
                        _ => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.check_expr(g, diag);
                    }
                    let body_ty = self.check_expr(&arm.body, diag);
                    for (name, prev) in bound {
                        if let Some(p) = prev {
                            self.symbol_types.insert(name, p);
                        } else {
                            self.symbol_types.remove(&name);
                        }
                    }
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible_with_args(u, Some(self.resolver))
                            && !u.is_compatible_with_args(&body_ty, Some(self.resolver))
                        {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in match arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Select { arms, else_arm, .. } => {
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    self.check_expr(&arm.condition, diag);
                    let body_ty = self.check_expr(&arm.body, diag);
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible_with_args(u, Some(self.resolver))
                            && !u.is_compatible_with_args(&body_ty, Some(self.resolver))
                        {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in select arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                if let Some(eb) = else_arm {
                    let else_ty = self.check_expr(eb, diag);
                    if let Some(ref u) = unified {
                        if !else_ty.is_compatible_with_args(u, Some(self.resolver))
                            && !u.is_compatible_with_args(&else_ty, Some(self.resolver))
                        {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in select else branch: '{}' and '{}'",
                                    u, else_ty
                                ),
                                Some(eb.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(else_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Lambda { params, body, .. } => {
                let mut bound = Vec::new();
                let mut param_types = Vec::new();
                for p in params {
                    let p_ty = p
                        .type_node
                        .as_ref()
                        .map(|t| self.resolve_type_node(t, diag))
                        .unwrap_or(DataraType::Int);
                    param_types.push(p_ty.clone());
                    let prev = self.symbol_types.insert(p.name.clone(), p_ty);
                    bound.push((p.name.clone(), prev));
                }
                let ret_ty = self.check_expr(body, diag);
                for (name, prev) in bound {
                    if let Some(p) = prev {
                        self.symbol_types.insert(name, p);
                    } else {
                        self.symbol_types.remove(&name);
                    }
                }
                DataraType::Function {
                    params: param_types,
                    return_type: Box::new(ret_ty),
                }
            }
            Expr::ListLiteral(items, _) => {
                // Unify element types: all elements of one type -> List(T);
                // mixed or unknown elements -> List(Val) (the dynamic type),
                // never a silent Int default.
                let mut elem: Option<DataraType> = None;
                for item in items {
                    let t = self.check_expr(item, diag);
                    if t == DataraType::Unit {
                        continue;
                    }
                    match &elem {
                        None => elem = Some(t),
                        Some(prev) if *prev == t => {}
                        Some(_) => elem = Some(DataraType::Val),
                    }
                }
                let elem_ty = elem.unwrap_or(DataraType::Val);
                self.last_list_element = Some(elem_ty.clone());
                DataraType::List(Box::new(elem_ty))
            }
            Expr::MapLiteral(entries, _) => {
                // Unify key/value types the same way list elements are
                // unified: one type -> Map(K, V); mixed/empty -> Val.
                let mut key: Option<DataraType> = None;
                let mut value: Option<DataraType> = None;
                for (k, v) in entries {
                    let kt = self.check_expr(k, diag);
                    let vt = self.check_expr(v, diag);
                    match &key {
                        None => key = Some(kt),
                        Some(prev) if *prev == kt => {}
                        Some(_) => key = Some(DataraType::Val),
                    }
                    match &value {
                        None => value = Some(vt),
                        Some(prev) if *prev == vt => {}
                        Some(_) => value = Some(DataraType::Val),
                    }
                }
                DataraType::Map(
                    Box::new(key.unwrap_or(DataraType::Val)),
                    Box::new(value.unwrap_or(DataraType::Val)),
                )
            }
            Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.check_expr(object, diag);
                let idx_ty = self.check_expr(index, diag);

                // Static negative index check and Range bounds verification
                let static_idx = match &**index {
                    Expr::Literal(LiteralValue::Int(n), _) => Some(*n),
                    Expr::Unary { op, expr, .. } if op == "-" => {
                        if let Expr::Literal(LiteralValue::Int(n), _) = &**expr {
                            Some(-*n)
                        } else {
                            Some(-1)
                        }
                    }
                    _ => None,
                };

                if let Some(n) = static_idx {
                    if n < 0 {
                        diag.error_with_help(
                            ErrorCode::RangeViolation,
                            format!("Array index cannot be negative: {}", n),
                            Some(index.span().clone()),
                            Some("Array indices in Datara must be non-negative (>= 0)".to_string()),
                        );
                    }
                } else if let DataraType::Range { min, .. } = &idx_ty
                    && *min < 0
                {
                    diag.error_with_help(
                        ErrorCode::RangeViolation,
                        format!(
                            "Array index range allows negative values (minimum is {})",
                            min
                        ),
                        Some(index.span().clone()),
                        Some(
                            "Constrain index range to be non-negative: e.g. UInt or Int<0..>"
                                .to_string(),
                        ),
                    );
                }

                if let Expr::Identifier(name, _) = &**object
                    && let Some(arr_len) = self.var_array_lengths.get(name).copied()
                {
                    if let Some(n) = static_idx {
                        if n >= 0 && (n as usize) >= arr_len {
                            diag.error_with_help(
                                ErrorCode::RangeViolation,
                                format!(
                                    "Index {} is out of bounds for array '{}' of length {}",
                                    n, name, arr_len
                                ),
                                Some(index.span().clone()),
                                Some(format!(
                                    "Valid indices are 0..{}",
                                    arr_len.saturating_sub(1)
                                )),
                            );
                        }
                    } else if let DataraType::Range { max, .. } = &idx_ty
                        && *max >= arr_len as i128
                    {
                        diag.error_with_help(
                            ErrorCode::RangeViolation,
                            format!(
                                "Index range maximum {} may exceed array '{}' bound of length {}",
                                max, name, arr_len
                            ),
                            Some(index.span().clone()),
                            Some(format!(
                                "Constrain index range to 0..{}",
                                arr_len.saturating_sub(1)
                            )),
                        );
                    }
                }

                if idx_ty == DataraType::Class("Range".into()) {
                    obj_ty
                } else {
                    match &obj_ty {
                        // Parametric collections carry their element types:
                        // `l[i]` -> T, `m[k]` -> V.
                        DataraType::List(elem) => (**elem).clone(),
                        DataraType::Map(_, val) => (**val).clone(),
                        DataraType::GenericInstance { name, args }
                            if name == "List" && !args.is_empty() =>
                        {
                            args[0].clone()
                        }
                        DataraType::GenericInstance { name, args }
                            if name == "Map" && args.len() == 2 =>
                        {
                            args[1].clone()
                        }
                        // Erased collections: fall back to the recorded
                        // element type, else the dynamic type `Val` — never
                        // a silent `Int` default.
                        DataraType::Class(c) if c == "List" => {
                            if let Expr::Identifier(name, _) = &**object
                                && let Some(elem) = self.var_element_types.get(name)
                            {
                                elem.clone()
                            } else {
                                self.last_list_element.clone().unwrap_or(DataraType::Val)
                            }
                        }
                        DataraType::Class(c) if c == "Map" => DataraType::Val,
                        // Indexing a non-collection has no statically known
                        // result; `Val` fails loudly in typed contexts instead
                        // of silently pretending to be an `Int`.
                        _ => DataraType::Val,
                    }
                }
            }
            Expr::Range { start, end, .. } => {
                self.check_expr(start, diag);
                self.check_expr(end, diag);
                DataraType::Class("Range".into())
            }
            Expr::Tuple(exprs, _) => {
                let types = exprs.iter().map(|e| self.check_expr(e, diag)).collect();
                DataraType::Tuple(types)
            }
            Expr::ErrorPropagate(inner, span) => {
                let t = self.check_expr(inner, diag);
                // `?` is real error propagation, not a silent no-op. Two hard
                // rules keep it predictable:
                //   1. the operand must be Result-like (`T!E`, `Result<T,E>`,
                //      `Outcome<T>`) or Option-like (`T?`, `Option<T>`, `Maybe<T>`);
                //   2. the enclosing function/method must return the same kind
                //      with a compatible payload, otherwise there is nothing to
                //      propagate the error into.
                let (kind, payload) = if let Some((ok, _err)) = t.result_like() {
                    (PropagationKind::Outcome, ok)
                } else if let Some(inner_ty) = t.option_like() {
                    (PropagationKind::Maybe, inner_ty)
                } else {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!(
                            "'?' requires a Result ('T!E') or Option ('T?') operand, got '{}'",
                            t
                        ),
                        Some(span.clone()),
                    );
                    return t;
                };

                let expected_payload = match kind {
                    PropagationKind::Outcome => self
                        .current_return_type
                        .as_ref()
                        .and_then(|r| r.result_like())
                        .map(|(ok, _)| ok),
                    PropagationKind::Maybe => self
                        .current_return_type
                        .as_ref()
                        .and_then(|r| r.option_like()),
                };
                match expected_payload {
                    None => {
                        diag.error(
                            ErrorCode::TypeMismatch,
                            format!(
                                "'?' propagates a {} but the enclosing function returns '{}'; the function must return the same Result/Option type to propagate",
                                match kind {
                                    PropagationKind::Outcome => "Result error",
                                    PropagationKind::Maybe => "None",
                                },
                                self.current_return_type
                                    .as_ref()
                                    .map(|r| r.to_string())
                                    .unwrap_or_else(|| "Unit".into()),
                            ),
                            Some(span.clone()),
                        );
                    }
                    Some(expected) => {
                        if !payload.is_compatible_with_args(&expected, Some(self.resolver)) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "'?' unwraps '{}' but the enclosing function returns a Result/Option of '{}'",
                                    payload, expected
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }

                self.propagation_sites.push(PropagationSite {
                    span: span.clone(),
                    kind,
                    payload_repr: payload.to_string(),
                });
                payload
            }
            Expr::OrRecovery { expr, arms, .. } => {
                let expr_ty = self.check_expr(expr, diag);
                let inner_ty = if let Some((ok, _err)) = expr_ty.result_like() {
                    ok
                } else if let Some(inner) = expr_ty.option_like() {
                    inner
                } else {
                    expr_ty
                };
                for arm in arms {
                    let _ = self.check_expr(&arm.body, diag);
                }
                inner_ty
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                let elem_ty = self.check_expr(elem, diag);
                DataraType::GenericInstance {
                    name: "Array".to_string(),
                    args: vec![elem_ty],
                }
            }
            Expr::Comptime { expr, .. } => self.check_expr(expr, diag),
            Expr::Block(stmts, value, _) => {
                // Lexical scope: declarations inside the block must not leak
                // into sibling arms or the enclosing scope (same pattern as
                // Stmt::Block in check_stmt).
                let saved_types = self.symbol_types.clone();
                let saved_mut = self.symbol_mutability.clone();
                let saved_elem = self.var_element_types.clone();
                let saved_refinements = self.var_refinements.clone();
                let saved_lengths = self.var_array_lengths.clone();
                for s in stmts {
                    self.check_stmt(s, diag);
                }
                let result = match value {
                    Some(v) => self.check_expr(v, diag),
                    None => DataraType::Unit,
                };
                self.symbol_types = saved_types;
                self.symbol_mutability = saved_mut;
                self.var_element_types = saved_elem;
                self.var_refinements = saved_refinements;
                self.var_array_lengths = saved_lengths;
                result
            }
        }
    }
}
