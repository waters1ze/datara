use crate::ast::*;
use crate::dmir::ir::*;
use std::collections::HashMap;

use super::Lowering;

impl<'a> Lowering<'a> {
    pub(crate) fn lower_function(&mut self, f: &FunctionDecl) -> Function {
        self.current_fn_name = f.name.clone();
        self.local_var_types.clear();
        self.current_blocks.clear();
        self.block_counter = 0;
        self.symbol_values.clear();
        self.current_line_spans.clear();

        let mut entry_id = self.create_block("entry");

        let mut params = Vec::new();
        let mut param_refinements = Vec::new();
        for p in &f.params {
            let p_val = self.next_val();
            let ty_str = p
                .type_node
                .as_ref()
                .map(|t| t.full_type_name())
                .unwrap_or_else(|| "Int".into());
            if ty_str.contains("Float") {
                self.class_field_types
                    .insert(p.name.clone(), "Float".into());
            }
            params.push((p.name.clone(), ty_str, p_val));
            let refn = p.type_node.as_ref().and_then(|t| {
                if let Some(r) = &t.refinement {
                    Some(r.clone())
                } else if let Some(td) = self.resolver.type_aliases.get(&t.name) {
                    td.base_type.refinement.clone()
                } else {
                    None
                }
            });
            param_refinements.push((p.name.clone(), refn));
            self.symbol_values.insert(p.name.clone(), p_val);
        }

        for req in &f.requires {
            if is_contract_statically_true(&req.condition, &param_refinements) {
                continue;
            }
            if let Some(cond_val) = self.lower_expr(&req.condition, &mut entry_id) {
                let msg_val = self.next_val();
                let user_msg = req.message.as_deref().unwrap_or("Precondition failed");
                let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                    ""
                } else {
                    "CONTRACT_VIOLATION: "
                };
                let msg_str = format!("{}{}", prefix, user_msg);
                self.get_block_mut(entry_id)
                    .instructions
                    .push(Inst::ConstStr {
                        dest: msg_val,
                        value: msg_str.into(),
                    });
                let dest = self.next_val();
                self.get_block_mut(entry_id).instructions.push(Inst::Call {
                    dest,
                    func: "datara_rt_assert".into(),
                    args: vec![cond_val, msg_val],
                    ty: "Unit".into(),
                });
            }
        }

        let (mut cur_block, ret_val) = self.lower_stmt_cfg(&f.body, entry_id);

