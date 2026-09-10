use crate::ast::*;
use crate::resolver::Resolver;
use crate::types::{DataraType, TypeChecker};
use std::collections::HashMap;

pub mod cfg;
pub mod ir;
pub mod verifier;

pub use ir::*;
pub use verifier::{verify_function, verify_module};

pub struct Lowering<'a> {
    pub resolver: &'a Resolver,
    pub types: &'a TypeChecker<'a>,
    pub val_counter: usize,
    pub block_counter: usize,
    pub symbol_values: HashMap<String, ValueId>,
    pub current_blocks: Vec<BasicBlock>,
    pub class_field_types: HashMap<String, String>,
    pub function_return_types: HashMap<String, String>,
    pub current_fn_name: String,
    pub local_var_types: HashMap<String, DataraType>,
    pub enum_variant_tags: HashMap<String, i64>,
    pub enum_variant_names: HashMap<i64, String>,
    pub enum_slots: HashMap<String, Vec<String>>,
    pub current_line_spans: Vec<crate::diagnostics::SourceSpan>,
    pub in_wrapping_mode: bool,
    pub in_saturating_mode: bool,
    pub local_lambdas: HashMap<String, (Vec<Param>, Expr)>,
}

/// How a match/decide/select arm's condition is produced during CFG-based
/// arm lowering. `Pattern` arms reuse the eager lowering's pattern logic.
enum ArmCond<'a> {
    Expr(&'a Expr),
    Pattern {
        val: ValueId,
        pattern: &'a Pattern,
        guard: Option<&'a Expr>,
    },
}

impl<'a> Lowering<'a> {
    pub fn new(resolver: &'a Resolver, types: &'a TypeChecker<'a>) -> Self {
        let mut function_return_types = HashMap::new();
        function_return_types.insert("str_to_int".into(), "Int".into());
        function_return_types.insert("datara_rt_str_to_int".into(), "Int".into());
        function_return_types.insert("str_to_float".into(), "Float".into());
        function_return_types.insert("datara_rt_str_to_float".into(), "Float".into());
        function_return_types.insert("str_index_of".into(), "Int".into());
        function_return_types.insert("datara_rt_str_index_of".into(), "Int".into());
        function_return_types.insert("str_contains".into(), "Bool".into());
        function_return_types.insert("datara_rt_str_contains".into(), "Bool".into());
        function_return_types.insert("str_starts_with".into(), "Bool".into());
        function_return_types.insert("datara_rt_str_starts_with".into(), "Bool".into());
        function_return_types.insert("str_ends_with".into(), "Bool".into());
        function_return_types.insert("datara_rt_str_ends_with".into(), "Bool".into());
        function_return_types.insert("str_trim".into(), "String".into());
        function_return_types.insert("datara_rt_str_trim".into(), "String".into());
        function_return_types.insert("input".into(), "String".into());
        function_return_types.insert("read_line".into(), "String".into());
        function_return_types.insert("datara_rt_input".into(), "String".into());
        function_return_types.insert("http_get".into(), "String".into());
        function_return_types.insert("datara_rt_http_get".into(), "String".into());
        function_return_types.insert("file_read".into(), "String".into());
        function_return_types.insert("read_file".into(), "String".into());
        function_return_types.insert("datara_rt_file_read".into(), "String".into());
        function_return_types.insert("file_write".into(), "Int".into());
        function_return_types.insert("write_file".into(), "Int".into());
        function_return_types.insert("datara_rt_file_write".into(), "Int".into());
        function_return_types.insert("file_append".into(), "Int".into());
        function_return_types.insert("datara_rt_file_append".into(), "Int".into());
        function_return_types.insert("file_exists".into(), "Bool".into());
        function_return_types.insert("datara_rt_file_exists".into(), "Bool".into());
        function_return_types.insert("args_count".into(), "Int".into());
        function_return_types.insert("datara_rt_args_count".into(), "Int".into());
        function_return_types.insert("args_get".into(), "String".into());
        function_return_types.insert("datara_rt_args_get".into(), "String".into());
        function_return_types.insert("env_get".into(), "String".into());
        function_return_types.insert("datara_rt_env_get".into(), "String".into());
        function_return_types.insert("path_join".into(), "String".into());
        function_return_types.insert("datara_rt_path_join".into(), "String".into());
        function_return_types.insert("now".into(), "Int".into());
        function_return_types.insert("now_ms".into(), "Int".into());
        function_return_types.insert("now_ns".into(), "Int".into());
        function_return_types.insert("datara_rt_now_ns".into(), "Int".into());
        function_return_types.insert("now_precise_ms".into(), "Int".into());
        function_return_types.insert("length".into(), "Int".into());
        function_return_types.insert("count".into(), "Int".into());
        function_return_types.insert("map".into(), "List".into());
        function_return_types.insert("filter".into(), "List".into());
        function_return_types.insert("reduce".into(), "Int".into());
        function_return_types.insert("find".into(), "Int".into());
        function_return_types.insert("any".into(), "Bool".into());
        function_return_types.insert("all".into(), "Bool".into());
        function_return_types.insert("str_len".into(), "Int".into());
        function_return_types.insert("datara_rt_str_len".into(), "Int".into());
        function_return_types.insert("int_to_str".into(), "String".into());
        function_return_types.insert("datara_rt_int_to_str".into(), "String".into());
        function_return_types.insert("float_to_str".into(), "String".into());
        function_return_types.insert("datara_rt_float_to_str".into(), "String".into());
        function_return_types.insert("socket_create".into(), "Int".into());
        function_return_types.insert("socket_bind".into(), "Int".into());
        function_return_types.insert("socket_listen".into(), "Int".into());
        function_return_types.insert("socket_accept".into(), "Int".into());
        function_return_types.insert("socket_connect".into(), "Int".into());
        function_return_types.insert("socket_send".into(), "Int".into());
        function_return_types.insert("socket_recv".into(), "String".into());
        function_return_types.insert("socket_close".into(), "Unit".into());
        function_return_types.insert("sha256".into(), "String".into());
        function_return_types.insert("base64_encode".into(), "String".into());
        function_return_types.insert("base64_decode".into(), "String".into());
        function_return_types.insert("uuid_v4".into(), "String".into());
        function_return_types.insert("datara_rt_uuid_v4".into(), "String".into());
        function_return_types.insert("datara_rt_dialog_info".into(), "Int".into());
        function_return_types.insert("datara_rt_dialog_alert".into(), "Int".into());
        function_return_types.insert("datara_rt_dialog_confirm".into(), "Int".into());
        function_return_types.insert("process_run".into(), "Int".into());
        function_return_types.insert("system".into(), "Int".into());
        function_return_types.insert("process_output".into(), "String".into());
        function_return_types.insert("exec".into(), "String".into());
        for f in &[
            "math_sqrt",
            "datara_rt_math_sqrt",
            "math_pow",
            "datara_rt_math_pow",
            "math_abs",
            "datara_rt_math_abs",
            "math_sin",
            "datara_rt_math_sin",
            "math_cos",
            "datara_rt_math_cos",
            "math_tan",
            "datara_rt_math_tan",
            "math_floor",
            "datara_rt_math_floor",
            "math_ceil",
            "datara_rt_math_ceil",
            "math_round",
            "datara_rt_math_round",
            "math_min",
            "datara_rt_math_min",
            "math_max",
            "datara_rt_math_max",
            "math_hypot",
            "datara_rt_math_hypot",
            "math_log",
            "datara_rt_math_log",
            "math_exp",
            "datara_rt_math_exp",
            "math_clamp",
            "datara_rt_math_clamp",
            "clamp",
        ] {
            function_return_types.insert((*f).into(), "Float".into());
        }
        for f in &[
            "math_min_int",
            "datara_rt_math_min_int",
            "math_max_int",
            "datara_rt_math_max_int",
            "math_clamp_int",
            "datara_rt_math_clamp_int",
            "clamp_int",
            "math_abs_int",
            "datara_rt_math_abs_int",
            "math_ctz",
            "datara_rt_math_ctz",
            "ctz",
            "math_shr",
            "datara_rt_math_shr",
            "shr",
            "math_shl",
            "datara_rt_math_shl",
            "shl",
            "math_xor",
            "datara_rt_math_xor",
            "xor",
            "math_and",
            "datara_rt_math_and",
            "and",
            "math_or",
            "datara_rt_math_or",
            "or",
        ] {
            function_return_types.insert((*f).into(), "Int".into());
        }
        for f in &["int4", "datara_rt_int4"] {
            function_return_types.insert((*f).into(), "Int4".into());
        }
        for f in &["float4", "datara_rt_float4", "min4", "max4"] {
            function_return_types.insert((*f).into(), "Float4".into());
        }
        for f in &[
            "dot",
            "datara_rt_float4_dot",
            "float4_x",
            "float4_y",
            "float4_z",
            "float4_w",
            "lane0",
            "lane1",
            "lane2",
            "lane3",
        ] {
            function_return_types.insert((*f).into(), "Float".into());
        }
        for f in &["int4_x", "int4_y", "int4_z", "int4_w"] {
            function_return_types.insert((*f).into(), "Int".into());
        }

        Self {
            resolver,
            types,
            val_counter: 0,
            block_counter: 0,
            symbol_values: HashMap::new(),
            current_blocks: Vec::new(),
            class_field_types: HashMap::new(),
            function_return_types,
            current_fn_name: String::new(),
            local_var_types: HashMap::new(),
            enum_variant_tags: HashMap::new(),
            enum_variant_names: HashMap::new(),
            enum_slots: HashMap::new(),
            current_line_spans: Vec::new(),
            in_wrapping_mode: false,
            in_saturating_mode: false,
            local_lambdas: HashMap::new(),
        }
    }

