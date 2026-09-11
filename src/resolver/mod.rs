use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use std::collections::{HashMap, HashSet};

pub mod symbols;
pub(crate) mod visit;

pub use symbols::{Scope, Symbol, SymbolKind};
pub struct Resolver {
    pub scopes: Vec<Scope>,
    pub classes: HashMap<String, Symbol>,
    pub components: HashMap<String, Symbol>,
    pub roles: HashMap<String, Symbol>,
    pub traits: HashMap<String, Symbol>,
    pub functions: HashMap<String, Symbol>,
    pub packets: HashMap<String, PacketDecl>,
    pub extern_functions: HashMap<String, ExternFnDecl>,
    pub type_aliases: HashMap<String, TypeDecl>,
    pub enums: HashMap<String, EnumDecl>,
    pub current_target_type: Option<String>,
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
            "now_ns",
            "datara_rt_now_ns",
            "now_precise_ms",
            "time_precise_ms",
            "datara_rt_time_precise_ms",
            "time_delta_ms",
            "datara_rt_time_delta_ms",
            "path_join",
            "datara_rt_path_join",
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
            "any",
            "all",
            "length",
            "count",
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
            "byte_len",
            "str_chars",
            "char_len",
            "validate_utf8",
            "str_sanitize_utf8",
            "str_scalar_at",
            "str_next_offset",
            "str_substring",
            "str_char_at",
            "str_byte_at",
            "byte_at",
            "str_contains",
            "str_starts_with",
            "str_ends_with",
            "str_index_of",
            "str_trim",
            "str_to_int",
            "str_to_float",
            "str_repeat",
            "repeat",
            "str_pad_left",
            "pad_left",
            "str_pad_right",
            "pad_right",
            "str_replace",
            "replace",
            "str_to_upper",
            "to_upper",
            "str_to_lower",
            "to_lower",
            "str_split",
            "split",
            "datara_rt_str_split",
            "str_join",
            "join",
            "datara_rt_str_join",
            "format_percent",
            "format_int_with_commas",
            "js_eval",
            "js_eval_int",
            "js_eval_float",
            "js_require",
            "js_call",
            "js_call_0",
            "js_call_1",
            "js_call_2",
            "js_set_global",
            "js_get_global",
            "datara_js_eval",
            "datara_js_eval_int",
            "datara_js_eval_float",
            "datara_js_require",
            "datara_js_call",
            "datara_js_call_0",
            "datara_js_call_1",
            "datara_js_call_2",
            "datara_js_set_global",
            "datara_js_get_global",
            "datara_js_export_list_f64",
            "datara_js_assert_same_ptr",
            "py_eval",
            "py_eval_safe",
            "py_eval_int",
            "py_eval_float",
            "py_call",
            "py_call_1_str",
            "py_call_1_float",
            "py_import",
            "py_exec",
            "py_last_error",
            "py_clear_error",
            "datara_py_eval",
            "datara_py_eval_safe",
            "datara_py_eval_int",
            "datara_py_eval_float",
            "datara_py_call",
            "datara_py_call_1_str",
            "datara_py_call_1_float",
            "datara_py_import",
            "datara_py_exec",
            "datara_py_last_error",
            "datara_py_clear_error",
            "datara_py_export_list_f64",
            "datara_py_assert_same_ptr",
            "int_to_str",
            "datara_rt_int_to_str",
            "float_to_str",
            "datara_rt_float_to_str",
            "datara_rt_str_len",
            "datara_rt_byte_len",
            "datara_rt_str_chars",
            "datara_rt_char_len",
            "datara_rt_validate_utf8",
            "datara_rt_str_sanitize_utf8",
            "datara_rt_str_scalar_at",
            "datara_rt_str_byte_at",
            "datara_rt_str_next_offset",
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
            "math_clamp",
            "clamp",
            "datara_rt_math_clamp",
            "math_hypot",
            "math_log",
            "datara_rt_math_log",
            "math_exp",
            "datara_rt_math_exp",
            "math_min_int",
            "math_max_int",
            "math_clamp_int",
            "clamp_int",
            "datara_rt_math_clamp_int",
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
            "float4_x",
            "float4_y",
            "float4_z",
            "float4_w",
            "lane0",
            "lane1",
            "lane2",
            "lane3",
            "int4_x",
            "int4_y",
            "int4_z",
            "int4_w",
            "wrapping",
            "saturating",
            "wrapping_add",
            "wrapping_sub",
            "wrapping_mul",
            "saturating_add",
            "saturating_sub",
            "saturating_mul",
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
            traits: HashMap::new(),
            functions: HashMap::new(),
            packets: HashMap::new(),
            extern_functions: HashMap::new(),
            type_aliases: HashMap::new(),
            enums: HashMap::new(),
            current_target_type: None,
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
                            let mut sym = Symbol {
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

                            // Wave 5.2: use python module resolution with typed signatures from JSON manifest
                            if first_seg == Some("python") {
                                let py_mod = u.path.get(1).map(|s| s.as_str()).unwrap_or("python");
                                let manifest_paths = [
                                    format!("manifests/{}.json", py_mod),
                                    format!("modules/python/{}.json", py_mod),
                                    format!("{}.json", py_mod),
                                ];
                                for mp in &manifest_paths {
                                    if let Ok(manifest_content) = std::fs::read_to_string(mp) {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(
                                            &manifest_content,
                                        ) {
                                            if let Some(funcs) =
                                                val.get("functions").and_then(|f| f.as_array())
                                            {
                                                for f_obj in funcs {
                                                    if let Some(fn_name) =
                                                        f_obj.get("name").and_then(|n| n.as_str())
                                                    {
                                                        let ret_ty = f_obj
                                                            .get("return_type")
                                                            .and_then(|r| r.as_str())
                                                            .unwrap_or("Val");
                                                        sym.methods.insert(
                                                            fn_name.to_string(),
                                                            Symbol {
                                                                name: fn_name.to_string(),
                                                                kind: SymbolKind::Method,
                                                                is_mut: false,
                                                                is_export: true,
                                                                span: u.span.clone(),
                                                                fields: HashMap::new(),
                                                                methods: HashMap::new(),
                                                                base_type: None,
                                                                compositions: Vec::new(),
                                                                generic_params: Vec::new(),
                                                                type_node: Some(TypeNode {
                                                                    name: ret_ty.to_string(),
                                                                    generic_args: Vec::new(),
                                                                    is_option: false,
                                                                    error_type: None,
                                                                    refinement: None,
                                                                    span: u.span.clone(),
                                                                }),
                                                                return_type: None,
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                            }

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
                            ClassItem::Invariant(_, _) => {}
                        }
                    }

                    self.classes.insert(c.name.clone(), sym.clone());
                    self.scopes[0].define(c.name.clone(), sym);
                }

                Decl::Enum(e) => {
                    if self.enums.contains_key(&e.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate enum '{}'", e.name),
                            Some(e.span.clone()),
                        );
                        continue;
                    }
                    if self.classes.contains_key(&e.name) {
                        diag.error(
                            ErrorCode::ResolveDuplicateSymbol,
                            format!("Duplicate type name '{}'", e.name),
                            Some(e.span.clone()),
                        );
                        continue;
                    }
                    self.enums.insert(e.name.clone(), e.clone());
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
                            ClassItem::Invariant(_, _) => {}
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

                Decl::Trait(t) => {
                    let mut sym = Symbol {
                        name: t.name.clone(),
                        kind: SymbolKind::Trait,
                        is_mut: false,
                        is_export: t.is_export,
                        span: t.span.clone(),
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        base_type: None,
                        compositions: t.super_traits.clone(),
                        generic_params: t.generic_params.clone(),
                        type_node: None,
                        return_type: None,
                    };
                    for m in &t.methods {
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
                    self.traits.insert(t.name.clone(), sym.clone());
                    self.scopes[0].define(t.name.clone(), sym);
                }

                Decl::Impl(i) => {
                    if let Some(target_class) = self.classes.get_mut(&i.target_type) {
                        for m in &i.methods {
                            target_class.methods.insert(
                                m.name.clone(),
                                Symbol {
                                    name: m.name.clone(),
                                    kind: SymbolKind::Method,
                                    is_mut: false,
                                    is_export: m.is_export,
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
                        // Keep the first declaration; do not overwrite.
                        continue;
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
                        // Keep the first declaration; do not overwrite.
                        continue;
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
                        // Keep the first declaration; do not overwrite.
                        continue;
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
                Decl::CImport(_) => {}
            }
        }

        // Pass 2a: Merge base class inheritance (from) and component compositions (+) into classes.
        // Deterministic: classes are processed in sorted name order and each
        // class's parent/component chain is merged recursively (parent-first)
        // with cycle detection, replacing the old 10-iteration fixpoint over
        // HashMap-ordered names.
        let mut class_names: Vec<String> = self.classes.keys().cloned().collect();
        class_names.sort();
        let mut visiting: HashSet<String> = HashSet::new();
        let mut resolved: HashSet<String> = HashSet::new();
        for cls_name in &class_names {
            self.merge_class_hierarchy(cls_name, &mut visiting, &mut resolved, diag);
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
                    let mut req_methods: Vec<&String> = role_sym.methods.keys().collect();
                    req_methods.sort();
                    for req_method in req_methods {
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

    /// Recursively merge a class's inherited base members and composed
    /// component members into its field/method sets. Parents are resolved
    /// before children so deep chains merge completely in one deterministic
    /// pass. `visiting` detects circular inheritance; `resolved` makes each
    /// class's merge happen exactly once.
    fn merge_class_hierarchy(
        &mut self,
        cls_name: &str,
        visiting: &mut HashSet<String>,
        resolved: &mut HashSet<String>,
        diag: &mut DiagnosticEngine,
    ) {
        if resolved.contains(cls_name) {
            return;
        }
        if !visiting.insert(cls_name.to_string()) {
            let span = self
                .classes
                .get(cls_name)
                .map(|c| c.span.clone())
                .unwrap_or_default();
            diag.error(
                ErrorCode::ResolveCircularDependency,
                format!(
                    "Circular inheritance detected involving class '{}'",
                    cls_name
                ),
                Some(span),
            );
            return;
        }

        let (base_class, compositions) = self
            .classes
            .get(cls_name)
            .map(|c| (c.base_type.clone(), c.compositions.clone()))
            .unwrap_or_default();

        // Resolve the parent chain (and any composed classes) first so their
        // inherited members are already in place before merging downward.
        if let Some(base_name) = &base_class
            && base_name != cls_name
            && self.classes.contains_key(base_name)
        {
            self.merge_class_hierarchy(base_name, visiting, resolved, diag);
        }
        for comp_name in &compositions {
            if comp_name != cls_name && self.classes.contains_key(comp_name) {
                self.merge_class_hierarchy(comp_name, visiting, resolved, diag);
            }
        }

        // Inherit from base_class
        if let Some(base_name) = &base_class
            && let Some(base_sym) = self.classes.get(base_name).cloned()
            && let Some(cls_sym) = self.classes.get_mut(cls_name)
        {
            for (f_name, f_sym) in &base_sym.fields {
                if !cls_sym.fields.contains_key(f_name) {
                    cls_sym.fields.insert(f_name.clone(), f_sym.clone());
                }
            }
            for (m_name, m_sym) in &base_sym.methods {
                if !cls_sym.methods.contains_key(m_name) {
                    cls_sym.methods.insert(m_name.clone(), m_sym.clone());
                }
            }
        }

        // Inline components or used classes (components merge one level,
        // matching the previous behavior).
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
                    }
                }
                for (m_name, m_sym) in &comp_sym.methods {
                    if !cls_sym.methods.contains_key(m_name) {
                        cls_sym.methods.insert(m_name.clone(), m_sym.clone());
                    }
                }
            }
        }

        visiting.remove(cls_name);
        resolved.insert(cls_name.to_string());
    }

    pub(crate) fn define_local(
        &mut self,
        name: &str,
        kind: SymbolKind,
        is_mut: bool,
        span: &SourceSpan,
    ) {
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
        let lookup_name = if name == "Self" {
            self.current_target_type.as_deref().unwrap_or(name)
        } else {
            name
        };
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(lookup_name) {
                return Some(sym);
            }
        }
        None
    }

    pub(crate) fn enter_scope(&mut self, name: &str) {
        self.scopes.push(Scope::new(name));
    }

    pub(crate) fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn test_resolver_role_unsatisfied_deterministic() {
        let src = r#"
role Worker {
    fn alpha();
    fn beta();
}
class Employee with Worker {
}
"#;
        let mut diag = DiagnosticEngine::new("en");
        let mut lexer = Lexer::new(src, "test.dtr");
        let tokens = lexer.tokenize(&mut diag);
        let mut parser = Parser::new(tokens, &mut diag, "test.dtr");
        let program = parser.parse_program();

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program, &mut diag);
        assert!(diag.has_errors());
    }
}
