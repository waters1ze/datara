use crate::ast::*;
use crate::dmir::ir::*;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub(crate) fn lower_list_higher_order(
        &mut self,
        list_val: ValueId,
        method: &str,
        args: &[Expr],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        match method {
            "map" => {
                if args.is_empty() {
                    return None;
                }
                let idx_var = format!("__map_idx_{}", self.next_val().0);
                let res_var = format!("__map_res_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let res_init = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: res_init,
                        func: "datara_rt_list_create".into(),
                        args: vec![len_val],
                        ty: "List".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: res_init,
                    });

                let header_id = self.create_block("map_header");
                let body_id = self.create_block("map_body");
                let exit_id = self.create_block("map_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });

                let mut body_block = body_id;
                let mapped_elem =
                    self.lower_inline_closure_call(&args[0], &[elem_val], &mut body_block)?;

                let cur_res = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_res,
                        name: res_var.clone(),
                    });
                let next_res = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::Call {
                        dest: next_res,
                        func: "datara_rt_list_append".into(),
                        args: vec![cur_res, mapped_elem],
                        ty: "List".into(),
                    });
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: next_res,
                    });

                let one = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let body_idx = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: body_idx,
                        name: idx_var.clone(),
                    });
                let next_idx = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest: next_idx,
                        op: "+".into(),
                        left: body_idx,
                        right: one,
                        ty: "Int".into(),
                    });
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(body_block, header_id);

                *cur_block = exit_id;
                let final_res = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_res,
                        name: res_var,
                    });
                Some(final_res)
            }
            "filter" => {
                if args.is_empty() {
                    return None;
                }
                let idx_var = format!("__filter_idx_{}", self.next_val().0);
                let res_var = format!("__filter_res_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                let res_init = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: res_init,
                        func: "datara_rt_list_create".into(),
                        args: vec![zero],
                        ty: "List".into(),
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: res_init,
                    });

                let header_id = self.create_block("filter_header");
                let body_id = self.create_block("filter_body");
                let exit_id = self.create_block("filter_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });

                let mut body_block = body_id;
                let keep_cond =
                    self.lower_inline_closure_call(&args[0], &[elem_val], &mut body_block)?;

                let then_append_id = self.create_block("filter_append");
                let inc_id = self.create_block("filter_inc");

                self.get_block_mut(body_block).terminator = Terminator::CondBranch {
                    cond: keep_cond,
                    then_block: then_append_id,
                    then_args: Vec::new(),
                    else_block: inc_id,
                    else_args: Vec::new(),
                };

                let cur_res = self.next_val();
                self.get_block_mut(then_append_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_res,
                        name: res_var.clone(),
                    });
                let next_res = self.next_val();
                self.get_block_mut(then_append_id)
                    .instructions
                    .push(Inst::Call {
                        dest: next_res,
                        func: "datara_rt_list_append".into(),
                        args: vec![cur_res, elem_val],
                        ty: "List".into(),
                    });
                self.get_block_mut(then_append_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: next_res,
                    });
                self.get_block_mut(then_append_id).terminator = Terminator::Branch {
                    target: inc_id,
                    args: Vec::new(),
                };

                let one = self.next_val();
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let inc_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::LoadVar {
                    dest: inc_idx,
                    name: idx_var.clone(),
                });
                let next_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::BinOp {
                    dest: next_idx,
                    op: "+".into(),
                    left: inc_idx,
                    right: one,
                    ty: "Int".into(),
                });
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(inc_id, header_id);

                *cur_block = exit_id;
                let final_res = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_res,
                        name: res_var,
                    });
                Some(final_res)
            }
            "reduce" => {
                if args.len() < 2 {
                    return None;
                }
                let initial_val = self.lower_expr(&args[0], cur_block)?;
                let idx_var = format!("__reduce_idx_{}", self.next_val().0);
                let acc_var = format!("__reduce_acc_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: acc_var.clone(),
                        value: initial_val,
                    });

                let header_id = self.create_block("reduce_header");
                let body_id = self.create_block("reduce_body");
                let exit_id = self.create_block("reduce_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });
                let cur_acc = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_acc,
                        name: acc_var.clone(),
                    });

                let mut body_block = body_id;
                let next_acc = self.lower_inline_closure_call(
                    &args[1],
                    &[cur_acc, elem_val],
                    &mut body_block,
                )?;
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: acc_var.clone(),
                        value: next_acc,
                    });

                let one = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let b_idx = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: b_idx,
                        name: idx_var.clone(),
                    });
                let next_idx = self.next_val();
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::BinOp {
                        dest: next_idx,
                        op: "+".into(),
                        left: b_idx,
                        right: one,
                        ty: "Int".into(),
                    });
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(body_block, header_id);

                *cur_block = exit_id;
                let final_acc = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_acc,
                        name: acc_var,
                    });
                Some(final_acc)
            }
            "find" => {
                if args.is_empty() {
                    return None;
                }
                let idx_var = format!("__find_idx_{}", self.next_val().0);
                let res_var = format!("__find_res_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                let neg_one = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: neg_one,
                        value: -1,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: neg_one,
                    });

                let header_id = self.create_block("find_header");
                let body_id = self.create_block("find_body");
                let exit_id = self.create_block("find_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });

                let mut body_block = body_id;
                let matched =
                    self.lower_inline_closure_call(&args[0], &[elem_val], &mut body_block)?;

                let found_id = self.create_block("find_found");
                let inc_id = self.create_block("find_inc");

                self.get_block_mut(body_block).terminator = Terminator::CondBranch {
                    cond: matched,
                    then_block: found_id,
                    then_args: Vec::new(),
                    else_block: inc_id,
                    else_args: Vec::new(),
                };

                self.get_block_mut(found_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: elem_val,
                    });
                self.get_block_mut(found_id).terminator = Terminator::Branch {
                    target: exit_id,
                    args: Vec::new(),
                };

                let one = self.next_val();
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let inc_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::LoadVar {
                    dest: inc_idx,
                    name: idx_var.clone(),
                });
                let next_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::BinOp {
                    dest: next_idx,
                    op: "+".into(),
                    left: inc_idx,
                    right: one,
                    ty: "Int".into(),
                });
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(inc_id, header_id);

                *cur_block = exit_id;
                let final_res = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_res,
                        name: res_var,
                    });
                Some(final_res)
            }
            "any" => {
                if args.is_empty() {
                    return None;
                }
                let idx_var = format!("__any_idx_{}", self.next_val().0);
                let res_var = format!("__any_res_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                let f_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: f_val,
                        value: false,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: f_val,
                    });

                let header_id = self.create_block("any_header");
                let body_id = self.create_block("any_body");
                let exit_id = self.create_block("any_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });

                let mut body_block = body_id;
                let matched =
                    self.lower_inline_closure_call(&args[0], &[elem_val], &mut body_block)?;

                let found_id = self.create_block("any_found");
                let inc_id = self.create_block("any_inc");

                self.get_block_mut(body_block).terminator = Terminator::CondBranch {
                    cond: matched,
                    then_block: found_id,
                    then_args: Vec::new(),
                    else_block: inc_id,
                    else_args: Vec::new(),
                };

                let t_val = self.next_val();
                self.get_block_mut(found_id)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: t_val,
                        value: true,
                    });
                self.get_block_mut(found_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: t_val,
                    });
                self.get_block_mut(found_id).terminator = Terminator::Branch {
                    target: exit_id,
                    args: Vec::new(),
                };

                let one = self.next_val();
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let inc_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::LoadVar {
                    dest: inc_idx,
                    name: idx_var.clone(),
                });
                let next_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::BinOp {
                    dest: next_idx,
                    op: "+".into(),
                    left: inc_idx,
                    right: one,
                    ty: "Int".into(),
                });
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(inc_id, header_id);

                *cur_block = exit_id;
                let final_res = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_res,
                        name: res_var,
                    });
                Some(final_res)
            }
            "all" => {
                if args.is_empty() {
                    return None;
                }
                let idx_var = format!("__all_idx_{}", self.next_val().0);
                let res_var = format!("__all_res_{}", self.next_val().0);

                let len_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest: len_val,
                        func: "datara_rt_list_len".into(),
                        args: vec![list_val],
                        ty: "Int".into(),
                    });

                let zero = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: zero,
                        value: 0,
                    });
                let t_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: t_val,
                        value: true,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var.clone(),
                        value: zero,
                    });
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: t_val,
                    });

                let header_id = self.create_block("all_header");
                let body_id = self.create_block("all_body");
                let exit_id = self.create_block("all_exit");

                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: header_id,
                    args: Vec::new(),
                };

                let cur_idx = self.next_val();
                self.get_block_mut(header_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: cur_idx,
                        name: idx_var.clone(),
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

                let fetch_idx = self.next_val();
                self.get_block_mut(body_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: fetch_idx,
                        name: idx_var.clone(),
                    });
                let elem_val = self.next_val();
                self.get_block_mut(body_id).instructions.push(Inst::Call {
                    dest: elem_val,
                    func: "datara_rt_list_get".into(),
                    args: vec![list_val, fetch_idx],
                    ty: "Int".into(),
                });

                let mut body_block = body_id;
                let matched =
                    self.lower_inline_closure_call(&args[0], &[elem_val], &mut body_block)?;

                let fail_id = self.create_block("all_fail");
                let inc_id = self.create_block("all_inc");

                self.get_block_mut(body_block).terminator = Terminator::CondBranch {
                    cond: matched,
                    then_block: inc_id,
                    then_args: Vec::new(),
                    else_block: fail_id,
                    else_args: Vec::new(),
                };

                let f_val = self.next_val();
                self.get_block_mut(fail_id)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: f_val,
                        value: false,
                    });
                self.get_block_mut(fail_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: res_var.clone(),
                        value: f_val,
                    });
                self.get_block_mut(fail_id).terminator = Terminator::Branch {
                    target: exit_id,
                    args: Vec::new(),
                };

                let one = self.next_val();
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::ConstInt {
                        dest: one,
                        value: 1,
                    });
                let inc_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::LoadVar {
                    dest: inc_idx,
                    name: idx_var.clone(),
                });
                let next_idx = self.next_val();
                self.get_block_mut(inc_id).instructions.push(Inst::BinOp {
                    dest: next_idx,
                    op: "+".into(),
                    left: inc_idx,
                    right: one,
                    ty: "Int".into(),
                });
                self.get_block_mut(inc_id)
                    .instructions
                    .push(Inst::AssignVar {
                        name: idx_var,
                        value: next_idx,
                    });
                self.set_back_edge(inc_id, header_id);

                *cur_block = exit_id;
                let final_res = self.next_val();
                self.get_block_mut(exit_id)
                    .instructions
                    .push(Inst::LoadVar {
                        dest: final_res,
                        name: res_var,
                    });
                Some(final_res)
            }
            _ => None,
        }
    }

    pub(crate) fn find_packet_for_member(
        &self,
        object: &Expr,
        member: &str,
    ) -> Option<(usize, usize)> {
        if let Expr::Identifier(var_name, _) = object
            && let Some(crate::types::DataraType::Class(cls_name)) = self.lookup_var_type(var_name)
            && let Some(pkt) = self.resolver.packets.get(&cls_name)
        {
            let mut off = 0;
            for f in &pkt.fields {
                if f.name == member {
                    return Some((off, f.bits));
                }
                off += f.bits;
            }
        }
        for pkt in self.resolver.packets.values() {
            let mut off = 0;
            for f in &pkt.fields {
                if f.name == member {
                    return Some((off, f.bits));
                }
                off += f.bits;
            }
        }
        None
    }
}
