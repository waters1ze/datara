use super::{DataraType, TypeChecker};
use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use crate::resolver::Resolver;
use std::collections::{HashMap, HashSet};

impl<'a> TypeChecker<'a> {
    /// Static type-node resolution used where no diagnostic engine is
    /// available (e.g. the prelude signature table). Expands type aliases
    /// exactly like `resolve_type_node` so prelude signatures match the
    /// types produced by checked code paths.
    pub fn resolve_tn(resolver: &Resolver, tn: &TypeNode) -> DataraType {
        Self::resolve_type_node_in(resolver, tn, &mut HashSet::new(), None)
    }

    pub fn resolve_type_node(&self, tn: &TypeNode, diag: &mut DiagnosticEngine) -> DataraType {
        Self::resolve_type_node_in(self.resolver, tn, &mut HashSet::new(), Some(diag))
    }

    fn resolve_type_node_in(
        resolver: &Resolver,
        tn: &TypeNode,
        visiting: &mut HashSet<String>,
        mut diag: Option<&mut DiagnosticEngine>,
    ) -> DataraType {
        // Type aliases are transparently expanded. `visiting` guards against
        // circular aliases such as `type A = A;`, which would otherwise
        // recurse until the stack overflows.
        if let Some(td) = resolver.type_aliases.get(&tn.name) {
            if !visiting.insert(tn.name.clone()) {
                if let Some(d) = diag.as_deref_mut() {
                    d.error(
                        ErrorCode::ResolveCircularDependency,
                        format!("Circular type alias '{}'", tn.name),
                        Some(tn.span.clone()),
                    );
                }
                return DataraType::Unit;
            }
            let expanded =
                Self::resolve_type_node_in(resolver, &td.base_type, visiting, diag.as_deref_mut());
            visiting.remove(&tn.name);
            return expanded;
        }
        if !tn.generic_args.is_empty() {
            let args: Vec<DataraType> = tn
                .generic_args
                .iter()
                .map(|arg| Self::resolve_type_node_in(resolver, arg, visiting, diag.as_deref_mut()))
                .collect();
            // `Result<T, E>` and `Option<T>` written in generic form are the
            // same abstract types as the `T!E` / `T?` suffix forms.
            if tn.name == "Result" && args.len() == 2 {
                return DataraType::Result(Box::new(args[0].clone()), Box::new(args[1].clone()));
            }
            if tn.name == "Option" && args.len() == 1 {
                return DataraType::Option(Box::new(args[0].clone()));
            }
            // Parametric collections: `List<T>` / `Map<K, V>` annotations carry
            // their element types into the checker so indexing, for-loop
            // binding and method returns stay typed. `Display` erases them
            // back to the bare class name for diagnostics/codegen.
            if tn.name == "List" && args.len() == 1 {
                return DataraType::List(Box::new(args[0].clone()));
            }
            if tn.name == "Map" && args.len() == 2 {
                return DataraType::Map(Box::new(args[0].clone()), Box::new(args[1].clone()));
            }
            if (tn.name == "Float"
                || tn.name == "Float64"
                || tn.name == "Float32"
                || tn.name == "Int"
                || tn.name == "Int64"
                || tn.name == "UInt"
                || tn.name == "UInt64")
                && tn.generic_args.len() == 1
            {
                let unit = tn.generic_args[0].name.clone();
                let base = if tn.name.starts_with("Float") {
                    DataraType::Float
                } else {
                    DataraType::Int
                };
                return DataraType::Measure {
                    base: Box::new(base),
                    unit,
                };
            }
            return DataraType::GenericInstance {
                name: tn.name.clone(),
                args,
            };
        }

        let mut base = match tn.name.as_str() {
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
            // A name is only treated as a generic type parameter when it is
            // NOT a declared class/enum/type alias; otherwise a field like
            // `value: Value` (with a real `Value` class) would silently
            // resolve to a type parameter.
            other
                if other.len() == 1
                    && other.chars().next().unwrap().is_ascii_uppercase()
                    && !resolver.classes.contains_key(other)
                    && !resolver.type_aliases.contains_key(other) =>
            {
                DataraType::TypeParam(other.to_string())
            }
            "Item" | "Key" | "Value" | "Element" | "Err" | "Target"
                if !resolver.classes.contains_key(tn.name.as_str())
                    && !resolver.type_aliases.contains_key(tn.name.as_str()) =>
            {
                DataraType::TypeParam(tn.name.clone())
            }
            other => DataraType::Class(other.to_string()),
        };

        if let Some(Refinement::Range {
            start,
            end,
            inclusive,
        }) = &tn.refinement
        {
            let min = match &**start {
                Expr::Literal(LiteralValue::Int(n), _) => *n as i128,
                _ => 0,
            };
            let max = match &**end {
                Expr::Literal(LiteralValue::Int(n), _) => {
                    if *inclusive {
                        *n as i128
                    } else {
                        (*n - 1) as i128
                    }
                }
                _ => i128::MAX,
            };
            base = DataraType::Range {
                base: Box::new(base),
                min,
                max,
            };
        }

        if tn.is_option {
            DataraType::Option(Box::new(base))
        } else if let Some(err) = &tn.error_type {
            let err_type = Self::resolve_type_node_in(resolver, err, visiting, diag);
            DataraType::Result(Box::new(base), Box::new(err_type))
        } else {
            base
        }
    }

    pub fn get_refinement<'b>(&'b self, tn: &'b TypeNode) -> Option<&'b Refinement> {
        Self::get_refinement_in(&self.resolver.type_aliases, tn, &mut HashSet::new())
    }

    fn get_refinement_in<'b>(
        aliases: &'b HashMap<String, TypeDecl>,
        tn: &'b TypeNode,
        visiting: &mut HashSet<String>,
    ) -> Option<&'b Refinement> {
        if let Some(r) = &tn.refinement {
            return Some(r);
        }
        // Refinements survive alias expansion; guard against circular
        // aliases the same way `resolve_type_node_in` does.
        if let Some(td) = aliases.get(&tn.name) {
            if !visiting.insert(tn.name.clone()) {
                return None;
            }
            let r = Self::get_refinement_in(aliases, &td.base_type, visiting);
            visiting.remove(&tn.name);
            return r;
        }
        None
    }
}