        if !f.ensures.is_empty() {
            if let Some(rv) = ret_val {
                self.symbol_values.insert("result".into(), rv);
            }
            for ens in &f.ensures {
                if is_contract_statically_true(&ens.condition, &[]) {
                    continue;
                }
                if let Some(cond_val) = self.lower_expr(&ens.condition, &mut cur_block) {
                    let msg_val = self.next_val();
                    let user_msg = ens.message.as_deref().unwrap_or("Postcondition failed");
                    let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                        ""
                    } else {
                        "CONTRACT_VIOLATION: "
                    };
                    let msg_str = format!("{}{}", prefix, user_msg);
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::ConstStr {
                            dest: msg_val,
                            value: msg_str.into(),
                        });
                    let dest = self.next_val();
                    self.get_block_mut(cur_block).instructions.push(Inst::Call {
                        dest,
                        func: "datara_rt_assert".into(),
                        args: vec![cond_val, msg_val],
                        ty: "Unit".into(),
                    });
                }
            }
            self.symbol_values.remove("result");
        }
        self.symbol_values.clear();

        let cur = self.get_block_mut(cur_block);
        if matches!(
            cur.terminator,
            Terminator::Unreachable | Terminator::Return { value: None }
        ) {
            cur.terminator = Terminator::Return { value: ret_val };
        }
        if !cur
            .instructions
            .iter()
            .any(|i| matches!(i, Inst::Return { .. }))
            && ret_val.is_some()
        {
            cur.instructions.push(Inst::Return { value: ret_val });
        }

        Function {
            name: f.name.clone(),
            params,
            param_refinements,
            requires: f.requires.clone(),
            return_type: f
                .return_type
                .as_ref()
                .map(Self::repr_type_string)
                .unwrap_or_else(|| "Unit".into()),
            entry_block: entry_id,
            blocks: self.current_blocks.clone(),
        }
    }

    pub(crate) fn lower_class(&mut self, c: &ClassDecl, program: &Program, module: &mut Module) {
        for item in &c.body_items {
            if let ClassItem::Method(m) = item {
                let lowered = self.lower_method(m, &c.name);
                let fn_name = format!("{}_{}", c.name, m.name);
                module.functions.insert(fn_name.clone(), lowered);
                module
                    .function_spans
                    .insert(fn_name.clone(), m.span.clone());
                module
                    .function_line_spans
                    .insert(fn_name, self.current_line_spans.clone());
            }
        }
        for item in &c.body_items {
            if let ClassItem::Using(other_name, _) = item {
                for decl in &program.declarations {
                    if let Decl::Class(oc) = decl
                        && oc.name == *other_name
                    {
                        for o_item in &oc.body_items {
                            if let ClassItem::Method(m) = o_item
                                    && !c.body_items.iter().any(|it| matches!(it, ClassItem::Method(my_m) if my_m.name == m.name)) {
                                        let lowered = self.lower_method(m, &c.name);
                                        let fn_name = format!("{}_{}", c.name, m.name);
                                        module.functions.insert(fn_name.clone(), lowered);
                                        module.function_spans.insert(fn_name.clone(), m.span.clone());
                                        module.function_line_spans.insert(fn_name, self.current_line_spans.clone());
                                    }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn lower_behavior(&mut self, b: &BehaviorDecl, module: &mut Module) {
        for item in &b.body_items {
            if let ClassItem::Method(m) = item {
                let lowered = self.lower_method(m, &b.target_type);
                let fn_name = format!("{}_{}", b.target_type, m.name);
                module.functions.insert(fn_name.clone(), lowered);
                module
                    .function_spans
                    .insert(fn_name.clone(), m.span.clone());
                module
                    .function_line_spans
                    .insert(fn_name, self.current_line_spans.clone());
            }
        }
    }

    pub(crate) fn lower_impl(&mut self, i: &ImplBlock, module: &mut Module) {
        for m in &i.methods {
            let lowered = self.lower_impl_function(m, &i.target_type);
            let fn_name = format!("{}_{}", i.target_type, m.name);
            module.functions.insert(fn_name.clone(), lowered.clone());
            module
                .function_spans
                .insert(fn_name.clone(), m.span.clone());
            module
                .function_line_spans
                .insert(fn_name.clone(), self.current_line_spans.clone());
            if !module.functions.contains_key(&m.name) {
                module.functions.insert(m.name.clone(), lowered);
                module.function_spans.insert(m.name.clone(), m.span.clone());
                module
                    .function_line_spans
                    .insert(m.name.clone(), self.current_line_spans.clone());
            }
        }
    }

    pub(crate) fn lower_impl_function(&mut self, f: &FunctionDecl, target_type: &str) -> Function {
        self.current_fn_name = format!("{}_{}", target_type, f.name);
        self.local_var_types.clear();
        self.symbol_values.clear();
        self.current_blocks.clear();
        self.block_counter = 0;
        self.current_line_spans.clear();

        let mut entry_id = self.create_block("entry");

        let mut params = Vec::new();
        let mut param_refinements = Vec::new();

        let has_self = f
            .params
            .first()
            .map(|p| {
                p.name == "self" || p.name == "&self" || p.name == "mut self" || p.name == "this"
            })
            .unwrap_or(false);

        if !has_self {
            let this_val = self.next_val();
            params.push(("this".to_string(), target_type.to_string(), this_val));
            param_refinements.push(("this".to_string(), None));
            self.symbol_values.insert("this".to_string(), this_val);
            self.symbol_values.insert("self".to_string(), this_val);
        }

        for p in &f.params {
            let p_val = self.next_val();
            let is_self_param =
                p.name == "self" || p.name == "&self" || p.name == "mut self" || p.name == "this";
            let ty_str = if is_self_param {
                target_type.to_string()
            } else {
                p.type_node
                    .as_ref()
                    .map(|t| t.full_type_name())
                    .unwrap_or_else(|| "Int".into())
            };
            if ty_str.contains("Float") {
                self.class_field_types
                    .insert(p.name.clone(), "Float".into());
            }
            params.push((p.name.clone(), ty_str, p_val));
            let refn = p.type_node.as_ref().and_then(|t| {
                if let Some(r) = &t.refinement {
                    Some(r.clone())
                } else if let Some(td) = self.resolver.type_aliases.get(&t.name) {
                    td.base_type.refinement.clone()
                } else {
                    None
                }
            });
            param_refinements.push((p.name.clone(), refn));
            self.symbol_values.insert(p.name.clone(), p_val);
            if is_self_param {
                self.symbol_values.insert("this".to_string(), p_val);
                self.symbol_values.insert("self".to_string(), p_val);
            }
        }

        for req in &f.requires {
            if is_contract_statically_true(&req.condition, &param_refinements) {
                continue;
            }
            if let Some(cond_val) = self.lower_expr(&req.condition, &mut entry_id) {
                let msg_val = self.next_val();
                let user_msg = req.message.as_deref().unwrap_or("Precondition failed");
                let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                    ""
                } else {
                    "CONTRACT_VIOLATION: "
                };
                let msg_str = format!("{}{}", prefix, user_msg);
                self.get_block_mut(entry_id)
                    .instructions
                    .push(Inst::ConstStr {
                        dest: msg_val,
                        value: msg_str.into(),
                    });
                let dest = self.next_val();
                self.get_block_mut(entry_id).instructions.push(Inst::Call {
                    dest,
                    func: "datara_rt_assert".into(),
                    args: vec![cond_val, msg_val],
                    ty: "Unit".into(),
                });
            }
        }

        let (mut cur_block, ret_val) = self.lower_stmt_cfg(&f.body, entry_id);

        if !f.ensures.is_empty() {
            if let Some(rv) = ret_val {
                self.symbol_values.insert("result".into(), rv);
            }
            for ens in &f.ensures {
                if is_contract_statically_true(&ens.condition, &[]) {
                    continue;
                }
                if let Some(cond_val) = self.lower_expr(&ens.condition, &mut cur_block) {
                    let msg_val = self.next_val();
                    let user_msg = ens.message.as_deref().unwrap_or("Postcondition failed");
                    let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                        ""
                    } else {
                        "CONTRACT_VIOLATION: "
                    };
                    let msg_str = format!("{}{}", prefix, user_msg);
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::ConstStr {
                            dest: msg_val,
                            value: msg_str.into(),
                        });
                    let dest = self.next_val();
                    self.get_block_mut(cur_block).instructions.push(Inst::Call {
                        dest,
                        func: "datara_rt_assert".into(),
                        args: vec![cond_val, msg_val],
                        ty: "Unit".into(),
                    });
                }
            }
            self.symbol_values.remove("result");
        }
        self.symbol_values.clear();

        let cur = self.get_block_mut(cur_block);
        if matches!(
            cur.terminator,
            Terminator::Unreachable | Terminator::Return { value: None }
        ) {
            cur.terminator = Terminator::Return { value: ret_val };
        }

        let return_type = f
            .return_type
            .as_ref()
            .map(Self::repr_type_string)
            .unwrap_or_else(|| "Unit".into());

        Function {
            name: format!("{}_{}", target_type, f.name),
            params,
            param_refinements,
            requires: f.requires.clone(),
            return_type,
            entry_block: entry_id,
            blocks: self.current_blocks.clone(),
        }
    }

    pub(crate) fn specialize_function_decl(
        &self,
        f: &FunctionDecl,
        mangled_name: &str,
        type_substs: &HashMap<String, String>,
    ) -> FunctionDecl {
        let mut cloned = f.clone();
        cloned.name = mangled_name.to_string();
        cloned.generic_params.clear();
        cloned.generic_constraints.clear();

        for p in &mut cloned.params {
            if let Some(tn) = &mut p.type_node {
                Self::subst_type_node(tn, type_substs);
            }
        }

        if let Some(rt) = &mut cloned.return_type {
            Self::subst_type_node(rt, type_substs);
        }

        Self::subst_stmt(&mut cloned.body, type_substs);
        cloned
    }

    pub(crate) fn subst_type_node(tn: &mut TypeNode, type_substs: &HashMap<String, String>) {
        if let Some(concrete) = type_substs.get(&tn.name) {
            tn.name = concrete.clone();
        }
        for arg in &mut tn.generic_args {
            Self::subst_type_node(arg, type_substs);
        }
    }

    pub(crate) fn subst_stmt(stmt: &mut Stmt, type_substs: &HashMap<String, String>) {
        match stmt {
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    Self::subst_stmt(s, type_substs);
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
                    Self::subst_type_node(tn, type_substs);
                }
                Self::subst_expr(init, type_substs);
            }
            Stmt::CompactBind { init, .. } => {
                Self::subst_expr(init, type_substs);
            }
            Stmt::Assign { value, .. } => {
                Self::subst_expr(value, type_substs);
            }
            Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
                Self::subst_expr(e, type_substs);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::subst_expr(condition, type_substs);
                Self::subst_stmt(then_branch, type_substs);
                if let Some(eb) = else_branch {
                    Self::subst_stmt(eb, type_substs);
                }
            }
            Stmt::For { iterable, body, .. } | Stmt::ParallelFor { iterable, body, .. } => {
                Self::subst_expr(iterable, type_substs);
                Self::subst_stmt(body, type_substs);
            }
            Stmt::While {
                condition, body, ..
            } => {
                Self::subst_expr(condition, type_substs);
                Self::subst_stmt(body, type_substs);
            }
            Stmt::Loop { body, .. } => {
                Self::subst_stmt(body, type_substs);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::subst_stmt(try_block, type_substs);
                Self::subst_stmt(catch_block, type_substs);
            }
            Stmt::Parallel(s, _) | Stmt::Unsafe { body: s, .. } => {
                Self::subst_stmt(s, type_substs);
            }
            Stmt::With { init, body, .. } => {
                Self::subst_expr(init, type_substs);
                Self::subst_stmt(body, type_substs);
            }
            Stmt::Return(Some(e), _) => {
                Self::subst_expr(e, type_substs);
            }
            _ => {}
        }
    }

    pub(crate) fn subst_expr(expr: &mut Expr, type_substs: &HashMap<String, String>) {
        match expr {
            Expr::Binary { left, right, .. } => {
                Self::subst_expr(left, type_substs);
                Self::subst_expr(right, type_substs);
            }
            Expr::Unary { expr, .. } => {
                Self::subst_expr(expr, type_substs);
            }
            Expr::Call { callee, args, .. } => {
                Self::subst_expr(callee, type_substs);
                for a in args {
                    Self::subst_expr(a, type_substs);
                }
            }
            Expr::MemberAccess { object, .. } => {
                Self::subst_expr(object, type_substs);
            }
            Expr::IndexAccess { object, index, .. } => {
                Self::subst_expr(object, type_substs);
                Self::subst_expr(index, type_substs);
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    Self::subst_expr(e, type_substs);
                }
            }
            Expr::ObjectInit {
                generic_args,
                fields,
                ..
            } => {
                for ga in generic_args {
                    Self::subst_type_node(ga, type_substs);
                }
                for (_, f_expr) in fields {
                    Self::subst_expr(f_expr, type_substs);
                }
            }
            Expr::ListLiteral(elements, _) => {
                for e in elements {
                    Self::subst_expr(e, type_substs);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    Self::subst_expr(k, type_substs);
                    Self::subst_expr(v, type_substs);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn lower_method(&mut self, m: &MethodDecl, class_name: &str) -> Function {
        self.current_fn_name = format!("{}_{}", class_name, m.name);
        self.local_var_types.clear();
        self.symbol_values.clear();
        self.current_blocks.clear();
        self.block_counter = 0;
        self.current_line_spans.clear();

        let mut entry_id = self.create_block("entry");

        let this_val = self.next_val();
        let mut params = vec![("this".to_string(), class_name.to_string(), this_val)];
        let mut param_refinements = vec![("this".to_string(), None)];
        self.symbol_values.insert("this".to_string(), this_val);
        self.symbol_values.insert("self".to_string(), this_val);

        for p in &m.params {
            let p_val = self.next_val();
            let ty_str = p
                .type_node
                .as_ref()
                .map(|t| t.full_type_name())
                .unwrap_or_else(|| "Int".into());
            if ty_str.contains("Float") {
                self.class_field_types
                    .insert(p.name.clone(), "Float".into());
            }
            params.push((p.name.clone(), ty_str, p_val));
            let refn = p.type_node.as_ref().and_then(|t| {
                if let Some(r) = &t.refinement {
                    Some(r.clone())
                } else if let Some(td) = self.resolver.type_aliases.get(&t.name) {
                    td.base_type.refinement.clone()
                } else {
                    None
                }
            });
            param_refinements.push((p.name.clone(), refn));
            self.symbol_values.insert(p.name.clone(), p_val);
        }

        for req in &m.requires {
            if is_contract_statically_true(&req.condition, &param_refinements) {
                continue;
            }
            if let Some(cond_val) = self.lower_expr(&req.condition, &mut entry_id) {
                let msg_val = self.next_val();
                let user_msg = req.message.as_deref().unwrap_or("Precondition failed");
                let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                    ""
                } else {
                    "CONTRACT_VIOLATION: "
                };
                let msg_str = format!("{}{}", prefix, user_msg);
                self.get_block_mut(entry_id)
                    .instructions
                    .push(Inst::ConstStr {
                        dest: msg_val,
                        value: msg_str.into(),
                    });
                let dest = self.next_val();
                self.get_block_mut(entry_id).instructions.push(Inst::Call {
                    dest,
                    func: "datara_rt_assert".into(),
                    args: vec![cond_val, msg_val],
                    ty: "Unit".into(),
                });
            }
        }

        let (mut cur_block, ret_val) = if let Some(body) = &m.body {
            self.lower_stmt_cfg(body, entry_id)
        } else {
            (entry_id, None)
        };

        if !m.ensures.is_empty() {
            if let Some(rv) = ret_val {
                self.symbol_values.insert("result".into(), rv);
            }
            for ens in &m.ensures {
                if is_contract_statically_true(&ens.condition, &[]) {
                    continue;
                }
                if let Some(cond_val) = self.lower_expr(&ens.condition, &mut cur_block) {
                    let msg_val = self.next_val();
                    let user_msg = ens.message.as_deref().unwrap_or("Postcondition failed");
                    let prefix = if user_msg.starts_with("CONTRACT_VIOLATION") {
                        ""
                    } else {
                        "CONTRACT_VIOLATION: "
                    };
                    let msg_str = format!("{}{}", prefix, user_msg);
                    self.get_block_mut(cur_block)
                        .instructions
                        .push(Inst::ConstStr {
                            dest: msg_val,
                            value: msg_str.into(),
                        });
                    let dest = self.next_val();
                    self.get_block_mut(cur_block).instructions.push(Inst::Call {
                        dest,
                        func: "datara_rt_assert".into(),
                        args: vec![cond_val, msg_val],
                        ty: "Unit".into(),
                    });
                }
            }
            self.symbol_values.remove("result");
        }
        self.symbol_values.clear();

        let cur = self.get_block_mut(cur_block);
        if matches!(
            cur.terminator,
            Terminator::Unreachable | Terminator::Return { value: None }
        ) {
            cur.terminator = Terminator::Return { value: ret_val };
        }
        if !cur
            .instructions
            .iter()
            .any(|i| matches!(i, Inst::Return { .. }))
            && ret_val.is_some()
        {
            cur.instructions.push(Inst::Return { value: ret_val });
        }

        Function {
            name: format!("{}_{}", class_name, m.name),
            params,
            param_refinements,
            requires: m.requires.clone(),
            return_type: m
                .return_type
                .as_ref()
                .map(Self::repr_type_string)
                .unwrap_or_else(|| "Unit".into()),
            entry_block: entry_id,
            blocks: self.current_blocks.clone(),
        }
    }
}
