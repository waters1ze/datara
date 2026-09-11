//! Ownership, Effects, and Profile-Guided Metadata to LLVM IR Attributes.
//!
//! Phase 11: Derives fine-grained LLVM IR attributes from Datara's affine
//! ownership solver and algebraic effects system:
//! - Uniqueness of ownership -> `noalias`
//! - Pure effect / non-mutating -> `readonly` / `readnone`
//! - Non-escaping pointer -> `nocapture`
//! - Known aggregate/class layout -> `dereferenceable(n)`
//! - Known aggregate alignment -> `align n`
//! - Branch hints: `!prof` metadata for guard/cold/biased branches and PGO
//! - Honest vectorizer metadata: only emit when target actually supports vector extensions

use crate::ast::{Decl, Param, Program};
use crate::codegen::target::{TargetInfo, VectorExtension};
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use crate::pgo::ProfileData;
use std::collections::HashMap;

/// Pre-analyzed function-level attributes context for LLVM IR emission.
#[derive(Debug, Clone)]
pub struct FunctionAttrContext {
    pub is_pure: bool,
    pub is_cold: bool,
    pub ast_params: Vec<Param>,
}

/// Helper to index all declared functions in AST Program.
pub fn index_program_signatures(program: &Program) -> HashMap<String, (Vec<Param>, bool)> {
    let mut sigs = HashMap::new();
    for decl in &program.declarations {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                let is_pure = f.attributes.iter().any(|a| a.name == "pure");
                sigs.insert(f.name.clone(), (f.params.clone(), is_pure));
            }
            Decl::Impl(i) => {
                for m in &i.methods {
                    let is_pure = m.attributes.iter().any(|a| a.name == "pure");
                    let qualified = format!("{}_{}", i.target_type, m.name);
                    sigs.insert(qualified, (m.params.clone(), is_pure));
                    sigs.insert(m.name.clone(), (m.params.clone(), is_pure));
                }
            }
            _ => {}
        }
    }
    sigs
}

/// Check if a DMIR type string maps to a pointer type in LLVM IR.
pub fn is_pointer_type(ty: &str) -> bool {
    !matches!(
        ty,
        "Int"
            | "Int64"
            | "UInt"
            | "UInt64"
            | "i64"
            | "u64"
            | "isize"
            | "usize"
            | "USize"
            | "Bool"
            | "Int32"
            | "UInt32"
            | "i32"
            | "u32"
            | "Int16"
            | "UInt16"
            | "i16"
            | "u16"
            | "Int8"
            | "UInt8"
            | "i8"
            | "u8"
            | "Byte"
            | "i128"
            | "u128"
            | "Float"
            | "Float64"
            | "f64"
            | "Float32"
            | "f32"
            | "f16"
            | "<4 x float>"
            | "<4 x i32>"
            | "Unit"
            | "void"
            | "Never"
    ) && !ty.starts_with("Int<")
        && !ty.starts_with("UInt<")
        && !ty.starts_with("Float<")
}