    pub fn lookup_var_type(&self, var_name: &str) -> Option<DataraType> {
        if let Some(ty) = self.local_var_types.get(var_name) {
            return Some(ty.clone());
        }
        if !self.current_fn_name.is_empty()
            && let Some(ty) = self
                .types
                .fn_symbol_types
                .get(&(self.current_fn_name.clone(), var_name.to_string()))
        {
            return Some(ty.clone());
        }
        if let Some(ty) = self.types.symbol_types.get(var_name) {
            return Some(ty.clone());
        }
        None
    }

    pub fn next_val(&mut self) -> ValueId {
        let v = ValueId(self.val_counter);
        self.val_counter += 1;
        v
    }

    pub fn create_block(&mut self, label: &str) -> BasicBlockId {
        let id = BasicBlockId(self.block_counter);
        self.block_counter += 1;
        self.current_blocks.push(BasicBlock {
            id,
            label: format!("{}_{}", label, id.0),
            params: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        id
    }

    pub fn get_block_mut(&mut self, id: BasicBlockId) -> &mut BasicBlock {
        if id.0 < self.current_blocks.len() && self.current_blocks[id.0].id == id {
            return &mut self.current_blocks[id.0];
        }
        self.current_blocks
            .iter_mut()
            .find(|b| b.id == id)
            .expect("Block must exist")
    }

    /// True when `id` still has the placeholder terminator, i.e. control really
    /// reaches the end of the block.
    pub fn block_falls_through(&self, id: BasicBlockId) -> bool {
        if id.0 < self.current_blocks.len() && self.current_blocks[id.0].id == id {
            return matches!(
                self.current_blocks[id.0].terminator,
                Terminator::Unreachable
            );
        }
        self.current_blocks
            .iter()
            .find(|b| b.id == id)
            .map(|b| matches!(b.terminator, Terminator::Unreachable))
            .unwrap_or(false)
    }

    /// Wires a loop back-edge from `from` to `target`.
    ///
    /// The edge is only installed when the block still falls through. A body
    /// that ends in `return` already has `Terminator::Return`; overwriting it
    /// would silently discard the early return and produce an infinite loop.
    pub fn set_back_edge(&mut self, from: BasicBlockId, target: BasicBlockId) {
        if self.block_falls_through(from) {
            self.get_block_mut(from).terminator = Terminator::Branch {
                target,
                args: Vec::new(),
            };
        }
    }

    /// Runtime representation string of a declared type.
    ///
    /// The abstract Result/Option spellings map to their concrete stdlib
    /// representations so the backend sees a class type (a returned pointer)
    /// instead of a bare scalar: `T!E`/`Result<T, E>` -> `Outcome<T>`,
    /// `T?`/`Option<T>` -> `Maybe<T>`. `full_type_name()` alone would reduce
    /// `Int!String` to "Int" and make the backend treat a returned Outcome
    /// object as an integer.
    fn repr_type_string(tn: &TypeNode) -> String {
        let is_result = tn.error_type.is_some() || tn.name == "Result";
        let is_option = tn.is_option || tn.name == "Option";
        if is_result {
            let ok = if tn.name == "Result" && !tn.generic_args.is_empty() {
                tn.generic_args[0].full_type_name()
            } else {
                tn.full_type_name()
            };
            format!("Outcome<{}>", ok)
        } else if is_option {
            let inner = if tn.name == "Option" && !tn.generic_args.is_empty() {
                tn.generic_args[0].full_type_name()
            } else {
                tn.full_type_name()
            };
            format!("Maybe<{}>", inner)
        } else if matches!(
            tn.name.as_str(),
            "Float"
                | "Float64"
                | "Float32"
                | "Int"
                | "Int64"
                | "Int32"
                | "Int16"
                | "Int8"
                | "UInt"
                | "UInt64"
                | "UInt32"
                | "UInt16"
                | "UInt8"
                | "Byte"
        ) {
            tn.name.clone()
        } else {
            tn.full_type_name()
        }
    }

    pub fn lower_program(&mut self, program: &Program, name: &str) -> Module {
        let mut module = Module::new(name);
        module.link_libraries = program.link_libraries.clone();

        for decl in &program.declarations {
            if let Decl::Class(c) = decl {
                for item in &c.body_items {
                    if let ClassItem::Field(f) = item {
                        if let Some(t) = &f.type_node {
                            self.class_field_types
                                .insert(format!("{}.{}", c.name, f.name), t.full_type_name());
                            self.class_field_types
                                .insert(f.name.clone(), t.full_type_name());
                        }
                    } else if let ClassItem::Method(m) = item {
                        let ret = m
                            .return_type
                            .as_ref()
                            .map(|t| t.full_type_name())
                            .unwrap_or_else(|| "Unit".into());
                        self.function_return_types
                            .insert(format!("{}_{}", c.name, m.name), ret.clone());
                        self.function_return_types.insert(m.name.clone(), ret);
                    }
                }
            } else if let Decl::Component(c) = decl {
                for item in &c.body_items {
                    if let ClassItem::Field(f) = item
                        && let Some(t) = &f.type_node
                    {
                        self.class_field_types
                            .insert(format!("{}.{}", c.name, f.name), t.full_type_name());
                        self.class_field_types
                            .insert(f.name.clone(), t.full_type_name());
                    }
                }
            } else if let Decl::Behavior(b) = decl {
                for item in &b.body_items {
                    if let ClassItem::Field(f) = item {
                        if let Some(t) = &f.type_node {
                            self.class_field_types.insert(
                                format!("{}.{}", b.target_type, f.name),
                                t.full_type_name(),
                            );
                            self.class_field_types
                                .insert(f.name.clone(), t.full_type_name());
                        }
                    } else if let ClassItem::Method(m) = item {
                        let ret = m
                            .return_type
                            .as_ref()
                            .map(Self::repr_type_string)
                            .unwrap_or_else(|| "Unit".into());
                        self.function_return_types
                            .insert(format!("{}_{}", b.target_type, m.name), ret.clone());
                        self.function_return_types.insert(m.name.clone(), ret);
                    }
                }
            } else if let Decl::Enum(e) = decl {
                let max_fields = e.variants.iter().map(|v| v.fields.len()).max().unwrap_or(0);
                let mut slot_types: Vec<String> = vec!["Int".to_string(); max_fields];
                for v in &e.variants {
                    for (idx, fty) in v.fields.iter().enumerate() {
                        slot_types[idx] = fty.full_type_name();
                    }
                }
                for (v_idx, v) in e.variants.iter().enumerate() {
                    let full_vname = format!("{}_{}", e.name, v.name);
                    self.enum_variant_tags
                        .insert(format!("{}.{}", e.name, v.name), v_idx as i64);
                    self.enum_variant_tags.insert(v.name.clone(), v_idx as i64);
                    self.enum_variant_names
                        .insert(v_idx as i64, full_vname.clone());
                    self.enum_slots
                        .insert(format!("{}.{}", e.name, v.name), slot_types.clone());
                    self.enum_slots.insert(v.name.clone(), slot_types.clone());
                    self.class_field_types
                        .insert(format!("{}.__tag", full_vname), "Int".into());
                    self.class_field_types.insert("__tag".into(), "Int".into());
                    for (s_idx, s_ty) in slot_types.iter().enumerate() {
                        self.class_field_types
                            .insert(format!("{}.f{}", full_vname, s_idx), s_ty.clone());
                        self.class_field_types
                            .insert(format!("f{}", s_idx), s_ty.clone());
                    }
                }
            } else if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl {
                let ret = f
                    .return_type
                    .as_ref()
                    .map(Self::repr_type_string)
                    .unwrap_or_else(|| "Unit".into());
                self.function_return_types.insert(f.name.clone(), ret);
            } else if let Decl::ExternFn(ef) = decl {
                let ret = ef
                    .return_type
                    .as_ref()
                    .map(Self::repr_type_string)
                    .unwrap_or_else(|| "Unit".into());
                let params: Vec<String> = ef
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(Self::repr_type_string)
                            .unwrap_or_else(|| "Int".into())
                    })
                    .collect();
                module
                    .extern_functions
                    .insert(ef.name.clone(), (params, ret.clone()));
                self.function_return_types.insert(ef.name.clone(), ret);
            } else if let Decl::Impl(i) = decl {
                for m in &i.methods {
                    let ret = m
                        .return_type
                        .as_ref()
                        .map(Self::repr_type_string)
                        .unwrap_or_else(|| "Unit".into());
                    self.function_return_types
                        .insert(format!("{}_{}", i.target_type, m.name), ret.clone());
                    self.function_return_types.insert(m.name.clone(), ret);
                }
            }
        }

        for (cls_name, cls_sym) in &self.resolver.classes {
            let mut f_names: Vec<String> = cls_sym.fields.keys().cloned().collect();
            f_names.sort();
            module.class_fields.insert(cls_name.clone(), f_names);
        }

        for decl in &program.declarations {
            match decl {
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    let lowered_fn = self.lower_function(f);
                    module.functions.insert(f.name.clone(), lowered_fn);
                    module.function_spans.insert(f.name.clone(), f.span.clone());
                    module
                        .function_line_spans
                        .insert(f.name.clone(), self.current_line_spans.clone());
                }
                Decl::Class(c) => {
                    self.lower_class(c, program, &mut module);
                }
                Decl::Behavior(b) => {
                    self.lower_behavior(b, &mut module);
                }
                Decl::Impl(i) => {
                    self.lower_impl(i, &mut module);
                }
                _ => {}
            }
        }

