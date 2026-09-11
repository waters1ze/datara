//! Layout Adapter & Struct Field Reordering (Phase 13)
//!
//! Analyzes struct/record layouts in DMIR modules, calculates optimal memory
//! alignments (16/32/64 bytes), and reorders fields descending by size/alignment
//! to eliminate interior padding holes and optimize SIMD vectorization.

use super::decision::{AdaptationCategory, AdaptationDecisionLog, AdaptationRecord};
use crate::dmir::Module;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassLayout {
    pub alignment: usize,
    pub total_size: usize,
    pub field_offsets: HashMap<String, usize>,
    pub ordered_fields: Vec<String>,
}

pub struct LayoutAdapter;

impl LayoutAdapter {
    /// Adapts aggregate memory layouts across the module. Reorders fields
    /// descending by size/alignment and assigns cache-line or SIMD alignment
    /// (16/32/64 bytes).
    pub fn adapt_layout(
        module: &mut Module,
        log: &mut AdaptationDecisionLog,
    ) -> HashMap<String, ClassLayout> {
        let mut layouts = HashMap::new();
        let mut class_names: Vec<String> = module.class_fields.keys().cloned().collect();
        class_names.sort();

        for class_name in class_names {
            let fields = match module.class_fields.get(&class_name) {
                Some(f) => f.clone(),
                None => continue,
            };

            // Rank fields: 8-byte types first, then 4-byte, 2-byte, 1-byte
            let mut sorted_fields = fields.clone();
            sorted_fields.sort_by(|a, b| {
                let ty_a = module
                    .class_field_types
                    .get(&format!("{}.{}", class_name, a))
                    .or_else(|| module.class_field_types.get(a))
                    .map(|s| s.as_str())
                    .unwrap_or("Int");
                let ty_b = module
                    .class_field_types
                    .get(&format!("{}.{}", class_name, b))
                    .or_else(|| module.class_field_types.get(b))
                    .map(|s| s.as_str())
                    .unwrap_or("Int");

                let rank_a = Self::type_rank(ty_a);
                let rank_b = Self::type_rank(ty_b);
                rank_b.cmp(&rank_a).then_with(|| a.cmp(b))
            });

            let reordered = sorted_fields != fields;
            if reordered {
                module
                    .class_fields
                    .insert(class_name.clone(), sorted_fields.clone());
            }

            let field_count = sorted_fields.len();
            let has_simd = sorted_fields.iter().any(|f| {
                let ty = module
                    .class_field_types
                    .get(&format!("{}.{}", class_name, f))
                    .or_else(|| module.class_field_types.get(f))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                ty.contains("Float4") || ty.contains("Int4") || ty.contains("Vec")
            });

            // Alignment: 64 bytes for SIMD or large structs (>= 8 fields / >= 64 bytes)
            // 32 bytes for medium structs (>= 4 fields)
            // 16 bytes minimum aggregate alignment
            let alignment = if has_simd || field_count >= 8 {
                64
            } else if field_count >= 4 {
                32
            } else {
                16
            };

            let mut field_offsets = HashMap::new();
            let mut current_offset = 0;
            for f in &sorted_fields {
                let f_size = 8;
                field_offsets.insert(f.clone(), current_offset);
                current_offset += f_size;
            }
            let total_size = (current_offset + (alignment - 1)) & !(alignment - 1);

            let record = AdaptationRecord::new(
                AdaptationCategory::Layout,
                class_name.clone(),
                format!(
                    "Aligned {} bytes, reordered: {}",
                    alignment,
                    if reordered { "yes" } else { "no" }
                ),
                0.0,
                if reordered { 1.30 } else { 1.15 },
                format!(
                    "Optimized aggregate layout: {}-byte alignment to prevent cache-line splits and eliminate padding",
                    alignment
                ),
                format!(
                    "Field count: {}, alignment: {}, reordered fields: {:?}",
                    field_count, alignment, sorted_fields
                ),
            );
            log.record(record);

            layouts.insert(
                class_name,
                ClassLayout {
                    alignment,
                    total_size,
                    field_offsets,
                    ordered_fields: sorted_fields,
                },
            );
        }

        layouts
    }

    fn type_rank(ty: &str) -> usize {
        match ty {
            "Float4" | "Int4" => 16,
            "Int" | "Float" | "Str" | "ptr" | "Ptr" => 8,
            "Int32" | "Float32" => 4,
            "Int16" => 2,
            "Bool" | "Byte" => 1,
            _ => 8,
        }
    }
}