/// Verify whether a pointer parameter escapes the function invocation.
pub fn is_pointer_escaped(func: &Function, val: ValueId) -> bool {
    for blk in &func.blocks {
        match &blk.terminator {
            Terminator::Return { value: Some(v) } if *v == val => return true,
            _ => {}
        }
        for inst in &blk.instructions {
            match inst {
                Inst::SetField { value, .. } if *value == val => return true,
                Inst::StructInit { fields, .. } if fields.iter().any(|(_, v)| *v == val) => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

/// Verify whether a pointer parameter is mutated inside the function.
pub fn is_pointer_mutated(func: &Function, val: ValueId) -> bool {
    for blk in &func.blocks {
        for inst in &blk.instructions {
            match inst {
                Inst::SetField { object, .. } if *object == val => return true,
                _ => {}
            }
        }
    }
    false
}

/// Check if a basic block represents a cold / panic / abort path.
pub fn is_cold_block(func: &Function, block_id: BasicBlockId) -> bool {
    let blk = match func.get_block(block_id) {
        Some(b) => b,
        None => return false,
    };
    if let Terminator::Unreachable = blk.terminator {
        return true;
    }
    for inst in &blk.instructions {
        match inst {
            Inst::Err { .. } => return true,
            Inst::Call { func: callee, .. } => {
                if callee.contains("panic")
                    || callee.contains("abort")
                    || callee.contains("overflow")
                    || callee.contains("err")
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Derive fine-grained LLVM IR parameter attributes from affine ownership and effects.
pub fn derive_param_attributes(
    func: &Function,
    param_idx: usize,
    _param_name: &str,
    param_ty: &str,
    param_val: ValueId,
    module: &Module,
    ast_params: Option<&[Param]>,
    is_pure: bool,
) -> String {
    if !is_pointer_type(param_ty) {
        return String::new();
    }

    let mut attrs: Vec<String> = Vec::new();

    // 1. Ownership / Uniqueness -> noalias
    let ownership_mode = ast_params
        .and_then(|ps| ps.get(param_idx))
        .map(|p| p.ownership_mode.as_str())
        .unwrap_or("owned");

    let ptr_param_count = func
        .params
        .iter()
        .filter(|(_, ty, _)| is_pointer_type(ty))
        .count();

    let is_unique_ownership = matches!(
        ownership_mode,
        "owned" | "own" | "val" | "mut-view" | "unique"
    ) || ptr_param_count <= 1;

    if is_unique_ownership {
        attrs.push("noalias".to_string());
    }

    // 2. Escape Analysis -> nocapture
    let escapes = is_pointer_escaped(func, param_val);
    if !escapes {
        attrs.push("nocapture".to_string());
    }

    // 3. Purity / Mutability -> readonly
    let mutated = is_pointer_mutated(func, param_val);
    if is_pure || !mutated || ownership_mode == "view" {
        attrs.push("readonly".to_string());
    }

    // 4. Known Aggregate Layout -> dereferenceable(n)
    let byte_size = if let Some(fields) = module.class_fields.get(param_ty) {
        fields.len().saturating_mul(8).max(8)
    } else if param_ty == "Str" || param_ty == "String" {
        16
    } else {
        8
    };
    attrs.push(format!("dereferenceable({})", byte_size));

    // 5. Aggregate Alignment -> align 8
    attrs.push("align 8".to_string());

    if attrs.is_empty() {
        String::new()
    } else {
        format!(" {}", attrs.join(" "))
    }
}

/// Derive function-level attributes (readonly, readnone, cold) for LLVM IR definition.
pub fn derive_fn_attributes(func: &Function, is_pure: bool, is_cold: bool) -> Vec<&'static str> {
    let mut attrs = Vec::new();
    if is_pure {
        let has_ptr = func.params.iter().any(|(_, ty, _)| is_pointer_type(ty));
        if has_ptr {
            attrs.push("readonly");
        } else {
            attrs.push("readnone");
        }
    }
    if is_cold {
        attrs.push("cold");
    }
    attrs
}

/// Derive branch metadata (!prof branch weights) for conditional branches.
#[allow(clippy::too_many_arguments)]
pub fn derive_branch_metadata(
    fn_name: &str,
    block_id: BasicBlockId,
    then_block: BasicBlockId,
    else_block: BasicBlockId,
    func: &Function,
    profile: Option<&ProfileData>,
    branch_weights_map: &mut HashMap<(u32, u32), usize>,
    next_meta_id: &mut usize,
) -> String {
    let mut weights: Option<(u32, u32)> = None;

    // 1. Check ProfileData if available
    if let Some(prof) = profile {
        let branch_id = format!("{}_{}", fn_name, block_id.0);
        if let Some(&(taken, total)) = prof.branch_frequencies.get(&branch_id) {
            let not_taken = total.saturating_sub(taken);
            weights = Some((taken.max(1) as u32, not_taken.max(1) as u32));
        }
    }

    // 2. Static guard heuristic
    if weights.is_none() {
        let then_cold = is_cold_block(func, then_block);
        let else_cold = is_cold_block(func, else_block);
        if then_cold && !else_cold {
            // Failure taken to then_block
            weights = Some((1, 1048576));
        } else if else_cold && !then_cold {
            // Failure taken to else_block
            weights = Some((1048576, 1));
        }
    }

    if let Some((w_then, w_else)) = weights {
        let node_id = *branch_weights_map
            .entry((w_then, w_else))
            .or_insert_with(|| {
                let id = *next_meta_id;
                *next_meta_id += 1;
                id
            });
        format!(", !prof !{}", node_id)
    } else {
        String::new()
    }
}

/// Check if vector extensions are supported by the target architecture.
pub fn is_vector_supported(target: &TargetInfo) -> bool {
    target.vector_support.contains(&VectorExtension::Avx2)
        || target.vector_support.contains(&VectorExtension::Avx)
        || target.vector_support.contains(&VectorExtension::Neon)
}
