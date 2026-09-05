use crate::dmir::{Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::{HashMap, HashSet};

pub struct MemoryOptimizer;

impl MemoryOptimizer {
    /// Escape Analysis & SROA Scalarization across top-level blocks and nested loop bodies
    pub fn scalarize_structures(
        f: &mut Function,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut allocations_eliminated = 0;

        // Field maps are tracked per instruction list with no cross-block
        // invalidation: a `SetField` in block A is deleted while a `GetField`
        // in block B would still read the pre-update field values. Only
        // scalarize functions whose entire body is one block.
        if f.blocks.len() == 1 {
            for block in &mut f.blocks {
                let (new_insts, eliminated) = Self::scalarize_instruction_list(
                    &block.instructions,
                    &block.terminator,
                    f.name.as_str(),
                    cost_model,
                    trace,
                );
                block.instructions = new_insts;
                allocations_eliminated += eliminated;
            }
        }

        allocations_eliminated += Self::eliminate_bounds_checks(f, cost_model, trace);

        allocations_eliminated
    }

    fn scalarize_instruction_list(
        instructions: &[Inst],
        terminator: &Terminator,
        fn_name: &str,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> (Vec<Inst>, usize) {
        let mut eliminated = 0;
        let mut struct_defs: HashMap<ValueId, (String, HashMap<String, ValueId>)> = HashMap::new();
        let mut var_to_struct: HashMap<String, (String, HashMap<String, ValueId>)> = HashMap::new();
        let mut escaping_structs: HashSet<ValueId> = HashSet::new();

        let mut var_to_val: HashMap<String, ValueId> = HashMap::new();
        let mut val_alias: HashMap<ValueId, ValueId> = HashMap::new();

        // 1. Identify struct allocations and escaping usages
        for inst in instructions {
            match inst {
                Inst::StructInit {
                    dest,
                    class_name,
                    fields,
                } => {
                    let field_map: HashMap<String, ValueId> = fields.iter().cloned().collect();
                    struct_defs.insert(*dest, (class_name.clone(), field_map.clone()));
                }
                Inst::AssignVar { name, value } => {
                    var_to_val.insert(name.clone(), *value);
                }
                Inst::LoadVar { dest, name } => {
                    if let Some(&v) = var_to_val.get(name) {
                        val_alias.insert(*dest, v);
                    }
                }
                Inst::MethodCall { object, args, .. } => {
                    escaping_structs.insert(*object);
                    if let Some(&orig) = val_alias.get(object) {
                        escaping_structs.insert(orig);
                    }
                    for a in args {
                        escaping_structs.insert(*a);
                        if let Some(&orig) = val_alias.get(a) {
                            escaping_structs.insert(orig);
                        }
                    }
                }
                // A field store mutates the object in place, so the object is
                // no longer interchangeable with its initial field values.
                Inst::SetField { value, .. } => {
                    escaping_structs.insert(*value);
                    if let Some(&orig) = val_alias.get(value) {
                        escaping_structs.insert(orig);
                    }
                }
                Inst::Call { args, .. } => {
                    for a in args {
                        escaping_structs.insert(*a);
                        if let Some(&orig) = val_alias.get(a) {
                            escaping_structs.insert(orig);
                        }
                    }
                }
                Inst::Return { value: Some(v) } => {
                    escaping_structs.insert(*v);
                    if let Some(&orig) = val_alias.get(v) {
                        escaping_structs.insert(orig);
                    }
                }
                Inst::Out { value } | Inst::Err { value } => {
                    escaping_structs.insert(*value);
                    if let Some(&orig) = val_alias.get(value) {
                        escaping_structs.insert(orig);
                    }
                }
                _ => {}
            }
        }

        // The terminator is a use as well: `return s` keeps `s`'s object
        // alive. Nested instruction lists (loop bodies) own no terminator,
        // represented here by the unreachable default.
        if let Terminator::Return { value: Some(v) } = terminator {
            escaping_structs.insert(*v);
            if let Some(&orig) = val_alias.get(v) {
                escaping_structs.insert(orig);
            }
        }

        // Collapse escapes to their alias roots.
        //
        // `escaping_structs` is keyed by whatever ValueId happened to be used,
        // which is usually a fresh `LoadVar` destination rather than the
        // `StructInit` that produced the object. Every later read of the same
        // variable produces *another* fresh ValueId, so a naive lookup reports
        // "not escaping" for the very object that just escaped, and field
        // forwarding substitutes the object's *initial* field values —
        // silently undoing every mutation the method performed.
        //
        // Resolving through the alias map (with a depth cap, because the map
        // is built from a single linear pass and must not loop) makes the
        // escape property follow the object instead of one of its names.
        let mut escaping_roots: HashSet<ValueId> = HashSet::new();
        for v in &escaping_structs {
            let mut root = *v;
            for _ in 0..16 {
                match val_alias.get(&root) {
                    Some(&next) if next != root => root = next,
                    _ => break,
                }
            }
            escaping_roots.insert(root);
        }
        escaping_structs.extend(escaping_roots);

        // Report the result only after escape analysis is complete. This helper
        // performs field forwarding; the outer optimizer is responsible for
        // removing a proven non-escaping StructInit. Therefore the record is
        // deliberately `Preserved`, not `Applied`, until the complete DMIR
        // delta has been verified.
        for (dest, (class_name, field_map)) in &struct_defs {
            let escapes = escaping_structs.contains(dest);
            let (apply, benefit, cost, reason) =
                cost_model.evaluate_sroa(class_name, field_map.len(), escapes);
            trace.record(
                "SROA",
                &format!("{}:{}", fn_name, class_name),
                if apply { "Preserved" } else { "Rejected" },
                &benefit,
                &cost,
                if apply {
                    "Non-escaping candidate: field forwarding is available, but this helper does not claim allocation removal"
                } else {
                    &reason
                },
            );
        }

        // 2. Perform field forwarding and eliminate field access overhead
        let mut new_instructions = Vec::new();
        for inst in instructions {
            match inst {
                Inst::StructInit { .. } => {
                    new_instructions.push(inst.clone());
                }
                Inst::WhileLoop {
                    condition_insts,
                    cond_val,
                    body_insts,
                } => {
                    let (new_body, loop_eliminated) = Self::scalarize_instruction_list(
                        body_insts,
                        &Terminator::default(),
                        fn_name,
                        cost_model,
                        trace,
                    );
                    eliminated += loop_eliminated;
                    new_instructions.push(Inst::WhileLoop {
                        condition_insts: condition_insts.clone(),
                        cond_val: *cond_val,
                        body_insts: new_body,
                    });
                }
                Inst::AssignVar { name, value } => {
                    if let Some(s_def) = struct_defs.get(value) {
                        var_to_struct.insert(name.clone(), s_def.clone());
                    }
                    new_instructions.push(inst.clone());
                }
                Inst::LoadVar { dest, name } => {
                    if let Some(s_def) = var_to_struct.get(name) {
                        struct_defs.insert(*dest, s_def.clone());
                    }
                    new_instructions.push(inst.clone());
                }
                Inst::SetField {
                    object,
                    field,
                    value,
                } => {
                    let mut object_root = *object;
                    for _ in 0..16 {
                        match val_alias.get(&object_root) {
                            Some(&next) if next != object_root => object_root = next,
                            _ => break,
                        }
                    }
                    if !escaping_structs.contains(&object_root) {
                        let mut updated = false;
                        if let Some((_, field_map)) = struct_defs.get_mut(&object_root) {
                            field_map.insert(field.clone(), *value);
                            updated = true;
                        }
                        if let Some((_, field_map)) = struct_defs.get_mut(object) {
                            field_map.insert(field.clone(), *value);
                            updated = true;
                        }
                        // Update ONLY the field maps of variables that alias
                        // this object. Poisoning every tracked struct
                        // variable's map would forward the wrong field value
                        // through unrelated LoadVars.
                        for (name, &bound_val) in &var_to_val {
                            if bound_val == object_root
                                && let Some((_, field_map)) = var_to_struct.get_mut(name)
                            {
                                field_map.insert(field.clone(), *value);
                            }
                        }
                        if updated {
                            eliminated += 1;
                            continue;
                        }
                    }
                    new_instructions.push(inst.clone());
                }
                Inst::GetField {
                    dest,
                    object,
                    field,
                    ty,
                } => {
                    // Resolve the object to its alias root before asking
                    // whether it escaped; a fresh load of an escaped object is
                    // still the escaped object.
                    let mut object_root = *object;
                    for _ in 0..16 {
                        match val_alias.get(&object_root) {
                            Some(&next) if next != object_root => object_root = next,
                            _ => break,
                        }
                    }

                    if !escaping_structs.contains(&object_root)
                        && let Some((_, field_map)) = struct_defs.get(object)
                        && let Some(&field_val) = field_map.get(field)
                    {
                        // Direct scalar register copy, bypassing object allocation
                        new_instructions.push(Inst::UnOp {
                            dest: *dest,
                            op: "copy".to_string(),
                            operand: field_val,
                            ty: ty.clone(),
                        });
                        eliminated += 1;
                        continue;
                    }
                    new_instructions.push(inst.clone());
                }
                _ => {
                    new_instructions.push(inst.clone());
                }
            }
        }

        (new_instructions, eliminated)
    }

    /// Bounds-check analysis.
    ///
    /// DMIR currently has no explicit array-bounds-check instruction. A
    /// comparison (`<`/`<=`) is not itself a bounds check and deleting it would
    /// change program semantics. Until the array access and its check are
    /// represented explicitly, this pass is analysis-only and returns zero.
    pub fn eliminate_bounds_checks(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        for block in &f.blocks {
            let comparison_count = block
                .instructions
                .iter()
                .filter(|inst| matches!(inst, Inst::BinOp { op, .. } if op == "<" || op == "<="))
                .count();

            if comparison_count > 0 {
                trace.record(
                    "BCE",
                    &format!("{}:bb{}", f.name, block.id.0),
                    "Rejected",
                    "Bounds-check candidate not eliminated",
                    "No explicit bounds-check/access pair in DMIR",
                    "A comparison alone is not proof that a runtime array check can be removed; IR is preserved.",
                );
            }
        }

        0
    }
}
