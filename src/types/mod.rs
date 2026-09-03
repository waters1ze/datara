use crate::ast::*;
use crate::diagnostics::span::SourceSpan;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::resolver::Resolver;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataraType {
    Int,
    Float,
    Bool,
    String,
    Char,
    Unit,
    Never,
    Class(String),
    TypeParam(String),
    GenericInstance {
        name: String,
        args: Vec<DataraType>,
    },
    Option(Box<DataraType>),
    Result(Box<DataraType>, Box<DataraType>),
    Tuple(Vec<DataraType>),
    Function {
        params: Vec<DataraType>,
        return_type: Box<DataraType>,
    },
    Val,
    Dec64,
    Dec128,
    RawPtr,
}

impl DataraType {
    pub fn is_compatible(&self, other: &DataraType) -> bool {
        if self == other || *self == DataraType::Never || *other == DataraType::Never {
            return true;
        }
        if *self == DataraType::Val || *other == DataraType::Val {
            return true;
        }
        if let (DataraType::Tuple(t1), DataraType::Tuple(t2)) = (self, other)
            && t1.len() == t2.len()
        {
            return t1.iter().zip(t2.iter()).all(|(a, b)| a.is_compatible(b));
        }
        if let (DataraType::Option(o1), DataraType::Option(o2)) = (self, other) {
            if **o1 == DataraType::Unit || **o2 == DataraType::Unit {
                return true;
            }
            return o1.is_compatible(o2);
        }
        if let DataraType::Option(target) = other
            && self.is_compatible(target)
        {
            return true;
        }
        if let (DataraType::Result(ok1, err1), DataraType::Result(ok2, err2)) = (self, other) {
            return ok1.is_compatible(ok2) && err1.is_compatible(err2);
        }
        false
    }

    /// If this type flows through `?` as a Result-like value, return its
    /// (ok, err) payload types.
    ///
    /// Both the abstract `T!E`/`Result<T, E>` forms and the concrete stdlib
    /// `Outcome<T>` representation (whose error channel is `error_msg: String`)
    /// classify as Result-like.
    pub fn result_like(&self) -> Option<(DataraType, DataraType)> {
        match self {
            DataraType::Result(ok, err) => Some(((**ok).clone(), (**err).clone())),
            DataraType::GenericInstance { name, args } if name == "Outcome" && !args.is_empty() => {
                Some((args[0].clone(), DataraType::String))
            }
            _ => None,
        }
    }

    /// If this type flows through `?` as an Option-like value, return its
    /// inner payload type. Covers `T?`, `Option<T>` and stdlib `Maybe<T>`.
    pub fn option_like(&self) -> Option<DataraType> {
        match self {
            DataraType::Option(inner) => Some((**inner).clone()),
            DataraType::GenericInstance { name, args } if name == "Maybe" && !args.is_empty() => {
                Some(args[0].clone())
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for DataraType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataraType::Int => write!(f, "Int"),
            DataraType::Float => write!(f, "Float"),
            DataraType::Bool => write!(f, "Bool"),
            DataraType::String => write!(f, "Str"),
            DataraType::Char => write!(f, "Char"),
            DataraType::Unit => write!(f, "Unit"),
            DataraType::Never => write!(f, "Never"),
            DataraType::Class(name) => write!(f, "{}", name),
            DataraType::TypeParam(p) => write!(f, "{}", p),
            DataraType::GenericInstance { name, args } => {
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{}<{}>", name, args_str)
            }
            DataraType::Option(inner) => write!(f, "{}?", inner),
            DataraType::Result(ok, err) => write!(f, "{}!{}", ok, err),
            DataraType::Tuple(items) => {
                let items_str = items
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({})", items_str)
            }
            DataraType::Function {
                params,
                return_type,
            } => {
                let p_str = params
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({}) -> {}", p_str, return_type)
            }
            DataraType::Val => write!(f, "Val"),
            DataraType::Dec64 => write!(f, "dec64"),
            DataraType::Dec128 => write!(f, "dec128"),
            DataraType::RawPtr => write!(f, "RawPtr"),
        }
    }
}

/// Which concrete stdlib representation an error-propagation site uses.
///
/// `T!E` (Result) is represented by `Outcome<T>` (fields: `is_success`,
/// `value`, `error_msg`); `T?` (Option) by `Maybe<T>` (fields: `is_some`,
/// `value`). The lowering needs the concrete field names to build the
/// check/extract CFG, so the type checker records the decision per site
/// instead of the backend guessing from heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropagationKind {
    Outcome,
    Maybe,
}

impl PropagationKind {
    /// The boolean flag field that discriminates success/presence.
    pub fn flag_field(&self) -> &'static str {
        match self {
            PropagationKind::Outcome => "is_success",
            PropagationKind::Maybe => "is_some",
        }
    }

    /// The field that carries the unwrapped payload.
    pub fn payload_field(&self) -> &'static str {
        "value"
    }
}

/// One `expr?` site, recorded by the type checker and consumed by the DMIR
/// lowering. Keyed by source span because the lowering walks the same AST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationSite {
    pub span: SourceSpan,
    pub kind: PropagationKind,
    /// Representation type string of the unwrapped value (e.g. "Int",
    /// "String", "Outcome<Int>").
    pub payload_repr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutabilityKind {
    Immutable,
    MutableFixed,
    Dynamic { is_mut: bool },
}

