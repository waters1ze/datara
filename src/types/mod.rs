use crate::ast::*;
use crate::diagnostics::span::SourceSpan;
use crate::resolver::Resolver;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod check;
mod match_check;
mod prelude;
mod refine;
mod resolve;

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
    /// A list with a statically known element type (`List<Int>`).
    /// `Display` prints just `List` so diagnostics and downstream
    /// Class-name matching keep working on the erased form.
    List(Box<DataraType>),
    /// A map with statically known key/value types (`Map<Str, Int>`).
    /// `Display` prints just `Map`.
    Map(Box<DataraType>, Box<DataraType>),
    Tuple(Vec<DataraType>),
    Function {
        params: Vec<DataraType>,
        return_type: Box<DataraType>,
    },
    Val,
    Dec64,
    Dec128,
    RawPtr,
    Range {
        base: Box<DataraType>,
        min: i128,
        max: i128,
    },
    Measure {
        base: Box<DataraType>,
        unit: String,
    },
}

impl DataraType {
    pub fn is_compatible(&self, other: &DataraType) -> bool {
        self.is_compatible_impl(other, None)
    }

    /// Resolver-aware variant of [`DataraType::is_compatible`].
    ///
    /// When a resolver is supplied, a bare `Class("C")` only matches
    /// `GenericInstance { name: "C", .. }` when `C` is a raw non-generic
    /// class. For a generic class the name-only match would wrongly accept
    /// e.g. `Pair<Int, Str>` against a bare `Pair`, so it is rejected. With
    /// no resolver available the historical name-only behavior is preserved.
    pub fn is_compatible_with_args(&self, other: &DataraType, resolver: Option<&Resolver>) -> bool {
        self.is_compatible_impl(other, resolver)
    }

    /// True when `name` is not a known generic class. Classes the resolver
    /// does not know about are treated as non-generic (conservative accept).
    fn raw_non_generic_class(resolver: Option<&Resolver>, name: &str) -> bool {
        match resolver {
            None => true,
            Some(r) => r
                .classes
                .get(name)
                .map(|sym| sym.generic_params.is_empty())
                .unwrap_or(true),
        }
    }

    fn is_compatible_impl(&self, other: &DataraType, resolver: Option<&Resolver>) -> bool {
        if self == other || *self == DataraType::Never || *other == DataraType::Never {
            return true;
        }
        if *self == DataraType::Val || *other == DataraType::Val {
            return true;
        }
        // Parametric collections: List(a) ~ List(b) elementwise, and a typed
        // collection coerces to (or from) its erased `Class("List")`/`Class("Map")`
        // form so bare `List` annotations and method returns keep working.
        if let (DataraType::List(a), DataraType::List(b)) = (self, other) {
            return a.is_compatible_impl(b, resolver);
        }
        if let (DataraType::Map(k1, v1), DataraType::Map(k2, v2)) = (self, other) {
            return k1.is_compatible_impl(k2, resolver) && v1.is_compatible_impl(v2, resolver);
        }
        if let (DataraType::List(_), DataraType::Class(c)) = (self, other)
            && c == "List"
        {
            return true;
        }
        if let (DataraType::Class(c), DataraType::List(_)) = (self, other)
            && c == "List"
        {
            return true;
        }
        if let (DataraType::Map(..), DataraType::Class(c)) = (self, other)
            && c == "Map"
        {
            return true;
        }
        if let (DataraType::Class(c), DataraType::Map(..)) = (self, other)
            && c == "Map"
        {
            return true;
        }
        if let (DataraType::Tuple(t1), DataraType::Tuple(t2)) = (self, other)
            && t1.len() == t2.len()
        {
            return t1
                .iter()
                .zip(t2.iter())
                .all(|(a, b)| a.is_compatible_impl(b, resolver));
        }
        if let (DataraType::Option(o1), DataraType::Option(o2)) = (self, other) {
            if **o1 == DataraType::Unit || **o2 == DataraType::Unit {
                return true;
            }
            return o1.is_compatible_impl(o2, resolver);
        }
        if let DataraType::Option(target) = other
            && self.is_compatible_impl(target, resolver)
        {
            return true;
        }
        if let (DataraType::Result(ok1, err1), DataraType::Result(ok2, err2)) = (self, other) {
            return ok1.is_compatible_impl(ok2, resolver)
                && err1.is_compatible_impl(err2, resolver);
        }
        if let (
            DataraType::GenericInstance { name: n1, args: a1 },
            DataraType::GenericInstance { name: n2, args: a2 },
        ) = (self, other)
            && n1 == n2
            && a1.len() == a2.len()
        {
            return a1
                .iter()
                .zip(a2.iter())
                .all(|(a, b)| a.is_compatible_impl(b, resolver));
        }
        // A bare `Class("C")` matches a `GenericInstance { name: "C", .. }`
        // only when `C` is a raw non-generic class. For a generic class the
        // name-only match would claim `Pair<Int, Str>` is compatible with
        // `Pair<Bool, Bool>` (both sides carry the same name but different
        // type arguments), so it is rejected when a resolver says `C` has
        // generic parameters. Note the both-sides-args case
        // (`GenericInstance` vs `GenericInstance`) is handled above by
        // pairwise argument comparison.
        if let (DataraType::Class(c), DataraType::GenericInstance { name: g, .. }) = (self, other)
            && c == g
            && Self::raw_non_generic_class(resolver, c)
        {
            return true;
        }
        if let (DataraType::GenericInstance { name: g, .. }, DataraType::Class(c)) = (self, other)
            && c == g
            && Self::raw_non_generic_class(resolver, c)
        {
            return true;
        }
        if let (
            DataraType::Measure { base: b1, unit: u1 },
            DataraType::Measure { base: b2, unit: u2 },
        ) = (self, other)
        {
            return u1 == u2 && b1.is_compatible_impl(b2, resolver);
        }
        if let (
            DataraType::Range {
                base: b1,
                min: min1,
                max: max1,
            },
            DataraType::Range {
                base: b2,
                min: min2,
                max: max2,
            },
        ) = (self, other)
        {
            return b1.is_compatible_impl(b2, resolver) && min1 >= min2 && max1 <= max2;
        }
        // A refined `Range{..}` coerces down to its base type (reading a
        // ranged value as a plain Int is safe), but the reverse is NOT
        // accepted here: lifting a plain Int into an arbitrary Range would
        // launder away the range constraint. Declaration/assignment sites
        // that legitimately accept a base value into a refined type use
        // `is_compatible_with_refined`, which pairs this coercion with the
        // value-level checks in `check_refinement`.
        if let DataraType::Range { base, .. } = self
            && !matches!(other, DataraType::Range { .. })
            && base.is_compatible_impl(other, resolver)
        {
            return true;
        }
        false
    }

