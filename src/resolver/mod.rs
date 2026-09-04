use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub is_mut: bool,
    pub is_export: bool,
    pub span: SourceSpan,
    pub fields: HashMap<String, Symbol>,
    pub methods: HashMap<String, Symbol>,
    pub base_type: Option<String>,
    pub compositions: Vec<String>,
    pub generic_params: Vec<String>,
    pub type_node: Option<TypeNode>,
    pub return_type: Option<TypeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Param,
    Function,
    Class,
    Component,
    Role,
    Field,
    Method,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            symbols: HashMap::new(),
        }
    }

    pub fn define(&mut self, name: String, sym: Symbol) {
        self.symbols.insert(name, sym);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

pub struct Resolver {
    pub scopes: Vec<Scope>,
    pub classes: HashMap<String, Symbol>,
    pub components: HashMap<String, Symbol>,
    pub roles: HashMap<String, Symbol>,
    pub functions: HashMap<String, Symbol>,
    pub packets: HashMap<String, PacketDecl>,
    pub extern_functions: HashMap<String, ExternFnDecl>,
    pub type_aliases: HashMap<String, TypeDecl>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver {
    pub fn new() -> Self {
        let mut global_scope = Scope::new("global");

        // Built-in / Intrinsic functions & stdlib symbols
        let builtins = [
            "print",
            "println",
            "eprintln",
            "input",
            "input_int",
            "input_float",
            "panic",
            "assert",
            "require",
            "exit",
            "len",
            "now",
            "now_ms",
            "now_precise_ms",
            "out",
            "err",
            "http_get",
            "http_post",
            "db_query",
            "read_file",
            "write_file",
            "read",
            "write",
            "glob",
            "open",
            "run",
            "slice",
            "map",
            "filter",
            "each",
            "reduce",
            "find",
            "length",
            "view",
            "mut_view",
            "mutView",
            "destroy",
            "unsafe_op",
            "file_read",
            "file_write",
            "file_append",
            "file_exists",
            "fs_open",
            "fs_read",
            "fs_write",
            "net_connect",
            "net_listen",
            "proc_spawn",
            "env_get",
            "args_count",
            "args_get",
            "sleep",
            "str_len",
            "str_substring",
            "str_char_at",
            "str_contains",
            "str_starts_with",
            "str_ends_with",
            "str_index_of",
            "str_trim",
            "str_to_int",
            "str_to_float",
            "int_to_str",
            "datara_rt_int_to_str",
            "datara_rt_str_len",
            "parallel_for",
            "parallel_invoke",
            "num_workers",
            "read_line",
            "socket_create",
            "socket_bind",
            "socket_listen",
            "socket_accept",
            "socket_connect",
            "socket_send",
            "socket_recv",
            "socket_close",
            "sha256",
            "base64_encode",
            "base64_decode",
            "uuid_v4",
            "datara_rt_uuid_v4",
            "datara_rt_dialog_info",
            "datara_rt_dialog_alert",
            "datara_rt_dialog_confirm",
            "process_run",
            "process_output",
            "system",
            "exec",
            "math_sqrt",
            "math_pow",
            "math_abs",
            "math_sin",
            "math_cos",
            "math_tan",
            "math_floor",
            "math_ceil",
            "math_round",
            "math_min",
            "math_max",
            "math_hypot",
            "math_min_int",
            "math_max_int",
            "math_abs_int",
            "math_ctz",
            "ctz",
            "math_shr",
            "shr",
            "math_shl",
            "shl",
            "math_xor",
            "xor",
            "math_and",
            "and",
            "math_or",
            "or",
            "float4",
            "int4",
            "min4",
            "max4",
            "dot",
        ];
        for b in &builtins {
            global_scope.define(
                b.to_string(),
                Symbol {
                    name: b.to_string(),
                    kind: SymbolKind::Function,
                    is_mut: false,
                    is_export: true,
                    span: SourceSpan::default(),
                    fields: HashMap::new(),
                    methods: HashMap::new(),
                    base_type: None,
                    compositions: Vec::new(),
                    generic_params: Vec::new(),
                    type_node: None,
                    return_type: None,
                },
            );
        }

        Self {
            scopes: vec![global_scope],
            classes: HashMap::new(),
            components: HashMap::new(),
            roles: HashMap::new(),
            functions: HashMap::new(),
            packets: HashMap::new(),
            extern_functions: HashMap::new(),
            type_aliases: HashMap::new(),
        }
    }

    pub fn resolve_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        let mut behaviors = Vec::new();
        let mut use_decls = Vec::new();

        // Pass 1: Register top-level classes, components, roles, functions, uses
        for decl in &program.declarations {
            match decl {
                Decl::Use(u) => {
                    use_decls.push(u.clone());
                    let first_seg = u.path.first().map(|s| s.as_str());
                    if matches!(
                        first_seg,
                        Some("python" | "rust" | "c" | "cpp" | "cxx" | "npm" | "js" | "ts")
                    ) {
                        let alias = u
                            .alias
                            .clone()
                            .unwrap_or_else(|| u.path.last().cloned().unwrap_or_default());
                        if !alias.is_empty() && !self.scopes[0].symbols.contains_key(&alias) {
                            let sym = Symbol {
                                name: alias.clone(),
                                kind: SymbolKind::Variable,
                                is_mut: false,
                                is_export: true,
                                span: u.span.clone(),
                                fields: HashMap::new(),
                                methods: HashMap::new(),
                                base_type: None,
                                compositions: Vec::new(),
                                generic_params: Vec::new(),
                                type_node: Some(TypeNode {
                                    name: "Val".to_string(),
                                    generic_args: Vec::new(),
                                    is_option: false,
                                    error_type: None,
                                    refinement: None,
                                    span: u.span.clone(),
                                }),
                                return_type: None,
                            };
                            self.scopes[0].define(alias, sym);
                        }
                    }
                }
                Decl::Class(c) => {
                    if self.classes.contains_key(&c.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate class '{}'", c.name),
                            Some(c.span.clone()),
                        );
                        continue;
                    }
                    let mut sym = Symbol {
                        name: c.name.clone(),
                        kind: SymbolKind::Class,
                        is_mut: false,
                        is_export: c.is_export,
                        span: c.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: c.base_type.clone(),
                        compositions: c.compositions.clone(),
                        generic_params: c.generic_params.clone(),
                        type_node: None,
                        return_type: None,
                    };

                    for item in &c.body_items {
                        match item {
                            ClassItem::Field(f) => {
                                sym.fields.insert(
                                    f.name.clone(),
                                    Symbol {
                                        name: f.name.clone(),
                                        kind: SymbolKind::Field,
                                        is_mut: f.is_mut,
                                        is_export: false,
                                        span: f.span.clone(),
                                        fields: HashMap::new(),
                                        methods: HashMap::new(),
                                        base_type: None,
                                        compositions: Vec::new(),
                                        generic_params: Vec::new(),
                                        type_node: f.type_node.clone(),
                                        return_type: None,
                                    },
                                );
                            }
                            ClassItem::Method(m) => {
                                sym.methods.insert(
                                    m.name.clone(),
                                    Symbol {
                                        name: m.name.clone(),
                                        kind: SymbolKind::Method,
                                        is_mut: false,
                                        is_export: false,
                                        span: m.span.clone(),
                                        fields: HashMap::new(),
                                        methods: HashMap::new(),
                                        base_type: None,
                                        compositions: Vec::new(),
                                        generic_params: m.generic_params.clone(),
                                        type_node: None,
                                        return_type: m.return_type.clone(),
                                    },
                                );
                            }
                            ClassItem::Using(u, _) => {
                                sym.compositions.push(u.clone());
                            }
                        }
                    }

                    self.classes.insert(c.name.clone(), sym.clone());
                    self.scopes[0].define(c.name.clone(), sym);
                }

                Decl::Enum(e) => {
                    if self.classes.contains_key(&e.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate type name '{}'", e.name),
                            Some(e.span.clone()),
                        );
                        continue;
                    }
                    let mut sym = Symbol {
                        name: e.name.clone(),
                        kind: SymbolKind::Class,
                        is_mut: false,
                        is_export: e.is_export,
                        span: e.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: e.generic_params.clone(),
                        type_node: None,
                        return_type: None,
                    };

                    for v in &e.variants {
                        let v_sym = Symbol {
                            name: v.name.clone(),
                            kind: SymbolKind::Method,
                            is_mut: false,
                            is_export: e.is_export,
                            span: v.span.clone(),
                            fields: HashMap::new(),
                            methods: HashMap::new(),
                            base_type: None,
                            compositions: Vec::new(),
                            generic_params: Vec::new(),
                            type_node: None,
                            return_type: Some(TypeNode {
                                name: e.name.clone(),
                                generic_args: Vec::new(),
                                is_option: false,
                                error_type: None,
                                refinement: None,
                                span: v.span.clone(),
                            }),
                        };
                        sym.methods.insert(v.name.clone(), v_sym.clone());
                        sym.fields.insert(v.name.clone(), v_sym.clone());
                        self.scopes[0].define(v.name.clone(), v_sym);
                    }

                    self.classes.insert(e.name.clone(), sym.clone());
                    self.scopes[0].define(e.name.clone(), sym);
                }

                Decl::Component(c) => {
                    let mut sym = Symbol {
                        name: c.name.clone(),
                        kind: SymbolKind::Component,
                        is_mut: false,
                        is_export: c.is_export,
                        span: c.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: None,
                        return_type: None,
                    };
                    for item in &c.body_items {
                        match item {
                            ClassItem::Field(f) => {
                                sym.fields.insert(
                                    f.name.clone(),
                                    Symbol {
                                        name: f.name.clone(),
                                        kind: SymbolKind::Field,
                                        is_mut: f.is_mut,
                                        is_export: false,
                                        span: f.span.clone(),
                                        fields: HashMap::new(),
                                        methods: HashMap::new(),
                                        base_type: None,
                                        compositions: Vec::new(),
                                        generic_params: Vec::new(),
                                        type_node: f.type_node.clone(),
                                        return_type: None,
                                    },
                                );
                            }
                            ClassItem::Method(m) => {
                                sym.methods.insert(
                                    m.name.clone(),
                                    Symbol {
                                        name: m.name.clone(),
                                        kind: SymbolKind::Method,
                                        is_mut: false,
                                        is_export: false,
                                        span: m.span.clone(),
                                        fields: HashMap::new(),
                                        methods: HashMap::new(),
                                        base_type: None,
                                        compositions: Vec::new(),
                                        generic_params: m.generic_params.clone(),
                                        type_node: None,
                                        return_type: m.return_type.clone(),
                                    },
                                );
                            }
                            ClassItem::Using(u, _) => {
                                sym.compositions.push(u.clone());
                            }
                        }
                    }
                    self.components.insert(c.name.clone(), sym.clone());
                    self.scopes[0].define(c.name.clone(), sym);
                }

                Decl::Role(r) => {
                    let mut sym = Symbol {
                        name: r.name.clone(),
                        kind: SymbolKind::Role,
                        is_mut: false,
                        is_export: r.is_export,
                        span: r.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: None,
                        return_type: None,
                    };
                    for m in &r.methods {
                        sym.methods.insert(
                            m.name.clone(),
                            Symbol {
                                name: m.name.clone(),
                                kind: SymbolKind::Method,
                                is_mut: false,
                                is_export: false,
                                span: m.span.clone(),
                                fields: HashMap::new(),
                                methods: HashMap::new(),
                                base_type: None,
                                compositions: Vec::new(),
                                generic_params: m.generic_params.clone(),
                                type_node: None,
                                return_type: m.return_type.clone(),
                            },
                        );
                    }
                    self.roles.insert(r.name.clone(), sym.clone());
                    self.scopes[0].define(r.name.clone(), sym);
                }

                Decl::Behavior(b) => {
                    behaviors.push(b.clone());
                }

                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    let sym = Symbol {
                        name: f.name.clone(),
                        kind: SymbolKind::Function,
                        is_mut: false,
                        is_export: f.is_export,
                        span: f.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: f.generic_params.clone(),
                        type_node: None,
                        return_type: f.return_type.clone(),
                    };
                    if self.functions.contains_key(&f.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate function definition '{}'", f.name),
                            Some(f.span.clone()),
                        );
                    }
                    self.functions.insert(f.name.clone(), sym.clone());
                    self.scopes[0].define(f.name.clone(), sym);
                }

                Decl::ExternFn(ef) => {
                    let sym = Symbol {
                        name: ef.name.clone(),
                        kind: SymbolKind::Function,
                        is_mut: false,
                        is_export: true,
                        span: ef.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: None,
                        return_type: ef.return_type.clone(),
                    };
                    if self.functions.contains_key(&ef.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate extern declaration '{}'", ef.name),
                            Some(ef.span.clone()),
                        );
                    }
                    self.functions.insert(ef.name.clone(), sym.clone());
                    self.extern_functions.insert(ef.name.clone(), ef.clone());
                    self.scopes[0].define(ef.name.clone(), sym);
                }

                Decl::Packet(p) => {
                    let mut fields = HashMap::new();
                    for f in &p.fields {
                        fields.insert(
                            f.name.clone(),
                            Symbol {
                                name: f.name.clone(),
                                kind: SymbolKind::Field,
                                is_mut: true,
                                is_export: false,
                                span: f.span.clone(),
                                fields: HashMap::new(),
                                methods: HashMap::new(),
                                base_type: None,
                                compositions: Vec::new(),
                                generic_params: Vec::new(),
                                type_node: Some(TypeNode {
                                    name: "Int".to_string(),
                                    generic_args: Vec::new(),
                                    is_option: false,
                                    error_type: None,
                                    refinement: None,
                                    span: f.span.clone(),
                                }),
                                return_type: None,
                            },
                        );
                    }
                    let sym = Symbol {
                        name: p.name.clone(),
                        kind: SymbolKind::Class,
                        is_mut: false,
                        is_export: true,
                        span: p.span.clone(),
                        fields,
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: None,
                        return_type: None,
                    };
                    self.packets.insert(p.name.clone(), p.clone());
                    self.classes.insert(p.name.clone(), sym.clone());
                    self.scopes[0].define(p.name.clone(), sym);
                }

                Decl::Type(td) => {
                    if self.type_aliases.contains_key(&td.name)
                        || self.classes.contains_key(&td.name)
                    {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate type alias '{}'", td.name),
                            Some(td.span.clone()),
                        );
                    }
                    self.type_aliases.insert(td.name.clone(), td.clone());
                    let sym = Symbol {
                        name: td.name.clone(),
                        kind: SymbolKind::Class,
                        is_mut: false,
                        is_export: td.is_export,
                        span: td.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: Some(td.base_type.name.clone()),
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: Some(td.base_type.clone()),
                        return_type: None,
                    };
                    self.classes.insert(td.name.clone(), sym.clone());
                    self.scopes[0].define(td.name.clone(), sym);
                }

                Decl::Register(reg) => {
                    let mut sym = Symbol {
                        name: reg.name.clone(),
                        kind: SymbolKind::Variable,
                        is_mut: true,
                        is_export: true,
                        span: reg.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: Vec::new(),
                        generic_params: Vec::new(),
                        type_node: Some(TypeNode {
                            name: reg.name.clone(),
                            generic_args: Vec::new(),
                            is_option: false,
                            error_type: None,
                            refinement: None,
                            span: reg.span.clone(),
                        }),
                        return_type: None,
                    };
                    for field in &reg.fields {
                        sym.fields.insert(
                            field.name.clone(),
                            Symbol {
                                name: field.name.clone(),
                                kind: SymbolKind::Field,
                                is_mut: true,
                                is_export: true,
                                span: field.span.clone(),
                                fields: HashMap::new(),
                                methods: HashMap::new(),
                                base_type: None,
                                compositions: Vec::new(),
                                generic_params: Vec::new(),
                                type_node: Some(field.type_node.clone()),
                                return_type: None,
                            },
                        );
                    }
                    self.classes.insert(reg.name.clone(), sym.clone());
                    self.scopes[0].define(reg.name.clone(), sym);
                }
            }
        }

        // Pass 2a: Merge base class inheritance (from) and component compositions (+) into classes
        let class_names: Vec<String> = self.classes.keys().cloned().collect();
        for _ in 0..10 {
            let mut changed = false;
            for cls_name in &class_names {
                let (base_class, compositions) = self
                    .classes
                    .get(cls_name)
                    .map(|c| (c.base_type.clone(), c.compositions.clone()))
                    .unwrap_or_default();

                // Inherit from base_class
                if let Some(base_name) = &base_class
                    && let Some(base_sym) = self.classes.get(base_name).cloned()
                    && let Some(cls_sym) = self.classes.get_mut(cls_name)
                {
                    for (f_name, f_sym) in &base_sym.fields {
                        if !cls_sym.fields.contains_key(f_name) {
                            cls_sym.fields.insert(f_name.clone(), f_sym.clone());
                            changed = true;
                        }
                    }
                    for (m_name, m_sym) in &base_sym.methods {
                        if !cls_sym.methods.contains_key(m_name) {
                            cls_sym.methods.insert(m_name.clone(), m_sym.clone());
                            changed = true;
                        }
                    }
                }

                // Inline components or used classes
                for comp_name in &compositions {
                    let comp_sym = self
                        .components
                        .get(comp_name)
                        .cloned()
                        .or_else(|| self.classes.get(comp_name).cloned());
                    if let Some(comp_sym) = comp_sym
                        && let Some(cls_sym) = self.classes.get_mut(cls_name)
                    {
                        for (f_name, f_sym) in &comp_sym.fields {
                            if !cls_sym.fields.contains_key(f_name) {
                                cls_sym.fields.insert(f_name.clone(), f_sym.clone());
                                changed = true;
                            }
                        }
                        for (m_name, m_sym) in &comp_sym.methods {
                            if !cls_sym.methods.contains_key(m_name) {
                                cls_sym.methods.insert(m_name.clone(), m_sym.clone());
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Pass 2b: Merge Split Behavior blocks into target classes and check replaces
        for beh in behaviors {
            if let Some(target_class) = self.classes.get_mut(&beh.target_type) {
                for item in beh.body_items {
                    if let ClassItem::Method(m) = item {
                        if !m.is_replaces && target_class.methods.contains_key(&m.name) {
                            // Ambiguity collision without explicit replaces
                            diag.error(ErrorCode::ResolveDuplicateSymbol, format!("[E-AMBIGUOUS-OVERRIDE] Method '{}' in behavior for '{}' collides with existing method. Use 'replaces' to override explicitly.", m.name, beh.target_type), Some(m.span.clone()));
                        }
                        target_class.methods.insert(
                            m.name.clone(),
                            Symbol {
                                name: m.name.clone(),
                                kind: SymbolKind::Method,
                                is_mut: false,
                                is_export: false,
                                span: m.span.clone(),
                                fields: HashMap::new(),
                                methods: HashMap::new(),
                                base_type: None,
                                compositions: Vec::new(),
                                generic_params: m.generic_params.clone(),
                                type_node: None,
                                return_type: m.return_type.clone(),
                            },
                        );
                    }
                }
            } else {
                diag.error(
                    ErrorCode::ResolveUnknownType,
                    format!(
                        "Behavior defines methods for unknown class '{}'",
                        beh.target_type
                    ),
                    Some(beh.span.clone()),
                );
            }
        }

        // Pass 2c: Verify Role capability contracts
        for cls_name in &class_names {
            let (compositions, methods, span) = if let Some(cls) = self.classes.get(cls_name) {
                (
                    cls.compositions.clone(),
                    cls.methods.clone(),
                    cls.span.clone(),
                )
            } else {
                continue;
            };

            for comp_name in &compositions {
                if let Some(role_sym) = self.roles.get(comp_name).cloned() {
                    for req_method in role_sym.methods.keys() {
                        if !methods.contains_key(req_method) {
                            diag.error(ErrorCode::TypeMismatch, format!("[E-ROLE-UNSATISFIED] Class '{}' declares role '{}' but does not implement required method '{}'", cls_name, comp_name, req_method), Some(span.clone()));
                        }
                    }
                }
            }
        }

        // Pass 3: Resolve bodies and local variables
        for decl in &program.declarations {
            self.resolve_decl(decl, diag);
        }
    }

    fn resolve_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                self.enter_scope(&format!("fn_{}", f.name));
                for p in &f.params {
                    self.define_local(&p.name, SymbolKind::Param, false, &p.span);
                }
                self.resolve_stmt(&f.body, diag);
                self.exit_scope();
            }
            Decl::Class(c) => {
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item {
                        self.resolve_method(m, diag);
                    }
                }
            }
            Decl::Behavior(b) => {
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item {
                        self.resolve_method(m, diag);
                    }
                }
            }
            _ => {}
        }
    }

    fn resolve_method(&mut self, m: &MethodDecl, diag: &mut DiagnosticEngine) {
        self.enter_scope(&format!("method_{}", m.name));
        self.define_local("this", SymbolKind::Param, false, &m.span);
        for p in &m.params {
            self.define_local(&p.name, SymbolKind::Param, false, &p.span);
        }
        if let Some(body) = &m.body {
            self.resolve_stmt(body, diag);
        }
        self.exit_scope();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt, diag: &mut DiagnosticEngine) {
        match stmt {
            Stmt::Block(stmts, _) => {
                self.enter_scope("block");
                for s in stmts {
                    self.resolve_stmt(s, diag);
                }
                self.exit_scope();
            }
            Stmt::Let {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::Mut {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, true, span);
            }
            Stmt::Val {
                name,
                init,
                is_mut,
                span,
                ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, *is_mut, span);
            }
            Stmt::Const {
                name, init, span, ..
            } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::CompactBind { name, init, span } => {
                self.resolve_expr(init, diag);
                self.define_local(name, SymbolKind::Variable, false, span);
            }
            Stmt::Assign {
                target,
                value,
                span,
                ..
            } => {
                if let Expr::Identifier(name, id_span) = target {
                    if self.resolve_symbol(name).is_none() {
                        let mut candidates: Vec<&str> = Vec::new();
                        for s in self.scopes.iter().rev() {
                            for k in s.symbols.keys() {
                                candidates.push(k.as_str());
                            }
                        }
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
                            Some(id_span.clone()),
                            Some(help_msg),
                        );
                    }
                } else {
                    self.resolve_expr(target, diag);
                }
                let _ = span;
                self.resolve_expr(value, diag);
            }
            Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.resolve_expr(e, diag);
            }
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    self.resolve_expr(e, diag);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expr(condition, diag);
                self.resolve_stmt(then_branch, diag);
                if let Some(eb) = else_branch {
                    self.resolve_stmt(eb, diag);
                }
            }
            Stmt::For {
                var_name,
                iterable,
                body,
                span,
            } => {
                self.resolve_expr(iterable, diag);
                self.enter_scope("for");
                self.define_local(var_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.resolve_expr(condition, diag);
                self.resolve_stmt(body, diag);
            }
            Stmt::Loop { body, .. } => {
                self.resolve_stmt(body, diag);
            }
            Stmt::TryCatch {
                try_block,
                err_var,
                catch_block,
                span,
            } => {
                self.resolve_stmt(try_block, diag);
                self.enter_scope("catch");
                self.define_local(err_var, SymbolKind::Variable, false, span);
                self.resolve_stmt(catch_block, diag);
                self.exit_scope();
            }
            Stmt::Parallel(body, _) => {
                self.resolve_stmt(body, diag);
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                span,
            } => {
                self.resolve_expr(iterable, diag);
                self.enter_scope("parallel_for");
                self.define_local(var_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::With {
                resource_name,
                init,
                body,
                span,
            } => {
                self.resolve_expr(init, diag);
                self.enter_scope("with");
                self.define_local(resource_name, SymbolKind::Variable, false, span);
                self.resolve_stmt(body, diag);
                self.exit_scope();
            }
            Stmt::Unsafe { body, .. } => {
                self.resolve_stmt(body, diag);
            }
            Stmt::Asm { .. } => {}
        }
    }

    fn resolve_expr(&mut self, expr: &Expr, diag: &mut DiagnosticEngine) {
        match expr {
            Expr::Identifier(name, span) => {
                if self.resolve_symbol(name).is_none() {
                    let has_field = self.scopes.iter().any(|s| {
                        if s.get("this").is_some() {
                            for cls in self.classes.values() {
                                if cls.fields.contains_key(name) {
                                    return true;
                                }
                            }
                        }
                        false
                    });

                    if !has_field {
                        let mut candidates: Vec<&str> = Vec::new();
                        for s in self.scopes.iter().rev() {
                            for k in s.symbols.keys() {
                                candidates.push(k.as_str());
                            }
                        }
                        for f in self.functions.keys() {
                            candidates.push(f.as_str());
                        }
                        for c in self.classes.keys() {
                            candidates.push(c.as_str());
                        }

                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(name, candidates)
                        {
                            format!("a symbol with a similar name exists: '{}'", similar)
                        } else {
                            format!(
                                "ensure '{}' is declared with 'let'/'mut' or imported via 'use'",
                                name
                            )
                        };

                        diag.error_with_help(
                            ErrorCode::ResolveUndefinedSymbol,
                            format!("Undefined symbol '{}'", name),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                }
            }
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left, diag);
                self.resolve_expr(right, diag);
            }
            Expr::Unary { expr, .. } | Expr::ErrorPropagate(expr, _) => {
                self.resolve_expr(expr, diag);
            }
            Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee, diag);
                for a in args {
                    self.resolve_expr(a, diag);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.resolve_expr(object, diag);
            }
            Expr::ObjectInit {
                class_name,
                span,
                fields,
                ..
            } => {
                if !self.classes.contains_key(class_name) {
                    let candidates: Vec<&str> = self.classes.keys().map(|s| s.as_str()).collect();
                    let help_msg = if let Some(similar) =
                        crate::diagnostics::suggestions::find_best_match(class_name, candidates)
                    {
                        format!("a class with a similar name exists: '{}'", similar)
                    } else {
                        format!("define class '{}' or import it via 'use'", class_name)
                    };
                    diag.error_with_help(
                        ErrorCode::ResolveUndefinedSymbol,
                        format!(
                            "Unknown class '{}' in initialization (is a matching 'use' import missing?)",
                            class_name
                        ),
                        Some(span.clone()),
                        Some(help_msg),
                    );
                }
                for (_, f_expr) in fields {
                    self.resolve_expr(f_expr, diag);
                }
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    self.resolve_expr(s, diag);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for arm in arms {
                    self.resolve_expr(&arm.condition, diag);
                    self.resolve_expr(&arm.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.resolve_expr(eb, diag);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.resolve_expr(value, diag);
                for arm in arms {
                    self.enter_scope("match_arm");
                    match &arm.pattern {
                        Pattern::Identifier(id, span) => {
                            self.define_local(id, SymbolKind::Variable, false, span);
                        }
                        Pattern::Variant { bindings, span, .. } => {
                            for b in bindings {
                                self.define_local(b, SymbolKind::Variable, false, span);
                            }
                        }
                        _ => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.resolve_expr(g, diag);
                    }
                    self.resolve_expr(&arm.body, diag);
                    self.exit_scope();
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for arm in arms {
                    self.resolve_expr(&arm.condition, diag);
                    self.resolve_expr(&arm.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.resolve_expr(eb, diag);
                }
            }
            Expr::Lambda { params, body, .. } => {
                self.enter_scope("lambda");
                for p in params {
                    self.define_local(&p.name, SymbolKind::Param, false, &p.span);
                }
                self.resolve_expr(body, diag);
                self.exit_scope();
            }
            Expr::ListLiteral(items, _) => {
                for item in items {
                    self.resolve_expr(item, diag);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.resolve_expr(k, diag);
                    self.resolve_expr(v, diag);
                }
            }
            Expr::IndexAccess { object, index, .. } => {
                self.resolve_expr(object, diag);
                self.resolve_expr(index, diag);
            }
            Expr::Range { start, end, .. } => {
                self.resolve_expr(start, diag);
                self.resolve_expr(end, diag);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.resolve_expr(e, diag);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.resolve_expr(e, diag);
                }
            }
            Expr::OrRecovery { expr, arms, .. } => {
                self.resolve_expr(expr, diag);
                for arm in arms {
                    self.resolve_expr(&arm.body, diag);
                }
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                self.resolve_expr(elem, diag);
            }
            _ => {}
        }
    }

    fn define_local(&mut self, name: &str, kind: SymbolKind, is_mut: bool, span: &SourceSpan) {
        let sym = Symbol {
            name: name.to_string(),
            kind,
            is_mut,
            is_export: false,
            span: span.clone(),
            fields: HashMap::new(),
            methods: HashMap::new(),
            base_type: None,
            compositions: Vec::new(),
            generic_params: Vec::new(),
            type_node: None,
            return_type: None,
        };
        if let Some(top) = self.scopes.last_mut() {
            top.define(name.to_string(), sym);
        }
    }

    pub fn resolve_symbol(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym);
            }
        }
        None
    }

    fn enter_scope(&mut self, name: &str) {
        self.scopes.push(Scope::new(name));
    }

    fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }
}
