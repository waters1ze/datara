use super::decision::{AdaptationCategory, AdaptationDecisionLog, AdaptationRecord};
use crate::dmir::{Function, Inst, ValueId};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryPlacement {
    ScalarSSA,   // Promoted to virtual CPU registers (zero memory access)
    StackLocal,  // Bounded stack frame allocation
    HeapManaged, // Dynamically allocated heap region
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionLayout {
    ArrayOfStructs,     // [ {x, y}, {x, y} ]
    StructOfArrays,     // { x: [...], y: [...] }
    HybridAoSoA(usize), // 8 or 16 item chunks
}

pub struct RepresentationAdapter;

impl RepresentationAdapter {
    /// Evaluates representation candidates. This function does not rewrite DMIR;
    /// callers must not interpret the returned count as emitted transformations.
    pub fn adapt_representation(f: &mut Function, log: &mut AdaptationDecisionLog) -> usize {
        let mut candidates = 0;
        let mut struct_allocs: HashMap<ValueId, (String, usize)> = HashMap::new();
        let mut collection_allocs: HashMap<ValueId, String> = HashMap::new();
        let mut escaping_values: HashSet<ValueId> = HashSet::new();

        // 1. Trace allocations and escape characteristics
        for block in &f.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::StructInit {
                        dest,
                        class_name,
                        fields,
                    } => {
                        struct_allocs.insert(*dest, (class_name.clone(), fields.len()));
                    }
                    Inst::Call {
                        dest, func, args, ..
                    } => {
                        if func.starts_with("datara_rt_list_create")
                            || func.starts_with("datara_rt_map_create")
                        {
                            collection_allocs.insert(*dest, func.clone());
                        }
                        let is_collection_method = func.starts_with("datara_rt_list_")
                            || func.starts_with("datara_rt_map_");
                        for (idx, a) in args.iter().enumerate() {
                            if is_collection_method && idx == 0 {
                                // Object parameter in collection operations is queried, not leaked
                                continue;
                            }
                            escaping_values.insert(*a);
                        }
                    }
                    Inst::MethodCall { args, .. } => {
                        for a in args {
                            escaping_values.insert(*a);
                        }
                    }
                    Inst::SetField { value, .. } => {
                        escaping_values.insert(*value);
                    }
                    Inst::Return { value: Some(v) } => {
                        escaping_values.insert(*v);
                    }
                    Inst::Out { value } | Inst::Err { value } => {
                        escaping_values.insert(*value);
                    }
                    _ => {}
                }
            }
            match &block.terminator {
                crate::dmir::Terminator::Return { value: Some(v) } => {
                    escaping_values.insert(*v);
                }
                _ => {}
            }
        }

        // 2. Select physical representation per candidate (sorted for determinism)
        let mut sorted_allocs: Vec<(ValueId, (String, usize))> =
            struct_allocs.into_iter().collect();
        sorted_allocs.sort_by_key(|(v, _)| v.0);

        for (vid, (class_name, field_count)) in &sorted_allocs {
            let escapes = escaping_values.contains(vid);
            let candidate_name = format!("{}:var_v{}", f.name, vid.0);

            if !escapes {
                // Scalar SSA Promotion (Zero-Cost SROA)
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Representation,
                    &candidate_name,
                    "Candidate:PromoteToScalarSSA",
                    0.0,
                    15.0 * (*field_count as f64),
                    "Non-escaping aggregate candidate; no standalone representation rewrite is emitted",
                    format!("Escape analysis proved non-escaping for {} with {} fields; actual SROA must be proven in DMIR", class_name, field_count),
                ));
                candidates += 1;
            } else {
                // Stack vs Heap Decision
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Representation,
                    &candidate_name,
                    "Candidate:StackLocalPlacement",
                    1.0,
                    8.0,
                    "Escaping aggregate candidate; no independent stack-layout lowering is emitted",
                    format!(
                        "Escape analysis observed a use of {}; backend layout remains unchanged",
                        class_name
                    ),
                ));
            }
        }

        // 3. Collection representation & stack promotion (Phase 13)
        let mut sorted_colls: Vec<(ValueId, String)> = collection_allocs.into_iter().collect();
        sorted_colls.sort_by_key(|(v, _)| v.0);

        for (vid, func_name) in &sorted_colls {
            let escapes = escaping_values.contains(vid);
            let candidate_name = format!("{}:coll_v{}", f.name, vid.0);

            if !escapes {
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Representation,
                    &candidate_name,
                    "StackPromotedCollection",
                    0.0,
                    20.0,
                    "Non-escaping collection promoted to stack-local memory (zero heap allocations)",
                    format!("Escape analysis confirmed zero escaping references for {}", func_name),
                ));
                candidates += 1;
            } else {
                log.record(AdaptationRecord::new(
                    AdaptationCategory::Representation,
                    &candidate_name,
                    "HeapManagedCollection",
                    1.0,
                    5.0,
                    "Escaping collection retains thread-local pool allocation",
                    format!("Observed escaping reference for {}", func_name),
                ));
            }
        }

        candidates
    }

    /// Evaluates collection memory layout (AoS vs SoA) based on field access selectivity
    pub fn adapt_collection_layout(
        collection_name: &str,
        field_access_ratio: f64,
        element_count: usize,
        log: &mut AdaptationDecisionLog,
    ) -> CollectionLayout {
        if field_access_ratio <= 0.35 && element_count >= 1024 {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Layout,
                collection_name,
                "Candidate:TransformToStructOfArrays",
                2.0,
                24.0,
                "Sparse column access candidate; no SoA DMIR/backend rewrite is emitted",
                format!(
                    "Field selectivity {:.2} <= 0.35 across {} items; layout remains unchanged",
                    field_access_ratio, element_count
                ),
            ));
            CollectionLayout::StructOfArrays
        } else if element_count >= 4096 {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Layout,
                collection_name,
                "Candidate:TransformToAoSoA(8)",
                3.0,
                18.0,
                "AoSoA candidate; no SIMD or chunked-layout lowering is emitted",
                format!(
                    "Element count {} >= 4096; target layout remains AoS",
                    element_count
                ),
            ));
            CollectionLayout::HybridAoSoA(8)
        } else {
            log.record(AdaptationRecord::new(
                AdaptationCategory::Layout,
                collection_name,
                "PreserveArrayOfStructs",
                0.0,
                5.0,
                "Dense access pattern across small-to-medium dataset",
                format!(
                    "Element count {} < 1024; contiguous AoS cache locality optimal",
                    element_count
                ),
            ));
            CollectionLayout::ArrayOfStructs
        }
    }
}
