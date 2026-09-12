use crate::ast::*;
use crate::dmir::ir::*;
use crate::types::DataraType;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub fn lower_stmt_cfg(
        &mut self,
        stmt: &Stmt,
        mut cur_block: BasicBlockId,
    ) -> (BasicBlockId, Option<ValueId>) {
        let sp = stmt.span();
        if sp.start_line > 0 {
            self.current_line_spans.push(sp.clone());
        }
        match stmt {
            Stmt::Block(stmts, _) => {
                let mut last = None;
                for s in stmts {
                    let (next_b, val) = self.lower_stmt_cfg(s, cur_block);
                    cur_block = next_b;
                    last = val;
                }
                (cur_block, last)
            }
            Stmt::Let { name, init, .. }
            | Stmt::Mut { name, init, .. }
            | Stmt::Const { name, init, .. }
            | Stmt::Val { name, init, .. }
            | Stmt::CompactBind { name, init, .. } => {
                let type_node = match stmt {
                    Stmt::Let { type_node, .. }
                    | Stmt::Mut { type_node, .. }
                    | Stmt::Const { type_node, .. }
                    | Stmt::Val { type_node, .. } => type_node.as_ref(),
                    _ => None,
                };
                if let Some(tn) = type_node {
                    if tn.name == "List" {
                        let elem_ty = match tn.generic_args.first().map(|a| a.name.as_str()) {
                            Some("Float" | "Float64" | "f64" | "Float32" | "f32") => {
                                DataraType::Float
                            }
                            Some("String" | "Str") => DataraType::String,
                            Some("Bool") => DataraType::Bool,
                            _ => DataraType::Int,
                        };
                        self.local_var_types
                            .insert(name.clone(), DataraType::List(Box::new(elem_ty.clone())));
                        if elem_ty == DataraType::Float {
                            self.class_field_types.insert(name.clone(), "Float".into());
                        }
                    } else if matches!(
                        tn.name.as_str(),
                        "Float" | "Float64" | "f64" | "Float32" | "f32"
                    ) {
                        self.local_var_types.insert(name.clone(), DataraType::Float);
                        self.class_field_types.insert(name.clone(), "Float".into());
                    } else if matches!(tn.name.as_str(), "String" | "Str") {
                        self.local_var_types
                            .insert(name.clone(), DataraType::String);
                    } else if tn.name == "Bool" {
                        self.local_var_types.insert(name.clone(), DataraType::Bool);
                    }
                }
                if let Some(ty) = self.types.symbol_types.get(name) {
                    self.local_var_types.insert(name.clone(), ty.clone());
                }
                if let Expr::ObjectInit { class_name, .. } = init {
                    self.local_var_types
                        .insert(name.clone(), DataraType::Class(class_name.clone()));
                } else if let Expr::MapLiteral(entries, _) = init {
                    let mut is_flt = false;
                    for (_, v) in entries {
                        if self.is_expr_float(v) {
                            is_flt = true;
                            break;
                        }
                    }
                    let val_ty = if is_flt {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    self.local_var_types.insert(
                        name.clone(),
                        DataraType::Map(Box::new(DataraType::String), Box::new(val_ty)),
                    );
                } else if let Expr::ListLiteral(elements, _) = init {
                    if !self.local_var_types.contains_key(name) {
                        let mut is_flt = false;
                        for e in elements {
                            if self.is_expr_float(e) {
                                is_flt = true;
                                break;
                            }
                        }
                        let val_ty = if is_flt {
                            DataraType::Float
                        } else {
                            DataraType::Int
                        };
                        self.local_var_types
                            .insert(name.clone(), DataraType::List(Box::new(val_ty)));
                    }
                } else if self.is_expr_str(init) {
                    self.local_var_types
                        .insert(name.clone(), DataraType::String);
                } else if self.is_expr_float(init) {
                    self.local_var_types.insert(name.clone(), DataraType::Float);
                } else if self.is_expr_bool(init) {
                    self.local_var_types.insert(name.clone(), DataraType::Bool);
                }
                if let Expr::Lambda { params, body, .. } = init {
                    self.local_lambdas
                        .insert(name.clone(), (params.clone(), *body.clone()));
                }
                if !self.local_var_types.contains_key(name) {
                    if let Some(inferred) = self.infer_expr_datara_type(init) {
                        self.local_var_types.insert(name.clone(), inferred);
                    }
                }
                if !self.local_var_types.contains_key(name)
                    && let Some(ty) = self.lookup_var_type(name)
                {
                    self.local_var_types.insert(name.clone(), ty);
                }
                let val = self.lower_expr(init, &mut cur_block);
                if let Some(v) = val {
                    self.symbol_values.insert(name.clone(), v);
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::AssignVar {
                            name: name.clone(),
                            value: v,
                        });
                }
                (cur_block, val)
            }
            Stmt::Assign { target, value, .. } => {
                let val = self.lower_expr(value, &mut cur_block);
                if let Some(v) = val {
                    match target {
                        Expr::Identifier(name, _) => {
                            self.symbol_values.insert(name.clone(), v);
                            self.get_block_mut(cur_block)
                                .instructions
                                .push(Inst::AssignVar {
                                    name: name.clone(),
                                    value: v,
                                });
                        }
                        // `this.field = v` / `obj.field = v`.
                        //
                        // This is the only place a field store is produced.
                        // It used to be missing entirely: `Stmt::Assign` only
                        // matched `Expr::Identifier`, so every assignment whose
                        // target was a member access was silently dropped on
                        // the floor. Mutating methods compiled to no-ops and
                        // the field kept its initial value forever — with no
                        // diagnostic, because the statement *was* visited, it
                        // just produced no instruction.
                        Expr::MemberAccess { object, member, .. } => {
                            if let Some(obj_val) = self.lower_expr(object, &mut cur_block) {
                                self.get_block_mut(cur_block)
                                    .instructions
                                    .push(Inst::SetField {
                                        object: obj_val,
                                        field: member.clone(),
                                        value: v,
                                    });
                            }
                        }
                        Expr::IndexAccess { object, index, .. } => {
                            if let Some(obj_val) = self.lower_expr(object, &mut cur_block)
                                && let Some(idx_val) = self.lower_expr(index, &mut cur_block)
                            {
                                let is_map = self.is_expr_str(index)
                                    || match &**object {
                                        Expr::Identifier(name, _) => matches!(
                                            self.lookup_var_type(name),
                                            Some(DataraType::Map(..))
                                        ),
                                        _ => false,
                                    };
                                let (func_name, ty) = if is_map {
                                    ("datara_rt_map_insert", "Map")
                                } else {
                                    ("datara_rt_list_set", "List")
                                };
                                let ret_val = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(Inst::Call {
                                    dest: ret_val,
                                    func: func_name.into(),
                                    args: vec![obj_val, idx_val, v],
                                    ty: ty.into(),
                                });
                                if let Expr::Identifier(var_name, _) = &**object {
                                    self.get_block_mut(cur_block).instructions.push(
                                        Inst::AssignVar {
                                            name: var_name.clone(),
                                            value: ret_val,
                                        },
                                    );
                                    self.symbol_values.insert(var_name.clone(), ret_val);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                (cur_block, val)
            }
            Stmt::Expr(e, _) => {
                let val = self.lower_expr(e, &mut cur_block);
                if let Expr::Call { callee, .. } = e
                    && let Expr::MemberAccess { object, member, .. } = &**callee
                    && matches!(
                        member.as_str(),
                        "push" | "append" | "add" | "set" | "insert"
                    )
                    && let Expr::Identifier(var_name, _) = &**object
                {
                    let is_collection = if let Some(ty) = self.lookup_var_type(var_name) {
                        match ty {
                            crate::types::DataraType::List(_)
                            | crate::types::DataraType::Map(..) => true,
                            crate::types::DataraType::Class(name) => {
                                name == "List" || name == "Array" || name == "Map"
                            }
                            crate::types::DataraType::GenericInstance { name, .. } => {
                                name == "List" || name == "Array" || name == "Map"
                            }
                            _ => false,
                        }
                    } else {
                        false
                    };
                    if is_collection && let Some(v) = val {
                        self.get_block_mut(cur_block)
                            .instructions
                            .push(Inst::AssignVar {
                                name: var_name.clone(),
                                value: v,
                            });
                        self.symbol_values.insert(var_name.clone(), v);
                    }
                }
                (cur_block, val)
            }
            Stmt::Out(e, _) => {
                if let Some(val) = self.lower_expr(e, &mut cur_block) {
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::Out { value: val });
                }
                (cur_block, None)
            }
            Stmt::Err(e, _) => {
                if let Some(val) = self.lower_expr(e, &mut cur_block) {
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::Err { value: val });
                }
                (cur_block, None)
            }
            Stmt::Return(opt_e, _) => {
                let val = if let Some(e) = opt_e {
                    self.lower_expr(e, &mut cur_block)
                } else {
                    None
                };
                let b = self.get_block_mut(cur_block);
                b.instructions.push(Inst::Return { value: val });
                b.terminator = Terminator::Return { value: val };
                (cur_block, val)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_val = self
                    .lower_expr(condition, &mut cur_block)
                    .unwrap_or(ValueId(0));
                let then_id = self.create_block("if_then");
                let else_id = self.create_block("if_else");
                let merge_id = self.create_block("if_merge");

                self.get_block_mut(cur_block).terminator = Terminator::CondBranch {
                    cond: cond_val,
                    then_block: then_id,
                    then_args: Vec::new(),
                    else_block: else_id,
                    else_args: Vec::new(),
                };

                let (then_end, _) = self.lower_stmt_cfg(then_branch, then_id);
                if matches!(
                    self.get_block_mut(then_end).terminator,
                    Terminator::Unreachable
                ) {
                    self.get_block_mut(then_end).terminator = Terminator::Branch {
                        target: merge_id,
                        args: Vec::new(),
                    };
                }

                let (else_end, _) = if let Some(eb) = else_branch {
                    self.lower_stmt_cfg(eb, else_id)
                } else {
                    (else_id, None)
                };
                if matches!(
                    self.get_block_mut(else_end).terminator,
                    Terminator::Unreachable
                ) {
                    self.get_block_mut(else_end).terminator = Terminator::Branch {
                        target: merge_id,
                        args: Vec::new(),
                    };
                }

                (merge_id, None)
            }
            Stmt::While {
                condition, body, ..
            } => {
                let mut header_id = self.create_block("while_header");
                // The back edge must re-enter the loop at the FIRST block of the
                // condition, not wherever condition lowering left off: a
                // short-circuiting condition ("a && b") spans several blocks and
                // every one of them has to run again on the next iteration.
                let loop_header_id = header_id;
                let body_id = self.create_block("while_body");
                let exit_id = self.create_block("while_exit");

                self.get_block_mut(cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cond_val = self
                    .lower_expr(condition, &mut header_id)
                    .unwrap_or(ValueId(0));
                self.get_block_mut(header_id).terminator = Terminator::CondBranch {
                    cond: cond_val,
                    then_block: body_id,
                    then_args: Vec::new(),
                    else_block: exit_id,
                    else_args: Vec::new(),
                };

                let (body_end, _) = self.lower_stmt_cfg(body, body_id);
                self.set_back_edge(body_end, loop_header_id);

                // No compound `WhileLoop` snapshot is emitted here. The legacy
                // node duplicated the header/body instructions inside a single
                // instruction: the backend ignored it, and under strict SSA
                // verification its duplicate definitions and out-of-dominance
                // uses are illegal. The real CFG blocks above are the loop.

                (exit_id, None)
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            } => {
                // `for v in start..end` desugars into a counted loop:
                //     v = start
                //     while v < end { body; v = v + 1 }
                //
                // Iterating a non-range value (a list, map or string) is not
                // implemented: the runtime exposes no iterator protocol yet, so
                // the expression is evaluated and the body runs once, exactly as
                // before. See docs/AUDIT_OPTIMIZATION_FIXES.md.
                match iterable {
                    Expr::Range { start, end, .. } => {
                        let start_val = self.lower_expr(start, &mut cur_block);
                        let end_val = self.lower_expr(end, &mut cur_block);

                        let header_id = self.create_block("for_header");
                        let body_id = self.create_block("for_body");
                        let exit_id = self.create_block("for_exit");

                        if let Some(sv) = start_val {
                            self.get_block_mut(cur_block)
                                .instructions
                                .push(Inst::AssignVar {
                                    name: var_name.clone(),
                                    value: sv,
                                });
                            // Registering the name is what makes `Expr::Identifier`
                            // inside the body emit a `LoadVar` for the induction variable.
                            self.symbol_values.insert(var_name.clone(), sv);
                        }
                        self.get_block_mut(cur_block).terminator = Terminator::Branch {
                            target: header_id,
                            args: Vec::new(),
                        };

                        // Header: cond = (v < end)
                        let cur = self.next_val();
                        self.get_block_mut(header_id)
                            .instructions
                            .push(Inst::LoadVar {
                                dest: cur,
                                name: var_name.clone(),
                            });
                        let cond = self.next_val();
                        match end_val {
                            Some(ev) => {
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::BinOp {
                                        dest: cond,
                                        op: "<".into(),
                                        left: cur,
                                        right: ev,
                                        ty: "Int".into(),
                                    })
                            }
                            None => {
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::ConstBool {
                                        dest: cond,
                                        value: false,
                                    })
                            }
                        }
                        self.get_block_mut(header_id).terminator = Terminator::CondBranch {
                            cond,
                            then_block: body_id,
                            then_args: Vec::new(),
                            else_block: exit_id,
                            else_args: Vec::new(),
                        };

                        let (body_end, _) = self.lower_stmt_cfg(body, body_id);

                        // A body that ends in `return` has no back edge, so
                        // there is nothing to increment either.
                        if self.block_falls_through(body_end) {
                            // Increment: v = v + 1
                            let one = self.next_val();
                            self.get_block_mut(body_end)
                                .instructions
                                .push(Inst::ConstInt {
                                    dest: one,
                                    value: 1,
                                });
                            let loaded = self.next_val();
                            self.get_block_mut(body_end)
                                .instructions
                                .push(Inst::LoadVar {
                                    dest: loaded,
                                    name: var_name.clone(),
                                });
                            let next = self.next_val();
                            self.get_block_mut(body_end).instructions.push(Inst::BinOp {
                                dest: next,
                                op: "+".into(),
                                left: loaded,
                                right: one,
                                ty: "Int".into(),
                            });
                            self.get_block_mut(body_end)
                                .instructions
                                .push(Inst::AssignVar {
                                    name: var_name.clone(),
                                    value: next,
                                });
                        }
                        self.set_back_edge(body_end, header_id);

                        (exit_id, None)
                    }
                    _ if self.is_expr_str(iterable) => {
                        let str_val = self.lower_expr(iterable, &mut cur_block);
                        match str_val {
                            Some(sv) => {
                                let offset_name = format!("__for_stroff_{}", self.next_val().0);
                                let zero = self.next_val();
                                self.get_block_mut(cur_block)
                                    .instructions
                                    .push(Inst::ConstInt {
                                        dest: zero,
                                        value: 0,
                                    });
                                self.get_block_mut(cur_block)
                                    .instructions
                                    .push(Inst::AssignVar {
                                        name: offset_name.clone(),
                                        value: zero,
                                    });
                                let len_val = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(Inst::Call {
                                    dest: len_val,
                                    func: "datara_rt_str_len".into(),
                                    args: vec![sv],
                                    ty: "Int".into(),
                                });

                                let header_id = self.create_block("for_str_header");
                                let body_id = self.create_block("for_str_body");
                                let exit_id = self.create_block("for_str_exit");
                                self.get_block_mut(cur_block).terminator = Terminator::Branch {
                                    target: header_id,
                                    args: Vec::new(),
                                };

                                let cur_off = self.next_val();
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::LoadVar {
                                        dest: cur_off,
                                        name: offset_name.clone(),
                                    });
                                let cond = self.next_val();
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::BinOp {
                                        dest: cond,
                                        op: "<".into(),
                                        left: cur_off,
                                        right: len_val,
                                        ty: "Int".into(),
                                    });
                                self.get_block_mut(header_id).terminator = Terminator::CondBranch {
                                    cond,
                                    then_block: body_id,
                                    then_args: Vec::new(),
                                    else_block: exit_id,
                                    else_args: Vec::new(),
                                };

                                let fetch_off = self.next_val();
                                self.get_block_mut(body_id)
                                    .instructions
                                    .push(Inst::LoadVar {
                                        dest: fetch_off,
                                        name: offset_name.clone(),
                                    });
                                let scalar_val = self.next_val();
                                self.get_block_mut(body_id).instructions.push(Inst::Call {
                                    dest: scalar_val,
                                    func: "datara_rt_str_scalar_at".into(),
                                    args: vec![sv, fetch_off],
                                    ty: "String".into(),
                                });
                                self.get_block_mut(body_id)
                                    .instructions
                                    .push(Inst::AssignVar {
                                        name: var_name.clone(),
                                        value: scalar_val,
                                    });
                                self.symbol_values.insert(var_name.clone(), scalar_val);

                                let (body_end, _) = self.lower_stmt_cfg(body, body_id);

                                if self.block_falls_through(body_end) {
                                    let body_off = self.next_val();
                                    self.get_block_mut(body_end)
                                        .instructions
                                        .push(Inst::LoadVar {
                                            dest: body_off,
                                            name: offset_name.clone(),
                                        });
                                    let next_off = self.next_val();
                                    self.get_block_mut(body_end).instructions.push(Inst::Call {
                                        dest: next_off,
                                        func: "datara_rt_str_next_offset".into(),
                                        args: vec![sv, body_off],
                                        ty: "Int".into(),
                                    });
                                    self.get_block_mut(body_end).instructions.push(
                                        Inst::AssignVar {
                                            name: offset_name.clone(),
                                            value: next_off,
                                        },
                                    );
                                }
                                self.set_back_edge(body_end, header_id);

                                (exit_id, None)
                            }
                            None => (cur_block, None),
                        }
                    }
                    _ => {
                        // `for item in <list>`: lower into a counted loop over
                        // the runtime list protocol:
                        //     idx = 0; len = list_len(list)
                        //     while idx < len {
                        //         item = list_get(list, idx)
                        //         body
                        //         idx = idx + 1
                        //     }
                        // The list pointer is materialised once, before the
                        // loop, and the header reads only the induction var.
                        let list_val = self.lower_expr(iterable, &mut cur_block);
                        match list_val {
                            Some(lv) => {
                                let idx_name = format!("__for_idx_{}", self.next_val().0);
                                let zero = self.next_val();
                                self.get_block_mut(cur_block)
                                    .instructions
                                    .push(Inst::ConstInt {
                                        dest: zero,
                                        value: 0,
                                    });
                                self.get_block_mut(cur_block)
                                    .instructions
                                    .push(Inst::AssignVar {
                                        name: idx_name.clone(),
                                        value: zero,
                                    });
                                let len_val = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(Inst::Call {
                                    dest: len_val,
                                    func: "datara_rt_list_len".into(),
                                    args: vec![lv],
                                    ty: "Int".into(),
                                });

                                let header_id = self.create_block("for_header");
                                let body_id = self.create_block("for_body");
                                let exit_id = self.create_block("for_exit");
                                self.get_block_mut(cur_block).terminator = Terminator::Branch {
                                    target: header_id,
                                    args: Vec::new(),
                                };

                                let cur_idx = self.next_val();
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::LoadVar {
                                        dest: cur_idx,
                                        name: idx_name.clone(),
                                    });
                                let cond = self.next_val();
                                self.get_block_mut(header_id)
                                    .instructions
                                    .push(Inst::BinOp {
                                        dest: cond,
                                        op: "<".into(),
                                        left: cur_idx,
                                        right: len_val,
                                        ty: "Int".into(),
                                    });
                                self.get_block_mut(header_id).terminator = Terminator::CondBranch {
                                    cond,
                                    then_block: body_id,
                                    then_args: Vec::new(),
                                    else_block: exit_id,
                                    else_args: Vec::new(),
                                };

                                // Fetch the element and bind it before the
                                // user statements run. Registering the loop
                                // var in symbol_values makes every
                                // `Expr::Identifier` inside the body emit a
                                // `LoadVar`, which is correct across
                                // iterations (same pattern as the counted
                                // `for i in a..b` loop above).
                                let fetch_idx = self.next_val();
                                self.get_block_mut(body_id)
                                    .instructions
                                    .push(Inst::LoadVar {
                                        dest: fetch_idx,
                                        name: idx_name.clone(),
                                    });
                                // The element representation must match the
                                // loop variable's checked type: the backend
                                // keys string/bool/float printing and concat
                                // off the Call's `ty` field, so a `List<Str>`
                                // loop must fetch with `ty: "String"`, not the
                                // integer default.
                                let elem_type =
                                    self.lookup_var_type(var_name).or_else(|| match iterable {
                                        Expr::Identifier(name, _) => {
                                            match self.lookup_var_type(name) {
                                                Some(DataraType::List(inner)) => Some(*inner),
                                                _ => None,
                                            }
                                        }
                                        _ => None,
                                    });
                                if let Some(ref et) = elem_type {
                                    self.local_var_types.insert(var_name.clone(), et.clone());
                                }
                                let elem_repr = match elem_type.as_ref() {
                                    Some(DataraType::String) | Some(DataraType::Char) => "String",
                                    Some(DataraType::Bool) => "Bool",
                                    Some(DataraType::Float) => "Float",
                                    Some(DataraType::List(_)) => "List",
                                    Some(DataraType::Map(..)) => "Map",
                                    _ => "Int",
                                };
                                let item_val = self.next_val();
                                self.get_block_mut(body_id).instructions.push(Inst::Call {
                                    dest: item_val,
                                    func: "datara_rt_list_get".into(),
                                    args: vec![lv, fetch_idx],
                                    ty: elem_repr.into(),
                                });
                                self.get_block_mut(body_id)
                                    .instructions
                                    .push(Inst::AssignVar {
                                        name: var_name.clone(),
                                        value: item_val,
                                    });
                                self.symbol_values.insert(var_name.clone(), item_val);

                                let (body_end, _) = self.lower_stmt_cfg(body, body_id);

                                if self.block_falls_through(body_end) {
                                    let one = self.next_val();
                                    self.get_block_mut(body_end).instructions.push(
                                        Inst::ConstInt {
                                            dest: one,
                                            value: 1,
                                        },
                                    );
                                    let loaded = self.next_val();
                                    self.get_block_mut(body_end)
                                        .instructions
                                        .push(Inst::LoadVar {
                                            dest: loaded,
                                            name: idx_name.clone(),
                                        });
                                    let next = self.next_val();
                                    self.get_block_mut(body_end).instructions.push(Inst::BinOp {
                                        dest: next,
                                        op: "+".into(),
                                        left: loaded,
                                        right: one,
                                        ty: "Int".into(),
                                    });
                                    self.get_block_mut(body_end).instructions.push(
                                        Inst::AssignVar {
                                            name: idx_name.clone(),
                                            value: next,
                                        },
                                    );
                                }
                                self.set_back_edge(body_end, header_id);

                                (exit_id, None)
                            }
                            None => self.lower_stmt_cfg(body, cur_block),
                        }
                    }
                }
            }
            Stmt::Loop { body, .. } => {
                // `loop { .. }` is `while true { .. }`. The exit block only
                // exists so the value after the loop is well formed; it is
                // unreachable unless the body returns.
                let header_id = self.create_block("loop_header");
                let body_id = self.create_block("loop_body");
                let exit_id = self.create_block("loop_exit");

                self.get_block_mut(cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let true_val = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: true_val,
                        value: 1,
                    });
                self.get_block_mut(header_id).terminator = Terminator::CondBranch {
                    cond: true_val,
                    then_block: body_id,
                    then_args: Vec::new(),
                    else_block: exit_id,
                    else_args: Vec::new(),
                };

                let (body_end, _) = self.lower_stmt_cfg(body, body_id);
                self.set_back_edge(body_end, header_id);

                (exit_id, None)
            }
            Stmt::TryCatch { try_block, .. } => self.lower_stmt_cfg(try_block, cur_block),
            Stmt::Parallel(body, _) => {
                if let Stmt::Block(stmts, _) = body.as_ref() {
                    let get_call_info = |s: &Stmt| -> Option<(String, Option<Expr>)> {
                        match s {
                            Stmt::Expr(Expr::Call { callee, args, .. }, _) => {
                                if let Expr::Identifier(fn_name, _) = callee.as_ref() {
                                    if args.is_empty() {
                                        return Some((fn_name.clone(), None));
                                    } else if args.len() == 1 {
                                        return Some((fn_name.clone(), Some(args[0].clone())));
                                    }
                                }
                            }
                            Stmt::Block(inner, _) if inner.len() == 1 => {
                                if let Stmt::Expr(Expr::Call { callee, args, .. }, _) = &inner[0]
                                    && let Expr::Identifier(fn_name, _) = callee.as_ref()
                                {
                                    if args.is_empty() {
                                        return Some((fn_name.clone(), None));
                                    } else if args.len() == 1 {
                                        return Some((fn_name.clone(), Some(args[0].clone())));
                                    }
                                }
                            }
                            _ => {}
                        }
                        None
                    };

                    let mut call_infos = Vec::new();
                    for s in stmts {
                        if let Some(info) = get_call_info(s) {
                            call_infos.push(info);
                        }
                    }

                    if call_infos.len() == stmts.len() && call_infos.len() >= 2 {
                        for chunk in call_infos.chunks(2) {
                            if chunk.len() == 2 {
                                let (ref fn1_name, ref arg1) = chunk[0];
                                let (ref fn2_name, ref arg2) = chunk[1];
                                let ctx1 = if let Some(arg) = arg1 {
                                    self.lower_expr(arg, &mut cur_block).unwrap_or_else(|| {
                                        let z = self.next_val();
                                        self.get_block_mut(cur_block)
                                            .instructions
                                            .push(Inst::ConstInt { dest: z, value: 0 });
                                        z
                                    })
                                } else {
                                    let z = self.next_val();
                                    self.get_block_mut(cur_block)
                                        .instructions
                                        .push(Inst::ConstInt { dest: z, value: 0 });
                                    z
                                };

                                let ctx2 = if let Some(arg) = arg2 {
                                    self.lower_expr(arg, &mut cur_block).unwrap_or_else(|| {
                                        let z = self.next_val();
                                        self.get_block_mut(cur_block)
                                            .instructions
                                            .push(Inst::ConstInt { dest: z, value: 0 });
                                        z
                                    })
                                } else {
                                    let z = self.next_val();
                                    self.get_block_mut(cur_block)
                                        .instructions
                                        .push(Inst::ConstInt { dest: z, value: 0 });
                                    z
                                };

                                let fn1_addr = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(
                                    Inst::GetFuncAddr {
                                        dest: fn1_addr,
                                        func_name: fn1_name.clone(),
                                    },
                                );

                                let fn2_addr = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(
                                    Inst::GetFuncAddr {
                                        dest: fn2_addr,
                                        func_name: fn2_name.clone(),
                                    },
                                );

                                let dummy = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(Inst::Call {
                                    dest: dummy,
                                    func: "datara_rt_parallel_invoke".into(),
                                    args: vec![fn1_addr, ctx1, fn2_addr, ctx2],
                                    ty: "Unit".into(),
                                });
                            } else {
                                let (ref fn_name, ref arg) = chunk[0];
                                let call_args = if let Some(a) = arg {
                                    let ctx =
                                        self.lower_expr(a, &mut cur_block).unwrap_or_else(|| {
                                            let z = self.next_val();
                                            self.get_block_mut(cur_block)
                                                .instructions
                                                .push(Inst::ConstInt { dest: z, value: 0 });
                                            z
                                        });
                                    vec![ctx]
                                } else {
                                    vec![]
                                };
                                let dummy = self.next_val();
                                self.get_block_mut(cur_block).instructions.push(Inst::Call {
                                    dest: dummy,
                                    func: fn_name.clone(),
                                    args: call_args,
                                    ty: "Unit".into(),
                                });
                            }
                        }
                        return (cur_block, None);
                    }
                }
                self.lower_stmt_cfg(body, cur_block)
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                span,
            } => {
                if let Expr::Range { start, end, .. } = iterable {
                    let worker_opt = match body.as_ref() {
                        Stmt::Expr(Expr::Call { callee, args, .. }, _) => {
                            if let Expr::Identifier(fn_name, _) = callee.as_ref() {
                                if args.len() == 1 {
                                    if let Expr::Identifier(arg_name, _) = &args[0] {
                                        if arg_name == var_name {
                                            Some(fn_name.clone())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        Stmt::Block(stmts, _) if stmts.len() == 1 => {
                            if let Stmt::Expr(Expr::Call { callee, args, .. }, _) = &stmts[0] {
                                if let Expr::Identifier(fn_name, _) = callee.as_ref() {
                                    if args.len() == 1 {
                                        if let Expr::Identifier(arg_name, _) = &args[0] {
                                            if arg_name == var_name {
                                                Some(fn_name.clone())
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(fn_name) = worker_opt
                        && let (Some(s_val), Some(e_val)) = (
                            self.lower_expr(start, &mut cur_block),
                            self.lower_expr(end, &mut cur_block),
                        )
                    {
                        let fn_addr = self.next_val();
                        self.get_block_mut(cur_block)
                            .instructions
                            .push(Inst::GetFuncAddr {
                                dest: fn_addr,
                                func_name: fn_name,
                            });
                        let zero = self.next_val();
                        self.get_block_mut(cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: zero,
                                value: 0,
                            });
                        let dummy = self.next_val();
                        self.get_block_mut(cur_block).instructions.push(Inst::Call {
                            dest: dummy,
                            func: "datara_rt_parallel_for".into(),
                            args: vec![s_val, e_val, fn_addr, zero],
                            ty: "Unit".into(),
                        });
                        return (cur_block, None);
                    }
                }
                let for_stmt = Stmt::For {
                    var_name: var_name.clone(),
                    iterable: iterable.clone(),
                    body: body.clone(),
                    span: span.clone(),
                };
                self.lower_stmt_cfg(&for_stmt, cur_block)
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                let prev_sym = self.symbol_values.get(resource_name).cloned();
                if let Some(init_val) = self.lower_expr(init, &mut cur_block) {
                    self.symbol_values.insert(resource_name.clone(), init_val);
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::AssignVar {
                            name: resource_name.clone(),
                            value: init_val,
                        });
                }
                let (body_end, ret_val) = self.lower_stmt_cfg(body, cur_block);
                if self.block_falls_through(body_end) {
                    let res_val = self.next_val();
                    self.get_block_mut(body_end)
                        .instructions
                        .push(Inst::LoadVar {
                            dest: res_val,
                            name: resource_name.clone(),
                        });
                    let close_dest = self.next_val();
                    self.get_block_mut(body_end)
                        .instructions
                        .push(Inst::MethodCall {
                            dest: close_dest,
                            object: res_val,
                            method: "close".into(),
                            args: Vec::new(),
                            ty: "Unit".into(),
                        });
                }
                match prev_sym {
                    Some(v) => self.symbol_values.insert(resource_name.clone(), v),
                    None => self.symbol_values.remove(resource_name),
                };
                (body_end, ret_val)
            }
            Stmt::Unsafe { body, .. } => self.lower_stmt_cfg(body, cur_block),
            Stmt::Asm {
                instructions,
                options,
                ..
            } => {
                let template = instructions.join("; ");
                self.get_block_mut(cur_block)
                    .instructions
                    .push(Inst::InlineAsm {
                        template,
                        outputs: Vec::new(),
                        inputs: Vec::new(),
                        clobbers: Vec::new(),
                        options: options.clone(),
                    });
                (cur_block, None)
            }
        }
    }
}