        // Static Monomorphization: instantiate generic functions per concrete type argument
        for decl in &program.declarations {
            if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl {
                if !f.generic_params.is_empty() {
                    if let Some(specs) = self.types.generic_specializations.get(&f.name) {
                        for spec_args in specs {
                            let mut type_substs: HashMap<String, String> = HashMap::new();
                            let mut mangled_suffixes = Vec::new();
                            for (gp, concrete_ty) in f.generic_params.iter().zip(spec_args.iter()) {
                                let c_name = match concrete_ty {
                                    DataraType::Class(c) => c.clone(),
                                    DataraType::Int => "Int".to_string(),
                                    DataraType::Float => "Float".to_string(),
                                    DataraType::String => "String".to_string(),
                                    DataraType::Bool => "Bool".to_string(),
                                    other => other.to_string(),
                                };
                                type_substs.insert(gp.clone(), c_name.clone());
                                mangled_suffixes.push(c_name);
                            }
                            let mangled_name = format!("{}_{}", f.name, mangled_suffixes.join("_"));
                            let specialized_f =
                                self.specialize_function_decl(f, &mangled_name, &type_substs);
                            let ret = specialized_f
                                .return_type
                                .as_ref()
                                .map(Self::repr_type_string)
                                .unwrap_or_else(|| "Unit".into());
                            self.function_return_types.insert(mangled_name.clone(), ret);
                            let lowered_spec = self.lower_function(&specialized_f);
                            module.functions.insert(mangled_name.clone(), lowered_spec);
                            module
                                .function_spans
                                .insert(mangled_name.clone(), f.span.clone());
                            module
                                .function_line_spans
                                .insert(mangled_name.clone(), self.current_line_spans.clone());
                        }
                    }
                }
            }
        }