pub struct TypeChecker<'a> {
    pub resolver: &'a Resolver,
    pub symbol_types: HashMap<String, DataraType>,
    pub symbol_mutability: HashMap<String, MutabilityKind>,
    pub function_signatures: HashMap<String, (Vec<DataraType>, DataraType, Vec<String>)>,
    pub class_fields: HashMap<String, HashMap<String, DataraType>>,
    pub class_methods: HashMap<String, HashMap<String, DataraType>>,
    pub generic_templates: HashMap<String, (Vec<String>, HashMap<String, DataraType>)>,
    pub generic_specializations: HashMap<String, HashSet<Vec<DataraType>>>,
    /// Return type of the function/method currently being checked, used to
    /// enforce that `?` only appears where the error can actually propagate
    /// and that `return` values match a Result/Option signature.
    pub current_return_type: Option<DataraType>,
    /// `expr?` sites recorded during checking, consumed by the lowering.
    pub propagation_sites: Vec<PropagationSite>,
    /// Inferred element type per list-typed variable name (`let xs = [1, 2]`
    /// records `xs -> Int`). Indexing and for-loop iteration use it instead of
    /// defaulting every element to `Int`.
    pub var_element_types: HashMap<String, DataraType>,
    /// Element type of the most recently checked list literal, consumed by
    /// the declaration handlers right after they check their initializer.
    pub last_list_element: Option<DataraType>,
    /// Name of the function/method currently being type checked.
    pub current_fn_name: Option<String>,
    /// Preserved mapping of (function_name, variable_name) -> DataraType
    /// across the whole program, surviving block exits so DMIR lowering has full types.
    pub fn_symbol_types: HashMap<(String, String), DataraType>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(resolver: &'a Resolver) -> Self {
        let mut class_fields = HashMap::new();
        let mut class_methods = HashMap::new();
        let mut generic_templates = HashMap::new();
        let mut function_signatures = HashMap::new();
        let fn_symbol_types = HashMap::new();

        // Built-in prelude function signatures
        function_signatures.insert(
            "println".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "print".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "eprintln".to_string(),
            (vec![DataraType::String], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "panic".to_string(),
            (vec![DataraType::String], DataraType::Never, Vec::new()),
        );
        function_signatures.insert(
            "assert".to_string(),
            (
                vec![DataraType::Bool, DataraType::String],
                DataraType::Unit,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "length".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert("now".to_string(), (vec![], DataraType::Int, Vec::new()));
        function_signatures.insert("now_ms".to_string(), (vec![], DataraType::Int, Vec::new()));
        function_signatures.insert(
            "now_precise_ms".to_string(),
            (vec![], DataraType::Int, Vec::new()),
        );
        for name in &["math_ctz", "ctz"] {
            function_signatures.insert(
                name.to_string(),
                (vec![DataraType::Int], DataraType::Int, Vec::new()),
            );
        }
        for name in &[
            "math_shr", "shr", "math_shl", "shl", "math_xor", "xor", "math_and", "and", "math_or",
            "or",
        ] {
            function_signatures.insert(
                name.to_string(),
                (
                    vec![DataraType::Int, DataraType::Int],
                    DataraType::Int,
                    Vec::new(),
                ),
            );
        }
        function_signatures.insert(
            "exit".to_string(),
            (vec![DataraType::Int], DataraType::Never, Vec::new()),
        );
        function_signatures.insert(
            "input".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "sleep".to_string(),
            (vec![DataraType::Int], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "file_read".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "read_file".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "file_write".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "write_file".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "file_append".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "file_exists".to_string(),
            (vec![DataraType::String], DataraType::Bool, Vec::new()),
        );
        function_signatures.insert(
            "env_get".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "args_count".to_string(),
            (vec![], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "args_get".to_string(),
            (vec![DataraType::Int], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "str_contains".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_starts_with".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_ends_with".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Bool,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_index_of".to_string(),
            (
                vec![DataraType::String, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_len".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_substring".to_string(),
            (
                vec![DataraType::String, DataraType::Int, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_char_at".to_string(),
            (
                vec![DataraType::String, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "str_trim".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "str_to_int".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "str_to_float".to_string(),
            (vec![DataraType::String], DataraType::Float, Vec::new()),
        );
        function_signatures.insert(
            "read_line".to_string(),
            (vec![], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "socket_create".to_string(),
            (vec![DataraType::Int], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "socket_bind".to_string(),
            (
                vec![DataraType::Int, DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_listen".to_string(),
            (
                vec![DataraType::Int, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_accept".to_string(),
            (vec![DataraType::Int], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "socket_connect".to_string(),
            (
                vec![DataraType::Int, DataraType::String, DataraType::Int],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_send".to_string(),
            (
                vec![DataraType::Int, DataraType::String],
                DataraType::Int,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_recv".to_string(),
            (
                vec![DataraType::Int, DataraType::Int],
                DataraType::String,
                Vec::new(),
            ),
        );
        function_signatures.insert(
            "socket_close".to_string(),
            (vec![DataraType::Int], DataraType::Unit, Vec::new()),
        );
        function_signatures.insert(
            "sha256".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "base64_encode".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "base64_decode".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "process_run".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "system".to_string(),
            (vec![DataraType::String], DataraType::Int, Vec::new()),
        );
        function_signatures.insert(
            "process_output".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );
        function_signatures.insert(
            "exec".to_string(),
            (vec![DataraType::String], DataraType::String, Vec::new()),
        );

        for (name, sym) in &resolver.functions {
            let mut p_types = Vec::new();
            if let Some(params) = &sym.type_node {
                p_types.push(Self::resolve_tn(params));
            }
            let ret = sym
                .return_type
                .as_ref()
                .map(Self::resolve_tn)
                .unwrap_or(DataraType::Unit);
            function_signatures.insert(name.clone(), (p_types, ret, sym.generic_params.clone()));
        }

        for (name, sym) in &resolver.classes {
            let mut fields = HashMap::new();
            // 1. Base class fields (recursive)
            let mut curr_base = sym.base_type.clone();
            let mut visited_bases = Vec::new();
            while let Some(b_name) = curr_base {
                if visited_bases.contains(&b_name) {
                    break;
                }
                visited_bases.push(b_name.clone());
                if let Some(b_sym) = resolver.classes.get(&b_name) {
                    for (f_name, f_sym) in &b_sym.fields {
                        let f_type = f_sym
                            .type_node
                            .as_ref()
                            .map(Self::resolve_tn)
                            .unwrap_or(DataraType::String);
                        fields.insert(f_name.clone(), f_type);
                    }
                    for comp_name in &b_sym.compositions {
                        if let Some(comp_sym) = resolver.components.get(comp_name) {
                            for (f_name, f_sym) in &comp_sym.fields {
                                let f_type = f_sym
                                    .type_node
                                    .as_ref()
                                    .map(Self::resolve_tn)
                                    .unwrap_or(DataraType::String);
                                fields.insert(f_name.clone(), f_type);
                            }
                        }
                    }
                    curr_base = b_sym.base_type.clone();
                } else {
                    break;
                }
            }

            // 2. Composed components
            for comp_name in &sym.compositions {
                if let Some(comp_sym) = resolver.components.get(comp_name) {
                    for (f_name, f_sym) in &comp_sym.fields {
                        let f_type = f_sym
                            .type_node
                            .as_ref()
                            .map(Self::resolve_tn)
                            .unwrap_or(DataraType::String);
                        fields.insert(f_name.clone(), f_type);
                    }
                }
            }

            // 3. Own fields
            for (f_name, f_sym) in &sym.fields {
                let f_type = f_sym
                    .type_node
                    .as_ref()
                    .map(Self::resolve_tn)
                    .unwrap_or(DataraType::String);
                fields.insert(f_name.clone(), f_type);
            }
            class_fields.insert(name.clone(), fields.clone());

            let mut methods = HashMap::new();
            for (m_name, m_sym) in &sym.methods {
                let m_type = m_sym
                    .return_type
                    .as_ref()
                    .map(Self::resolve_tn)
                    .unwrap_or(DataraType::Unit);
                methods.insert(m_name.clone(), m_type);
            }
            class_methods.insert(name.clone(), methods);

            if !sym.generic_params.is_empty() {
                generic_templates.insert(name.clone(), (sym.generic_params.clone(), fields));
            }
        }

        Self {
            resolver,
            symbol_types: HashMap::new(),
            symbol_mutability: HashMap::new(),
            function_signatures,
            class_fields,
            class_methods,
            generic_templates,
            generic_specializations: HashMap::new(),
            current_return_type: None,
            propagation_sites: Vec::new(),
            var_element_types: HashMap::new(),
            last_list_element: None,
            current_fn_name: None,
            fn_symbol_types,
        }
    }

    pub fn resolve_tn(tn: &TypeNode) -> DataraType {
        if !tn.generic_args.is_empty() {
            let args: Vec<DataraType> = tn.generic_args.iter().map(Self::resolve_tn).collect();
            // `Result<T, E>` and `Option<T>` written in generic form are the
            // same abstract types as the `T!E` / `T?` suffix forms.
            if tn.name == "Result" && args.len() == 2 {
                return DataraType::Result(Box::new(args[0].clone()), Box::new(args[1].clone()));
            }
            if tn.name == "Option" && args.len() == 1 {
                return DataraType::Option(Box::new(args[0].clone()));
            }
            return DataraType::GenericInstance {
                name: tn.name.clone(),
                args,
            };
        }

        let base = match tn.name.as_str() {
            "Int" | "Int64" | "Int32" | "Int16" | "Int8" | "UInt" | "UInt64" | "UInt32"
            | "UInt16" | "UInt8" | "i64" | "i32" | "i16" | "i8" | "isize" | "u64" | "u32"
            | "u16" | "u8" | "u128" | "i128" | "usize" | "USize" | "Byte" => DataraType::Int,
            "Float" | "Float64" | "Float32" | "f64" | "f32" | "f16" => DataraType::Float,
            "dec64" => DataraType::Dec64,
            "dec128" => DataraType::Dec128,
            "Bool" => DataraType::Bool,
            "String" | "Str" => DataraType::String,
            "Char" => DataraType::Char,
            "Unit" => DataraType::Unit,
            "val" | "Val" => DataraType::Val,
            "RawPtr" => DataraType::RawPtr,
            other if other.len() == 1 && other.chars().next().unwrap().is_ascii_uppercase() => {
                DataraType::TypeParam(other.to_string())
            }
            "Item" | "Key" | "Value" | "Element" | "Err" | "Target" => {
                DataraType::TypeParam(tn.name.clone())
            }
            other => DataraType::Class(other.to_string()),
        };

        if tn.is_option {
            DataraType::Option(Box::new(base))
        } else if let Some(err) = &tn.error_type {
            let err_type = Self::resolve_tn(err);
            DataraType::Result(Box::new(base), Box::new(err_type))
        } else {
            base
        }
    }

    pub fn suggest_type_fix(expected: &DataraType, found: &DataraType) -> Option<String> {
        match (expected, found) {
            (DataraType::Int, DataraType::Float) => Some(
                "use explicit cast 'val as Int' or call 'datara_rt_math_floor(val)' to convert Float to Int".into(),
            ),
            (DataraType::Float, DataraType::Int) => Some(
                "use a floating-point literal (e.g. '10.0') or cast with 'val as Float'".into(),
            ),
            (DataraType::String, DataraType::Int) => Some(
                "convert Int to String using string interpolation '\"{val}\"' or 'int_to_str(val)'".into(),
            ),
            (DataraType::String, DataraType::Float) => Some(
                "convert Float to String using string interpolation '\"{val}\"' or 'float_to_str(val)'".into(),
            ),
            (DataraType::String, DataraType::Bool) => Some(
                "convert Bool to String using 'bool_to_str(val)' or '\"{val}\"'".into(),
            ),
            (DataraType::Int, DataraType::String) => Some(
                "parse String to Int using 'str_to_int(val)'".into(),
            ),
            (DataraType::Float, DataraType::String) => Some(
                "parse String to Float using 'str_to_float(val)'".into(),
            ),
            (DataraType::Bool, DataraType::Int) => Some(
                "in Datara, integers are not implicitly boolean; use an explicit comparison like 'val != 0'".into(),
            ),
            (DataraType::Bool, _) => Some(
                "expression must evaluate to a Bool; use comparison operators (==, !=, <, >)".into(),
            ),
            _ => None,
        }
    }

    pub fn check_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        // Collect function signatures first
        for decl in &program.declarations {
            if let Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) = decl {
                let p_types: Vec<DataraType> = f
                    .params
                    .iter()
                    .map(|p| {
                        p.type_node
                            .as_ref()
                            .map(Self::resolve_tn)
                            .unwrap_or(DataraType::Int)
                    })
                    .collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(Self::resolve_tn)
                    .unwrap_or(DataraType::Unit);
                let gen_params: Vec<String> = f.generic_params.clone();
                self.function_signatures
                    .insert(f.name.clone(), (p_types, ret, gen_params));
            }
        }

        for decl in &program.declarations {
            self.check_decl(decl, diag);
        }
    }

    fn check_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                self.current_fn_name = Some(f.name.clone());
                for p in &f.params {
                    let p_type = p
                        .type_node
                        .as_ref()
                        .map(Self::resolve_tn)
                        .unwrap_or(DataraType::Int);
                    self.symbol_types.insert(p.name.clone(), p_type.clone());
                    self.fn_symbol_types
                        .insert((f.name.clone(), p.name.clone()), p_type);
                }
                if let Some(rt) = &f.return_type {
                    Self::validate_error_channels(rt, diag);
                }
                let expected = f
                    .return_type
                    .as_ref()
                    .map(Self::resolve_tn)
                    .unwrap_or(DataraType::Unit);
                self.current_return_type = Some(expected.clone());
                let body_type = self.check_stmt(&f.body, diag);
                self.current_return_type = None;
                self.current_fn_name = None;

                if f.is_expression_body
                    && !body_type.is_compatible(&expected)
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
            }
            Decl::Class(c) => {
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let m_fn_name = format!("{}_{}", c.name, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());
                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(c.name.clone()));
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(c.name.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(Self::resolve_tn)
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                        }
                        if let Some(rt) = &m.return_type {
                            Self::validate_error_channels(rt, diag);
                        }
                        self.current_return_type = Some(
                            m.return_type
                                .as_ref()
                                .map(Self::resolve_tn)
                                .unwrap_or(DataraType::Unit),
                        );
                        self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;
                    }
                }
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
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        let m_fn_name = format!("{}_{}", b.target_type, m.name);
                        self.current_fn_name = Some(m_fn_name.clone());
                        self.symbol_types
                            .insert("this".to_string(), DataraType::Class(b.target_type.clone()));
                        self.fn_symbol_types.insert(
                            (m_fn_name.clone(), "this".to_string()),
                            DataraType::Class(b.target_type.clone()),
                        );
                        for p in &m.params {
                            let p_type = p
                                .type_node
                                .as_ref()
                                .map(Self::resolve_tn)
                                .unwrap_or(DataraType::Int);
                            self.symbol_types.insert(p.name.clone(), p_type.clone());
                            self.fn_symbol_types
                                .insert((m_fn_name.clone(), p.name.clone()), p_type);
                        }
                        if let Some(rt) = &m.return_type {
                            Self::validate_error_channels(rt, diag);
                        }
                        self.current_return_type = Some(
                            m.return_type
                                .as_ref()
                                .map(Self::resolve_tn)
                                .unwrap_or(DataraType::Unit),
                        );
                        self.check_stmt(body, diag);
                        self.current_return_type = None;
                        self.current_fn_name = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// The `T!E` sugar is represented at runtime by the stdlib `Outcome<T>`
    /// class whose error channel is the fixed `error_msg: String` field. An
    /// error type of anything but String has no representation, so reject it
    /// loudly instead of silently coercing (no JS-style magic).
    fn validate_error_channels(tn: &TypeNode, diag: &mut DiagnosticEngine) {
        let err_node = if let Some(err) = &tn.error_type {
            Some((err.as_ref(), tn.full_type_name()))
        } else if tn.name == "Result" && tn.generic_args.len() == 2 {
            Some((&tn.generic_args[1], tn.full_type_name()))
        } else {
            None
        };
        if let Some((err_node, type_str)) = err_node {
            let err_ty = Self::resolve_tn(err_node);
            if err_ty != DataraType::String {
                diag.error(
                    ErrorCode::TypeMismatch,
                    format!(
                        "the error channel of '{}' must be String (Outcome representation), got '{}'",
                        type_str, err_ty
                    ),
                    Some(tn.span.clone()),
                );
            }
        }
        for a in &tn.generic_args {
            Self::validate_error_channels(a, diag);
        }
    }

    pub fn record_var_type(&mut self, name: &str, ty: DataraType) {
        if let Some(ref fn_name) = self.current_fn_name {
            self.fn_symbol_types
                .insert((fn_name.clone(), name.to_string()), ty.clone());
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
                let mut last = DataraType::Unit;
                for s in stmts {
                    last = self.check_stmt(s, diag);
                }
                self.symbol_types = saved_types;
                self.symbol_mutability = saved_mut;
                self.var_element_types = saved_elem;
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
                    let declared = Self::resolve_tn(tn);
                    if !init_type.is_compatible(&declared) {
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
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if matches!(*init, Expr::ListLiteral(_, _))
                    && let Some(e) = self.last_list_element.take()
                {
                    self.var_element_types.insert(name.clone(), e);
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
                    let declared = Self::resolve_tn(tn);
                    if !init_type.is_compatible(&declared) {
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
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if matches!(*init, Expr::ListLiteral(_, _))
                    && let Some(e) = self.last_list_element.take()
                {
                    self.var_element_types.insert(name.clone(), e);
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
                    let declared = Self::resolve_tn(tn);
                    if !init_type.is_compatible(&declared) {
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
                    declared
                } else {
                    init_type
                };
                self.record_var_type(name, final_ty.clone());
                if matches!(*init, Expr::ListLiteral(_, _))
                    && let Some(e) = self.last_list_element.take()
                {
                    self.var_element_types.insert(name.clone(), e);
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
                    let declared = Self::resolve_tn(tn);
                    if !init_type.is_compatible(&declared) {
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
                    declared
                } else if !*is_mut {
                    // Type promotion: immutable `val` promotes directly to its concrete scalar SSA register type
                    init_type
                } else {
                    DataraType::Val
                };
                self.record_var_type(name, final_ty.clone());
                if matches!(*init, Expr::ListLiteral(_, _))
                    && let Some(e) = self.last_list_element.take()
                {
                    self.var_element_types.insert(name.clone(), e);
                }
                self.symbol_mutability
                    .insert(name.clone(), MutabilityKind::Dynamic { is_mut: *is_mut });
                final_ty
            }
            Stmt::CompactBind { name, init, .. } => {
                let init_type = self.check_expr(init, diag);
                self.record_var_type(name, init_type.clone());
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
                                if let Some(existing) = self.symbol_types.get(name)
                                    && !val_type.is_compatible(existing)
                                {
                                    let help_msg = Self::suggest_type_fix(existing, &val_type);
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
                            MutabilityKind::Dynamic { is_mut: true } => {
                                self.symbol_types.insert(name.clone(), val_type.clone());
                            }
                        }
                    } else {
                        let candidates: Vec<&str> =
                            self.symbol_types.keys().map(|s| s.as_str()).collect();
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
                    if !val_type.is_compatible(&tgt_type) {
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
                    let enc_res = expected.result_like();
                    let enc_opt = expected.option_like();
                    if enc_res.is_some() || enc_opt.is_some() {
                        let matches = if let Some((eok, eerr)) = &enc_res {
                            match t.result_like() {
                                Some((tok, terr)) => {
                                    tok.is_compatible(eok) && terr.is_compatible(eerr)
                                }
                                None => false,
                            }
                        } else if let Some(einner) = &enc_opt {
                            match t.option_like() {
                                Some(tinner) => tinner.is_compatible(einner),
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
                if !cond_type.is_compatible(&DataraType::Bool) {
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
            } => {
                let iter_type = self.check_expr(iterable, diag);
                let elem_type = match &iter_type {
                    DataraType::GenericInstance { name, args }
                        if name == "List" && !args.is_empty() =>
                    {
                        args[0].clone()
                    }
                    DataraType::Class(c) if c == "Range" => DataraType::Int,
                    DataraType::String => DataraType::Char,
                    DataraType::Class(c) if c == "List" => {
                        if let Expr::Identifier(n, _) = iterable {
                            self.var_element_types
                                .get(n)
                                .cloned()
                                .unwrap_or(DataraType::Int)
                        } else {
                            self.last_list_element.clone().unwrap_or(DataraType::Int)
                        }
                    }
                    _ => DataraType::Int,
                };
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), var_name.clone()), elem_type.clone());
                }
                let prev = self.symbol_types.insert(var_name.clone(), elem_type);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(var_name.clone(), p);
                } else {
                    self.symbol_types.remove(var_name);
                }
                DataraType::Unit
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                let cond_type = self.check_expr(condition, diag);
                if !cond_type.is_compatible(&DataraType::Bool) {
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
                self.symbol_types
                    .insert(err_var.clone(), DataraType::String);
                self.check_stmt(catch_block, diag);
                DataraType::Unit
            }
            Stmt::Parallel(body, _) => {
                self.check_stmt(body, diag);
                DataraType::Unit
            }
            Stmt::ParallelFor {
                var_name,
                iterable,
                body,
                ..
            } => {
                let iter_type = self.check_expr(iterable, diag);
                let elem_type = match &iter_type {
                    DataraType::GenericInstance { name, args }
                        if name == "List" && !args.is_empty() =>
                    {
                        args[0].clone()
                    }
                    DataraType::Class(c) if c == "Range" => DataraType::Int,
                    DataraType::String => DataraType::Char,
                    DataraType::Class(c) if c == "List" => {
                        if let Expr::Identifier(n, _) = iterable {
                            self.var_element_types
                                .get(n)
                                .cloned()
                                .unwrap_or(DataraType::Int)
                        } else {
                            self.last_list_element.clone().unwrap_or(DataraType::Int)
                        }
                    }
                    _ => DataraType::Int,
                };
                if let Some(ref fn_name) = self.current_fn_name {
                    self.fn_symbol_types
                        .insert((fn_name.clone(), var_name.clone()), elem_type.clone());
                }
                let prev = self.symbol_types.insert(var_name.clone(), elem_type);
                self.check_stmt(body, diag);
                if let Some(p) = prev {
                    self.symbol_types.insert(var_name.clone(), p);
                } else {
                    self.symbol_types.remove(var_name);
                }
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
                self.symbol_types.insert(resource_name.clone(), init_type);
                self.check_stmt(body, diag);
                DataraType::Unit
            }
        }
    }

    pub fn check_expr(&mut self, expr: &Expr, diag: &mut DiagnosticEngine) -> DataraType {
        match expr {
            Expr::Literal(lit, _) => match lit {
                LiteralValue::Int(_) => DataraType::Int,
                LiteralValue::Float(_) => DataraType::Float,
                LiteralValue::String(_) => DataraType::String,
                LiteralValue::Bool(_) => DataraType::Bool,
                LiteralValue::Char(_) => DataraType::Char,
                LiteralValue::None => DataraType::Option(Box::new(DataraType::Unit)),
            },
            Expr::Identifier(name, _) => {
                if let Some(t) = self.symbol_types.get(name) {
                    return t.clone();
                }
                if self.resolver.classes.contains_key(name) {
                    return DataraType::Class(name.clone());
                }
                DataraType::Int
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.check_expr(e, diag);
                }
                DataraType::String
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                let lt = self.check_expr(left, diag);
                let rt = self.check_expr(right, diag);

                match op.as_str() {
                    "+" if lt == DataraType::String || rt == DataraType::String => {
                        DataraType::String
                    }
                    "+" | "-" | "*" | "/" | "%" => {
                        if lt == DataraType::Float || rt == DataraType::Float {
                            DataraType::Float
                        } else {
                            DataraType::Int
                        }
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => DataraType::Bool,
                    _ => lt,
                }
            }
            Expr::Unary { op, expr, .. } => {
                let inner = self.check_expr(expr, diag);
                if op == "!" { DataraType::Bool } else { inner }
            }
            Expr::Call { callee, args, span } => {
                let mut arg_types = Vec::new();
                for a in args {
                    arg_types.push(self.check_expr(a, diag));
                }

                if let Expr::Identifier(fn_name, _) = &**callee {
                    if fn_name == "view" || fn_name == "mut_view" || fn_name == "mutView" {
                        return arg_types.first().cloned().unwrap_or(DataraType::Unit);
                    }
                    if fn_name == "destroy" || fn_name == "unsafe_op" {
                        return DataraType::Unit;
                    }
                    if fn_name == "println" || fn_name == "print" || fn_name == "eprintln" {
                        return DataraType::Unit;
                    }
                    if fn_name == "len" || fn_name == "now" {
                        return DataraType::Int;
                    }
                    if fn_name == "panic" || fn_name == "exit" {
                        return DataraType::Never;
                    }
                    if fn_name == "assert" {
                        return DataraType::Unit;
                    }
                    if fn_name == "input" || fn_name == "read_line" {
                        return DataraType::String;
                    }
                    if fn_name == "str_to_float" {
                        return DataraType::Float;
                    }

                    if let Some((param_types, ret_type, _gen_params)) =
                        self.function_signatures.get(fn_name).cloned()
                    {
                        let mut type_bindings: HashMap<String, DataraType> = HashMap::new();

                        for (idx, (p_ty, a_ty)) in
                            param_types.iter().zip(arg_types.iter()).enumerate()
                        {
                            if let DataraType::TypeParam(param_name) = p_ty {
                                if let Some(existing_bound) = type_bindings.get(param_name) {
                                    if !existing_bound.is_compatible(a_ty) {
                                        diag.error(
                                            ErrorCode::TypeMismatch,
                                            format!(
                                                "Generic type parameter '{}' bound to '{}' but argument {} received '{}'",
                                                param_name, existing_bound, idx + 1, a_ty
                                            ),
                                            Some(span.clone()),
                                        );
                                    }
                                } else {
                                    type_bindings.insert(param_name.clone(), a_ty.clone());
                                }
                            } else if !a_ty.is_compatible(p_ty) {
                                diag.error(
                                    ErrorCode::TypeMismatch,
                                    format!(
                                        "Type mismatch for argument {}: expected '{}', got '{}'",
                                        idx + 1,
                                        p_ty,
                                        a_ty
                                    ),
                                    Some(span.clone()),
                                );
                            }
                        }

                        if let DataraType::TypeParam(p) = &ret_type
                            && let Some(bound) = type_bindings.get(p)
                        {
                            return bound.clone();
                        }
                        return ret_type;
                    }
                }

                if let Expr::MemberAccess { object, member, .. } = &**callee {
                    let obj_type = self.check_expr(object, diag);
                    if let DataraType::Class(cls) = &obj_type {
                        let full_name = format!("{}.{}", cls, member);
                        if let Some(t) = self.symbol_types.get(&full_name) {
                            return t.clone();
                        }
                        if cls == "List" {
                            if member == "length"
                                || member == "count"
                                || member == "get"
                                || member == "pop"
                                || member == "len"
                            {
                                return DataraType::Int;
                            }
                            if member == "set" || member == "push" || member == "append" {
                                return DataraType::Class("List".into());
                            }
                        }
                        if cls == "Map" {
                            if member == "get" {
                                return DataraType::Int;
                            }
                            if member == "insert" {
                                return DataraType::Class("Map".into());
                            }
                        }
                        if let Some(m_type) =
                            self.class_methods.get(cls).and_then(|m| m.get(member))
                        {
                            return m_type.clone();
                        }
                        let specialized = format!("{}_{}", cls, member);
                        if let Some((_, ret_ty, _)) = self.function_signatures.get(&specialized) {
                            return ret_ty.clone();
                        }
                    }
                    if let Some((_, ret_ty, _)) = self.function_signatures.get(member) {
                        return ret_ty.clone();
                    }
                }

                let callee_ty = self.check_expr(callee, diag);
                if let DataraType::Function { return_type, .. } = callee_ty {
                    return *return_type;
                }
                DataraType::Unit
            }
            Expr::MemberAccess {
                object,
                member,
                span,
            } => {
                let obj_type = self.check_expr(object, diag);
                match &obj_type {
                    DataraType::Class(cls_name) => {
                        let full_name = format!("{}.{}", cls_name, member);
                        if let Some(t) = self.symbol_types.get(&full_name) {
                            return t.clone();
                        }
                        if member == "view" || member == "clone" || member == "mut_view" {
                            return DataraType::Class(cls_name.clone());
                        }
                        if let Some(pkt) = self.resolver.packets.get(cls_name)
                            && pkt.fields.iter().any(|f| &f.name == member)
                        {
                            return DataraType::Int;
                        }
                        if let Some(fields) = self.class_fields.get(cls_name)
                            && let Some(f_type) = fields.get(member)
                        {
                            return f_type.clone();
                        }
                        if let Some((_, t_fields)) = self.generic_templates.get(cls_name)
                            && let Some(f_type) = t_fields.get(member)
                        {
                            return f_type.clone();
                        }
                        if let Some(methods) = self.class_methods.get(cls_name)
                            && methods.contains_key(member)
                        {
                            return DataraType::Unit;
                        }
                        let mut known_fields: Vec<&str> = Vec::new();
                        if let Some(fields) = self.class_fields.get(cls_name) {
                            known_fields.extend(fields.keys().map(|s| s.as_str()));
                        }
                        if let Some((_, t_fields)) = self.generic_templates.get(cls_name) {
                            known_fields.extend(t_fields.keys().map(|s| s.as_str()));
                        }
                        if let Some(methods) = self.class_methods.get(cls_name) {
                            known_fields.extend(methods.keys().map(|s| s.as_str()));
                        }
                        let help_msg = if let Some(similar) =
                            crate::diagnostics::suggestions::find_best_match(member, known_fields)
                        {
                            format!(
                                "class '{}' has a field or method with a similar name: '{}'",
                                cls_name, similar
                            )
                        } else {
                            format!(
                                "class '{}' does not define field or method '{}'",
                                cls_name, member
                            )
                        };
                        diag.error_with_help(
                            ErrorCode::TypeInvalidMemberAccess,
                            format!(
                                "Class '{}' has no field or method named '{}'",
                                cls_name, member
                            ),
                            Some(span.clone()),
                            Some(help_msg),
                        );
                    }
                    DataraType::GenericInstance { name, args } => {
                        if member == "view" || member == "clone" || member == "mut_view" {
                            return DataraType::GenericInstance {
                                name: name.clone(),
                                args: args.clone(),
                            };
                        }
                        if let Some((params, t_fields)) = self.generic_templates.get(name)
                            && let Some(field_type) = t_fields.get(member)
                        {
                            if let DataraType::TypeParam(p) = field_type
                                && let Some(idx) = params.iter().position(|param| param == p)
                                && idx < args.len()
                            {
                                return args[idx].clone();
                            }
                            return field_type.clone();
                        }
                    }
                    _ => {}
                }
                DataraType::String
            }
            Expr::ObjectInit {
                class_name,
                generic_args,
                fields,
                ..
            } => {
                let mut inferred_args = Vec::new();
                for g in generic_args {
                    inferred_args.push(Self::resolve_tn(g));
                }

                let mut field_types = HashMap::new();
                for (fname, val) in fields {
                    let ft = self.check_expr(val, diag);
                    field_types.insert(fname.clone(), ft);
                }

                if let Some((params, _)) = self.generic_templates.get(class_name) {
                    if inferred_args.is_empty()
                        && !params.is_empty()
                        && let Some((_, first_val_type)) = field_types.iter().next()
                    {
                        inferred_args.push(first_val_type.clone());
                    }

                    if !inferred_args.is_empty() {
                        self.generic_specializations
                            .entry(class_name.clone())
                            .or_default()
                            .insert(inferred_args.clone());

                        return DataraType::GenericInstance {
                            name: class_name.clone(),
                            args: inferred_args,
                        };
                    }
                }

                DataraType::Class(class_name.clone())
            }
            Expr::Pipeline { stages, .. } => {
                let mut current = DataraType::Int;
                for s in stages {
                    current = self.check_expr(s, diag);
                }
                current
            }
            Expr::Decide { arms, else_arm, .. } => {
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    self.check_expr(&arm.condition, diag);
                    let body_ty = self.check_expr(&arm.body, diag);
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible(u) && !u.is_compatible(&body_ty) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in decide arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                if let Some(eb) = else_arm {
                    let else_ty = self.check_expr(eb, diag);
                    if let Some(ref u) = unified {
                        if !else_ty.is_compatible(u) && !u.is_compatible(&else_ty) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in decide else branch: '{}' and '{}'",
                                    u, else_ty
                                ),
                                Some(eb.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(else_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Match { value, arms, .. } => {
                let val_ty = self.check_expr(value, diag);
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    let mut bound = Vec::new();
                    match &arm.pattern {
                        Pattern::Identifier(name, _) if name != "_" => {
                            let prev = self.symbol_types.insert(name.clone(), val_ty.clone());
                            bound.push((name.clone(), prev));
                        }
                        Pattern::Variant { bindings, .. } => {
                            for b in bindings {
                                let prev = self.symbol_types.insert(b.clone(), val_ty.clone());
                                bound.push((b.clone(), prev));
                            }
                        }
                        _ => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.check_expr(g, diag);
                    }
                    let body_ty = self.check_expr(&arm.body, diag);
                    for (name, prev) in bound {
                        if let Some(p) = prev {
                            self.symbol_types.insert(name, p);
                        } else {
                            self.symbol_types.remove(&name);
                        }
                    }
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible(u) && !u.is_compatible(&body_ty) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in match arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Select { arms, else_arm, .. } => {
                let mut unified: Option<DataraType> = None;
                for arm in arms {
                    self.check_expr(&arm.condition, diag);
                    let body_ty = self.check_expr(&arm.body, diag);
                    if let Some(ref u) = unified {
                        if !body_ty.is_compatible(u) && !u.is_compatible(&body_ty) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in select arms: '{}' and '{}'",
                                    u, body_ty
                                ),
                                Some(arm.body.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(body_ty);
                    }
                }
                if let Some(eb) = else_arm {
                    let else_ty = self.check_expr(eb, diag);
                    if let Some(ref u) = unified {
                        if !else_ty.is_compatible(u) && !u.is_compatible(&else_ty) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Incompatible types in select else branch: '{}' and '{}'",
                                    u, else_ty
                                ),
                                Some(eb.span().clone()),
                            );
                        }
                    } else {
                        unified = Some(else_ty);
                    }
                }
                unified.unwrap_or(DataraType::Unit)
            }
            Expr::Lambda { params, body, .. } => {
                let mut bound = Vec::new();
                let mut param_types = Vec::new();
                for p in params {
                    let p_ty = p
                        .type_node
                        .as_ref()
                        .map(Self::resolve_tn)
                        .unwrap_or(DataraType::Int);
                    param_types.push(p_ty.clone());
                    let prev = self.symbol_types.insert(p.name.clone(), p_ty);
                    bound.push((p.name.clone(), prev));
                }
                let ret_ty = self.check_expr(body, diag);
                for (name, prev) in bound {
                    if let Some(p) = prev {
                        self.symbol_types.insert(name, p);
                    } else {
                        self.symbol_types.remove(&name);
                    }
                }
                DataraType::Function {
                    params: param_types,
                    return_type: Box::new(ret_ty),
                }
            }
            Expr::ListLiteral(items, _) => {
                let mut elem: Option<DataraType> = None;
                let mut heterogeneous = false;
                for item in items {
                    let t = self.check_expr(item, diag);
                    if t == DataraType::Unit {
                        continue;
                    }
                    match &elem {
                        Some(prev) if *prev != t => heterogeneous = true,
                        Some(_) => {}
                        None => elem = Some(t),
                    }
                }
                self.last_list_element = if heterogeneous { None } else { elem };
                DataraType::Class("List".into())
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.check_expr(k, diag);
                    self.check_expr(v, diag);
                }
                DataraType::Class("Map".into())
            }
            Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.check_expr(object, diag);
                let idx_ty = self.check_expr(index, diag);
                if idx_ty == DataraType::Class("Range".into()) {
                    obj_ty
                } else if let Expr::Identifier(name, _) = &**object {
                    // Element type recorded from the list literal initializer,
                    // so `let names = ["a"]; names[0]` is a String, not Int.
                    self.var_element_types
                        .get(name)
                        .cloned()
                        .unwrap_or(DataraType::Int)
                } else {
                    DataraType::Int
                }
            }
            Expr::Range { start, end, .. } => {
                self.check_expr(start, diag);
                self.check_expr(end, diag);
                DataraType::Class("Range".into())
            }
            Expr::Tuple(exprs, _) => {
                let types = exprs.iter().map(|e| self.check_expr(e, diag)).collect();
                DataraType::Tuple(types)
            }
            Expr::ErrorPropagate(inner, span) => {
                let t = self.check_expr(inner, diag);
                // `?` is real error propagation, not a silent no-op. Two hard
                // rules keep it predictable:
                //   1. the operand must be Result-like (`T!E`, `Result<T,E>`,
                //      `Outcome<T>`) or Option-like (`T?`, `Option<T>`, `Maybe<T>`);
                //   2. the enclosing function/method must return the same kind
                //      with a compatible payload, otherwise there is nothing to
                //      propagate the error into.
                let (kind, payload) = if let Some((ok, _err)) = t.result_like() {
                    (PropagationKind::Outcome, ok)
                } else if let Some(inner_ty) = t.option_like() {
                    (PropagationKind::Maybe, inner_ty)
                } else {
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!(
                            "'?' requires a Result ('T!E') or Option ('T?') operand, got '{}'",
                            t
                        ),
                        Some(span.clone()),
                    );
                    return t;
                };

                let expected_payload = match kind {
                    PropagationKind::Outcome => self
                        .current_return_type
                        .as_ref()
                        .and_then(|r| r.result_like())
                        .map(|(ok, _)| ok),
                    PropagationKind::Maybe => self
                        .current_return_type
                        .as_ref()
                        .and_then(|r| r.option_like()),
                };
                match expected_payload {
                    None => {
                        diag.error(
                            ErrorCode::TypeMismatch,
                            format!(
                                "'?' propagates a {} but the enclosing function returns '{}'; the function must return the same Result/Option type to propagate",
                                match kind {
                                    PropagationKind::Outcome => "Result error",
                                    PropagationKind::Maybe => "None",
                                },
                                self.current_return_type
                                    .as_ref()
                                    .map(|r| r.to_string())
                                    .unwrap_or_else(|| "Unit".into()),
                            ),
                            Some(span.clone()),
                        );
                    }
                    Some(expected) => {
                        if !payload.is_compatible(&expected) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "'?' unwraps '{}' but the enclosing function returns a Result/Option of '{}'",
                                    payload, expected
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }

                self.propagation_sites.push(PropagationSite {
                    span: span.clone(),
                    kind,
                    payload_repr: payload.to_string(),
                });
                payload
            }
            Expr::OrRecovery { expr, arms, .. } => {
                let expr_ty = self.check_expr(expr, diag);
                let inner_ty = if let Some((ok, _err)) = expr_ty.result_like() {
                    ok
                } else if let Some(inner) = expr_ty.option_like() {
                    inner
                } else {
                    expr_ty
                };
                for arm in arms {
                    let _ = self.check_expr(&arm.body, diag);
                }
                inner_ty
            }
            Expr::ArrayRepeatLiteral { elem, .. } => {
                let elem_ty = self.check_expr(elem, diag);
                DataraType::GenericInstance {
                    name: "Array".to_string(),
                    args: vec![elem_ty],
                }
            }
        }
    }
}
