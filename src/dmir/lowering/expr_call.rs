use crate::ast::*;
use crate::dmir::ir::*;
use crate::types::DataraType;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub(crate) fn lower_expr_call(
        &mut self,
        _expr: &Expr,
        callee: &Expr,
        args: &[Expr],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        if let Expr::Lambda { params, body, .. } = callee {
            let mut arg_vals = Vec::new();
            for a in args {
                if let Some(av) = self.lower_expr(a, cur_block) {
                    arg_vals.push(av);
                }
            }
            for (p, aval) in params.iter().zip(arg_vals) {
                self.symbol_values.insert(p.name.clone(), aval);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: p.name.clone(),
                        value: aval,
                    });
            }
            return self.lower_expr(body, cur_block);
        }
        if let Expr::Identifier(fn_name, _) = callee {
            if let Some((params, body)) = self.local_lambdas.get(fn_name).cloned() {
                let mut arg_vals = Vec::new();
                for a in args {
                    if let Some(av) = self.lower_expr(a, cur_block) {
                        arg_vals.push(av);
                    }
                }
                for (p, aval) in params.iter().zip(arg_vals) {
                    self.symbol_values.insert(p.name.clone(), aval);
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::AssignVar {
                            name: p.name.clone(),
                            value: aval,
                        });
                }
                return self.lower_expr(&body, cur_block);
            }
            if (fn_name == "view" || fn_name == "borrow" || fn_name == "clone" || fn_name == "move")
                && args.len() == 1
            {
                return self.lower_expr(&args[0], cur_block);
            }
            if (fn_name == "destroy" || fn_name == "drop") && args.len() == 1 {
                let arg_val = self.lower_expr(&args[0], cur_block);
                let dest = self.next_val();
                let mut call_args = Vec::new();
                if let Some(v) = arg_val {
                    call_args.push(v);
                }
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: fn_name.clone(),
                        args: call_args,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "println" || fn_name == "print" {
                if args.is_empty() {
                    let dest = self.next_val();
                    if fn_name == "println" {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_print_newline".into(),
                                args: vec![],
                                ty: "Unit".into(),
                            });
                    } else {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt { dest, value: 0 });
                    }
                    return Some(dest);
                }

                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        let sp_dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest: sp_dest,
                                func: "datara_rt_print_space".into(),
                                args: vec![],
                                ty: "Unit".into(),
                            });
                    }

                    let arg_val = self.lower_expr(arg, cur_block)?;
                    let print_func = if self.is_expr_str(arg) {
                        "datara_rt_print_str"
                    } else if self.is_expr_float(arg) {
                        "datara_rt_print_float"
                    } else if self.is_expr_bool(arg) {
                        "datara_rt_print_bool"
                    } else if self.is_expr_list(arg) {
                        "datara_rt_print_list"
                    } else {
                        "datara_rt_print_int"
                    };

                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: print_func.into(),
                            args: vec![arg_val],
                            ty: "Unit".into(),
                        });
                }

                let final_dest = self.next_val();
                if fn_name == "println" {
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest: final_dest,
                            func: "datara_rt_print_newline".into(),
                            args: vec![],
                            ty: "Unit".into(),
                        });
                } else {
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest: final_dest,
                            func: "datara_rt_flush".into(),
                            args: vec![],
                            ty: "Unit".into(),
                        });
                }

                return Some(final_dest);
            }
            if fn_name == "eprintln" && args.len() == 1 {
                let arg_val = self.lower_expr(&args[0], cur_block)?;
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Err { value: arg_val });
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt { dest, value: 0 });
                return Some(dest);
            }
            if fn_name == "len" && args.len() == 1 {
                let arg_val = self.lower_expr(&args[0], cur_block)?;
                let dest = self.next_val();
                let func = if self.is_expr_str(&args[0]) {
                    "datara_rt_len"
                } else {
                    "datara_rt_list_len"
                };
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: func.into(),
                        args: vec![arg_val],
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "now" && args.is_empty() {
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_now_ms".into(),
                        args: vec![],
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "panic" && args.len() == 1 {
                let arg_val = self.lower_expr(&args[0], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_panic".into(),
                        args: vec![arg_val],
                        ty: "Never".into(),
                    });
                return Some(dest);
            }
            if (fn_name == "wrapping" || fn_name == "wrapping_mode") && args.len() == 1 {
                let prev = self.in_wrapping_mode;
                self.in_wrapping_mode = true;
                let res = self.lower_expr(&args[0], cur_block);
                self.in_wrapping_mode = prev;
                return res;
            }
            if (fn_name == "saturating" || fn_name == "saturating_mode") && args.len() == 1 {
                let prev = self.in_saturating_mode;
                self.in_saturating_mode = true;
                let res = self.lower_expr(&args[0], cur_block);
                self.in_saturating_mode = prev;
                return res;
            }
            if (fn_name == "map"
                || fn_name == "filter"
                || fn_name == "find"
                || fn_name == "any"
                || fn_name == "all")
                && args.len() == 2
                && self.is_expr_list(&args[0])
                && self.is_callable_expr(&args[1])
            {
                let list_val = self.lower_expr(&args[0], cur_block)?;
                return self.lower_list_higher_order(list_val, fn_name, &args[1..], cur_block);
            }
            if fn_name == "reduce"
                && args.len() == 3
                && self.is_expr_list(&args[0])
                && self.is_callable_expr(&args[2])
            {
                let list_val = self.lower_expr(&args[0], cur_block)?;
                return self.lower_list_higher_order(list_val, fn_name, &args[1..], cur_block);
            }
            if fn_name == "wrapping_add" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "wrapping_+".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "wrapping_sub" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "wrapping_-".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "wrapping_mul" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "wrapping_*".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "saturating_add" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "saturating_+".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "saturating_sub" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "saturating_-".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "saturating_mul" && args.len() == 2 {
                let l = self.lower_expr(&args[0], cur_block)?;
                let r = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: "saturating_*".into(),
                        left: l,
                        right: r,
                        ty: "Int".into(),
                    });
                return Some(dest);
            }
            if fn_name == "assert" && !args.is_empty() {
                let cond_val = self.lower_expr(&args[0], cur_block)?;
                let msg_val = if args.len() >= 2 {
                    self.lower_expr(&args[1], cur_block)?
                } else {
                    let m = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstStr {
                            dest: m,
                            value: "Assertion failed".into(),
                        });
                    m
                };
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_assert".into(),
                        args: vec![cond_val, msg_val],
                        ty: "Unit".into(),
                    });
                return Some(dest);
            }
            if fn_name == "exit" && args.len() == 1 {
                let arg_val = self.lower_expr(&args[0], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_exit".into(),
                        args: vec![arg_val],
                        ty: "Never".into(),
                    });
                return Some(dest);
            }
            if (fn_name == "input"
                || fn_name == "read_line"
                || fn_name == "input_int"
                || fn_name == "input_float")
                && args.len() <= 1
            {
                let prompt_val = if !args.is_empty() {
                    self.lower_expr(&args[0], cur_block)?
                } else {
                    let empty = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstStr {
                            dest: empty,
                            value: "".into(),
                        });
                    empty
                };
                let (target_func, ret_ty) = if fn_name == "input_int" {
                    ("datara_rt_input_int", "Int")
                } else if fn_name == "input_float" {
                    ("datara_rt_input_float", "Float")
                } else {
                    ("datara_rt_input", "String")
                };
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: target_func.into(),
                        args: vec![prompt_val],
                        ty: ret_ty.into(),
                    });
                return Some(dest);
            }
            if (fn_name == "str_to_float" || fn_name == "datara_rt_str_to_float") && args.len() == 1
            {
                let arg_val = self.lower_expr(&args[0], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_str_to_float".into(),
                        args: vec![arg_val],
                        ty: "Float".into(),
                    });
                return Some(dest);
            }

            if let Some(&tag) = self.enum_variant_tags.get(fn_name) {
                let tag_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: tag_val,
                        value: tag,
                    });
                let mut fields = vec![("__tag".to_string(), tag_val)];
                for (idx, a) in args.iter().enumerate() {
                    if let Some(av) = self.lower_expr(a, cur_block) {
                        fields.push((format!("f{}", idx), av));
                    }
                }
                let slots = self.enum_slots.get(fn_name).cloned().unwrap_or_default();
                for (idx, s_ty) in slots.iter().enumerate().skip(args.len()) {
                    let pad_dest = self.next_val();
                    if s_ty.contains("Float") {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstFloat {
                                dest: pad_dest,
                                value: 0.0,
                            });
                    } else {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: pad_dest,
                                value: 0,
                            });
                    }
                    fields.push((format!("f{}", idx), pad_dest));
                }
                let dest = self.next_val();
                let class_name = self
                    .enum_variant_names
                    .get(&tag)
                    .cloned()
                    .unwrap_or_else(|| fn_name.clone());
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::StructInit {
                        dest,
                        class_name,
                        fields,
                    });
                return Some(dest);
            }
        }

        if let Expr::MemberAccess { object, member, .. } = callee {
            if let Expr::Identifier(class_name, _) = &**object {
                let enum_key = format!("{}.{}", class_name, member);
                if let Some(&tag) = self.enum_variant_tags.get(&enum_key) {
                    let tag_val = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: tag_val,
                            value: tag,
                        });
                    let mut fields = vec![("__tag".to_string(), tag_val)];
                    for (idx, a) in args.iter().enumerate() {
                        if let Some(av) = self.lower_expr(a, cur_block) {
                            fields.push((format!("f{}", idx), av));
                        }
                    }
                    let slots = self.enum_slots.get(&enum_key).cloned().unwrap_or_default();
                    for (idx, s_ty) in slots.iter().enumerate().skip(args.len()) {
                        let pad_dest = self.next_val();
                        if s_ty.contains("Float") {
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstFloat {
                                    dest: pad_dest,
                                    value: 0.0,
                                });
                        } else {
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstInt {
                                    dest: pad_dest,
                                    value: 0,
                                });
                        }
                        fields.push((format!("f{}", idx), pad_dest));
                    }
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::StructInit {
                            dest,
                            class_name: format!("{}_{}", class_name, member),
                            fields,
                        });
                    return Some(dest);
                }

                let static_func_name = format!("{}_{}", class_name, member);
                if self.function_return_types.contains_key(&static_func_name) {
                    let dummy_this = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: dummy_this,
                            value: 0,
                        });
                    let mut call_args = vec![dummy_this];
                    for a in args {
                        if let Some(av) = self.lower_expr(a, cur_block) {
                            call_args.push(av);
                        }
                    }
                    let dest = self.next_val();
                    let ret_ty = self
                        .function_return_types
                        .get(&static_func_name)
                        .cloned()
                        .unwrap_or_else(|| "Unit".into());
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: static_func_name,
                            args: call_args,
                            ty: ret_ty,
                        });
                    return Some(dest);
                }
            }

            if member == "view" && args.is_empty() {
                return self.lower_expr(object, cur_block);
            }
            if (member == "grant_readonly" || member == "grant_readwrite" || member == "open")
                && args.len() == 1
            {
                return self.lower_expr(&args[0], cur_block);
            }
            if member == "read_all" {
                let dest = self.next_val();
                let arg = if args.is_empty() {
                    self.lower_expr(object, cur_block)?
                } else {
                    self.lower_expr(&args[0], cur_block)?
                };
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_file_read".into(),
                        args: vec![arg],
                        ty: "String".into(),
                    });
                return Some(dest);
            }
            let obj_val = self.lower_expr(object, cur_block)?;
            if self.is_expr_list(object) {
                if (member == "map"
                    || member == "filter"
                    || member == "find"
                    || member == "any"
                    || member == "all")
                    && args.len() == 1
                    && self.is_callable_expr(&args[0])
                {
                    return self.lower_list_higher_order(obj_val, member, args, cur_block);
                }
                if member == "reduce" && args.len() == 2 && self.is_callable_expr(&args[1]) {
                    return self.lower_list_higher_order(obj_val, member, args, cur_block);
                }
                if member == "pop" && args.is_empty() {
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_list_pop".into(),
                            args: vec![obj_val],
                            ty: "Int".into(),
                        });
                    return Some(dest);
                }
            }
            if member == "insert" && args.len() == 2 && self.is_expr_map(object) {
                let k = self.lower_expr(&args[0], cur_block)?;
                let v = self.lower_expr(&args[1], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_map_insert".into(),
                        args: vec![obj_val, k, v],
                        ty: "Map".into(),
                    });
                return Some(dest);
            }
            if ((member == "contains" && self.is_expr_map(object))
                || member == "has_key"
                || member == "contains_key")
                && args.len() == 1
            {
                let k = self.lower_expr(&args[0], cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_map_contains".into(),
                        args: vec![obj_val, k],
                        ty: "Bool".into(),
                    });
                return Some(dest);
            }
            let mut arg_vals = Vec::new();
            for a in args {
                if let Some(av) = self.lower_expr(a, cur_block) {
                    arg_vals.push(av);
                }
            }
            let mut method_ty = self
                .function_return_types
                .get(member)
                .or_else(|| self.class_field_types.get(member))
                .cloned()
                .unwrap_or_else(|| {
                    if member.contains("float") || member.contains("flt") {
                        "Float".into()
                    } else if member.contains("string")
                        || member.contains("to_str")
                        || member.starts_with("str_")
                        || member.ends_with("_str")
                        || member.contains("render")
                        || member.contains("format")
                        || member.contains("quote")
                        || member.starts_with("wrap")
                    {
                        "String".into()
                    } else {
                        "Int".into()
                    }
                });
            if member == "unwrap" || member == "unwrap_or" {
                let obj_ty = match &**object {
                    Expr::Identifier(name, _) => self.lookup_var_type(name),
                    _ => None,
                };
                if let Some(DataraType::GenericInstance { name, args }) = &obj_ty {
                    if name == "Outcome" && !args.is_empty() {
                        method_ty = match &args[0] {
                            DataraType::Float => "Float".into(),
                            DataraType::String => "String".into(),
                            DataraType::Bool => "Bool".into(),
                            DataraType::Int => "Int".into(),
                            DataraType::Class(c) => c.clone(),
                            _ => "Int".into(),
                        };
                    }
                } else if let Some(DataraType::Result(ok, _)) = &obj_ty {
                    method_ty = match &**ok {
                        DataraType::Float => "Float".into(),
                        DataraType::String => "String".into(),
                        DataraType::Bool => "Bool".into(),
                        DataraType::Int => "Int".into(),
                        DataraType::Class(c) => c.clone(),
                        _ => "Int".into(),
                    };
                }
            } else if member == "is_ok" || member == "is_err" {
                method_ty = "Bool".into();
            } else if member == "await" {
                let obj_ty = match &**object {
                    Expr::Identifier(name, _) => self.lookup_var_type(name),
                    _ => None,
                };
                if let Some(DataraType::GenericInstance { name, args }) = &obj_ty {
                    if (name == "Future" || name == "Task") && !args.is_empty() {
                        method_ty = match &args[0] {
                            DataraType::Float => "Float".into(),
                            DataraType::String => "String".into(),
                            DataraType::Bool => "Bool".into(),
                            DataraType::Int => "Int".into(),
                            DataraType::Class(c) => c.clone(),
                            _ => "Int".into(),
                        };
                    }
                } else if let Some(DataraType::Class(c)) = &obj_ty
                    && (c == "Future" || c == "Task")
                {
                    method_ty = "String".into();
                }
            }
            let dest = self.next_val();
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::MethodCall {
                    dest,
                    object: obj_val,
                    method: member.clone(),
                    args: arg_vals,
                    ty: method_ty,
                });
            return Some(dest);
        }

        let mut arg_vals = Vec::new();
        for a in args {
            if let Some(av) = self.lower_expr(a, cur_block) {
                arg_vals.push(av);
            }
        }
        let mut func_name = if let Expr::Identifier(fn_name, _) = callee {
            fn_name.clone()
        } else {
            "func".into()
        };

        if let Some(specs) = self.types.generic_specializations.get(&func_name) {
            let mut candidate_mangled = None;
            if let Some(first_arg) = args.first() {
                let arg_ty_opt = match first_arg {
                    Expr::Identifier(var_name, _) => self.lookup_var_type(var_name),
                    Expr::ObjectInit { class_name, .. } => {
                        Some(DataraType::Class(class_name.clone()))
                    }
                    _ => None,
                };
                if let Some(arg_ty) = arg_ty_opt {
                    let type_str = match &arg_ty {
                        DataraType::Class(c) => c.clone(),
                        DataraType::Int => "Int".to_string(),
                        DataraType::Float => "Float".to_string(),
                        DataraType::String => "String".to_string(),
                        DataraType::Bool => "Bool".to_string(),
                        other => other.to_string(),
                    };
                    let candidate = format!("{}_{}", func_name, type_str);
                    if self.function_return_types.contains_key(&candidate) {
                        candidate_mangled = Some(candidate);
                    }
                }
            }
            if candidate_mangled.is_none()
                && specs.len() == 1
                && let Some(first_spec) = specs.iter().next()
            {
                let s_names: Vec<String> = first_spec
                    .iter()
                    .map(|t| match t {
                        DataraType::Class(c) => c.clone(),
                        DataraType::Int => "Int".to_string(),
                        DataraType::Float => "Float".to_string(),
                        DataraType::String => "String".to_string(),
                        DataraType::Bool => "Bool".to_string(),
                        other => other.to_string(),
                    })
                    .collect();
                let candidate = format!("{}_{}", func_name, s_names.join("_"));
                if self.function_return_types.contains_key(&candidate) {
                    candidate_mangled = Some(candidate);
                }
            }
            if let Some(mangled) = candidate_mangled {
                func_name = mangled;
            }
        }

        if func_name == "require" {
            let zero = self.next_val();
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::ConstInt {
                    dest: zero,
                    value: 0,
                });
            return Some(zero);
        }

        let dest = self.next_val();
        let ret_ty = self.infer_fn_ret_ty(&func_name);
        self.get_block_mut(*cur_block)
            .instructions
            .push(Inst::Call {
                dest,
                func: func_name,
                args: arg_vals,
                ty: ret_ty,
            });
        Some(dest)
    }
}
