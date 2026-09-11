use super::r#abstract::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::dmir::{BasicBlockId, Inst, ValueId};
use std::collections::HashMap;

impl<'a> DmirOwnershipAnalyzer<'a> {
    pub(crate) fn apply_transfer(
        &self,
        inst: &Inst,
        inst_idx: usize,
        bb_id: BasicBlockId,
        state: &mut DenseFunctionDataflowState,
        var_names: &[String],
        var_index: &HashMap<String, usize>,
        interner: &mut BorrowSetInterner,
        mut use_states: Option<&mut HashMap<(BasicBlockId, usize, ValueId), AbstractVarState>>,
        mut diag: Option<&mut DiagnosticEngine>,
    ) {
        match inst {
            Inst::AssignVar { name, value } => {
                let val_state = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), val_state.to_abstract(interner));
                }

                // Check active borrow conflict on re-assignment
                if let Some(&var_idx) = var_index.get(name) {
                    let prev = state.get_var(var_idx);
                    if prev.borrows > 0 {
                        if let Some(ref mut d) = diag {
                            d.error_with_help(
                                ErrorCode::BorrowConflictActiveView,
                                format!(
                                    "Cannot mutate or reassign '{}' while active borrow exists",
                                    name
                                ),
                                None,
                                Some(format!(
                                    "ensure view is finished before modifying '{}'",
                                    name
                                )),
                            );
                        }
                    }

                    // Killed state replaced
                    state.set_var(var_idx, val_state);
                }
            }

            Inst::LoadVar { dest, name } => {
                let (var_idx, var_state) = if let Some(&idx) = var_index.get(name) {
                    (Some(idx), state.get_var(idx).clone())
                } else {
                    (None, DenseVarState::uninit())
                };

                // Check if variable was moved
                if let VarBaseState::Moved { at_inst, reason } = &var_state.base {
                    let at_info = at_inst.as_deref().unwrap_or("earlier call");
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!(
                                "Use of moved value '{}'. Value was moved at {} ({})",
                                name, at_info, reason
                            ),
                            None,
                        );
                    }
                } else if var_state.mut_borrows > 0 {
                    if let Some(ref mut d) = diag {
                        d.error_with_help(
                            ErrorCode::BorrowConflict,
                            format!(
                                "Cannot read '{}' because it is actively mutably borrowed",
                                name
                            ),
                            None,
                            Some(format!(
                                "ensure mutable view is finished before reading '{}'",
                                name
                            )),
                        );
                    }
                }

                if let Some(idx) = var_idx {
                    state.set_val_origin(dest.0, Some(idx as u32));
                }
                state.set_value(dest.0, var_state);
            }

            Inst::Call {
                dest,
                func,
                args,
                ty: _,
            } => {
                let is_destroy = func == "destroy" || func == "drop";
                let is_view = func == "view" || func == "borrow";
                let is_mut_view = func == "mut_view" || func == "mutView";

                let sig_params = self.fn_signatures.get(func).cloned();

                for (arg_idx, arg_val) in args.iter().enumerate() {
                    let arg_state = state.get_value(arg_val.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *arg_val), arg_state.to_abstract(interner));
                    }

                    let origin_name = state
                        .get_val_origin(arg_val.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned();

                    let is_owned_param = is_destroy
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| {
                                let mode = p.ownership_mode.as_str();
                                let is_class = p
                                    .type_node
                                    .as_ref()
                                    .is_some_and(|t| self.class_names.contains(&t.name));
                                (mode == "owned" || mode == "own") && is_class
                            })
                            .unwrap_or(false);

                    let is_borrow_param = is_view
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| p.ownership_mode == "view")
                            .unwrap_or(false);

                    let is_mut_borrow_param = is_mut_view
                        || sig_params
                            .as_ref()
                            .and_then(|p| p.get(arg_idx))
                            .map(|p| p.ownership_mode == "mut-view")
                            .unwrap_or(false);

                    if is_owned_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!("Cannot move '{}' because it was already moved", name),
                                    None,
                                );
                            }
                        } else if arg_state.borrows > 0 || arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowConflictActiveView,
                                    format!(
                                        "Cannot move '{}' because it is actively borrowed",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else {
                            let moved_st = DenseVarState::moved(
                                format!("consumed by call to '{}'", func),
                                Some(format!("call '{}'", func)),
                            );
                            state.set_value(arg_val.0, moved_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, moved_st);
                            }
                        }
                    } else if is_mut_borrow_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!(
                                        "Cannot borrow '{}' because it was previously moved",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else if arg_state.borrows > 0 || arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error_with_help(
                                    ErrorCode::BorrowMultipleMutableViews,
                                    format!("Cannot borrow '{}' as mutable because it is already borrowed", name),
                                    None,
                                    Some(format!("Datara enforces XOR view semantics: only one mutable view of '{}' can exist at a time", name)),
                                );
                            }
                        } else {
                            let mut borrowed_st = arg_state.clone();
                            borrowed_st.borrows += 1;
                            borrowed_st.mut_borrows += 1;
                            borrowed_st.borrow_set_id =
                                interner.insert(borrowed_st.borrow_set_id, func.clone());
                            state.set_value(arg_val.0, borrowed_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, borrowed_st);
                            }
                        }
                    } else if is_borrow_param {
                        if arg_state.is_moved() {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error(
                                    ErrorCode::BorrowUseAfterMove,
                                    format!(
                                        "Cannot borrow '{}' because it was previously moved",
                                        name
                                    ),
                                    None,
                                );
                            }
                        } else if arg_state.mut_borrows > 0 {
                            let name = origin_name.as_deref().unwrap_or("value");
                            if let Some(ref mut d) = diag {
                                d.error_with_help(
                                    ErrorCode::BorrowConflictActiveView,
                                    format!("Cannot borrow '{}' as immutable because it is already mutably borrowed", name),
                                    None,
                                    Some(format!("ensure mutable view is released before creating immutable view of '{}'", name)),
                                );
                            }
                        } else {
                            let mut borrowed_st = arg_state.clone();
                            borrowed_st.borrows += 1;
                            borrowed_st.borrow_set_id =
                                interner.insert(borrowed_st.borrow_set_id, func.clone());
                            state.set_value(arg_val.0, borrowed_st.clone());
                            if let Some(var_idx) = state.get_val_origin(arg_val.0) {
                                state.set_var(var_idx as usize, borrowed_st);
                            }
                        }
                    }
                }

                // Return value produces Owned temporary
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::MethodCall {
                dest,
                object,
                method,
                args,
                ty: _,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}' in method call '{}'", name, method),
                            None,
                        );
                    }
                }

                for arg in args {
                    let a_st = state.get_value(arg.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *arg), a_st.to_abstract(interner));
                    }
                }

                if method == "view" {
                    let mut b_st = obj_state;
                    b_st.borrows += 1;
                    b_st.borrow_set_id = interner.insert(b_st.borrow_set_id, "method_view".into());
                    state.set_value(object.0, b_st.clone());
                    if let Some(var_idx) = state.get_val_origin(object.0) {
                        state.set_var(var_idx as usize, b_st);
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::SetField {
                object,
                field: _,
                value,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                let val_state = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), val_state.to_abstract(interner));
                }

                // Conservative: state of base unchanged, but check active borrows on base
                if obj_state.borrows > 0 {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowConflictActiveView,
                            format!(
                                "Cannot mutate field of '{}' while active borrow exists",
                                name
                            ),
                            None,
                        );
                    }
                }
                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
            }

            Inst::GetField {
                dest,
                object,
                field: _,
                ty: _,
            } => {
                let obj_state = state.get_value(object.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *object), obj_state.to_abstract(interner));
                }

                if obj_state.is_moved() {
                    let name = state
                        .get_val_origin(object.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "object".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::BinOp {
                dest,
                op: _,
                left,
                right,
                ty: _,
            } => {
                let l_st = state.get_value(left.0).clone();
                let r_st = state.get_value(right.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *left), l_st.to_abstract(interner));
                    uses.insert((bb_id, inst_idx, *right), r_st.to_abstract(interner));
                }

                if l_st.is_moved() {
                    let name = state
                        .get_val_origin(left.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
                if r_st.is_moved() {
                    let name = state
                        .get_val_origin(right.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::UnOp {
                dest,
                op: _,
                operand,
                ty: _,
            } => {
                let op_st = state.get_value(operand.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *operand), op_st.to_abstract(interner));
                }

                if op_st.is_moved() {
                    let name = state
                        .get_val_origin(operand.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "operand".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }

                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. } => {
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::GetFuncAddr { dest, .. } => {
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::InlineAsm { outputs, .. } => {
                for (_, dest) in outputs {
                    state.set_value(dest.0, DenseVarState::owned());
                }
            }

            Inst::Out { value } | Inst::Err { value } => {
                let v_st = state.get_value(value.0).clone();
                if let Some(ref mut uses) = use_states {
                    uses.insert((bb_id, inst_idx, *value), v_st.to_abstract(interner));
                }

                if v_st.is_moved() {
                    let name = state
                        .get_val_origin(value.0)
                        .and_then(|idx| var_names.get(idx as usize))
                        .cloned()
                        .unwrap_or_else(|| "value".into());
                    if let Some(ref mut d) = diag {
                        d.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!("Use of moved value '{}'", name),
                            None,
                        );
                    }
                }
            }

            Inst::FormatStr {
                dest,
                parts: _,
                values,
            } => {
                for v in values {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                    if v_st.is_moved() {
                        let name = state
                            .get_val_origin(v.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "value".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::StructInit {
                dest,
                class_name: _,
                fields,
            } => {
                for (_, f_val) in fields {
                    let f_st = state.get_value(f_val.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *f_val), f_st.to_abstract(interner));
                    }
                    if f_st.is_moved() {
                        let name = state
                            .get_val_origin(f_val.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "field".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ty: _,
            } => {
                for v in [cond, then_val, else_val] {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Decide {
                dest,
                arms,
                else_val,
                ty: _,
            } => {
                for (cond, body) in arms {
                    for v in [cond, body] {
                        let v_st = state.get_value(v.0).clone();
                        if let Some(ref mut uses) = use_states {
                            uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                        }
                    }
                }
                if let Some(ev) = else_val {
                    let v_st = state.get_value(ev.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *ev), v_st.to_abstract(interner));
                    }
                }
                state.set_value(dest.0, DenseVarState::owned());
            }

            Inst::Return { value } => {
                if let Some(v) = value {
                    let v_st = state.get_value(v.0).clone();
                    if let Some(ref mut uses) = use_states {
                        uses.insert((bb_id, inst_idx, *v), v_st.to_abstract(interner));
                    }
                    if v_st.is_moved() {
                        let name = state
                            .get_val_origin(v.0)
                            .and_then(|idx| var_names.get(idx as usize))
                            .cloned()
                            .unwrap_or_else(|| "return value".into());
                        if let Some(ref mut d) = diag {
                            d.error(
                                ErrorCode::BorrowUseAfterMove,
                                format!("Use of moved value '{}'", name),
                                None,
                            );
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
