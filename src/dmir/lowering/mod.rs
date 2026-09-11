use crate::ast::*;
use crate::resolver::Resolver;
use crate::types::{DataraType, TypeChecker};
use std::collections::HashMap;

use crate::dmir::ir::*;

pub mod decl;
pub mod expr;
pub mod expr_call;
pub mod expr_composite;
pub mod higher_order;
pub mod infer;
pub mod match_arm;
pub mod stmt;

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
        function_return_types.insert("time_precise_ms".into(), "Float".into());
        function_return_types.insert("datara_rt_time_precise_ms".into(), "Float".into());
        function_return_types.insert("time_delta_ms".into(), "Float".into());
        function_return_types.insert("datara_rt_time_delta_ms".into(), "Float".into());
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
            if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl
                && !f.generic_params.is_empty()
            {
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

        module.class_field_types = self.class_field_types.clone();
        module
    }
}