        module.class_field_types = self.class_field_types.clone();
        module
    }

    fn lower_function(&mut self, f: &FunctionDecl) -> Function {
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

    fn lower_class(&mut self, c: &ClassDecl, program: &Program, module: &mut Module) {
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

    fn lower_behavior(&mut self, b: &BehaviorDecl, module: &mut Module) {
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

    fn lower_impl(&mut self, i: &ImplBlock, module: &mut Module) {
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

    fn lower_impl_function(&mut self, f: &FunctionDecl, target_type: &str) -> Function {
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

    fn specialize_function_decl(
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

    fn subst_type_node(tn: &mut TypeNode, type_substs: &HashMap<String, String>) {
        if let Some(concrete) = type_substs.get(&tn.name) {
            tn.name = concrete.clone();
        }
        for arg in &mut tn.generic_args {
            Self::subst_type_node(arg, type_substs);
        }
    }

    fn subst_stmt(stmt: &mut Stmt, type_substs: &HashMap<String, String>) {
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
            Stmt::Return(opt_e, _) => {
                if let Some(e) = opt_e {
                    Self::subst_expr(e, type_substs);
                }
            }
            _ => {}
        }
    }

    fn subst_expr(expr: &mut Expr, type_substs: &HashMap<String, String>) {
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

    fn lower_method(&mut self, m: &MethodDecl, class_name: &str) -> Function {
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
                    if let Some(ty) = self.lookup_var_type(name) {
                        self.local_var_types.insert(name.clone(), ty);
                    }
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
                                let elem_repr = match self.lookup_var_type(var_name) {
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

    /// Computes a match arm's condition value (pattern comparison + optional
    /// guard), shared by the eager Decide path and the CFG arm path.
    fn lower_match_arm_cond(
        &mut self,
        val: ValueId,
        pattern: &Pattern,
        guard: Option<&Expr>,
        cur_block: &mut BasicBlockId,
    ) -> ValueId {
        let cond = match pattern {
            Pattern::Literal(lit, span) => {
                let lit_expr = Expr::Literal(lit.clone(), span.clone());
                if let Some(lit_val) = self.lower_expr(&lit_expr, cur_block) {
                    let eq_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::BinOp {
                            dest: eq_dest,
                            op: "==".into(),
                            left: val,
                            right: lit_val,
                            ty: "Bool".into(),
                        });
                    eq_dest
                } else {
                    val
                }
            }
            Pattern::Identifier(name, _) if name == "_" => {
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
            Pattern::Identifier(name, _) => {
                self.symbol_values.insert(name.clone(), val);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: name.clone(),
                        value: val,
                    });
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
            Pattern::Variant {
                enum_name,
                variant_name,
                bindings,
                ..
            } => {
                let expected_tag = if let Some(en) = enum_name {
                    self.enum_variant_tags
                        .get(&format!("{}.{}", en, variant_name))
                        .copied()
                } else {
                    self.enum_variant_tags.get(variant_name).copied()
                };

                if let Some(tag) = expected_tag {
                    let tag_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::GetField {
                            dest: tag_dest,
                            object: val,
                            field: "__tag".to_string(),
                            ty: "Int".to_string(),
                        });
                    let const_tag = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: const_tag,
                            value: tag,
                        });
                    let eq_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::BinOp {
                            dest: eq_dest,
                            op: "==".into(),
                            left: tag_dest,
                            right: const_tag,
                            ty: "Bool".into(),
                        });

                    for (idx, b_name) in bindings.iter().enumerate() {
                        let field_dest = self.next_val();
                        let field_name = format!("f{}", idx);
                        let field_ty = self
                            .class_field_types
                            .get(&field_name)
                            .cloned()
                            .unwrap_or_else(|| "Int".into());
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::GetField {
                                dest: field_dest,
                                object: val,
                                field: field_name,
                                ty: field_ty,
                            });
                        self.symbol_values.insert(b_name.clone(), field_dest);
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::AssignVar {
                                name: b_name.clone(),
                                value: field_dest,
                            });
                    }
                    eq_dest
                } else {
                    let true_val = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstBool {
                            dest: true_val,
                            value: true,
                        });
                    true_val
                }
            }
            _ => {
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
        };

        if let Some(g) = guard
            && let Some(g_val) = self.lower_expr(g, cur_block)
        {
            let and_dest = self.next_val();
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::BinOp {
                    dest: and_dest,
                    op: "&&".into(),
                    left: cond,
                    right: g_val,
                    ty: "Bool".into(),
                });
            return and_dest;
        }
        cond
    }

    /// CFG-based lowering for a `match` whose arms have block bodies.
    fn lower_match_arms_cfg(
        &mut self,
        val: ValueId,
        arms: &[MatchArm],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let cfg_arms: Vec<(ArmCond<'_>, &Expr)> = arms
            .iter()
            .map(|arm| {
                (
                    ArmCond::Pattern {
                        val,
                        pattern: &arm.pattern,
                        guard: arm.guard.as_ref(),
                    },
                    &arm.body,
                )
            })
            .collect();
        self.lower_arm_conds_cfg(&cfg_arms, None, cur_block)
    }

    /// Lowers match/decide/select arms as real control flow: each arm body is
    /// lowered into its own block and its value stored to a temp, so block
    /// bodies with side effects only execute when their arm is selected.
    /// Control falls through the condition chain into the merge block, which
    /// falls through to whatever the surrounding lowering emits next (the same
    /// pattern as Stmt::If).
    fn lower_arm_conds_cfg(
        &mut self,
        arms: &[(ArmCond<'_>, &Expr)],
        else_body: Option<&Expr>,
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let temp = format!("__match_val_{}", self.val_counter);
        // Zero-init so a fall-through (no arm taken, no else) yields 0.
        let zero = self.next_val();
        self.get_block_mut(*cur_block)
            .instructions
            .push(Inst::ConstInt {
                dest: zero,
                value: 0,
            });
        self.symbol_values.insert(temp.clone(), zero);
        self.get_block_mut(*cur_block)
            .instructions
            .push(Inst::AssignVar {
                name: temp.clone(),
                value: zero,
            });

        let mut arm_ends: Vec<BasicBlockId> = Vec::new();
        for (cond_src, body) in arms {
            let cond = match cond_src {
                ArmCond::Expr(cond_expr) => {
                    self.lower_expr(cond_expr, cur_block).unwrap_or_else(|| {
                        let t = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstBool {
                                dest: t,
                                value: true,
                            });
                        t
                    })
                }
                ArmCond::Pattern {
                    val,
                    pattern,
                    guard,
                } => self.lower_match_arm_cond(*val, pattern, *guard, cur_block),
            };
            let arm_id = self.create_block("match_arm");
            let next_id = self.create_block("match_next");
            self.get_block_mut(*cur_block).terminator = Terminator::CondBranch {
                cond,
                then_block: arm_id,
                then_args: Vec::new(),
                else_block: next_id,
                else_args: Vec::new(),
            };
            let mut body_block = arm_id;
            let body_val = self.lower_expr(body, &mut body_block);
            if let Some(bv) = body_val {
                self.symbol_values.insert(temp.clone(), bv);
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: temp.clone(),
                        value: bv,
                    });
            }
            if self.block_falls_through(body_block) {
                arm_ends.push(body_block);
            }
            *cur_block = next_id;
        }
        if let Some(eb) = else_body {
            // The else body lowers into the final "next" block, which only
            // runs when no arm matched — never into the merge itself, or it
            // would overwrite the selected arm's value.
            let else_val = self.lower_expr(eb, cur_block);
            if let Some(bv) = else_val {
                self.symbol_values.insert(temp.clone(), bv);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: temp.clone(),
                        value: bv,
                    });
            }
        }

        // With an else body the last "next" block is the else block, so the
        // merge point is a fresh block; without one the fall-through block is
        // the merge. Arm blocks branch forward into it (back-fixup now that
        // its id is known).
        let merge_id = if else_body.is_some() {
            let m = self.create_block("match_merge");
            if self.block_falls_through(*cur_block) {
                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: m,
                    args: Vec::new(),
                };
            }
            *cur_block = m;
            m
        } else {
            *cur_block
        };
        for b in arm_ends {
            self.get_block_mut(b).terminator = Terminator::Branch {
                target: merge_id,
                args: Vec::new(),
            };
        }
        let dest = self.next_val();
        self.get_block_mut(merge_id)
            .instructions
            .push(Inst::LoadVar {
                dest,
                name: temp.clone(),
            });
        Some(dest)
    }

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
            Expr::Call { callee, args, .. } => {
                if let Expr::Lambda { params, body, .. } = &**callee {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        if let Some(av) = self.lower_expr(a, cur_block) {
                            arg_vals.push(av);
                        }
                    }
                    for (p, aval) in params.iter().zip(arg_vals) {
                        self.symbol_values.insert(p.name.clone(), aval);
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::AssignVar {
                                name: p.name.clone(),
                                value: aval,
                            });
                    }
                    return self.lower_expr(body, cur_block);
                }
                if let Expr::Identifier(fn_name, _) = &**callee {
                    if let Some((params, body)) = self.local_lambdas.get(fn_name).cloned() {
                        let mut arg_vals = Vec::new();
                        for a in args {
                            if let Some(av) = self.lower_expr(a, cur_block) {
                                arg_vals.push(av);
                            }
                        }
                        for (p, aval) in params.iter().zip(arg_vals) {
                            self.symbol_values.insert(p.name.clone(), aval);
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::AssignVar {
                                    name: p.name.clone(),
                                    value: aval,
                                });
                        }
                        return self.lower_expr(&body, cur_block);
                    }
                    if (fn_name == "view"
                        || fn_name == "borrow"
                        || fn_name == "clone"
                        || fn_name == "move")
                        && args.len() == 1
                    {
                        return self.lower_expr(&args[0], cur_block);
                    }
                    if (fn_name == "destroy" || fn_name == "drop") && args.len() == 1 {
                        let arg_val = self.lower_expr(&args[0], cur_block);
                        let dest = self.next_val();
                        let mut call_args = Vec::new();
                        if let Some(v) = arg_val {
                            call_args.push(v);
                        }
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: fn_name.clone(),
                                args: call_args,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "println" || fn_name == "print" {
                        if args.is_empty() {
                            let dest = self.next_val();
                            if fn_name == "println" {
                                self.get_block_mut(*cur_block)
                                    .instructions
                                    .push(Inst::Call {
                                        dest,
                                        func: "datara_rt_print_newline".into(),
                                        args: vec![],
                                        ty: "Unit".into(),
                                    });
                            } else {
                                self.get_block_mut(*cur_block)
                                    .instructions
                                    .push(Inst::ConstInt { dest, value: 0 });
                            }
                            return Some(dest);
                        }

                        for (idx, arg) in args.iter().enumerate() {
                            if idx > 0 {
                                let sp_dest = self.next_val();
                                self.get_block_mut(*cur_block)
                                    .instructions
                                    .push(Inst::Call {
                                        dest: sp_dest,
                                        func: "datara_rt_print_space".into(),
                                        args: vec![],
                                        ty: "Unit".into(),
                                    });
                            }

                            let arg_val = self.lower_expr(arg, cur_block)?;
                            let print_func = if self.is_expr_str(arg) {
                                "datara_rt_print_str"
                            } else if self.is_expr_float(arg) {
                                "datara_rt_print_float"
                            } else if self.is_expr_bool(arg) {
                                "datara_rt_print_bool"
                            } else if self.is_expr_list(arg) {
                                "datara_rt_print_list"
                            } else {
                                "datara_rt_print_int"
                            };

                            let dest = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::Call {
                                    dest,
                                    func: print_func.into(),
                                    args: vec![arg_val],
                                    ty: "Unit".into(),
                                });
                        }

                        let final_dest = self.next_val();
                        if fn_name == "println" {
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::Call {
                                    dest: final_dest,
                                    func: "datara_rt_print_newline".into(),
                                    args: vec![],
                                    ty: "Unit".into(),
                                });
                        } else {
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::Call {
                                    dest: final_dest,
                                    func: "datara_rt_flush".into(),
                                    args: vec![],
                                    ty: "Unit".into(),
                                });
                        }

                        return Some(final_dest);
                    }
                    if fn_name == "eprintln" && args.len() == 1 {
                        let arg_val = self.lower_expr(&args[0], cur_block)?;
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Err { value: arg_val });
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt { dest, value: 0 });
                        return Some(dest);
                    }
                    if fn_name == "len" && args.len() == 1 {
                        let arg_val = self.lower_expr(&args[0], cur_block)?;
                        let dest = self.next_val();
                        let func = if self.is_expr_str(&args[0]) {
                            "datara_rt_len"
                        } else {
                            "datara_rt_list_len"
                        };
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: func.into(),
                                args: vec![arg_val],
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "now" && args.is_empty() {
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_now_ms".into(),
                                args: vec![],
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "panic" && args.len() == 1 {
                        let arg_val = self.lower_expr(&args[0], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_panic".into(),
                                args: vec![arg_val],
                                ty: "Never".into(),
                            });
                        return Some(dest);
                    }
                    if (fn_name == "wrapping" || fn_name == "wrapping_mode") && args.len() == 1 {
                        let prev = self.in_wrapping_mode;
                        self.in_wrapping_mode = true;
                        let res = self.lower_expr(&args[0], cur_block);
                        self.in_wrapping_mode = prev;
                        return res;
                    }
                    if (fn_name == "saturating" || fn_name == "saturating_mode") && args.len() == 1
                    {
                        let prev = self.in_saturating_mode;
                        self.in_saturating_mode = true;
                        let res = self.lower_expr(&args[0], cur_block);
                        self.in_saturating_mode = prev;
                        return res;
                    }
                    if (fn_name == "map"
                        || fn_name == "filter"
                        || fn_name == "find"
                        || fn_name == "any"
                        || fn_name == "all")
                        && args.len() == 2
                        && self.is_expr_list(&args[0])
                        && self.is_callable_expr(&args[1])
                    {
                        let list_val = self.lower_expr(&args[0], cur_block)?;
                        return self.lower_list_higher_order(
                            list_val,
                            fn_name,
                            &args[1..],
                            cur_block,
                        );
                    }
                    if fn_name == "reduce"
                        && args.len() == 3
                        && self.is_expr_list(&args[0])
                        && self.is_callable_expr(&args[2])
                    {
                        let list_val = self.lower_expr(&args[0], cur_block)?;
                        return self.lower_list_higher_order(
                            list_val,
                            fn_name,
                            &args[1..],
                            cur_block,
                        );
                    }
                    if fn_name == "wrapping_add" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "wrapping_+".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "wrapping_sub" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "wrapping_-".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "wrapping_mul" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "wrapping_*".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "saturating_add" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "saturating_+".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "saturating_sub" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "saturating_-".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "saturating_mul" && args.len() == 2 {
                        let l = self.lower_expr(&args[0], cur_block)?;
                        let r = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::BinOp {
                                dest,
                                op: "saturating_*".into(),
                                left: l,
                                right: r,
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "assert" && !args.is_empty() {
                        let cond_val = self.lower_expr(&args[0], cur_block)?;
                        let msg_val = if args.len() >= 2 {
                            self.lower_expr(&args[1], cur_block)?
                        } else {
                            let m = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstStr {
                                    dest: m,
                                    value: "Assertion failed".into(),
                                });
                            m
                        };
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_assert".into(),
                                args: vec![cond_val, msg_val],
                                ty: "Unit".into(),
                            });
                        return Some(dest);
                    }
                    if fn_name == "exit" && args.len() == 1 {
                        let arg_val = self.lower_expr(&args[0], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_exit".into(),
                                args: vec![arg_val],
                                ty: "Never".into(),
                            });
                        return Some(dest);
                    }
                    if (fn_name == "input"
                        || fn_name == "read_line"
                        || fn_name == "input_int"
                        || fn_name == "input_float")
                        && args.len() <= 1
                    {
                        let prompt_val = if !args.is_empty() {
                            self.lower_expr(&args[0], cur_block)?
                        } else {
                            let empty = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstStr {
                                    dest: empty,
                                    value: "".into(),
                                });
                            empty
                        };
                        let (target_func, ret_ty) = if fn_name == "input_int" {
                            ("datara_rt_input_int", "Int")
                        } else if fn_name == "input_float" {
                            ("datara_rt_input_float", "Float")
                        } else {
                            ("datara_rt_input", "String")
                        };
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: target_func.into(),
                                args: vec![prompt_val],
                                ty: ret_ty.into(),
                            });
                        return Some(dest);
                    }
                    if (fn_name == "str_to_float" || fn_name == "datara_rt_str_to_float")
                        && args.len() == 1
                    {
                        let arg_val = self.lower_expr(&args[0], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_str_to_float".into(),
                                args: vec![arg_val],
                                ty: "Float".into(),
                            });
                        return Some(dest);
                    }

                    if let Some(&tag) = self.enum_variant_tags.get(fn_name) {
                        let tag_val = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstInt {
                                dest: tag_val,
                                value: tag,
                            });
                        let mut fields = vec![("__tag".to_string(), tag_val)];
                        for (idx, a) in args.iter().enumerate() {
                            if let Some(av) = self.lower_expr(a, cur_block) {
                                fields.push((format!("f{}", idx), av));
                            }
                        }
                        let slots = self.enum_slots.get(fn_name).cloned().unwrap_or_default();
                        for (idx, s_ty) in slots.iter().enumerate().skip(args.len()) {
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
                        let class_name = self
                            .enum_variant_names
                            .get(&tag)
                            .cloned()
                            .unwrap_or_else(|| fn_name.clone());
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::StructInit {
                                dest,
                                class_name,
                                fields,
                            });
                        return Some(dest);
                    }
                }

                if let Expr::MemberAccess { object, member, .. } = &**callee {
                    if let Expr::Identifier(class_name, _) = &**object {
                        let enum_key = format!("{}.{}", class_name, member);
                        if let Some(&tag) = self.enum_variant_tags.get(&enum_key) {
                            let tag_val = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstInt {
                                    dest: tag_val,
                                    value: tag,
                                });
                            let mut fields = vec![("__tag".to_string(), tag_val)];
                            for (idx, a) in args.iter().enumerate() {
                                if let Some(av) = self.lower_expr(a, cur_block) {
                                    fields.push((format!("f{}", idx), av));
                                }
                            }
                            let slots = self.enum_slots.get(&enum_key).cloned().unwrap_or_default();
                            for (idx, s_ty) in slots.iter().enumerate().skip(args.len()) {
                                let pad_dest = self.next_val();
                                if s_ty.contains("Float") {
                                    self.get_block_mut(*cur_block).instructions.push(
                                        Inst::ConstFloat {
                                            dest: pad_dest,
                                            value: 0.0,
                                        },
                                    );
                                } else {
                                    self.get_block_mut(*cur_block).instructions.push(
                                        Inst::ConstInt {
                                            dest: pad_dest,
                                            value: 0,
                                        },
                                    );
                                }
                                fields.push((format!("f{}", idx), pad_dest));
                            }
                            let dest = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::StructInit {
                                    dest,
                                    class_name: format!("{}_{}", class_name, member),
                                    fields,
                                });
                            return Some(dest);
                        }

                        let static_func_name = format!("{}_{}", class_name, member);
                        if self.function_return_types.contains_key(&static_func_name) {
                            let dummy_this = self.next_val();
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::ConstInt {
                                    dest: dummy_this,
                                    value: 0,
                                });
                            let mut call_args = vec![dummy_this];
                            for a in args {
                                if let Some(av) = self.lower_expr(a, cur_block) {
                                    call_args.push(av);
                                }
                            }
                            let dest = self.next_val();
                            let ret_ty = self
                                .function_return_types
                                .get(&static_func_name)
                                .cloned()
                                .unwrap_or_else(|| "Unit".into());
                            self.get_block_mut(*cur_block)
                                .instructions
                                .push(Inst::Call {
                                    dest,
                                    func: static_func_name,
                                    args: call_args,
                                    ty: ret_ty,
                                });
                            return Some(dest);
                        }
                    }

                    if member == "view" && args.is_empty() {
                        return self.lower_expr(object, cur_block);
                    }
                    if (member == "grant_readonly"
                        || member == "grant_readwrite"
                        || member == "open")
                        && args.len() == 1
                    {
                        return self.lower_expr(&args[0], cur_block);
                    }
                    if member == "read_all" {
                        let dest = self.next_val();
                        let arg = if args.is_empty() {
                            self.lower_expr(object, cur_block)?
                        } else {
                            self.lower_expr(&args[0], cur_block)?
                        };
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_file_read".into(),
                                args: vec![arg],
                                ty: "String".into(),
                            });
                        return Some(dest);
                    }
                    let obj_val = self.lower_expr(object, cur_block)?;
                    if self.is_expr_list(object) {
                        if (member == "map"
                            || member == "filter"
                            || member == "find"
                            || member == "any"
                            || member == "all")
                            && args.len() == 1
                            && self.is_callable_expr(&args[0])
                        {
                            return self.lower_list_higher_order(obj_val, member, args, cur_block);
                        }
                        if member == "reduce" && args.len() == 2 && self.is_callable_expr(&args[1])
                        {
                            return self.lower_list_higher_order(obj_val, member, args, cur_block);
                        }
                    }
                    if member == "pop" && args.is_empty() {
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_list_pop".into(),
                                args: vec![obj_val],
                                ty: "Int".into(),
                            });
                        return Some(dest);
                    }
                    if member == "insert" && args.len() == 2 {
                        let k = self.lower_expr(&args[0], cur_block)?;
                        let v = self.lower_expr(&args[1], cur_block)?;
                        let dest = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::Call {
                                dest,
                                func: "datara_rt_map_insert".into(),
                                args: vec![obj_val, k, v],
                                ty: "Map".into(),
                            });
                        return Some(dest);
                    }
                    let mut arg_vals = Vec::new();
                    for a in args {
                        if let Some(av) = self.lower_expr(a, cur_block) {
                            arg_vals.push(av);
                        }
                    }
                    let mut method_ty = self
                        .function_return_types
                        .get(member)
                        .or_else(|| self.class_field_types.get(member))
                        .cloned()
                        .unwrap_or_else(|| {
                            if member.contains("float") || member.contains("flt") {
                                "Float".into()
                            } else if member.contains("string")
                                || member.contains("to_str")
                                || member.starts_with("str_")
                                || member.ends_with("_str")
                                || member.contains("render")
                                || member.contains("format")
                                || member.contains("quote")
                                || member.starts_with("wrap")
                            {
                                "String".into()
                            } else {
                                "Int".into()
                            }
                        });
                    if member == "unwrap" || member == "unwrap_or" {
                        let obj_ty = match &**object {
                            Expr::Identifier(name, _) => self.lookup_var_type(name),
                            _ => None,
                        };
                        if let Some(DataraType::GenericInstance { name, args }) = &obj_ty {
                            if name == "Outcome" && !args.is_empty() {
                                method_ty = match &args[0] {
                                    DataraType::Float => "Float".into(),
                                    DataraType::String => "String".into(),
                                    DataraType::Bool => "Bool".into(),
                                    DataraType::Int => "Int".into(),
                                    DataraType::Class(c) => c.clone(),
                                    _ => "Int".into(),
                                };
                            }
                        } else if let Some(DataraType::Result(ok, _)) = &obj_ty {
                            method_ty = match &**ok {
                                DataraType::Float => "Float".into(),
                                DataraType::String => "String".into(),
                                DataraType::Bool => "Bool".into(),
                                DataraType::Int => "Int".into(),
                                DataraType::Class(c) => c.clone(),
                                _ => "Int".into(),
                            };
                        }
                    } else if member == "is_ok" || member == "is_err" {
                        method_ty = "Bool".into();
                    } else if member == "await" {
                        let obj_ty = match &**object {
                            Expr::Identifier(name, _) => self.lookup_var_type(name),
                            _ => None,
                        };
                        if let Some(DataraType::GenericInstance { name, args }) = &obj_ty {
                            if (name == "Future" || name == "Task") && !args.is_empty() {
                                method_ty = match &args[0] {
                                    DataraType::Float => "Float".into(),
                                    DataraType::String => "String".into(),
                                    DataraType::Bool => "Bool".into(),
                                    DataraType::Int => "Int".into(),
                                    DataraType::Class(c) => c.clone(),
                                    _ => "Int".into(),
                                };
                            }
                        } else if let Some(DataraType::Class(c)) = &obj_ty {
                            if c == "Future" || c == "Task" {
                                method_ty = "String".into();
                            }
                        }
                    }
                    let dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::MethodCall {
                            dest,
                            object: obj_val,
                            method: member.clone(),
                            args: arg_vals,
                            ty: method_ty,
                        });
                    return Some(dest);
                }

                let mut arg_vals = Vec::new();
                for a in args {
                    if let Some(av) = self.lower_expr(a, cur_block) {
                        arg_vals.push(av);
                    }
                }
                let mut func_name = if let Expr::Identifier(fn_name, _) = &**callee {
                    fn_name.clone()
                } else {
                    "func".into()
                };

                if let Some(specs) = self.types.generic_specializations.get(&func_name) {
                    let mut candidate_mangled = None;
                    if let Some(first_arg) = args.first() {
                        let arg_ty_opt = match first_arg {
                            Expr::Identifier(var_name, _) => self.lookup_var_type(var_name),
                            Expr::ObjectInit { class_name, .. } => {
                                Some(DataraType::Class(class_name.clone()))
                            }
                            _ => None,
                        };
                        if let Some(arg_ty) = arg_ty_opt {
                            let type_str = match &arg_ty {
                                DataraType::Class(c) => c.clone(),
                                DataraType::Int => "Int".to_string(),
                                DataraType::Float => "Float".to_string(),
                                DataraType::String => "String".to_string(),
                                DataraType::Bool => "Bool".to_string(),
                                other => other.to_string(),
                            };
                            let candidate = format!("{}_{}", func_name, type_str);
                            if self.function_return_types.contains_key(&candidate) {
                                candidate_mangled = Some(candidate);
                            }
                        }
                    }
                    if candidate_mangled.is_none() && specs.len() == 1 {
                        if let Some(first_spec) = specs.iter().next() {
                            let s_names: Vec<String> = first_spec
                                .iter()
                                .map(|t| match t {
                                    DataraType::Class(c) => c.clone(),
                                    DataraType::Int => "Int".to_string(),
                                    DataraType::Float => "Float".to_string(),
                                    DataraType::String => "String".to_string(),
                                    DataraType::Bool => "Bool".to_string(),
                                    other => other.to_string(),
                                })
                                .collect();
                            let candidate = format!("{}_{}", func_name, s_names.join("_"));
                            if self.function_return_types.contains_key(&candidate) {
                                candidate_mangled = Some(candidate);
                            }
                        }
                    }
                    if let Some(mangled) = candidate_mangled {
                        func_name = mangled;
                    }
                }

                if func_name == "require" {
                    let zero = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: zero,
                            value: 0,
                        });
                    return Some(zero);
                }

                let dest = self.next_val();
                let ret_ty = self.infer_fn_ret_ty(&func_name);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::Call {
                        dest,
                        func: func_name,
                        args: arg_vals,
                        ty: ret_ty,
                    });
                Some(dest)
            }
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
        }
    }

    /// Representation type string of `object.member`, resolved through the
    /// type checker with generic substitution (e.g. `r.value` on an
    /// `Outcome<String>` yields "String", not the raw template "T").
    ///
    /// Returns None when the object has no statically known type; callers
    /// fall back to the legacy class_field_types map / heuristics.
    fn member_field_repr(&self, object: &Expr, member: &str) -> Option<String> {
        let obj_name = match object {
            Expr::Identifier(name, _) => name,
            _ => return None,
        };
        let obj_ty = self.lookup_var_type(obj_name)?;
        match obj_ty {
            DataraType::GenericInstance { name, args } => {
                let (params, t_fields) = self.types.generic_templates.get(&name)?;
                let field_type = t_fields.get(member)?;
                if let DataraType::TypeParam(p) = field_type
                    && let Some(idx) = params.iter().position(|param| param == p)
                    && idx < args.len()
                {
                    return Some(args[idx].to_string());
                }
                Some(field_type.to_string())
            }
            DataraType::Class(cls_name) => {
                let fields = self.types.class_fields.get(&cls_name)?;
                Some(fields.get(member)?.to_string())
            }
            _ => None,
        }
    }

    fn infer_fn_ret_ty(&self, func_name: &str) -> String {
        if let Some(ty) = self.function_return_types.get(func_name) {
            return ty.clone();
        }
        if func_name == "str_to_int"
            || func_name == "datara_rt_str_to_int"
            || func_name == "js_eval_int"
            || func_name == "datara_js_eval_int"
            || func_name == "py_eval_int"
            || func_name == "datara_py_eval_int"
            || func_name == "py_import"
            || func_name == "datara_py_import"
            || func_name == "py_exec"
            || func_name == "datara_py_exec"
            || func_name == "datara_py_export_list_f64"
            || func_name == "datara_py_assert_same_ptr"
            || func_name == "str_len"
            || func_name == "datara_rt_str_len"
            || func_name.ends_with("_to_int")
            || func_name.contains("count")
            || func_name.contains("index")
        {
            "Int".into()
        } else if func_name == "str_to_float"
            || func_name == "datara_rt_str_to_float"
            || func_name == "js_eval_float"
            || func_name == "datara_js_eval_float"
            || func_name == "py_eval_float"
            || func_name == "datara_py_eval_float"
            || func_name == "py_call_1_float"
            || func_name == "datara_py_call_1_float"
            || func_name.ends_with("_to_float")
            || func_name.contains("float")
            || func_name.contains("flt")
        {
            "Float".into()
        } else if func_name == "str_split"
            || func_name == "split"
            || func_name == "datara_rt_str_split"
        {
            "List".into()
        } else if func_name.contains("is_")
            || func_name.contains("has_")
            || func_name.contains("contains")
            || func_name.contains("starts_with")
            || func_name.ends_with("_with")
        {
            "Bool".into()
        } else if func_name.contains("string")
            || func_name.contains("to_str")
            || func_name.ends_with("_str")
            || func_name.starts_with("js_")
            || func_name.starts_with("datara_js_")
            || func_name.starts_with("py_")
            || func_name.starts_with("datara_py_")
            || func_name.contains("classify")
            || func_name.contains("handle")
            || func_name.contains("format")
            || func_name.contains("render")
            || func_name.contains("quote")
            || func_name.contains("summary")
            || func_name.contains("repeat")
            || func_name.contains("pad")
            || func_name.contains("replace")
            || func_name.contains("upper")
            || func_name.contains("lower")
            || func_name.contains("join")
            || func_name == "join"
            || func_name == "str_join"
            || func_name == "datara_rt_str_join"
            || func_name == "read"
            || func_name == "file_read"
            || func_name == "datara_rt_file_read"
            || func_name == "env_get"
            || func_name == "datara_rt_env_get"
            || func_name == "args_get"
            || func_name == "datara_rt_args_get"
            || func_name == "str_trim"
            || func_name == "datara_rt_str_trim"
            || func_name == "socket_recv"
            || func_name == "datara_rt_socket_recv"
            || func_name == "sha256"
            || func_name == "datara_rt_sha256"
            || func_name == "base64_encode"
            || func_name == "datara_rt_base64_encode"
            || func_name == "base64_decode"
            || func_name == "datara_rt_base64_decode"
            || func_name == "uuid_v4"
            || func_name == "datara_rt_uuid_v4"
            || func_name == "int_to_str"
            || func_name == "datara_rt_int_to_str"
            || func_name == "float_to_str"
            || func_name == "datara_rt_float_to_str"
        {
            "String".into()
        } else {
            "Int".into()
        }
    }

    fn is_expr_str(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(LiteralValue::String(_), _) | Expr::InterpolatedString { .. } => true,
            Expr::MemberAccess { member, .. } => {
                self.class_field_types
                    .get(member)
                    .map(|t| t == "String")
                    .unwrap_or(false)
                    || member == "name"
                    || member == "version"
                    || member == "title"
                    || member == "path"
                    || member.contains("str")
            }
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name)
                    && ty == crate::types::DataraType::String
                {
                    return true;
                }
                self.class_field_types
                    .get(name)
                    .map(|t| t == "String" || t == "Str")
                    .unwrap_or(false)
                    || name == "name"
                    || name == "version"
                    || name == "title"
                    || name == "path"
                    || name.contains("str")
                    || name.contains("msg")
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == "+" {
                    self.is_expr_str(left) || self.is_expr_str(right)
                } else {
                    false
                }
            }
            Expr::Call { callee, .. } => match &**callee {
                Expr::Identifier(fn_name, _) => {
                    let ret = self.infer_fn_ret_ty(fn_name);
                    ret == "String" || ret == "Str"
                }
                Expr::MemberAccess { object, member, .. } => {
                    if member == "unwrap" || member == "unwrap_or" {
                        if let Expr::Identifier(var_name, _) = &**object {
                            if let Some(DataraType::GenericInstance { name, args }) =
                                self.lookup_var_type(var_name)
                            {
                                if name == "Outcome" && args.first() == Some(&DataraType::String) {
                                    return true;
                                }
                            }
                        }
                    }
                    let ret = self.infer_fn_ret_ty(member);
                    ret == "String" || ret == "Str"
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn is_expr_float(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(LiteralValue::Float(_), _) => true,
            Expr::MemberAccess { member, .. } => {
                self.class_field_types
                    .get(member)
                    .map(|t| t == "Float")
                    .unwrap_or(false)
                    || member.contains("flt")
                    || member.contains("float")
            }
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name)
                    && ty == crate::types::DataraType::Float
                {
                    return true;
                }
                self.class_field_types
                    .get(name)
                    .map(|t| t == "Float")
                    .unwrap_or(false)
                    || name.contains("flt")
                    || name.contains("float")
            }
            Expr::Binary { left, right, .. } => {
                self.is_expr_float(left) || self.is_expr_float(right)
            }
            Expr::Unary { expr, .. } => self.is_expr_float(expr),
            Expr::Call { callee, .. } => match &**callee {
                Expr::Identifier(name, _) => name.contains("float") || name.contains("flt"),
                Expr::MemberAccess { object, member, .. } => {
                    if member == "unwrap" || member == "unwrap_or" {
                        if let Expr::Identifier(var_name, _) = &**object {
                            if let Some(DataraType::GenericInstance { name, args }) =
                                self.lookup_var_type(var_name)
                            {
                                if name == "Outcome" && args.first() == Some(&DataraType::Float) {
                                    return true;
                                }
                            }
                        }
                    }
                    member.contains("float") || member.contains("flt")
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn is_expr_bool(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(LiteralValue::Bool(_), _) => true,
            Expr::Identifier(name, ..) => {
                if let Some(ty) = self.lookup_var_type(name)
                    && ty == crate::types::DataraType::Bool
                {
                    return true;
                }
                false
            }
            Expr::Binary { op, .. } => {
                matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
                )
            }
            Expr::Unary { op, .. } => op == "!",
            _ => false,
        }
    }

    fn is_expr_list(&self, expr: &Expr) -> bool {
        if matches!(
            expr,
            Expr::ListLiteral(..) | Expr::ArrayRepeatLiteral { .. }
        ) {
            return true;
        }
        if let Expr::Identifier(name, _) = expr {
            if let Some(ty) = self.lookup_var_type(name) {
                if matches!(ty, crate::types::DataraType::List(_)) {
                    return true;
                }
                if let crate::types::DataraType::Class(cls) = ty {
                    if cls == "List" {
                        return true;
                    }
                }
            }
        }
        if let Expr::Call { callee, args, .. } = expr {
            if let Expr::MemberAccess { object, member, .. } = &**callee {
                if (member == "map" || member == "filter") && self.is_expr_list(object) {
                    return true;
                }
            }
            if let Expr::Identifier(fn_name, _) = &**callee {
                if fn_name == "str_split" || fn_name == "split" || fn_name == "datara_rt_str_split"
                {
                    return true;
                }
                if (fn_name == "map" || fn_name == "filter")
                    && !args.is_empty()
                    && self.is_expr_list(&args[0])
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_callable_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Lambda { .. } => true,
            Expr::Identifier(name, _) => {
                if self.local_lambdas.contains_key(name) {
                    return true;
                }
                if let Some(ty) = self.lookup_var_type(name) {
                    if matches!(ty, crate::types::DataraType::Function { .. }) {
                        return true;
                    }
                }
                if self.resolver.functions.contains_key(name)
                    || self.function_return_types.contains_key(name)
                {
                    return true;
                }
                if let Some(global) = self.resolver.scopes.first() {
                    if global.get(name).is_some() {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn lower_inline_closure_call(
        &mut self,
        callable: &Expr,
        arg_vals: &[ValueId],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let (params, body) = match callable {
            Expr::Lambda { params, body, .. } => (params.clone(), (**body).clone()),
            Expr::Identifier(fn_name, _) => {
                if let Some(pair) = self.local_lambdas.get(fn_name).cloned() {
                    pair
                } else {
                    let dest = self.next_val();
                    let ret_ty = self
                        .function_return_types
                        .get(fn_name)
                        .cloned()
                        .unwrap_or_else(|| "Int".into());
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::Call {
                            dest,
                            func: fn_name.clone(),
                            args: arg_vals.to_vec(),
                            ty: ret_ty,
                        });
                    return Some(dest);
                }
            }
            _ => {
                return None;
            }
        };

        let mut shadowed_symbols: Vec<(String, Option<ValueId>)> = Vec::new();
        for (param, &aval) in params.iter().zip(arg_vals) {
            let old_val = self.symbol_values.get(&param.name).copied();
            shadowed_symbols.push((param.name.clone(), old_val));
            self.symbol_values.insert(param.name.clone(), aval);
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::AssignVar {
                    name: param.name.clone(),
                    value: aval,
                });
        }

        let res = self.lower_expr(&body, cur_block);

        for (name, old_val) in shadowed_symbols {
            if let Some(ov) = old_val {
                self.symbol_values.insert(name.clone(), ov);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar { name, value: ov });
            } else {
                self.symbol_values.remove(&name);
            }
        }

        res
    }

    fn lower_list_higher_order(
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

    fn find_packet_for_member(&self, object: &Expr, member: &str) -> Option<(usize, usize)> {
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
