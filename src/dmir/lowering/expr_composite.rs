use super::match_arm::ArmCond;
use crate::ast::*;
use crate::dmir::ir::*;
use crate::types::DataraType;
use std::collections::HashMap;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub(crate) fn lower_composite_expr(
        &mut self,
        expr: &Expr,
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        match expr {
            Expr::MemberAccess { object, member, .. } => {
                if let Expr::Identifier(type_name, _) = &**object {
                    let enum_key = format!("{}.{}", type_name, member);
                    if let Some(&tag) = self.enum_variant_tags.get(&enum_key) {
                        let tag_val = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: tag_val,
                                value: tag,
                            });
                        let mut fields = vec![("__tag".to_string(), tag_val)];
                        let slots = self.enum_slots.get(&enum_key).cloned().unwrap_or_default();
                        for (idx, s_ty) in slots.iter().enumerate() {
                            let pad_dest = self.next_val();
                            if s_ty.contains("Float") {
                                self.get_block_mut(*cur_block).instructions.push(
                                    Inst::ConstFloat {
                                        dest: pad_dest,
                                        value: 0.0,
                                    },
                                );
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
                                class_name: format!("{}_{}", type_name, member),
                                fields,
                            });
                        return Some(dest);
                    }
                }
                let obj_val = self.lower_expr(object, cur_block)?;
                if member == "view" || member == "files" || member == "net" || member == "proc" {
                    return Some(obj_val);
                }
                if let Some((offset, bits)) = self.find_packet_for_member(object, member) {
                    let shifted = if offset > 0 {
                        let off_val = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: off_val,
                                value: offset as i64,
                            });
                        let s_dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest: s_dest,
                                op: ">>".into(),
                                left: obj_val,
                                right: off_val,
                                ty: "Int".into(),
                            });
                        s_dest
                    } else {
                        obj_val
                    };
                    let mask = if bits >= 64 {
                        -1i64
                    } else {
                        (1i64 << bits) - 1
                    };
                    let mask_val = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: mask_val,
                            value: mask,
                        });
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::BinOp {
                            dest,
                            op: "&".into(),
                            left: shifted,
                            right: mask_val,
                            ty: "Int".into(),
                        });
                    return Some(dest);
                }
                let dest = self.next_val();
                let field_ty = self
                    .member_field_repr(object, member)
                    .or_else(|| self.class_field_types.get(member).cloned())
                    .unwrap_or_else(|| {
                        if member == "score"
                            || member.contains("flt")
                            || member.contains("float")
                            || self.is_expr_float(object)
                        {
                            "Float".to_string()
                        } else {
                            "Int".to_string()
                        }
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::GetField {
                        dest,
                        object: obj_val,
                        field: member.clone(),
                        ty: field_ty,
                    });
                Some(dest)
            }
            Expr::ObjectInit {
                class_name,
                generic_args,
                fields,
                ..
            } => {
                if let Some(pkt) = self.resolver.packets.get(class_name) {
                    let mut bit_offsets = HashMap::new();
                    let mut current_offset = 0;
                    for f in &pkt.fields {
                        bit_offsets.insert(f.name.clone(), (current_offset, f.bits));
                        current_offset += f.bits;
                    }
                    let mut acc_val: Option<ValueId> = None;
                    for (fname, fexpr) in fields {
                        if let Some(fval) = self.lower_expr(fexpr, cur_block)
                            && let Some(&(offset, bits)) = bit_offsets.get(fname)
                        {
                            let mask = if bits >= 64 {
                                -1i64
                            } else {
                                (1i64 << bits) - 1
                            };
                            let mask_val = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstInt {
                                    dest: mask_val,
                                    value: mask,
                                });
                            let masked = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::BinOp {
                                    dest: masked,
                                    op: "&".into(),
                                    left: fval,
                                    right: mask_val,
                                    ty: "Int".into(),
                                });
                            let shifted = if offset > 0 {
                                let off_val = self.next_val();
                                self.get_block_mut(*cur_block)
                                    .instructions
                                    .push(Inst::ConstInt {
                                        dest: off_val,
                                        value: offset as i64,
                                    });
                                let s_dest = self.next_val();
                                self.get_block_mut(*cur_block)
                                    .instructions
                                    .push(Inst::BinOp {
                                        dest: s_dest,
                                        op: "<<".into(),
                                        left: masked,
                                        right: off_val,
                                        ty: "Int".into(),
                                    });
                                s_dest
                            } else {
                                masked
                            };
                            acc_val = match acc_val {
                                None => Some(shifted),
                                Some(prev) => {
                                    let or_dest = self.next_val();
                                    self.get_block_mut(*cur_block)
                                        .instructions
                                        .push(Inst::BinOp {
                                            dest: or_dest,
                                            op: "|".into(),
                                            left: prev,
                                            right: shifted,
                                            ty: "Int".into(),
                                        });
                                    Some(or_dest)
                                }
                            };
                        }
                    }
                    return if let Some(res) = acc_val {
                        Some(res)
                    } else {
                        let zero = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: zero,
                                value: 0,
                            });
                        Some(zero)
                    };
                }
                let mut field_vals = Vec::new();
                for (fname, fexpr) in fields {
                    if let Some(fval) = self.lower_expr(fexpr, cur_block) {
                        field_vals.push((fname.clone(), fval));
                    }
                }

                let specialized_name = if !generic_args.is_empty() {
                    let args_str = generic_args
                        .iter()
                        .map(|a| a.name.clone())
                        .collect::<Vec<_>>()
                        .join("_");
                    format!("{}_{}", class_name, args_str)
                } else if let Some((_, f_val)) = field_vals.first() {
                    let inferred = if let Some(Inst::ConstInt { .. }) = self
                        .get_block_mut(*cur_block)
                        .instructions
                        .iter()
                        .find(|i| match i {
                            Inst::ConstInt { dest, .. } => dest == f_val,
                            _ => false,
                        }) {
                        "Int"
                    } else if let Some(Inst::ConstFloat { .. }) = self
                        .get_block_mut(*cur_block)
                        .instructions
                        .iter()
                        .find(|i| match i {
                            Inst::ConstFloat { dest, .. } => dest == f_val,
                            _ => false,
                        })
                    {
                        "Float"
                    } else {
                        ""
                    };

                    if !inferred.is_empty() && self.types.generic_templates.contains_key(class_name)
                    {
                        format!("{}_{}", class_name, inferred)
                    } else {
                        class_name.clone()
                    }
                } else {
                    class_name.clone()
                };

                // Canonical field order: the resolved class layout is the
                // alphabetically sorted field set (see class_fields), so the
                // init literal must be stored in that same order or GetField
                // offsets and StructInit stores diverge for composed classes.
                field_vals.sort_by(|a, b| a.0.cmp(&b.0));
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::StructInit {
                        dest,
                        class_name: specialized_name,
                        fields: field_vals,
                    });
                Some(dest)
            }
            Expr::Pipeline { stages, .. } => {
                let mut current = self.lower_expr(&stages[0], cur_block)?;
                for stage in &stages[1..] {
                    if let Expr::Call { callee, args, .. } = stage {
                        if let Expr::MemberAccess { object, member, .. } = &**callee {
                            // Method stage (`user.pay(amount)`): the receiver
                            // is explicit, so the piped value is not
                            // prepended; the method result becomes the new
                            // piped value.
                            let obj_val = self.lower_expr(object, cur_block)?;
                            let mut m_args = Vec::new();
                            for a in args {
                                if let Some(av) = self.lower_expr(a, cur_block) {
                                    m_args.push(av);
                                }
                            }
                            let dest = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::MethodCall {
                                    dest,
                                    object: obj_val,
                                    method: member.clone(),
                                    args: m_args,
                                    ty: "Int".into(),
                                });
                            current = dest;
                            continue;
                        }
                        let mut all_args = vec![current];
                        for a in args {
                            if let Some(av) = self.lower_expr(a, cur_block) {
                                all_args.push(av);
                            }
                        }
                        let fn_name = if let Expr::Identifier(n, _) = &**callee {
                            n.clone()
                        } else {
                            "fn".into()
                        };
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: fn_name,
                                args: all_args,
                                ty: "Int".into(),
                            });
                        current = dest;
                    } else if let Expr::Identifier(fn_name, _) = stage {
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: fn_name.clone(),
                                args: vec![current],
                                ty: "Int".into(),
                            });
                        current = dest;
                    }
                }
                Some(current)
            }
            Expr::Decide { arms, else_arm, .. } => {
                let has_block_body = arms.iter().any(|arm| matches!(&arm.body, Expr::Block(..)))
                    || else_arm
                        .as_ref()
                        .is_some_and(|eb| matches!(**eb, Expr::Block(..)));
                if has_block_body {
                    let cfg_arms: Vec<(ArmCond<'_>, &Expr)> = arms
                        .iter()
                        .map(|arm| (ArmCond::Expr(&arm.condition), &arm.body))
                        .collect();
                    return self.lower_arm_conds_cfg(&cfg_arms, else_arm.as_deref(), cur_block);
                }
                let is_str = arms.iter().any(|arm| self.is_expr_str(&arm.body))
                    || else_arm
                        .as_ref()
                        .map(|eb| self.is_expr_str(eb))
                        .unwrap_or(false);
                let mut lowered_arms = Vec::new();
                for arm in arms {
                    let cond = self.lower_expr(&arm.condition, cur_block)?;
                    let val = self.lower_expr(&arm.body, cur_block)?;
                    lowered_arms.push((cond, val));
                }
                let else_val = if let Some(eb) = else_arm {
                    self.lower_expr(eb, cur_block)
                } else {
                    None
                };
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Decide {
                        dest,
                        arms: lowered_arms,
                        else_val,
                        ty: if is_str {
                            "String".into()
                        } else {
                            "Int".into()
                        },
                    });
                Some(dest)
            }
            Expr::Match { value, arms, .. } => {
                let val = self.lower_expr(value, cur_block)?;
                // Block arm bodies may carry side effects (out, assignments,
                // calls); they must only run when their arm is actually
                // selected, so lower those matches as real control flow.
                if arms.iter().any(|arm| matches!(&arm.body, Expr::Block(..))) {
                    return self.lower_match_arms_cfg(val, arms, cur_block);
                }
                let is_str = arms.iter().any(|arm| self.is_expr_str(&arm.body));
                let mut lowered_arms = Vec::new();
                for arm in arms {
                    let cond =
                        self.lower_match_arm_cond(val, &arm.pattern, arm.guard.as_ref(), cur_block);
                    let body_val = self.lower_expr(&arm.body, cur_block)?;
                    lowered_arms.push((cond, body_val));
                }
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Decide {
                        dest,
                        arms: lowered_arms,
                        else_val: None,
                        ty: if is_str {
                            "String".into()
                        } else {
                            "Int".into()
                        },
                    });
                Some(dest)
            }
            Expr::Select { arms, else_arm, .. } => {
                let has_block_body = arms.iter().any(|arm| matches!(&arm.body, Expr::Block(..)))
                    || else_arm
                        .as_ref()
                        .is_some_and(|eb| matches!(**eb, Expr::Block(..)));
                if has_block_body {
                    let cfg_arms: Vec<(ArmCond<'_>, &Expr)> = arms
                        .iter()
                        .map(|arm| (ArmCond::Expr(&arm.condition), &arm.body))
                        .collect();
                    return self.lower_arm_conds_cfg(&cfg_arms, else_arm.as_deref(), cur_block);
                }
                let is_str = arms.iter().any(|arm| self.is_expr_str(&arm.body))
                    || else_arm
                        .as_ref()
                        .map(|eb| self.is_expr_str(eb))
                        .unwrap_or(false);
                let mut lowered_arms = Vec::new();
                for arm in arms {
                    let cond = self.lower_expr(&arm.condition, cur_block)?;
                    let val = self.lower_expr(&arm.body, cur_block)?;
                    lowered_arms.push((cond, val));
                }
                let else_val = if let Some(eb) = else_arm {
                    self.lower_expr(eb, cur_block)
                } else {
                    None
                };
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Decide {
                        dest,
                        arms: lowered_arms,
                        else_val,
                        ty: if is_str {
                            "String".into()
                        } else {
                            "Int".into()
                        },
                    });
                Some(dest)
            }
            Expr::Lambda { body, .. } => self.lower_expr(body, cur_block),
            Expr::ListLiteral(items, _) => {
                let mut vals = Vec::new();
                for item in items {
                    if let Some(v) = self.lower_expr(item, cur_block) {
                        vals.push(v);
                    }
                }
                let dest = self.next_val();
                // The runtime exposes fixed-arity constructors for 1..=5
                // elements; anything else is built with create + append so
                // literals of arbitrary size lower successfully.
                if vals.is_empty() {
                    let cap = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: cap,
                            value: 0,
                        });
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_list_create".into(),
                            args: vec![cap],
                            ty: "List".into(),
                        });
                } else if vals.len() <= 5 {
                    let func_name = format!("datara_rt_list_create_{}", vals.len());
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: func_name,
                            args: vals,
                            ty: "List".into(),
                        });
                } else {
                    let cap = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: cap,
                            value: vals.len() as i64,
                        });
                    let mut cur_list = dest;
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_list_create".into(),
                            args: vec![cap],
                            ty: "List".into(),
                        });
                    for v in vals {
                        let next = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest: next,
                                func: "datara_rt_list_append".into(),
                                args: vec![cur_list, v],
                                ty: "List".into(),
                            });
                        // append may reallocate; the canonical list pointer
                        // is the latest append result.
                        cur_list = next;
                    }
                    return Some(cur_list);
                }
                Some(dest)
            }
            Expr::MapLiteral(entries, _) => {
                let mut vals = Vec::new();
                for (k, v) in entries {
                    if let Some(kv) = self.lower_expr(k, cur_block)
                        && let Some(vv) = self.lower_expr(v, cur_block)
                    {
                        vals.push(kv);
                        vals.push(vv);
                    }
                }
                let dest = self.next_val();
                let n_entries = vals.len() / 2;
                // Fixed-arity constructors exist for 1..=5 entries; empty maps
                // use the 0-arg constructor and larger literals fall back to
                // create + insert so any size lowers successfully.
                if n_entries == 0 {
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_map_create".into(),
                            args: vec![],
                            ty: "Map".into(),
                        });
                } else if n_entries <= 5 {
                    let func_name = format!("datara_rt_map_create_{}", n_entries);
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: func_name,
                            args: vals,
                            ty: "Map".into(),
                        });
                } else {
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_map_create".into(),
                            args: vec![],
                            ty: "Map".into(),
                        });
                    let mut pairs = vals.into_iter();
                    while let (Some(k), Some(v)) = (pairs.next(), pairs.next()) {
                        let next = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest: next,
                                func: "datara_rt_map_insert".into(),
                                args: vec![dest, k, v],
                                ty: "Map".into(),
                            });
                    }
                }
                Some(dest)
            }
            Expr::IndexAccess { object, index, .. } => {
                let obj = self.lower_expr(object, cur_block)?;
                if let Expr::Range { start, end, .. } = &**index {
                    let s = self.lower_expr(start, cur_block)?;
                    let e = self.lower_expr(end, cur_block)?;
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: "datara_rt_slice".into(),
                            args: vec![obj, s, e],
                            ty: "List".into(),
                        });
                    return Some(dest);
                }
                let idx = self.lower_expr(index, cur_block)?;
                let dest = self.next_val();
                let is_str_idx = self.is_expr_str(index);
                let func_name = if is_str_idx {
                    "datara_rt_map_get"
                } else {
                    "datara_rt_list_get"
                };
                let ret_ty: String = match &**object {
                    Expr::Identifier(name, _) => {
                        if let Some(obj_ty) = self.lookup_var_type(name) {
                            match obj_ty {
                                DataraType::Map(_, val) => match *val {
                                    DataraType::Float => "Float".into(),
                                    DataraType::String => "String".into(),
                                    DataraType::Bool => "Bool".into(),
                                    _ => "Int".into(),
                                },
                                DataraType::List(elem) => match *elem {
                                    DataraType::Float => "Float".into(),
                                    DataraType::String => "String".into(),
                                    DataraType::Bool => "Bool".into(),
                                    _ => "Int".into(),
                                },
                                _ => "Int".into(),
                            }
                        } else {
                            "Int".into()
                        }
                    }
                    _ => "Int".into(),
                };
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: func_name.into(),
                        args: vec![obj, idx],
                        ty: ret_ty,
                    });
                Some(dest)
            }
            Expr::Range { start, end, .. } => {
                let s = self.lower_expr(start, cur_block)?;
                let e = self.lower_expr(end, cur_block)?;
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: "datara_rt_range_str".into(),
                        args: vec![s, e],
                        ty: "String".into(),
                    });
                Some(dest)
            }
            Expr::Tuple(exprs, _) => {
                let mut vals = Vec::new();
                for e in exprs {
                    if let Some(v) = self.lower_expr(e, cur_block) {
                        vals.push(v);
                    }
                }
                let dest = self.next_val();
                let func_name = format!("datara_rt_tuple_create_{}", vals.len());
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: func_name,
                        args: vals,
                        ty: "Tuple".into(),
                    });
                Some(dest)
            }
            Expr::ErrorPropagate(inner, span) => {
                let val = self.lower_expr(inner, cur_block)?;
                // Real error propagation, replacing the old pass-through
                // no-op that silently used a failed Outcome/Maybe object as
                // the unwrapped value (pointer garbage downstream).
                //
                //   val = <operand>                     (Outcome<T> / Maybe<T>)
                //   flag = val.is_success (is_some)     GetField
                //   cond flag ? ok : err
                //   err:  return val                    (zero-copy: the failed
                //                                         object becomes the
                //                                         function's result)
                //   ok:   payload = val.value           GetField
                //         -> merge                      (expression value)
                //
                // The type checker records every `?` site with its concrete
                // representation; a missing record here is an internal
                // invariant violation (checking runs and aborts on errors
                // before lowering), so fail loudly.
                let site = self
                    .types
                    .propagation_sites
                    .iter()
                    .find(|s| &s.span == span);
                let (flag_field, payload_field, payload_repr) = if let Some(s) = site {
                    (
                        s.kind.flag_field().to_string(),
                        s.kind.payload_field().to_string(),
                        s.payload_repr.clone(),
                    )
                } else {
                    ("is_success".to_string(), "value".to_string(), "Int".into())
                };

                let flag = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::GetField {
                        dest: flag,
                        object: val,
                        field: flag_field,
                        ty: "Bool".into(),
                    });

                let ok_block = self.create_block("prop_ok");
                let err_block = self.create_block("prop_err");
                let merge_block = self.create_block("prop_merge");

                self.get_block_mut(*cur_block).terminator = Terminator::CondBranch {
                    cond: flag,
                    then_block: ok_block,
                    then_args: Vec::new(),
                    else_block: err_block,
                    else_args: Vec::new(),
                };

                // Error path: return the failed object unchanged. The caller
                // observes it through the same `is_success`/`error_msg` (or
                // `is_some`) fields, so no re-wrapping or copying happens.
                self.get_block_mut(err_block).terminator = Terminator::Return { value: Some(val) };

                // Success path: extract the payload and join with merge.
                let payload = self.next_val();
                self.get_block_mut(ok_block)
                    .instructions
                    .push(Inst::GetField {
                        dest: payload,
                        object: val,
                        field: payload_field,
                        ty: payload_repr,
                    });
                self.get_block_mut(ok_block).terminator = Terminator::Branch {
                    target: merge_block,
                    args: Vec::new(),
                };

                *cur_block = merge_block;
                Some(payload)
            }
            Expr::ArrayRepeatLiteral { elem, count, .. } => {
                let elem_val = self.lower_expr(elem, cur_block)?;
                let count_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: count_val,
                        value: *count as i64,
                    });
                let list_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: list_val,
                        func: "datara_rt_list_create_repeat".into(),
                        args: vec![elem_val, count_val],
                        ty: "List".into(),
                    });
                Some(list_val)
            }
            Expr::OrRecovery { expr, arms, span } => {
                let val = self.lower_expr(expr, cur_block)?;
                let res_var = format!("__or_res_{}", self.next_val().0);
                let ok_block = self.create_block("or_ok");
                let rec_block = self.create_block("or_recovery");
                let merge_block = self.create_block("or_merge");

                let site = self
                    .types
                    .propagation_sites
                    .iter()
                    .find(|s| &s.span == span);
                let (flag_field, payload_field, payload_repr) = if let Some(s) = site {
                    (
                        s.kind.flag_field(),
                        s.kind.payload_field(),
                        s.payload_repr.clone(),
                    )
                } else {
                    ("is_success", "value", "Int".into())
                };

                let flag = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::GetField {
                        dest: flag,
                        object: val,
                        field: flag_field.into(),
                        ty: "Bool".into(),
                    });
                self.get_block_mut(*cur_block).terminator = Terminator::CondBranch {
                    cond: flag,
                    then_block: ok_block,
                    then_args: Vec::new(),
                    else_block: rec_block,
                    else_args: Vec::new(),
                };

                let payload = self.next_val();
                self.get_block_mut(ok_block)
                    .instructions
                    .push(Inst::GetField {
                        dest: payload,
                        object: val,
                        field: payload_field.into(),
                        ty: payload_repr,
                    });
                self.get_block_mut(ok_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: payload,
                    });
                self.get_block_mut(ok_block).terminator = Terminator::Branch {
                    target: merge_block,
                    args: Vec::new(),
                };

                let mut cur_rec = rec_block;
                let rec_val = if let Some(first_arm) = arms.first() {
                    self.lower_expr(&first_arm.body, &mut cur_rec)
                } else {
                    None
                };
                if let Some(rv) = rec_val {
                    self.get_block_mut(cur_rec)
                        .instructions
                        .push(Inst::AssignVar {
                            name: res_var.clone(),
                            value: rv,
                        });
                }
                self.get_block_mut(cur_rec).terminator = Terminator::Branch {
                    target: merge_block,
                    args: Vec::new(),
                };

                let result_val = self.next_val();
                self.get_block_mut(merge_block)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: result_val,
                        name: res_var,
                    });
                *cur_block = merge_block;
                Some(result_val)
            }
            Expr::Comptime { expr, .. } => self.lower_expr(expr, cur_block),
            Expr::Wrapping(inner, _) => {
                let prev = self.in_wrapping_mode;
                self.in_wrapping_mode = true;
                let res = self.lower_expr(inner, cur_block);
                self.in_wrapping_mode = prev;
                res
            }
            Expr::Saturating(inner, _) => {
                let prev = self.in_saturating_mode;
                self.in_saturating_mode = true;
                let res = self.lower_expr(inner, cur_block);
                self.in_saturating_mode = prev;
                res
            }
            _ => None,
        }
    }
}
