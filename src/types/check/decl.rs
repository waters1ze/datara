use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::types::{DataraType, MutabilityKind, TypeChecker};

impl<'a> TypeChecker<'a> {
    pub fn check_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        // Pre-register enum variants and FFI `use` imports so they resolve
        // regardless of declaration order. The resolver types FFI imports as
        // dynamic `Val` symbols.
        for decl in &program.declarations {
            match decl {
                Decl::Enum(e) => {
                    let enum_type = DataraType::Class(e.name.clone());
                    for v in &e.variants {
                        self.symbol_types.insert(v.name.clone(), enum_type.clone());
                        self.symbol_types
                            .insert(format!("{}.{}", e.name, v.name), enum_type.clone());
                    }
                }
                Decl::Use(u) => {
                    let first_seg = u.path.first().map(|s| s.as_str());
                    if matches!(
                        first_seg,
                        Some("python" | "rust" | "c" | "cpp" | "cxx" | "npm" | "js" | "ts")
                    ) {
                        let alias = u
                            .alias
                            .clone()
                            .unwrap_or_else(|| u.path.last().cloned().unwrap_or_default());
                        if !alias.is_empty() {
                            self.symbol_types.entry(alias).or_insert(DataraType::Val);
                        }
                    }
                }
                Decl::Trait(t) => {
                    self.traits.insert(t.name.clone(), t.clone());
                }
                Decl::Role(r) => {
                    let trait_def = crate::ast::TraitDef {
                        name: r.name.clone(),
                        generic_params: Vec::new(),
                        super_traits: Vec::new(),
                        methods: r
                            .methods
                            .iter()
                            .map(|m| crate::ast::TraitMethodSignature {
                                name: m.name.clone(),
                                generic_params: m.generic_params.clone(),
                                params: m.params.clone(),
                                return_type: m.return_type.clone(),
                                default_body: m.body.clone(),
                                span: m.span.clone(),
                            })
                            .collect(),
                        is_export: r.is_export,
                        span: r.span.clone(),
                    };
                    self.traits.insert(r.name.clone(), trait_def);
                }
                Decl::Impl(i) => {
                    if let Some(tr) = &i.trait_name {
                        self.impls
                            .insert((tr.clone(), i.target_type.clone()), i.clone());
                    }
                }
                _ => {}
            }
        }

