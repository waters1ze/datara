use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
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

    pub fn record_var_type(&mut self, name: &str, ty: DataraType) {
        if let Some(ref fn_name) = self.current_fn_name {
            // DMIR/codegen only understand the erased class form
            // (`Class("List")`); keep the parametric type checker-internal.
            self.fn_symbol_types
                .insert((fn_name.clone(), name.to_string()), ty.erasure());
        }
        self.symbol_types.insert(name.to_string(), ty);
    }

    pub fn check_stmt(&mut self, stmt: &Stmt, diag: &mut DiagnosticEngine) -> DataraType {
        match stmt {
            Stmt::Block(stmts, _) => {
                // Lexical scope: declarations inside the block must not leak
                // into sibling blocks or the enclosing scope.
                let saved_types = self.symbol_types.clone();
                let saved_mut = self.symbol_mutability.clone();
                let saved_elem = self.var_element_types.clone();
                let saved_refinements = self.var_refinements.clone();
                let saved_lengths = self.var_array_lengths.clone();
                let mut last = DataraType::Unit;
                for s in stmts {
                    last = self.check_stmt(s, diag);
                }
                self.symbol_types = saved_types;
                self.symbol_mutability = saved_mut;
                self.var_element_types = saved_elem;
                self.var_refinements = saved_refinements;
                self.var_array_lengths = saved_lengths;
                last
            }
            Stmt::Let {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Immutable);
                final_ty
            }
            Stmt::Const {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Immutable);
                final_ty
            }
            Stmt::Mut {
                name,
                type_node,
                init,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::MutableFixed);
                final_ty
            }
            Stmt::Val {
                name,
                type_node,
                init,
                is_mut,
                span,
            } => {
                let init_type = self.check_expr(init, diag);
                let final_ty = if let Some(tn) = type_node {
                    self.var_refinements.insert(name.clone(), tn.clone());
                    self.check_refinement(tn, init, span, diag);
                    let declared = self.resolve_type_node(tn, diag);
                    let is_literal_numeric = matches!(
                        init,
                        Expr::Literal(LiteralValue::Float(_), _)
                            | Expr::Literal(LiteralValue::Int(_), _)
                    );
                    let compatible = if let DataraType::Measure { base, .. } = &declared {
                        (is_literal_numeric
                            && init_type.is_compatible_with_args(base, Some(self.resolver)))
                            || init_type.is_compatible_with_refined_with_args(
                                &declared,
                                Some(self.resolver),
                            )
                    } else {
                        init_type
                            .is_compatible_with_refined_with_args(&declared, Some(self.resolver))
                    };
                    if !compatible {
                        let help_msg = Self::suggest_type_fix(&declared, &init_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in variable declaration: expected '{}', got '{}'",
                                declared, init_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                    self.check_range_and_measure_assignment(
                        &declared, &init_type, init, span, diag,
                    );
                    declared
                } else if !*is_mut {
                    // Type promotion: immutable `val` promotes directly to its concrete scalar SSA register type
                    init_type
                } else {
                    DataraType::Val
                };
                self.record_var_type(name, final_ty.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Dynamic { is_mut: *is_mut });
                final_ty
            }
            Stmt::CompactBind { name, init, .. } => {
                let init_type = self.check_expr(init, diag);
                self.record_var_type(name, init_type.clone());
                if let Expr::ListLiteral(elements, _) = init {
                    self.var_array_lengths.insert(name.clone(), elements.len());
                    if let Some(e) = self.last_list_element.take() {
                        self.var_element_types.insert(name.clone(), e);
                    }
                } else if let Expr::ArrayRepeatLiteral { count, .. } = init {
                    self.var_array_lengths.insert(name.clone(), *count);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::MutableFixed);
                init_type
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let val_type = self.check_expr(value, diag);
                if let Expr::Identifier(name, _) = target {
                    if let Some(tn) = self.var_refinements.get(name).cloned() {
                        self.check_refinement(&tn, value, span, diag);
                    }
                    if let Some(mut_kind) = self.symbol_mutability.get(name) {
                        match mut_kind {
                            MutabilityKind::Immutable => {
                                diag.error_with_help(
                                    ErrorCode::BorrowCannotMutateImmutable,
                                    format!("Cannot assign twice to immutable variable '{}'", name),
                                    Some(span.clone()),
                                    Some(format!(
                                        "consider declaring '{}' as mutable: 'mut {} = ...'",
                                        name, name
                                    )),
                                );
                            }
                            MutabilityKind::Dynamic { is_mut: false } => {
                                diag.error_with_help(
                                    ErrorCode::BorrowCannotMutateImmutable,
                                    format!(
                                        "Cannot assign to immutable val '{}'",
                                        name
                                    ),
                                    Some(span.clone()),
                                    Some(format!("'val' constants cannot be reassigned; use 'mut val {}' or 'mut {}' if mutation is required", name, name)),
                                );
                            }
                            MutabilityKind::MutableFixed => {
                                if let Some(existing) = self.symbol_types.get(name).cloned() {
                                    self.check_range_and_measure_assignment(
                                        &existing, &val_type, value, span, diag,
                                    );
                                    if !val_type.is_compatible_with_refined_with_args(
                                        &existing,
                                        Some(self.resolver),
                                    ) {
                                        let help_msg = Self::suggest_type_fix(&existing, &val_type);
                                        diag.error_with_help(
                                            ErrorCode::TypeMismatch,
                                            format!(
                                                "Type mismatch in assignment to mutable variable '{}': expected '{}', got '{}'",
                                                name, existing, val_type
                                            ),
                                            Some(span.clone()),
                                            help_msg,
                                        );
                                    }
                                }
                            }
                            MutabilityKind::Dynamic { is_mut: true } => {
                                if let Some(existing) = self.symbol_types.get(name).cloned() {
                                    self.check_range_and_measure_assignment(
                                        &existing, &val_type, value, span, diag,
                                    );
                                }
                                // `mut val` bindings stay dynamically typed:
                                // do NOT re-type the symbol to the assigned
                                // value's concrete type, otherwise a concrete
                                // type would flow into checked contexts while
                                // the runtime still treats it as `Val`.
                            }
                        }
                    } else {
                        let mut candidates: Vec<&str> =
                            self.symbol_types.keys().map(|s| s.as_str()).collect();
                        candidates.sort_unstable();
                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(name, candidates)
                        {
                            format!(
                                "a variable with a similar name exists: '{}'. Or declare with 'let {} = ...' / 'mut {} = ...'",
                                similar, name, name
                            )
                        } else {
                            format!(
                                "declare '{}' with 'let' (immutable) or 'mut' (mutable) before assigning to it",
                                name
                            )
                        };
                        diag.error_with_help(
                            ErrorCode::ResolveUndefinedSymbol,
                            format!("Assignment to undeclared variable '{}'", name),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                } else {
                    let tgt_type = self.check_expr(target, diag);
                    if !val_type.is_compatible_with_args(&tgt_type, Some(self.resolver)) {
                        let help_msg = Self::suggest_type_fix(&tgt_type, &val_type);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in assignment: expected '{}', got '{}'",
                                tgt_type, val_type
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                }
                val_type
            }
            Stmt::Expr(e, _) => self.check_expr(e, diag),
            Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.check_expr(e, diag);
                DataraType::Unit
            }
            Stmt::Return(opt_e, span) => {
                let t = if let Some(e) = opt_e {
                    self.check_expr(e, diag)
                } else {
                    DataraType::Unit
                };
                // When the enclosing signature is Result/Option-like, the
                // returned value must be the same kind with a compatible
                // payload — there is no implicit wrapping or coercion.
                if let Some(expected) = self.current_return_type.clone() {
                    if expected != DataraType::Unit && opt_e.is_none() {
                        diag.error_with_help(
                            ErrorCode::TypeMissingReturn,
                            format!(
                                "Return statement has no value, but function declares return type '{}'",
                                expected
                            ),
                            Some(span.clone()),
                            Some(format!("return an expression of type '{}'", expected)),
                        );
                    }
                    let enc_res = expected.result_like();
                    let enc_opt = expected.option_like();
                    if enc_res.is_some() || enc_opt.is_some() {
                        let matches = if let Some((eok, eerr)) = &enc_res {
                            match t.result_like() {
                                Some((tok, terr)) => {
                                    tok.is_compatible_with_args(eok, Some(self.resolver))
                                        && terr.is_compatible_with_args(eerr, Some(self.resolver))
                                }
                                None => false,
                            }
                        } else if let Some(einner) = &enc_opt {
                            match t.option_like() {
                                Some(tinner) => {
                                    tinner.is_compatible_with_args(einner, Some(self.resolver))
                                }
                                None => false,
                            }
                        } else {
                            false
                        };
                        if !matches {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "function signature returns '{}' but the return statement produces '{}'; construct it explicitly, e.g. Outcome<T> {{ is_success: .., value: .., error_msg: .. }}",
                                    expected, t
                                ),
                                Some(span.clone()),
                            );
                        }
                    } else if opt_e.is_some()
                        && expected != DataraType::Unit
                        && !matches!(expected, DataraType::TypeParam(_))
                        && !t.is_compatible_with_args(&expected, Some(self.resolver))
                    {
                        let help_msg = Self::suggest_type_fix(&expected, &t);
                        diag.error_with_help(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Type mismatch in return statement: expected '{}', got '{}'",
                                expected, t
                            ),
                            Some(span.clone()),
                            help_msg,
                        );
                    }
                }
                t
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let cond_type = self.check_expr(condition, diag);
                if !cond_type.is_compatible_with_args(&DataraType::Bool, Some(self.resolver)) {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!("Condition must be Bool, got '{}'", cond_type),
                        Some(span.clone()),
                    );
                }

                // Smart Type Narrowing for Option/Maybe:
                let is_neq_none = match condition {
                    Expr::Binary {
                        left, op, right, ..
                    } if op == "!=" => {
                        if let Expr::Identifier(var_name, _) = &**left {
                            if matches!(&**right, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else if let Expr::Identifier(var_name, _) = &**right {
                            if matches!(&**left, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                let is_eq_none = match condition {
                    Expr::Binary {
                        left, op, right, ..
                    } if op == "==" => {
                        if let Expr::Identifier(var_name, _) = &**left {
                            if matches!(&**right, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else if let Expr::Identifier(var_name, _) = &**right {
                            if matches!(&**left, Expr::Literal(LiteralValue::None, _)) {
                                Some(var_name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(ref v) = is_neq_none {
                    let orig = self.symbol_types.get(v).cloned();
                    if let Some(DataraType::Option(inner)) = &orig {
                        self.symbol_types.insert(v.clone(), *inner.clone());
                    }
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(o) = orig {
                        self.symbol_types.insert(v.clone(), o);
                    }
                    if let Some(eb) = else_branch {
                        let orig_else = self.symbol_types.get(v).cloned();
                        self.symbol_types
                            .insert(v.clone(), DataraType::Option(Box::new(DataraType::Unit)));
                        self.check_stmt(eb, diag);
                        if let Some(o) = orig_else {
                            self.symbol_types.insert(v.clone(), o);
                        }
                    }
                    res
                } else if let Some(ref v) = is_eq_none {
                    let orig = self.symbol_types.get(v).cloned();
                    self.symbol_types
                        .insert(v.clone(), DataraType::Option(Box::new(DataraType::Unit)));
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(o) = orig {
                        self.symbol_types.insert(v.clone(), o);
                    }
                    if let Some(eb) = else_branch {
                        let orig_else = self.symbol_types.get(v).cloned();
                        if let Some(DataraType::Option(inner)) = &orig_else {
                            self.symbol_types.insert(v.clone(), *inner.clone());
                        }
                        self.check_stmt(eb, diag);
                        if let Some(o) = orig_else {
                            self.symbol_types.insert(v.clone(), o);
                        }
                    }
                    res
                } else {
                    let res = self.check_stmt(then_branch, diag);
                    if let Some(eb) = else_branch {
                        self.check_stmt(eb, diag);
                    }
                    res
                }
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                ..
            }
            | Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                let iter_type = self.check_expr(iterable, diag);
                // Parametric collections carry their element types into the
                // loop variable: `for x in List(t)` binds `x: t`. Maps have no
                // key/value iterator protocol (the runtime lowers iteration to
                // the list protocol), so their elements stay dynamic `Val`.
                let elem_type = match &iter_type {
                    DataraType::List(elem) => (**elem).clone(),
                    DataraType::Map(..) => DataraType::Val,
                    DataraType::GenericInstance { name, args }
                        if name == "List" && !args.is_empty() =>
                    {
                        args[0].clone()
                    }
                    DataraType::Range { base, .. } => (**base).clone(),
                    DataraType::Class(c) if c == "Range" => DataraType::Int,
                    DataraType::String => DataraType::Char,
                    DataraType::Class(c) if c == "List" => {
                        // Erased collections: fall back to the recorded element
                        // type, else the dynamic type — never a silent Int.
                        if let Expr::Identifier(n, _) = iterable {
                            self.var_element_types
                                .get(n)
                                .cloned()
                                .unwrap_or(DataraType::Val)
                        } else {
                            self.last_list_element.clone().unwrap_or(DataraType::Val)
                        }
                    }
                    _ => DataraType::Val,
                };
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), var_name.clone()), elem_type.clone());
                }
                let prev = self.symbol_types.insert(var_name.clone(), elem_type);
                let prev_mut = self
                    .symbol_mutability
                    .insert(var_name.clone(), MutabilityKind::Immutable);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(var_name.clone(), p);
                } else {
                    self.symbol_types.remove(var_name);
                }
                if let Some(m) = prev_mut {
                    self.symbol_mutability.insert(var_name.clone(), m);
                } else {
                    self.symbol_mutability.remove(var_name);
                }
                DataraType::Unit
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                let cond_type = self.check_expr(condition, diag);
                if !cond_type.is_compatible_with_args(&DataraType::Bool, Some(self.resolver)) {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!("While condition must be Bool, got '{}'", cond_type),
                        Some(span.clone()),
                    );
                }
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::Loop { body, .. } => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                ..
            } => {
                self.check_stmt(try_block, diag);
                let prev = self
                    .symbol_types
                    .insert(err_var.clone(), DataraType::String);
                let prev_mut = self
                    .symbol_mutability
                    .insert(err_var.clone(), MutabilityKind::Immutable);
                self.check_stmt(catch_block, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(err_var.clone(), p);
                } else {
                    self.symbol_types.remove(err_var);
                }
                if let Some(m) = prev_mut {
                    self.symbol_mutability.insert(err_var.clone(), m);
                } else {
                    self.symbol_mutability.remove(err_var);
                }
                DataraType::Unit
            }
            Stmt::Parallel(body, _) => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::With {
                resource_name,
                init,
                body,
                ..
            } => {
                let init_type = self.check_expr(init, diag);
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), resource_name.clone()), init_type.clone());
                }
                let prev = self.symbol_types.insert(resource_name.clone(), init_type);
                let prev_mut = self
                    .symbol_mutability
                    .insert(resource_name.clone(), MutabilityKind::Immutable);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(resource_name.clone(), p);
                } else {
                    self.symbol_types.remove(resource_name);
                }
                if let Some(m) = prev_mut {
                    self.symbol_mutability.insert(resource_name.clone(), m);
                } else {
                    self.symbol_mutability.remove(resource_name);
                }
                DataraType::Unit
            }
            Stmt::Unsafe { body, .. } => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::Asm { .. } => DataraType::Unit,
        }
    }

    fn check_return_exhaustiveness(
        &self,
        fn_name: &str,
        body: &Stmt,
        expected: &DataraType,
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        if *expected == DataraType::Unit || matches!(expected, DataraType::TypeParam(_)) {
            return;
        }

        if !Self::stmt_guarantees_return(body) {
            diag.error_with_help(
                ErrorCode::TypeMissingReturn,
                format!(
                    "Function '{}' has declared return type '{}' but not all code paths return a value",
                    fn_name, expected
                ),
                Some(span.clone()),
                Some(format!("add an explicit 'return <expr>;' at the end of function '{}'", fn_name)),
            );
        }
    }

    fn stmt_guarantees_return(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return(Some(_), _) => true,
            Stmt::Block(stmts, _) => {
                if stmts.is_empty() {
                    return false;
                }
                for (i, s) in stmts.iter().enumerate() {
                    if Self::stmt_guarantees_return(s) {
                        return true;
                    }
                    if i == stmts.len() - 1 && matches!(s, Stmt::Expr(..)) {
                        return true;
                    }
                }
                false
            }
            Stmt::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                Self::stmt_guarantees_return(then_branch)
                    && Self::stmt_guarantees_return(else_branch)
            }
            Stmt::Expr(Expr::Match { arms, .. }, _) => !arms.is_empty(),
            Stmt::Expr(Expr::Decide { else_arm, .. }, _) => else_arm.is_some(),
            Stmt::Expr(..) => true,
            Stmt::Loop { .. } => true,
            Stmt::While { condition, .. }
                if matches!(condition, Expr::Literal(LiteralValue::Bool(true), _)) =>
            {
                true
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                Self::stmt_guarantees_return(try_block) && Self::stmt_guarantees_return(catch_block)
            }
            _ => false,
        }
    }
}