    /// Like `is_compatible`, but additionally allows lifting a plain numeric
    /// base value into its refined `Range{..}` form. Only used at declaration
    /// and assignment sites where the actual value is range-checked
    /// separately (`check_refinement` / `check_range_and_measure_assignment`).
    pub fn is_compatible_with_refined(&self, other: &DataraType) -> bool {
        self.is_compatible_with_refined_impl(other, None)
    }

    /// Resolver-aware variant of [`DataraType::is_compatible_with_refined`]
    /// (see [`DataraType::is_compatible_with_args`]).
    pub fn is_compatible_with_refined_with_args(
        &self,
        other: &DataraType,
        resolver: Option<&Resolver>,
    ) -> bool {
        self.is_compatible_with_refined_impl(other, resolver)
    }

    fn is_compatible_with_refined_impl(
        &self,
        other: &DataraType,
        resolver: Option<&Resolver>,
    ) -> bool {
        if self.is_compatible_impl(other, resolver) {
            return true;
        }
        if let DataraType::Range { base, .. } = other
            && !matches!(self, DataraType::Range { .. })
            && self.is_compatible_impl(base, resolver)
        {
            return true;
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

    /// The collection class name for parametric collection types:
    /// `List(t)` -> `"List"`, `Map(k, v)` -> `"Map"`. Lets code that matches
    /// `Class("List")`/`Class("Map")` accept the parametric forms too.
    pub fn collection_base(&self) -> Option<&str> {
        match self {
            DataraType::List(_) => Some("List"),
            DataraType::Map(..) => Some("Map"),
            _ => None,
        }
    }

    /// Type-erased form: `List(t)` -> `Class("List")`, `Map(k, v)` ->
    /// `Class("Map")`, everything else unchanged. Used at boundaries where
    /// types are handed to codegen/DMIR, which only understand the erased
    /// class form; keeps codegen churn at zero.
    pub fn erasure(&self) -> DataraType {
        match self {
            DataraType::List(_) => DataraType::Class("List".to_string()),
            DataraType::Map(..) => DataraType::Class("Map".to_string()),
            other => other.clone(),
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
            // Parametric collections print as their bare class name (no
            // element types) so diagnostics and string comparisons against
            // "List"/"Map" keep working on the erased form.
            DataraType::List(_) => write!(f, "List"),
            DataraType::Map(..) => write!(f, "Map"),
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
            DataraType::Range { base, min, max } => write!(f, "{}<{}..{}>", base, min, max),
            DataraType::Measure { base, unit } => write!(f, "{}<{}>", base, unit),
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
    pub var_refinements: HashMap<String, TypeNode>,
    pub function_param_nodes: HashMap<String, Vec<Option<TypeNode>>>,
    pub var_array_lengths: HashMap<String, usize>,
}