        // Collect function signatures first
        for decl in &program.declarations {
            if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl {
                let p_types: Vec<DataraType> = f
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Int)
                    })
                    .collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                let gen_params: Vec<String> = f.generic_params.clone();
                self.function_signatures
                    .insert(f.name.clone(), (p_types, ret, gen_params));
                let p_nodes: Vec<Option<TypeNode>> =
                    f.params.iter().map(|p| p.type_node.clone()).collect();
                self.function_param_nodes.insert(f.name.clone(), p_nodes);
                for (param, bound) in &f.generic_constraints {
                    self.trait_bounds
                        .entry(param.clone())
                        .or_default()
                        .push(bound.clone());
                    self.trait_bounds
                        .entry(format!("{}:{}", f.name, param))
                        .or_default()
                        .push(bound.clone());
                }
            } else if let Decl::ExternFn(ef) = decl {
                let p_types: Vec<DataraType> = ef
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Int)
                    })
                    .collect();
                let ret = ef
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                self.function_signatures
                    .insert(ef.name.clone(), (p_types, ret, Vec::new()));
            } else if let Decl::Impl(i) = decl {
                self.current_target_type = Some(i.target_type.clone());
                for m in &i.methods {
                    let p_types: Vec<DataraType> = m
                        .params
                        .iter()
                        .map(|p| {
                            if p.name == "self"
                                || p.name == "&self"
                                || p.name == "mut self"
                                || p.name == "this"
                            {
                                DataraType::Class(i.target_type.clone())
                            } else {
                                p.type_node
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Int)
                            }
                        })
                        .collect();
                    let ret = m
                        .return_type
                        .as_ref()
                        .map(|t| self.resolve_type_node(t, diag))
                        .unwrap_or(DataraType::Unit);
                    let m_fn_name = format!("{}_{}", i.target_type, m.name);
                    self.function_signatures.insert(
                        m_fn_name.clone(),
                        (p_types.clone(), ret.clone(), m.generic_params.clone()),
                    );
                    self.class_methods
                        .entry(i.target_type.clone())
                        .or_default()
                        .insert(m.name.clone(), ret.clone());
                    self.function_signatures.entry(m.name.clone()).or_insert((
                        p_types,
                        ret,
                        m.generic_params.clone(),
                    ));
                }

                if let Some(ref tr_name) = i.trait_name {
                    if let Some(t_def) = self.traits.get(tr_name).cloned() {
                        for tm in &t_def.methods {
                            if tm.default_body.is_some()
                                && !i.methods.iter().any(|m| m.name == tm.name)
                            {
                                let p_types: Vec<DataraType> = tm
                                    .params
                                    .iter()
                                    .map(|p| {
                                        if p.name == "self"
                                            || p.name == "&self"
                                            || p.name == "mut self"
                                            || p.name == "this"
                                        {
                                            DataraType::Class(i.target_type.clone())
                                        } else {
                                            p.type_node
                                                .as_ref()
                                                .map(|t| self.resolve_type_node(t, diag))
                                                .unwrap_or(DataraType::Int)
                                        }
                                    })
                                    .collect();
                                let ret = tm
                                    .return_type
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Unit);
                                let m_fn_name = format!("{}_{}", i.target_type, tm.name);
                                self.function_signatures.insert(
                                    m_fn_name.clone(),
                                    (p_types.clone(), ret.clone(), tm.generic_params.clone()),
                                );
                                self.class_methods
                                    .entry(i.target_type.clone())
                                    .or_default()
                                    .insert(tm.name.clone(), ret.clone());
                                self.function_signatures.entry(tm.name.clone()).or_insert((
                                    p_types,
                                    ret,
                                    tm.generic_params.clone(),
                                ));
                            }
                        }
                    }
                }
                self.current_target_type = None;
            }
        }

        for decl in &program.declarations {
            self.check_decl(decl, diag);
        }
    }

    fn check_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Type(td) => {
                let _ = self.resolve_type_node(&td.base_type, diag);
            }
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                let saved_types = self.symbol_types.clone();
                let saved_mut = self.symbol_mutability.clone();
                let saved_elem = self.var_element_types.clone();
                let saved_refinements = self.var_refinements.clone();
                let saved_lengths = self.var_array_lengths.clone();

                self.current_fn_name = Some(f.name.clone());
                for p in &f.params {
                    let p_type = p
                        .type_node
                        .as_ref()
                        .map(|t| self.resolve_type_node(t, diag))
                        .unwrap_or(DataraType::Int);
                    self.symbol_types.insert(p.name.clone(), p_type.clone());
                    self.symbol_mutability
                        .insert(p.name.clone(), MutabilityKind::Immutable);
                    self.fn_symbol_types
                        .insert((f.name.clone(), p.name.clone()), p_type.erasure());
                    if let Some(tn) = &p.type_node {
                        self.var_refinements.insert(p.name.clone(), tn.clone());
                    }
                }
                if let Some(rt) = &f.return_type {
                    self.validate_error_channels(rt, diag);
                }
                let expected = f
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type_node(t, diag))
                    .unwrap_or(DataraType::Unit);
                self.current_return_type = Some(expected.clone());

                for req in &f.requires {
                    self.check_expr(&req.condition, diag);
                }

                self.symbol_types.insert("result".into(), expected.clone());
                for ens in &f.ensures {
                    self.check_expr(&ens.condition, diag);
                }
                self.symbol_types.remove("result");

                let body_type = self.check_stmt(&f.body, diag);
                self.current_return_type = None;
                self.current_fn_name = None;

                self.symbol_types = saved_types;
                self.symbol_mutability = saved_mut;
                self.var_element_types = saved_elem;
                self.var_refinements = saved_refinements;
                self.var_array_lengths = saved_lengths;

                if f.is_expression_body {
                    if !body_type.is_compatible_with_args(&expected, Some(self.resolver))
                        && expected != DataraType::Unit
                        && !matches!(expected, DataraType::TypeParam(_))
                    {
                        let help_msg = Self::suggest_type_fix(&expected, &body_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch: expected '{}', got '{}'",
                                expected, body_type
                            ),
                            Some(f.span.clone()),
                            help_msg,
                        );
                    }
                } else {
                    self.check_return_exhaustiveness(&f.name, &f.body, &expected, &f.span, diag);
                }
            }
            Decl::Class(c) => {
                self.current_target_type = Some(c.name.clone());
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let saved_types = self.symbol_types.clone();
                        let saved_mut = self.symbol_mutability.clone();
                        let saved_elem = self.var_element_types.clone();
                        let saved_refinements = self.var_refinements.clone();
                        let saved_lengths = self.var_array_lengths.clone();

                        let m_fn_name = format!("{}_{}", c.name, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());

                        for field_item in &c.body_items {
                            if let ClassItem::Field(f) = field_item {
                                let f_type = f
                                    .type_node
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Int);
                                let f_mut = if f.is_mut {
                                    MutabilityKind::MutableFixed
                                } else {
                                    MutabilityKind::Immutable
                                };
                                self.symbol_types.insert(f.name.clone(), f_type.clone());
                                self.symbol_mutability.insert(f.name.clone(), f_mut);
                                self.fn_symbol_types
                                    .insert((m_fn_name.clone(), f.name.clone()), f_type);
                            }
                        }

                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(c.name.clone()));
                        self.symbol_mutability
                            .insert("this".to_string(), MutabilityKind::Immutable);
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(c.name.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(|t| self.resolve_type_node(t, diag))
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.symbol_mutability
                                .insert(p.name.clone(), MutabilityKind::Immutable);
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                            if let Some(tn) = &p.type_node {
                                self.var_refinements.insert(p.name.clone(), tn.clone());
                            }
                        }
                        if let Some(rt) = &m.return_type {
                            self.validate_error_channels(rt, diag);
                        }
                        let expected = m
                            .return_type
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Unit);
                        self.current_return_type = Some(expected.clone());

                        for req in &m.requires {
                            self.check_expr(&req.condition, diag);
                        }

                        self.symbol_types.insert("result".into(), expected.clone());
                        for ens in &m.ensures {
                            self.check_expr(&ens.condition, diag);
                        }
                        self.symbol_types.remove("result");

                        let body_type = self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;

                        self.symbol_types = saved_types;
                        self.symbol_mutability = saved_mut;
                        self.var_element_types = saved_elem;
                        self.var_refinements = saved_refinements;
                        self.var_array_lengths = saved_lengths;

                        if m.is_expression_body {
                            if !body_type.is_compatible_with_args(&expected, Some(self.resolver))
                                && expected != DataraType::Unit
                                && !matches!(expected, DataraType::TypeParam(_))
                            {
                                let help_msg = Self::suggest_type_fix(&expected, &body_type);
                                diag.error_with_help(
                                    ErrorCode::TypeMismatch,
                                    format!(
                                        "Type mismatch: expected '{}', got '{}'",
                                        expected, body_type
                                    ),
                                    Some(m.span.clone()),
                                    help_msg,
                                );
                            }
                        } else {
                            self.check_return_exhaustiveness(
                                &m.name, body, &expected, &m.span, diag,
                            );
                        }
                    }
                }
                self.current_target_type = None;
            }
            Decl::Enum(e) => {
                let enum_type = DataraType::Class(e.name.clone());
                for v in &e.variants {
                    self.symbol_types.insert(v.name.clone(), enum_type.clone());
                    self.symbol_types
                        .insert(format!("{}.{}", e.name, v.name), enum_type.clone());
                }
            }
            Decl::Behavior(b) => {
                self.current_target_type = Some(b.target_type.clone());
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let saved_types = self.symbol_types.clone();
                        let saved_mut = self.symbol_mutability.clone();
                        let saved_elem = self.var_element_types.clone();
                        let saved_refinements = self.var_refinements.clone();
                        let saved_lengths = self.var_array_lengths.clone();

                        let m_fn_name = format!("{}_{}", b.target_type, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());

                        if let Some(cls_sym) = self.resolver.classes.get(&b.target_type).cloned() {
                            for (f_name, f_sym) in &cls_sym.fields {
                                let f_type = f_sym
                                    .type_node
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Int);
                                let f_mut = if f_sym.is_mut {
                                    MutabilityKind::MutableFixed
                                } else {
                                    MutabilityKind::Immutable
                                };
                                self.symbol_types.insert(f_name.clone(), f_type.clone());
                                self.symbol_mutability.insert(f_name.clone(), f_mut);
                                self.fn_symbol_types
                                    .insert((m_fn_name.clone(), f_name.clone()), f_type);
                            }
                        }

                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(b.target_type.clone()));
                        self.symbol_mutability
                            .insert("this".to_string(), MutabilityKind::Immutable);
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(b.target_type.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(|t| self.resolve_type_node(t, diag))
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.symbol_mutability
                                .insert(p.name.clone(), MutabilityKind::Immutable);
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                            if let Some(tn) = &p.type_node {
                                self.var_refinements.insert(p.name.clone(), tn.clone());
                            }
                        }
                        if let Some(rt) = &m.return_type {
                            self.validate_error_channels(rt, diag);
                        }
                        let expected = m
                            .return_type
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Unit);
                        self.current_return_type = Some(expected.clone());

                        for req in &m.requires {
                            self.check_expr(&req.condition, diag);
                        }

                        self.symbol_types.insert("result".into(), expected.clone());
                        for ens in &m.ensures {
                            self.check_expr(&ens.condition, diag);
                        }
                        self.symbol_types.remove("result");

                        let body_type = self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;

                        self.symbol_types = saved_types;
                        self.symbol_mutability = saved_mut;
                        self.var_element_types = saved_elem;
                        self.var_refinements = saved_refinements;
                        self.var_array_lengths = saved_lengths;

                        if m.is_expression_body
                            && !body_type.is_compatible_with_args(&expected, Some(self.resolver))
                            && expected != DataraType::Unit
                            && !matches!(expected, DataraType::TypeParam(_))
                        {
                            let help_msg = Self::suggest_type_fix(&expected, &body_type);
                            diag.error_with_help(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Type mismatch: expected '{}', got '{}'",
                                    expected, body_type
                                ),
                                Some(m.span.clone()),
                                help_msg,
                            );
                        }
                    }
                }
                self.current_target_type = None;
            }
            Decl::Trait(t) => {
                for st in &t.super_traits {
                    if !self.traits.contains_key(st) {
                        diag.error(
                            ErrorCode::ResolveUnknownType,
                            format!("Super-trait '{}' not found for trait '{}'", st, t.name),
                            Some(t.span.clone()),
                        );
                    }
                }
            }
            Decl::Impl(i) => {
                self.current_target_type = Some(i.target_type.clone());
                if let Some(ref tr_name) = i.trait_name {
                    if let Some(trait_def) = self.traits.get(tr_name).cloned() {
                        if !trait_def.is_export
                            && !trait_def.span.file.is_empty()
                            && !i.span.file.is_empty()
                            && !crate::diagnostics::is_same_file_or_module(
                                &trait_def.span.file,
                                &i.span.file,
                            )
                            && !trait_def.span.file.contains("stdlib")
                        {
                            diag.error(
                                ErrorCode::PrivateItemAccess,
                                format!(
                                    "Cannot access private trait '{}' declared in '{}'",
                                    tr_name, trait_def.span.file
                                ),
                                Some(i.span.clone()),
                            );
                        }
                        for st in &trait_def.super_traits {
                            if !self
                                .impls
                                .contains_key(&(st.clone(), i.target_type.clone()))
                            {
                                diag.error(
                                    ErrorCode::TypeMismatch,
                                    format!(
                                        "Type '{}' implements '{}' but does not implement super-trait '{}'",
                                        i.target_type, tr_name, st
                                    ),
                                    Some(i.span.clone()),
                                );
                            }
                        }

                        for tm in &trait_def.methods {
                            if let Some(m) = i.methods.iter().find(|m| m.name == tm.name) {
                                let expected_ret = tm
                                    .return_type
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Unit);
                                let actual_ret = m
                                    .return_type
                                    .as_ref()
                                    .map(|t| self.resolve_type_node(t, diag))
                                    .unwrap_or(DataraType::Unit);
                                if !actual_ret.is_compatible_with_refined_with_args(
                                    &expected_ret,
                                    Some(self.resolver),
                                ) {
                                    diag.error(
                                        ErrorCode::TypeMismatch,
                                        format!(
                                            "Method '{}' in impl of trait '{}' has return type '{}', expected '{}'",
                                            m.name, tr_name, actual_ret, expected_ret
                                        ),
                                        Some(m.span.clone()),
                                    );
                                }
                            } else if tm.default_body.is_none() {
                                diag.error(
                                    ErrorCode::TypeMismatch,
                                    format!(
                                        "Missing implementation of trait method '{}' for type '{}'",
                                        tm.name, i.target_type
                                    ),
                                    Some(i.span.clone()),
                                );
                            }
                        }
                    } else {
                        diag.error(
                            ErrorCode::ResolveUnknownType,
                            format!("Trait '{}' is not defined", tr_name),
                            Some(i.span.clone()),
                        );
                    }
                }

                for m in &i.methods {
                    let saved_types = self.symbol_types.clone();
                    let saved_mut = self.symbol_mutability.clone();
                    let saved_elem = self.var_element_types.clone();
                    let saved_refinements = self.var_refinements.clone();
                    let saved_lengths = self.var_array_lengths.clone();

                    let m_fn_name = format!("{}_{}", i.target_type, m.name);
                    self.current_fn_name = Some(m_fn_name.clone());
                    self.symbol_types
                        .insert("this".to_string(), DataraType::Class(i.target_type.clone()));
                    self.symbol_types
                        .insert("self".to_string(), DataraType::Class(i.target_type.clone()));
                    self.symbol_mutability
                        .insert("this".to_string(), MutabilityKind::Immutable);
                    self.symbol_mutability
                        .insert("self".to_string(), MutabilityKind::Immutable);
                    self.fn_symbol_types.insert(
                        (m_fn_name.clone(), "this".to_string()),
                        DataraType::Class(i.target_type.clone()),
                    );
                    self.fn_symbol_types.insert(
                        (m_fn_name.clone(), "self".to_string()),
                        DataraType::Class(i.target_type.clone()),
                    );

                    for p in &m.params {
                        if p.name == "self"
                            || p.name == "&self"
                            || p.name == "mut self"
                            || p.name == "this"
                        {
                            continue;
                        }
                        let p_type = p
                            .type_node
                            .as_ref()
                            .map(|t| self.resolve_type_node(t, diag))
                            .unwrap_or(DataraType::Int);
                        self.symbol_types.insert(p.name.clone(), p_type.clone());
                        self.symbol_mutability
                            .insert(p.name.clone(), MutabilityKind::Immutable);
                        self.fn_symbol_types
                            .insert((m_fn_name.clone(), p.name.clone()), p_type);
                        if let Some(tn) = &p.type_node {
                            self.var_refinements.insert(p.name.clone(), tn.clone());
                        }
                    }

                    if let Some(rt) = &m.return_type {
                        self.validate_error_channels(rt, diag);
                    }
                    let expected = m
                        .return_type
                        .as_ref()
                        .map(|t| self.resolve_type_node(t, diag))
                        .unwrap_or(DataraType::Unit);
                    self.current_return_type = Some(expected.clone());

                    for req in &m.requires {
                        self.check_expr(&req.condition, diag);
                    }

                    self.symbol_types.insert("result".into(), expected.clone());
                    for ens in &m.ensures {
                        self.check_expr(&ens.condition, diag);
                    }
                    self.symbol_types.remove("result");

                    let body_type = self.check_stmt(&m.body, diag);
                    self.current_return_type = None;
                    self.current_fn_name = None;

                    self.symbol_types = saved_types;
                    self.symbol_mutability = saved_mut;
                    self.var_element_types = saved_elem;
                    self.var_refinements = saved_refinements;
                    self.var_array_lengths = saved_lengths;

                    if m.is_expression_body {
                        if !body_type.is_compatible_with_args(&expected, Some(self.resolver))
                            && expected != DataraType::Unit
                            && !matches!(expected, DataraType::TypeParam(_))
                        {
                            let help_msg = Self::suggest_type_fix(&expected, &body_type);
                            diag.error_with_help(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Type mismatch: expected '{}', got '{}'",
                                    expected, body_type
                                ),
                                Some(m.span.clone()),
                                help_msg,
                            );
                        }
                    } else {
                        self.check_return_exhaustiveness(
                            &m.name, &m.body, &expected, &m.span, diag,
                        );
                    }
                }
                self.current_target_type = None;
            }
            _ => {}
        }
    }
}
