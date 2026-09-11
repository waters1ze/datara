use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::types::{DataraType, TypeChecker};
use std::collections::{HashMap, HashSet};

impl<'a> TypeChecker<'a> {
    pub(crate) fn check_call(
        &mut self,
        callee: &Box<Expr>,
        args: &[Expr],
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) -> DataraType {
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
            if fn_name == "map" || fn_name == "filter" {
                return arg_types
                    .first()
                    .cloned()
                    .unwrap_or(DataraType::Class("List".into()));
            }
            if fn_name == "reduce" {
                return arg_types.get(1).cloned().unwrap_or(DataraType::Int);
            }
            if fn_name == "find" {
                if let Some(DataraType::List(elem)) = arg_types.first() {
                    return (**elem).clone();
                }
                return DataraType::Int;
            }
            if fn_name == "any" || fn_name == "all" {
                return DataraType::Bool;
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
            if fn_name == "float4"
                || fn_name == "datara_rt_float4"
                || fn_name == "min4"
                || fn_name == "max4"
            {
                return DataraType::Class("Float4".to_string());
            }
            if fn_name == "int4" || fn_name == "datara_rt_int4" {
                return DataraType::Class("Int4".to_string());
            }
            if fn_name == "dot"
                || fn_name == "datara_rt_float4_dot"
                || fn_name.starts_with("float4_")
                || fn_name.starts_with("lane")
            {
                return DataraType::Float;
            }
            if fn_name.starts_with("int4_") {
                return DataraType::Int;
            }

            if let Some(param_nodes) = self.function_param_nodes.get(fn_name).cloned() {
                for (arg, p_node) in args.iter().zip(param_nodes.iter()) {
                    if let Some(tn) = p_node {
                        self.check_refinement(tn, arg, arg.span(), diag);
                    }
                }
            }

            if let Some((param_types, ret_type, gen_params)) =
                self.function_signatures.get(fn_name).cloned()
            {
                let mut type_bindings: HashMap<String, DataraType> = HashMap::new();

                for (p_ty, a_ty) in param_types.iter().zip(arg_types.iter()) {
                    bind_type_params(p_ty, a_ty, &mut type_bindings);
                }

                for (idx, (p_ty, a_ty)) in param_types.iter().zip(arg_types.iter()).enumerate() {
                    let expected_ty = subst_type_params(p_ty, &type_bindings);
                    if !a_ty.is_compatible_with_refined_with_args(&expected_ty, Some(self.resolver))
                    {
                        diag.error(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch for argument {}: expected '{}', got '{}'",
                                idx + 1,
                                expected_ty,
                                a_ty
                            ),
                            Some(span.clone()),
                        );
                    }
                }

                for (param_name, concrete_ty) in &type_bindings {
                    let bounds = self
                        .trait_bounds
                        .get(&format!("{}:{}", fn_name, param_name))
                        .or_else(|| self.trait_bounds.get(param_name));
                    if let Some(trait_names) = bounds {
                        for tr_name in trait_names {
                            let type_str = match concrete_ty {
                                DataraType::Class(c) => c.clone(),
                                DataraType::Int => "Int".to_string(),
                                DataraType::Float => "Float".to_string(),
                                DataraType::String => "String".to_string(),
                                DataraType::Bool => "Bool".to_string(),
                                DataraType::Unit => "Unit".to_string(),
                                DataraType::GenericInstance { name, .. } => name.clone(),
                                other => other.to_string(),
                            };
                            let mut satisfied = self
                                .impls
                                .contains_key(&(tr_name.clone(), type_str.clone()));
                            if !satisfied {
                                if let DataraType::TypeParam(p) = concrete_ty {
                                    let caller_bounds = self
                                        .current_fn_name
                                        .as_ref()
                                        .and_then(|fn_n| {
                                            self.trait_bounds.get(&format!("{}:{}", fn_n, p))
                                        })
                                        .or_else(|| self.trait_bounds.get(p));
                                    if let Some(tb) = caller_bounds {
                                        if tb.contains(tr_name) {
                                            satisfied = true;
                                        }
                                    }
                                }
                            }
                            if !satisfied {
                                let mut queue = vec![type_str.clone()];
                                let mut visited = HashSet::new();
                                while let Some(cls_name) = queue.pop() {
                                    if !visited.insert(cls_name.clone()) {
                                        continue;
                                    }
                                    if self
                                        .impls
                                        .contains_key(&(tr_name.clone(), cls_name.clone()))
                                    {
                                        satisfied = true;
                                        break;
                                    }
                                    if let Some(cls_sym) = self.resolver.classes.get(&cls_name) {
                                        if cls_sym.compositions.iter().any(|comp| comp == tr_name) {
                                            satisfied = true;
                                            break;
                                        }
                                        for comp in &cls_sym.compositions {
                                            queue.push(comp.clone());
                                        }
                                        if let Some(base) = &cls_sym.base_type {
                                            queue.push(base.clone());
                                        }
                                    }
                                }
                            }
                            if !satisfied {
                                diag.error(
                                            ErrorCode::TypeMismatch,
                                            format!(
                                                "Type '{}' does not satisfy trait bound '{}' for parameter '{}' in function '{}'",
                                                concrete_ty, tr_name, param_name, fn_name
                                            ),
                                            Some(span.clone()),
                                        );
                            }
                        }
                    }
                }

                if !type_bindings.is_empty() {
                    let mut spec_args = Vec::new();
                    for gp in &gen_params {
                        if let Some(concrete) = type_bindings.get(gp) {
                            spec_args.push(concrete.clone());
                        }
                    }
                    if !spec_args.is_empty() {
                        self.generic_specializations
                            .entry(fn_name.clone())
                            .or_default()
                            .insert(spec_args);
                    }
                }

                return subst_type_params(&ret_type, &type_bindings);
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
                    "map" => {
                        if let Some(first_arg) = arg_types.first() {
                            if let DataraType::Function { return_type, .. } = first_arg {
                                return DataraType::List(return_type.clone());
                            }
                        }
                        return DataraType::List(elem.clone());
                    }
                    "filter" => return DataraType::List(elem.clone()),
                    "reduce" => {
                        if let Some(init_ty) = arg_types.first() {
                            return init_ty.clone();
                        }
                        return DataraType::Int;
                    }
                    "find" => return (**elem).clone(),
                    "any" | "all" => return DataraType::Bool,
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
                DataraType::Result(ok, err) => match member.as_str() {
                    "is_ok" | "is_err" => return DataraType::Bool,
                    "unwrap" | "ok" | "unwrap_or" => return (**ok).clone(),
                    "unwrap_err" | "err" => return (**err).clone(),
                    _ => {}
                },
                DataraType::Option(val) => match member.as_str() {
                    "is_some" | "is_none" => return DataraType::Bool,
                    "unwrap" | "unwrap_or" => return (**val).clone(),
                    _ => {}
                },
                _ => {}
            }
            if member == "await" {
                match &obj_type {
                    DataraType::GenericInstance { name, args }
                        if (name == "Future" || name == "Task") =>
                    {
                        return args.first().cloned().unwrap_or(DataraType::String);
                    }
                    DataraType::Class(name) if (name == "Future" || name == "Task") => {
                        return DataraType::String;
                    }
                    DataraType::Result(ok, _) => return (**ok).clone(),
                    DataraType::Option(val) => return (**val).clone(),
                    _ => return obj_type,
                }
            }
            let (cls_opt, gen_args_opt) = match &obj_type {
                DataraType::Class(cls) => (Some(cls.as_str()), None),
                DataraType::GenericInstance { name, args } => (Some(name.as_str()), Some(args)),
                _ => (None, None),
            };
            if let Some(cls) = cls_opt {
                let full_name = format!("{}.{}", cls, member);
                if let Some(t) = self.symbol_types.get(&full_name) {
                    if let (DataraType::TypeParam(p), Some(args)) = (t, gen_args_opt) {
                        if let Some((params, _)) = self.generic_templates.get(cls) {
                            if let Some(pos) = params.iter().position(|param| param == p) {
                                if let Some(concrete) = args.get(pos) {
                                    return concrete.clone();
                                }
                            }
                        }
                        if let Some(first) = args.first() {
                            return first.clone();
                        }
                    }
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
                        "set" | "push" | "append" | "map" | "filter" => {
                            return DataraType::Class("List".into());
                        }
                        "reduce" | "find" => {
                            return DataraType::Int;
                        }
                        "any" | "all" => {
                            return DataraType::Bool;
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
                if let Some(m_type) = self.class_methods.get(cls).and_then(|m| m.get(member)) {
                    if let (DataraType::TypeParam(p), Some(args)) = (m_type, gen_args_opt) {
                        if let Some((params, _)) = self.generic_templates.get(cls) {
                            if let Some(pos) = params.iter().position(|param| param == p) {
                                if let Some(concrete) = args.get(pos) {
                                    return concrete.clone();
                                }
                            }
                        }
                        if let Some(first) = args.first() {
                            return first.clone();
                        }
                    }
                    return m_type.clone();
                }
                let specialized = format!("{}_{}", cls, member);
                if let Some((_, ret_ty, _)) = self.function_signatures.get(&specialized) {
                    if let (DataraType::TypeParam(p), Some(args)) = (ret_ty, gen_args_opt) {
                        if let Some((params, _)) = self.generic_templates.get(cls) {
                            if let Some(pos) = params.iter().position(|param| param == p) {
                                if let Some(concrete) = args.get(pos) {
                                    return concrete.clone();
                                }
                            }
                        }
                        if let Some(first) = args.first() {
                            return first.clone();
                        }
                    }
                    return ret_ty.clone();
                }
            }
            if let DataraType::TypeParam(p) = &obj_type {
                let bounds = self
                    .current_fn_name
                    .as_ref()
                    .and_then(|fn_name| self.trait_bounds.get(&format!("{}:{}", fn_name, p)))
                    .or_else(|| self.trait_bounds.get(p));
                if let Some(trait_names) = bounds {
                    for tr_name in trait_names {
                        let mut visited = HashSet::new();
                        if let Some(tm) = self.find_trait_method(tr_name, member, &mut visited) {
                            let ret = tm
                                .return_type
                                .as_ref()
                                .map(|t| self.resolve_type_node(t, diag))
                                .unwrap_or(DataraType::Unit);
                            return ret;
                        }
                    }
                }
                let bound_str = bounds
                    .map(|b| b.join(" + "))
                    .unwrap_or_else(|| "none".to_string());
                diag.error(
                    ErrorCode::TypeMismatch,
                    format!(
                        "Method '{}' not found for type parameter '{}' with bounds '{}'",
                        member, p, bound_str
                    ),
                    Some(span.clone()),
                );
                return DataraType::Unit;
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
}

fn bind_type_params(
    p_ty: &DataraType,
    a_ty: &DataraType,
    bindings: &mut HashMap<String, DataraType>,
) {
    match (p_ty, a_ty) {
        (DataraType::TypeParam(param_name), _) => {
            bindings
                .entry(param_name.clone())
                .or_insert_with(|| a_ty.clone());
        }
        (DataraType::List(p_in), DataraType::List(a_in)) => {
            bind_type_params(p_in, a_in, bindings);
        }
        (DataraType::Map(pk, pv), DataraType::Map(ak, av)) => {
            bind_type_params(pk, ak, bindings);
            bind_type_params(pv, av, bindings);
        }
        (
            DataraType::GenericInstance {
                name: pn,
                args: pargs,
            },
            DataraType::GenericInstance {
                name: an,
                args: aargs,
            },
        ) if pn == an && pargs.len() == aargs.len() => {
            for (p, a) in pargs.iter().zip(aargs.iter()) {
                bind_type_params(p, a, bindings);
            }
        }
        _ => {}
    }
}

fn subst_type_params(ty: &DataraType, bindings: &HashMap<String, DataraType>) -> DataraType {
    match ty {
        DataraType::TypeParam(p) => bindings.get(p).cloned().unwrap_or_else(|| ty.clone()),
        DataraType::List(inner) => DataraType::List(Box::new(subst_type_params(inner, bindings))),
        DataraType::Map(k, v) => DataraType::Map(
            Box::new(subst_type_params(k, bindings)),
            Box::new(subst_type_params(v, bindings)),
        ),
        DataraType::GenericInstance { name, args } => DataraType::GenericInstance {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| subst_type_params(a, bindings))
                .collect(),
        },
        _ => ty.clone(),
    }
}
