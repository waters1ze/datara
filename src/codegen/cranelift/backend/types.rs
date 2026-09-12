use crate::dmir::{Function, Module, ValueId};
use cranelift_codegen::ir::{Signature, Type as ClifType, Value as ClifValue, types as clif_types};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{DataId, FuncId, Module as ClifModule};
use std::collections::{HashMap, HashSet};

pub fn clif_type(ty_str: &str) -> ClifType {
    match ty_str {
        "Int" | "Int64" | "UInt64" | "i64" | "u64" | "isize" | "usize" | "USize" => clif_types::I64,
        "Int32" | "UInt32" | "i32" | "u32" => clif_types::I32,
        "Int16" | "UInt16" | "i16" | "u16" => clif_types::I16,
        "Int8" | "UInt8" | "i8" | "u8" | "Byte" => clif_types::I8,
        "i128" | "u128" => clif_types::I128,
        "Float" | "Float64" | "f64" => clif_types::F64,
        "Float32" | "f32" | "f16" => clif_types::F32,
        s if s.starts_with("Float<") => clif_types::F64,
        s if s.starts_with("Int<") || s.starts_with("UInt<") => clif_types::I64,
        "f32x4" | "Float4" | "Vector4" | "Vec4" => clif_types::F32X4,
        "i32x4" | "Int4" | "IVec4" => clif_types::I32X4,
        "f64x2" | "Vec2d" => clif_types::F64X2,
        "i64x2" => clif_types::I64X2,
        "dec64" | "dec128" => clif_types::I64,
        "Bool" => clif_types::I64,
        "String" | "Str" => clif_types::I64,
        "Unit" => clif_types::I64,
        _ => clif_types::I64,
    }
}

#[derive(Clone, Copy)]
pub struct RuntimeIds {
    pub rt_out_int_id: FuncId,
    pub rt_out_bool_id: FuncId,
    pub rt_out_flt_id: FuncId,
    pub rt_out_str_id: FuncId,
    pub rt_err_id: FuncId,
    pub rt_concat_id: FuncId,
    pub rt_concat_3_id: FuncId,
    pub rt_concat_4_id: FuncId,
    pub rt_concat_5_id: FuncId,
    pub rt_int_to_str_id: FuncId,
    pub rt_bool_to_str_id: FuncId,
    pub rt_flt_to_str_id: FuncId,
    pub malloc_id: FuncId,
    pub rt_list_get_id: FuncId,
    pub rt_list_create_id: FuncId,
    pub rt_list_len_id: FuncId,
    pub rt_list_set_id: FuncId,
    pub rt_list_append_id: FuncId,
    pub rt_map_get_id: FuncId,
    pub rt_map_insert_id: FuncId,
    pub rt_pop_id: FuncId,
    pub rt_str_char_at_id: FuncId,
    pub rt_str_eq_id: FuncId,
    pub str_byte_at_id: FuncId,
    pub str_chars_id: FuncId,
    pub str_len_id: FuncId,
}

#[derive(Clone, Copy)]
pub struct CoreRuntimeIds {
    pub rt_out_int_id: FuncId,
    pub rt_out_bool_id: FuncId,
    pub rt_out_flt_id: FuncId,
    pub rt_out_str_id: FuncId,
    pub rt_err_id: FuncId,
    pub rt_concat_id: FuncId,
    pub rt_concat_3_id: FuncId,
    pub rt_concat_4_id: FuncId,
    pub rt_concat_5_id: FuncId,
    pub rt_int_to_str_id: FuncId,
    pub rt_bool_to_str_id: FuncId,
    pub rt_flt_to_str_id: FuncId,
    pub malloc_id: FuncId,
    pub rt_list_get_id: FuncId,
    pub rt_list_create_id: FuncId,
    pub rt_list_len_id: FuncId,
    pub rt_list_set_id: FuncId,
    pub rt_list_append_id: FuncId,
    pub rt_map_get_id: FuncId,
    pub rt_map_insert_id: FuncId,
    pub rt_pop_id: FuncId,
    pub str_byte_at_id: FuncId,
    pub str_chars_id: FuncId,
    pub str_len_id: FuncId,
}

pub struct ModuleDecls {
    pub class_field_offsets: HashMap<String, HashMap<String, i32>>,
    pub field_default_offsets: HashMap<String, i32>,
    pub string_fields: HashSet<String>,
    pub string_literal_map: HashMap<String, DataId>,
    pub main_entry_info: Option<(FuncId, Signature)>,
    pub sorted_func_names: Vec<String>,
    pub string_return_funcs: HashSet<String>,
}

pub struct FunctionCompileCtx<'a, 'b, M: ClifModule> {
    pub builder: &'a mut FunctionBuilder<'b>,
    pub module: &'a mut M,
    pub func_ids: &'a HashMap<String, (FuncId, Signature)>,
    pub runtime: &'a RuntimeIds,
    pub val_map: &'a mut HashMap<ValueId, ClifValue>,
    pub const_int_map: &'a mut HashMap<ValueId, i64>,
    pub const_float_map: &'a mut HashMap<ValueId, f64>,
    pub string_vids: &'a mut HashSet<ValueId>,
    pub bool_vids: &'a mut HashSet<ValueId>,
    pub list_vids: &'a mut HashSet<ValueId>,
    pub map_vids: &'a mut HashSet<ValueId>,
    pub val_to_class: &'a mut HashMap<ValueId, String>,
    pub var_to_class: &'a mut HashMap<String, String>,
    pub var_map: &'a mut HashMap<String, Variable>,
    pub string_vars: &'a mut HashSet<String>,
    pub bool_vars: &'a mut HashSet<String>,
    pub list_vars: &'a mut HashSet<String>,
    pub map_vars: &'a mut HashSet<String>,
    pub class_field_offsets: &'a HashMap<String, HashMap<String, i32>>,
    pub field_default_offsets: &'a HashMap<String, i32>,
    pub string_fields: &'a HashSet<String>,
    pub string_literal_map: &'a HashMap<String, DataId>,
    pub string_return_funcs: &'a HashSet<String>,
    pub dmir_module: &'a Module,
    pub current_func: &'a Function,
}
