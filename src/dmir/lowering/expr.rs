use crate::ast::*;
use crate::dmir::ir::*;
use crate::types::DataraType;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub fn lower_expr(&mut self, expr: &Expr, cur_block: &mut BasicBlockId) -> Option<ValueId> {
        match expr {
            Expr::Block(stmts, value, _) => {
                // Arm bodies like `match x { 1 => { let y = 2; y } }`: lower
                // the statements inline into the current block sequence (same
                // flow as Stmt::Block in lower_stmt_cfg), then produce the
                // trailing value (or a Unit-ish const when absent).
                for s in stmts {
                    let (next_b, _val) = self.lower_stmt_cfg(s, *cur_block);
                    *cur_block = next_b;
                }
                match value {
                    Some(v) => self.lower_expr(v, cur_block),
                    None => {
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt { dest, value: 0 });
                        Some(dest)
                    }
                }
            }
            Expr::Literal(lit, _) => {
                let dest = self.next_val();
                let b = self.get_block_mut(*cur_block);
                match lit {
                    LiteralValue::Int(v) => b.instructions.push(Inst::ConstInt { dest, value: *v }),
                    LiteralValue::Float(v) => {
                        b.instructions.push(Inst::ConstFloat { dest, value: *v })
                    }
                    LiteralValue::String(v) => b.instructions.push(Inst::ConstStr {
                        dest,
                        value: v.clone(),
                    }),
                    LiteralValue::Bool(v) => {
                        b.instructions.push(Inst::ConstBool { dest, value: *v })
                    }
                    LiteralValue::Char(v) => b.instructions.push(Inst::ConstInt {
                        dest,
                        value: *v as u32 as i64,
                    }),
                    LiteralValue::None => b.instructions.push(Inst::ConstInt { dest, value: 0 }),
                }
                Some(dest)
            }
            Expr::Identifier(name, _) => {
                if self.symbol_values.contains_key(name) {
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::LoadVar {
                            dest,
                            name: name.clone(),
                        });
                    return Some(dest);
                }
                if let Some(&tag) = self.enum_variant_tags.get(name) {
                    let tag_val = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: tag_val,
                            value: tag,
                        });
                    let mut fields = vec![("__tag".to_string(), tag_val)];
                    let slots = self.enum_slots.get(name).cloned().unwrap_or_default();
                    for (idx, s_ty) in slots.iter().enumerate() {
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
                        .unwrap_or_else(|| name.clone());
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::StructInit {
                            dest,
                            class_name,
                            fields,
                        });
                    return Some(dest);
                }
                let this_opt = self.symbol_values.get("this").cloned();
                if let Some(this_val) = this_opt {
                    let dest = self.next_val();
                    let field_ty = self
                        .class_field_types
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| {
                            if name == "score" || name.contains("flt") || name.contains("float") {
                                "Float".to_string()
                            } else if name == "age"
                                || name == "id"
                                || name == "count"
                                || name == "size"
                                || name == "line_count"
                                || name.contains("int")
                            {
                                "Int".to_string()
                            } else {
                                "String".to_string()
                            }
                        });
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::GetField {
                            dest,
                            object: this_val,
                            field: name.clone(),
                            ty: field_ty,
                        });
                    return Some(dest);
                }
                if !self.symbol_values.contains_key(name)
                    && self.function_return_types.contains_key(name)
                {
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::GetFuncAddr {
                            dest,
                            func_name: name.clone(),
                        });
                    return Some(dest);
                }
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::LoadVar {
                        dest,
                        name: name.clone(),
                    });
                Some(dest)
            }
            Expr::InterpolatedString {
                parts, expressions, ..
            } => {
                let mut vals = Vec::new();
                for e in expressions {
                    if let Some(v) = self.lower_expr(e, cur_block) {
                        vals.push(v);
                    }
                }
                let dest = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::FormatStr {
                        dest,
                        parts: parts.clone(),
                        values: vals,
                    });
                Some(dest)
            }
            // Short-circuit logical operators.
            //
            // These must NOT become an `Inst::BinOp`: the Cranelift backend has
            // no lowering for "&&"/"||" and its catch-all arm used to emit
            // `iadd`, so `5 && 3` compiled to `5 + 3` and printed 8. They also
            // have to skip the right operand entirely when the left one already
            // decides the answer, which a BinOp cannot express.
            Expr::Binary {
                op, left, right, ..
            } if op == "&&" || op == "||" => {
                // `a || b` short-circuits to 1 when `a` is truthy.
                // `a && b` short-circuits to 0 when `a` is falsy.
                let short_circuit_when_true = op == "||";
                let short_circuit_value: i64 = if short_circuit_when_true { 1 } else { 0 };

                let l = self.lower_expr(left, cur_block)?;

                let rhs_id = self.create_block("logic_rhs");
                let sc_id = self.create_block("logic_shortcircuit");
                let merge_id = self.create_block("logic_merge");

                // Branch to the short-circuit block when the left operand
                // already settles the result; otherwise fall through and
                // evaluate the right operand.
                self.get_block_mut(*cur_block).terminator = Terminator::CondBranch {
                    cond: l,
                    then_block: if short_circuit_when_true {
                        sc_id
                    } else {
                        rhs_id
                    },
                    then_args: Vec::new(),
                    else_block: if short_circuit_when_true {
                        rhs_id
                    } else {
                        sc_id
                    },
                    else_args: Vec::new(),
                };

                // Right operand: result is its truthiness, normalised to 0/1
                // so that Bool and Int operands behave identically.
                //
                // `rhs_id` stays the branch target; `rhs_cur` tracks where the
                // right operand finished, because it may itself short-circuit
                // and spill into further blocks.
                let mut rhs_cur = rhs_id;
                let r = self.lower_expr(right, &mut rhs_cur)?;
                let zero = self.next_val();
                self.get_block_mut(rhs_cur)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                let norm = self.next_val();
                self.get_block_mut(rhs_cur).instructions.push(Inst::BinOp {
                    dest: norm,
                    op: "!=".into(),
                    left: r,
                    right: zero,
                    ty: "Int".into(),
                });

                // Short-circuit path: the result is a constant; the right
                // operand is never evaluated.
                let sc_const = self.next_val();
                self.get_block_mut(sc_id).instructions.push(Inst::ConstInt {
                    dest: sc_const,
                    value: short_circuit_value,
                });

                // Both paths write the result into one dedicated temporary.
                // DMIR has no phi instruction with lowering support in the
                // backend, so a variable is the only way to join the two values.
                let tmp = format!("__logic_{}", self.val_counter);
                self.val_counter += 1;

                self.get_block_mut(rhs_cur)
                    .instructions
                    .push(Inst::AssignVar {
                        name: tmp.clone(),
                        value: norm,
                    });
                self.get_block_mut(rhs_cur).terminator = Terminator::Branch {
                    target: merge_id,
                    args: Vec::new(),
                };

                self.get_block_mut(sc_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: tmp.clone(),
                        value: sc_const,
                    });
                self.get_block_mut(sc_id).terminator = Terminator::Branch {
                    target: merge_id,
                    args: Vec::new(),
                };

                // Merge: read the value the chosen path stored.
                let dest = self.next_val();
                self.get_block_mut(merge_id)
                    .instructions
                    .push(Inst::LoadVar { dest, name: tmp });

                // Everything the caller emits next belongs in the merge block.
                *cur_block = merge_id;
                Some(dest)
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                let l = self.lower_expr(left, cur_block)?;
                let r = self.lower_expr(right, cur_block)?;
                let dest = self.next_val();
                let is_float = self.is_expr_float(left) || self.is_expr_float(right);
                let is_str_concat =
                    (op == "+") && (self.is_expr_str(left) || self.is_expr_str(right));
                let is_str_cmp = (op == "==" || op == "!=")
                    && (self.is_expr_str(left) || self.is_expr_str(right));
                let effective_op = if !is_float && !is_str_concat && !is_str_cmp {
                    if self.in_wrapping_mode {
                        match op.as_str() {
                            "+" => "wrapping_+".to_string(),
                            "-" => "wrapping_-".to_string(),
                            "*" => "wrapping_*".to_string(),
                            _ => op.clone(),
                        }
                    } else if self.in_saturating_mode {
                        match op.as_str() {
                            "+" => "saturating_+".to_string(),
                            "-" => "saturating_-".to_string(),
                            "*" => "saturating_*".to_string(),
                            _ => op.clone(),
                        }
                    } else {
                        op.clone()
                    }
                } else {
                    op.clone()
                };
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest,
                        op: effective_op,
                        left: l,
                        right: r,
                        ty: if is_str_concat || is_str_cmp {
                            "String".into()
                        } else if is_float {
                            "Float".into()
                        } else {
                            "Int".into()
                        },
                    });
                Some(dest)
            }
            Expr::Unary { op, expr, .. } => {
                let operand = self.lower_expr(expr, cur_block)?;
                if op == "await" {
                    let dest = self.next_val();
                    let (ret_ty, is_method) = match &**expr {
                        Expr::Identifier(name, _) => {
                            let ty = self.lookup_var_type(name);
                            match ty {
                                Some(DataraType::GenericInstance { name: gname, args })
                                    if (gname == "Future" || gname == "Task")
                                        && !args.is_empty() =>
                                {
                                    let rty = match &args[0] {
                                        DataraType::Float => "Float".to_string(),
                                        DataraType::String => "String".to_string(),
                                        DataraType::Bool => "Bool".to_string(),
                                        DataraType::Int => "Int".to_string(),
                                        DataraType::Class(c) => c.clone(),
                                        _ => "Int".to_string(),
                                    };
                                    (rty, true)
                                }
                                Some(DataraType::Class(c)) if c == "Future" || c == "Task" => {
                                    ("String".to_string(), true)
                                }
                                _ => ("Int".to_string(), false),
                            }
                        }
                        Expr::Call { callee, .. } => {
                            if let Expr::Identifier(fn_name, _) = &**callee {
                                let rty = self
                                    .function_return_types
                                    .get(fn_name)
                                    .cloned()
                                    .unwrap_or_else(|| "Int".to_string());
                                (rty, false)
                            } else if let Expr::MemberAccess { member, .. } = &**callee {
                                let rty = self
                                    .function_return_types
                                    .get(member)
                                    .cloned()
                                    .unwrap_or_else(|| "String".to_string());
                                (rty, true)
                            } else {
                                ("Int".to_string(), false)
                            }
                        }
                        _ => ("Int".to_string(), false),
                    };

                    if is_method {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::MethodCall {
                                dest,
                                object: operand,
                                method: "await".into(),
                                args: vec![],
                                ty: ret_ty,
                            });
                    } else {
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::UnOp {
                                dest,
                                op: "await".into(),
                                operand,
                                ty: ret_ty,
                            });
                    }
                    return Some(dest);
                }
                let dest = self.next_val();
                let is_float = match &**expr {
                    Expr::Literal(LiteralValue::Float(_), _) => true,
                    Expr::MemberAccess { member, .. } => {
                        self.class_field_types
                            .get(member)
                            .map(|t| t == "Float")
                            .unwrap_or(false)
                            || member.contains("flt")
                            || member.contains("float")
                    }
                    Expr::Identifier(n, _) => {
                        self.class_field_types
                            .get(n)
                            .map(|t| t == "Float")
                            .unwrap_or(false)
                            || n.contains("flt")
                            || n.contains("float")
                    }
                    _ => false,
                };
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::UnOp {
                        dest,
                        op: op.clone(),
                        operand,
                        ty: if is_float {
                            "Float".into()
                        } else {
                            "Int".into()
                        },
                    });
                Some(dest)
            }

            Expr::Call { callee, args, .. } => self.lower_expr_call(expr, callee, args, cur_block),
            _ => self.lower_composite_expr(expr, cur_block),
        }
    }
}
