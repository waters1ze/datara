use crate::ast::*;
use crate::dmir::ir::*;
use crate::types::DataraType;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub(crate) fn infer_expr_datara_type(&self, expr: &Expr) -> Option<DataraType> {
        match expr {
            Expr::Literal(lit, _) => match lit {
                LiteralValue::Int(_) => Some(DataraType::Int),
                LiteralValue::Float(_) => Some(DataraType::Float),
                LiteralValue::Bool(_) => Some(DataraType::Bool),
                LiteralValue::String(_) => Some(DataraType::String),
                LiteralValue::Char(_) => Some(DataraType::Char),
                LiteralValue::None => Some(DataraType::Unit),
            },
            Expr::Identifier(name, _) => {
                if let Some(ty) = self.lookup_var_type(name) {
                    return Some(ty);
                }
                if let Some(t) = self.class_field_types.get(name) {
                    if t.starts_with("List<") && t.ends_with('>') {
                        let inner = &t[5..t.len() - 1];
                        let elem_ty = match inner {
                            "Float" | "Float64" | "Float32" => DataraType::Float,
                            "Int" | "Int64" | "Int32" => DataraType::Int,
                            "String" | "Str" => DataraType::String,
                            "Bool" => DataraType::Bool,
                            _ => DataraType::Class(inner.to_string()),
                        };
                        return Some(DataraType::List(Box::new(elem_ty)));
                    }
                    match t.as_str() {
                        "Float" | "Float64" | "Float32" => return Some(DataraType::Float),
                        "Int" | "Int64" | "Int32" => return Some(DataraType::Int),
                        "String" | "Str" => return Some(DataraType::String),
                        "Bool" => return Some(DataraType::Bool),
                        _ => return Some(DataraType::Class(t.clone())),
                    }
                }
                None
            }
            Expr::MemberAccess { object, member, .. } => {
                let obj_ty = self.infer_expr_datara_type(object)?;
                match obj_ty {
                    DataraType::GenericInstance { name, args } => {
                        let (params, t_fields) = self.types.generic_templates.get(&name)?;
                        let field_type = t_fields.get(member)?;
                        if let DataraType::TypeParam(p) = field_type
                            && let Some(idx) = params.iter().position(|param| param == p)
                            && idx < args.len()
                        {
                            return Some(args[idx].clone());
                        }
                        Some(field_type.clone())
                    }
                    DataraType::Class(cls_name) => {
                        if let Some(fields) = self.types.class_fields.get(&cls_name) {
                            if let Some(ft) = fields.get(member) {
                                return Some(ft.clone());
                            }
                        }
                        let key = format!("{}.{}", cls_name, member);
                        if let Some(t) = self.class_field_types.get(&key) {
                            if t.starts_with("List<") && t.ends_with('>') {
                                let inner = &t[5..t.len() - 1];
                                let elem_ty = match inner {
                                    "Float" | "Float64" | "Float32" => DataraType::Float,
                                    "Int" | "Int64" | "Int32" => DataraType::Int,
                                    "String" | "Str" => DataraType::String,
                                    "Bool" => DataraType::Bool,
                                    _ => DataraType::Class(inner.to_string()),
                                };
                                return Some(DataraType::List(Box::new(elem_ty)));
                            }
                            match t.as_str() {
                                "Float" | "Float64" | "Float32" => return Some(DataraType::Float),
                                "Int" | "Int64" | "Int32" => return Some(DataraType::Int),
                                "String" | "Str" => return Some(DataraType::String),
                                "Bool" => return Some(DataraType::Bool),
                                _ => return Some(DataraType::Class(t.clone())),
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            Expr::IndexAccess { object, .. } => {
                let obj_ty = self.infer_expr_datara_type(object)?;
                match obj_ty {
                    DataraType::List(elem) => Some(*elem),
                    DataraType::Map(_, val) => Some(*val),
                    _ => None,
                }
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                if matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                ) {
                    return Some(DataraType::Bool);
                }
                let l_ty = self.infer_expr_datara_type(left);
                let r_ty = self.infer_expr_datara_type(right);
                if l_ty == Some(DataraType::Float) || r_ty == Some(DataraType::Float) {
                    return Some(DataraType::Float);
                }
                if l_ty == Some(DataraType::String) || r_ty == Some(DataraType::String) {
                    return Some(DataraType::String);
                }
                l_ty.or(r_ty)
            }
            Expr::Unary { op, expr, .. } => {
                if op == "!" {
                    Some(DataraType::Bool)
                } else {
                    self.infer_expr_datara_type(expr)
                }
            }
            Expr::Call { callee, .. } => match &**callee {
                Expr::Identifier(fn_name, _) => {
                    let ret = self.infer_fn_ret_ty(fn_name);
                    match ret.as_str() {
                        "Float" => Some(DataraType::Float),
                        "String" => Some(DataraType::String),
                        "Bool" => Some(DataraType::Bool),
                        "Int" => Some(DataraType::Int),
                        _ => None,
                    }
                }
                _ => None,
            },
            Expr::ListLiteral(elements, _) => {
                let elem_ty = elements
                    .first()
                    .and_then(|e| self.infer_expr_datara_type(e))
                    .unwrap_or(DataraType::Int);
                Some(DataraType::List(Box::new(elem_ty)))
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                let elem_ty = self.infer_expr_datara_type(elem).unwrap_or_else(|| {
                    if self.is_expr_float(elem) {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    }
                });
                Some(DataraType::List(Box::new(elem_ty)))
            }
            _ => None,
        }
    }

    pub(crate) fn member_field_repr(&self, object: &Expr, member: &str) -> Option<String> {
        let obj_ty = self.infer_expr_datara_type(object)?;
        match obj_ty {
            DataraType::GenericInstance { name, args } => {
                let (params, t_fields) = self.types.generic_templates.get(&name)?;
                let field_type = t_fields.get(member)?;
                if let DataraType::TypeParam(p) = field_type
                    && let Some(idx) = params.iter().position(|param| param == p)
                    && idx < args.len()
                {
                    return Some(args[idx].to_string());
                }
                Some(field_type.to_string())
            }
            DataraType::Class(cls_name) => {
                if let Some(fields) = self.types.class_fields.get(&cls_name) {
                    if let Some(f) = fields.get(member) {
                        return Some(f.to_string());
                    }
                }
                let key = format!("{}.{}", cls_name, member);
                self.class_field_types.get(&key).cloned()
            }
            _ => None,
        }
    }

    pub(crate) fn infer_fn_ret_ty(&self, func_name: &str) -> String {
        if let Some(ty) = self.function_return_types.get(func_name) {
            return ty.clone();
        }
        if func_name == "str_to_int"
            || func_name == "datara_rt_str_to_int"
            || func_name == "js_eval_int"
            || func_name == "datara_js_eval_int"
            || func_name == "py_eval_int"
            || func_name == "datara_py_eval_int"
            || func_name == "py_import"
            || func_name == "datara_py_import"
            || func_name == "py_exec"
            || func_name == "datara_py_exec"
            || func_name == "datara_py_export_list_f64"
            || func_name == "datara_py_assert_same_ptr"
            || func_name == "str_len"
            || func_name == "datara_rt_str_len"
            || func_name.ends_with("_to_int")
            || func_name.contains("count")
            || func_name.contains("index")
        {
            "Int".into()
        } else if func_name == "str_to_float"
            || func_name == "datara_rt_str_to_float"
            || func_name == "js_eval_float"
            || func_name == "datara_js_eval_float"
            || func_name == "py_eval_float"
            || func_name == "datara_py_eval_float"
            || func_name == "py_call_1_float"
            || func_name == "datara_py_call_1_float"
            || func_name.ends_with("_to_float")
            || func_name.contains("float")
            || func_name.contains("flt")
        {
            "Float".into()
        } else if func_name == "str_split"
            || func_name == "split"
            || func_name == "datara_rt_str_split"
        {
            "List".into()
        } else if func_name.contains("is_")
            || func_name.contains("has_")
            || func_name.contains("contains")
            || func_name.contains("starts_with")
            || func_name.ends_with("_with")
        {
            "Bool".into()
        } else if func_name.contains("string")
            || func_name.contains("to_str")
            || func_name.ends_with("_str")
            || func_name.starts_with("js_")
            || func_name.starts_with("datara_js_")
            || func_name.starts_with("py_")
            || func_name.starts_with("datara_py_")
            || func_name.contains("classify")
            || func_name.contains("handle")
            || func_name.contains("format")
            || func_name.contains("render")
            || func_name.contains("quote")
            || func_name.contains("summary")
            || func_name.contains("repeat")
            || func_name.contains("pad")
            || func_name.contains("replace")
            || func_name.contains("upper")
            || func_name.contains("lower")
            || func_name.contains("join")
            || func_name == "join"
            || func_name == "str_join"
            || func_name == "datara_rt_str_join"
            || func_name == "read"
            || func_name == "file_read"
            || func_name == "datara_rt_file_read"
            || func_name == "env_get"
            || func_name == "datara_rt_env_get"
            || func_name == "args_get"
            || func_name == "datara_rt_args_get"
            || func_name == "str_trim"
            || func_name == "datara_rt_str_trim"
            || func_name == "socket_recv"
            || func_name == "datara_rt_socket_recv"
            || func_name == "sha256"
            || func_name == "datara_rt_sha256"
            || func_name == "base64_encode"
            || func_name == "datara_rt_base64_encode"
            || func_name == "base64_decode"
            || func_name == "datara_rt_base64_decode"
            || func_name == "uuid_v4"
            || func_name == "datara_rt_uuid_v4"
            || func_name == "int_to_str"
            || func_name == "datara_rt_int_to_str"
            || func_name == "float_to_str"
            || func_name == "datara_rt_float_to_str"
            || func_name == "str_char_at"
            || func_name == "datara_rt_str_char_at"
            || func_name == "char_at"
            || func_name.ends_with("_char_at")
        {
            "String".into()
        } else {
            "Int".into()
        }
    }

    pub(crate) fn is_expr_str(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(LiteralValue::String(_), _) | Expr::InterpolatedString { .. } => true,
            Expr::MemberAccess { member, .. } => {
                if let Some(t) = self.class_field_types.get(member) {
                    return t == "String" || t == "Str";
                }
                member == "name"
                    || member == "version"
                    || member == "title"
                    || member == "path"
                    || member == "str"
            }
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name) {
                    return ty == crate::types::DataraType::String;
                }
                if let Some(t) = self.class_field_types.get(name) {
                    return t == "String" || t == "Str";
                }
                name == "name"
                    || name == "version"
                    || name == "title"
                    || name == "path"
                    || name == "str"
                    || name == "msg"
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == "+" {
                    self.is_expr_str(left) || self.is_expr_str(right)
                } else {
                    false
                }
            }
            Expr::Call { callee, .. } => match &**callee {
                Expr::Identifier(fn_name, _) => {
                    let ret = self.infer_fn_ret_ty(fn_name);
                    ret == "String" || ret == "Str"
                }
                Expr::MemberAccess { object, member, .. } => {
                    if (member == "unwrap" || member == "unwrap_or")
                        && let Expr::Identifier(var_name, _) = &**object
                        && let Some(DataraType::GenericInstance { name, args }) =
                            self.lookup_var_type(var_name)
                        && name == "Outcome"
                        && args.first() == Some(&DataraType::String)
                    {
                        return true;
                    }
                    let ret = self.infer_fn_ret_ty(member);
                    ret == "String" || ret == "Str"
                }
                _ => false,
            },
            _ => false,
        }
    }

    pub(crate) fn is_expr_float(&self, expr: &Expr) -> bool {
        if let Some(DataraType::Float) = self.infer_expr_datara_type(expr) {
            return true;
        }
        match expr {
            Expr::Literal(LiteralValue::Float(_), _) => true,
            Expr::MemberAccess { member, .. } => {
                if let Some(t) = self.class_field_types.get(member) {
                    return t == "Float" || t == "Float64" || t == "Float32";
                }
                member.contains("flt") || member.contains("float")
            }
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name) {
                    return ty == crate::types::DataraType::Float;
                }
                if let Some(t) = self.class_field_types.get(name) {
                    return t == "Float" || t == "Float64" || t == "Float32";
                }
                name.contains("flt") || name.contains("float")
            }
            Expr::Binary { left, right, .. } => {
                self.is_expr_float(left) || self.is_expr_float(right)
            }
            Expr::Unary { expr, .. } => self.is_expr_float(expr),
            Expr::Call { callee, .. } => match &**callee {
                Expr::Identifier(name, _) => name.contains("float") || name.contains("flt"),
                Expr::MemberAccess { object, member, .. } => {
                    if (member == "unwrap" || member == "unwrap_or")
                        && let Expr::Identifier(var_name, _) = &**object
                        && let Some(DataraType::GenericInstance { name, args }) =
                            self.lookup_var_type(var_name)
                        && name == "Outcome"
                        && args.first() == Some(&DataraType::Float)
                    {
                        return true;
                    }
                    member.contains("float") || member.contains("flt")
                }
                _ => false,
            },
            Expr::IndexAccess { object, .. } => {
                if let Some(DataraType::List(elem)) = self.infer_expr_datara_type(object) {
                    return *elem == DataraType::Float;
                }
                if let Expr::Identifier(name, ..) = &**object {
                    if let Some(ty) = self.lookup_var_type(name) {
                        if let crate::types::DataraType::List(elem) = ty {
                            return *elem == crate::types::DataraType::Float;
                        }
                    }
                    if let Some(t) = self.class_field_types.get(name) {
                        return t == "Float" || t == "Float64" || t == "Float32";
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub(crate) fn is_expr_bool(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(LiteralValue::Bool(_), _) => true,
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name)
                    && ty == crate::types::DataraType::Bool
                {
                    return true;
                }
                false
            }
            Expr::Binary { op, .. } => {
                matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                )
            }
            Expr::Unary { op, .. } => op == "!",
            _ => false,
        }
    }

    pub(crate) fn is_expr_list(&self, expr: &Expr) -> bool {
        if matches!(
            expr,
            Expr::ListLiteral(..) | Expr::ArrayRepeatLiteral { .. }
        ) {
            return true;
        }
        if let Expr::Identifier(name, _) = expr
            && let Some(ty) = self.lookup_var_type(name)
        {
            if matches!(ty, crate::types::DataraType::List(_)) {
                return true;
            }
            if let crate::types::DataraType::Class(cls) = ty
                && cls == "List"
            {
                return true;
            }
        }
        if let Expr::Call { callee, args, .. } = expr {
            if let Expr::MemberAccess { object, member, .. } = &**callee
                && (member == "map" || member == "filter")
                && self.is_expr_list(object)
            {
                return true;
            }
            if let Expr::Identifier(fn_name, _) = &**callee {
                if fn_name == "str_split" || fn_name == "split" || fn_name == "datara_rt_str_split"
                {
                    return true;
                }
                if (fn_name == "map" || fn_name == "filter")
                    && !args.is_empty()
                    && self.is_expr_list(&args[0])
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn is_expr_map(&self, expr: &Expr) -> bool {
        if matches!(expr, Expr::MapLiteral(..)) {
            return true;
        }
        if let Some(ty) = self.infer_expr_datara_type(expr) {
            match ty {
                crate::types::DataraType::Map(..) => return true,
                crate::types::DataraType::Class(ref cls) if cls == "Map" => return true,
                crate::types::DataraType::GenericInstance { ref name, .. } if name == "Map" => {
                    return true;
                }
                _ => return false,
            }
        }
        if let Expr::Identifier(name, _) = expr
            && let Some(ty) = self.lookup_var_type(name)
        {
            match ty {
                crate::types::DataraType::Map(..) => return true,
                crate::types::DataraType::Class(ref cls) if cls == "Map" => return true,
                crate::types::DataraType::GenericInstance { ref name, .. } if name == "Map" => {
                    return true;
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn is_callable_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Lambda { .. } => true,
            Expr::Identifier(name, _) => {
                if self.local_lambdas.contains_key(name) {
                    return true;
                }
                if let Some(ty) = self.lookup_var_type(name)
                    && matches!(ty, crate::types::DataraType::Function { .. })
                {
                    return true;
                }
                if self.resolver.functions.contains_key(name)
                    || self.function_return_types.contains_key(name)
                {
                    return true;
                }
                if let Some(global) = self.resolver.scopes.first()
                    && global.get(name).is_some()
                {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub(crate) fn lower_inline_closure_call(
        &mut self,
        callable: &Expr,
        arg_vals: &[ValueId],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let (params, body) = match callable {
            Expr::Lambda { params, body, .. } => (params.clone(), (**body).clone()),
            Expr::Identifier(fn_name, _) => {
                if let Some(pair) = self.local_lambdas.get(fn_name).cloned() {
                    pair
                } else {
                    let dest = self.next_val();
                    let ret_ty = self
                        .function_return_types
                        .get(fn_name)
                        .cloned()
                        .unwrap_or_else(|| "Int".into());
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: fn_name.clone(),
                            args: arg_vals.to_vec(),
                            ty: ret_ty,
                        });
                    return Some(dest);
                }
            }
            _ => {
                return None;
            }
        };

        let mut shadowed_symbols: Vec<(String, Option<ValueId>)> = Vec::new();
        for (param, &aval) in params.iter().zip(arg_vals) {
            let old_val = self.symbol_values.get(&param.name).copied();
            shadowed_symbols.push((param.name.clone(), old_val));
            self.symbol_values.insert(param.name.clone(), aval);
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::AssignVar {
                    name: param.name.clone(),
                    value: aval,
                });
        }

        let res = self.lower_expr(&body, cur_block);

        for (name, old_val) in shadowed_symbols {
            if let Some(ov) = old_val {
                self.symbol_values.insert(name.clone(), ov);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar { name, value: ov });
            } else {
                self.symbol_values.remove(&name);
            }
        }

        res
    }
}
