use crate::dmir::{BasicBlockId, Inst, Module, Terminator, ValueId};
use cranelift_codegen::ir::{
    Block, BlockArg, Function as ClifFunction, InstBuilder, Signature, StackSlotData,
    StackSlotKind, Value as ClifValue, types as clif_types,
};
use cranelift_codegen::isa::TargetFrontendConfig;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Module as ClifModule};
use std::collections::HashMap;

use super::inst_binop::{compile_binop, compile_unop};
use super::inst_call::{compile_call, compile_method_call};
use super::types::{FunctionCompileCtx, ModuleDecls, RuntimeIds, clif_type};

pub fn compile_all_functions<M: ClifModule>(
    module: &mut M,
    dmir_module: &Module,
    frontend_config: TargetFrontendConfig,
    func_ids: &HashMap<String, (FuncId, Signature)>,
    runtime: &RuntimeIds,
    decls: &ModuleDecls,
) -> Result<(), String> {
    let sorted_func_names = &decls.sorted_func_names;
    let string_literal_map = &decls.string_literal_map;
    let class_field_offsets = &decls.class_field_offsets;
    let field_default_offsets = &decls.field_default_offsets;
    let string_fields = &decls.string_fields;
    let string_return_funcs = &decls.string_return_funcs;

    let rt_out_int_id = runtime.rt_out_int_id;
    let rt_out_bool_id = runtime.rt_out_bool_id;
    let rt_out_flt_id = runtime.rt_out_flt_id;
    let rt_out_str_id = runtime.rt_out_str_id;
    let rt_err_id = runtime.rt_err_id;
    let rt_concat_id = runtime.rt_concat_id;
    let rt_concat_3_id = runtime.rt_concat_3_id;
    let rt_concat_4_id = runtime.rt_concat_4_id;
    let rt_concat_5_id = runtime.rt_concat_5_id;
    let rt_int_to_str_id = runtime.rt_int_to_str_id;
    let rt_bool_to_str_id = runtime.rt_bool_to_str_id;
    let rt_flt_to_str_id = runtime.rt_flt_to_str_id;
    let malloc_id = runtime.malloc_id;

    // 3. Compile functions
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    for name in sorted_func_names {
        let f = dmir_module
            .functions
            .get(name)
            .ok_or_else(|| format!("Code generation failed: function '{}' not found", name))?;

        let (func_id, sig) = func_ids
            .get(name)
            .ok_or_else(|| format!("Code generation failed: function '{}' not declared", name))?;
        let mut clif_fn = ClifFunction::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
            sig.clone(),
        );

        let mut builder = FunctionBuilder::new(&mut clif_fn, &mut fn_builder_ctx);

        let mut val_map: HashMap<ValueId, ClifValue> = HashMap::new();
        let mut const_int_map: HashMap<ValueId, i64> = HashMap::new();
        let mut const_float_map: HashMap<ValueId, f64> = HashMap::new();
        // Values known to be Bool (0/1). Unlike strings, Bools share the
        // I64 representation, so this set is the only thing separating
        // `print(is_adult)` from `print(1)`.
        let mut bool_vids: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
        let mut string_vids: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
        let mut list_vids: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
        let mut map_vids: std::collections::HashSet<ValueId> = std::collections::HashSet::new();

        let mut block_map: HashMap<BasicBlockId, Block> = HashMap::new();
        for b in &f.blocks {
            let clif_block = builder.create_block();
            block_map.insert(b.id, clif_block);
        }

        // Real SSA: non-entry blocks receive their block parameters
        // (phis). Values are registered in `val_map` immediately so any
        // later instruction or terminator operand resolves to them. Bool
        // parameters join `bool_vids` so `print(flag)` keeps boolean
        // semantics through the promotion. The entry block is skipped:
        // its parameter list is the function signature.
        for b in &f.blocks {
            if b.params.is_empty() || b.id == f.entry_block {
                continue;
            }
            let clif_block = *block_map.get(&b.id).ok_or_else(|| {
                format!(
                    "Code generation failed: block {} not found in function '{}'",
                    b.id, f.name
                )
            })?;
            for p in &b.params {
                builder.append_block_param(clif_block, clif_type(&p.ty));
            }
            for (idx, p) in b.params.iter().enumerate() {
                let v = builder.block_params(clif_block)[idx];
                val_map.insert(p.val, v);
                if p.ty == "Bool" {
                    bool_vids.insert(p.val);
                }
                if p.ty == "String" || p.ty == "Str" {
                    string_vids.insert(p.val);
                }
                if p.ty.starts_with("List") || p.ty.starts_with('[') {
                    list_vids.insert(p.val);
                }
                if p.ty == "Map" || p.ty.starts_with("Map<") {
                    map_vids.insert(p.val);
                }
            }
        }

        let entry_clif_block = *block_map.get(&f.entry_block).ok_or_else(|| {
            format!(
                "Code generation failed: entry block {} not found in function '{}'",
                f.entry_block, f.name
            )
        })?;
        builder.append_block_params_for_function_params(entry_clif_block);
        builder.switch_to_block(entry_clif_block);

        let mut var_map: HashMap<String, Variable> = HashMap::new();
        let mut string_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut bool_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut list_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut map_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut var_to_class: HashMap<String, String> = HashMap::new();
        let mut val_to_class: HashMap<ValueId, String> = HashMap::new();

        for (idx, (p_name, p_type, p_val)) in f.params.iter().enumerate() {
            let p_clif_val = builder.block_params(entry_clif_block)[idx];
            val_map.insert(*p_val, p_clif_val);
            if p_type == "String" || p_type == "Str" {
                string_vars.insert(p_name.clone());
                string_vids.insert(*p_val);
            }
            if p_type == "Bool" {
                bool_vars.insert(p_name.clone());
                bool_vids.insert(*p_val);
            }
            if p_type.starts_with("List") || p_type.starts_with('[') {
                list_vars.insert(p_name.clone());
                list_vids.insert(*p_val);
            } else if p_type == "Map" || p_type.starts_with("Map<") {
                map_vars.insert(p_name.clone());
                map_vids.insert(*p_val);
            } else if !p_type.is_empty()
                && p_type != "Int"
                && p_type != "Float"
                && p_type != "Bool"
                && p_type != "String"
                && p_type != "Str"
                && p_type != "Unit"
            {
                var_to_class.insert(p_name.clone(), p_type.clone());
                val_to_class.insert(*p_val, p_type.clone());
            }

            let var = builder.declare_var(clif_type(p_type));
            builder.def_var(var, p_clif_val);
            var_map.insert(p_name.clone(), var);
        }

        for b in &f.blocks {
            let current_clif_block = *block_map.get(&b.id).ok_or_else(|| {
                format!(
                    "Code generation failed: block {} not found in function '{}'",
                    b.id, f.name
                )
            })?;
            builder.switch_to_block(current_clif_block);

            for inst in &b.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        let v = builder.ins().iconst(clif_types::I64, *value);
                        val_map.insert(*dest, v);
                        const_int_map.insert(*dest, *value);
                    }
                    Inst::ConstFloat { dest, value } => {
                        let v = builder.ins().f64const(*value);
                        val_map.insert(*dest, v);
                        const_float_map.insert(*dest, *value);
                    }
                    Inst::ConstBool { dest, value } => {
                        let v = builder
                            .ins()
                            .iconst(clif_types::I64, if *value { 1 } else { 0 });
                        val_map.insert(*dest, v);
                        bool_vids.insert(*dest);
                    }
                    Inst::ConstStr { dest, value } => {
                        let str_data_id = *string_literal_map.get(value).ok_or_else(|| {
                                format!(
                                    "Code generation failed: string literal {:?} not found in string table in function '{}'",
                                    value, f.name
                                )
                            })?;
                        let global_val = module.declare_data_in_func(str_data_id, builder.func);
                        let v = builder.ins().symbol_value(clif_types::I64, global_val);
                        val_map.insert(*dest, v);
                        string_vids.insert(*dest);
                    }
                    Inst::GetFuncAddr { dest, func_name } => {
                        if let Some(&(fid, _)) = func_ids.get(func_name) {
                            let fref = module.declare_func_in_func(fid, builder.func);
                            let addr = builder.ins().func_addr(clif_types::I64, fref);
                            val_map.insert(*dest, addr);
                        } else {
                            return Err(format!(
                                "Code generation failed: unresolved function address for '{}' in function '{}'",
                                func_name, f.name
                            ));
                        }
                    }
                    Inst::InlineAsm { .. } => {
                        return Err(
                                "Code generation failed: [E0902] inline assembly is not supported on Cranelift backend; use --llvm backend instead".to_string()
                            );
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(&var) = var_map.get(name) {
                            let v = builder.use_var(var);
                            val_map.insert(*dest, v);
                        } else {
                            let v = builder.ins().iconst(clif_types::I64, 0);
                            val_map.insert(*dest, v);
                        }
                        if string_vars.contains(name) {
                            string_vids.insert(*dest);
                        }
                        if bool_vars.contains(name) {
                            bool_vids.insert(*dest);
                        }
                        if list_vars.contains(name) {
                            list_vids.insert(*dest);
                        }
                        if map_vars.contains(name) {
                            map_vids.insert(*dest);
                        }
                        if let Some(c) = var_to_class.get(name) {
                            val_to_class.insert(*dest, c.clone());
                        }
                    }
                    Inst::AssignVar { name, value } => {
                        let v = val_map
                            .get(value)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                        if string_vids.contains(value) {
                            string_vars.insert(name.clone());
                        }
                        if bool_vids.contains(value) {
                            bool_vars.insert(name.clone());
                        }
                        if list_vids.contains(value) {
                            list_vars.insert(name.clone());
                        }
                        if map_vids.contains(value) {
                            map_vars.insert(name.clone());
                        }
                        if let Some(c) = val_to_class.get(value) {
                            var_to_class.insert(name.clone(), c.clone());
                        }
                        if let Some(&var) = var_map.get(name) {
                            builder.def_var(var, v);
                        } else {
                            let v_ty = builder.func.dfg.value_type(v);
                            let var = builder.declare_var(v_ty);
                            builder.def_var(var, v);
                            var_map.insert(name.clone(), var);
                        }
                    }

                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ty,
                    } => {
                        let mut ctx = FunctionCompileCtx {
                            builder: &mut builder,
                            module: &mut *module,
                            func_ids,
                            runtime,
                            val_map: &mut val_map,
                            const_int_map: &mut const_int_map,
                            const_float_map: &mut const_float_map,
                            string_vids: &mut string_vids,
                            bool_vids: &mut bool_vids,
                            list_vids: &mut list_vids,
                            map_vids: &mut map_vids,
                            val_to_class: &mut val_to_class,
                            var_to_class: &mut var_to_class,
                            var_map: &mut var_map,
                            string_vars: &mut string_vars,
                            bool_vars: &mut bool_vars,
                            list_vars: &mut list_vars,
                            map_vars: &mut map_vars,
                            class_field_offsets,
                            field_default_offsets,
                            string_fields,
                            string_literal_map,
                            string_return_funcs,
                            dmir_module,
                            current_func: f,
                        };
                        compile_binop(&mut ctx, dest, op, left, right, ty)?;
                    }
                    Inst::UnOp {
                        dest,
                        op,
                        operand,
                        ty,
                        ..
                    } => {
                        let mut ctx = FunctionCompileCtx {
                            builder: &mut builder,
                            module: &mut *module,
                            func_ids,
                            runtime,
                            val_map: &mut val_map,
                            const_int_map: &mut const_int_map,
                            const_float_map: &mut const_float_map,
                            string_vids: &mut string_vids,
                            bool_vids: &mut bool_vids,
                            list_vids: &mut list_vids,
                            map_vids: &mut map_vids,
                            val_to_class: &mut val_to_class,
                            var_to_class: &mut var_to_class,
                            var_map: &mut var_map,
                            string_vars: &mut string_vars,
                            bool_vars: &mut bool_vars,
                            list_vars: &mut list_vars,
                            map_vars: &mut map_vars,
                            class_field_offsets,
                            field_default_offsets,
                            string_fields,
                            string_literal_map,
                            string_return_funcs,
                            dmir_module,
                            current_func: f,
                        };
                        compile_unop(&mut ctx, dest, op, operand, ty)?;
                    }
                    Inst::Call {
                        dest,
                        func,
                        args,
                        ty,
                    } => {
                        let mut ctx = FunctionCompileCtx {
                            builder: &mut builder,
                            module: &mut *module,
                            func_ids,
                            runtime,
                            val_map: &mut val_map,
                            const_int_map: &mut const_int_map,
                            const_float_map: &mut const_float_map,
                            string_vids: &mut string_vids,
                            bool_vids: &mut bool_vids,
                            list_vids: &mut list_vids,
                            map_vids: &mut map_vids,
                            val_to_class: &mut val_to_class,
                            var_to_class: &mut var_to_class,
                            var_map: &mut var_map,
                            string_vars: &mut string_vars,
                            bool_vars: &mut bool_vars,
                            list_vars: &mut list_vars,
                            map_vars: &mut map_vars,
                            class_field_offsets,
                            field_default_offsets,
                            string_fields,
                            string_literal_map,
                            string_return_funcs,
                            dmir_module,
                            current_func: f,
                        };
                        compile_call(&mut ctx, dest, func, args, ty)?;
                    }
                    Inst::MethodCall {
                        dest,
                        object,
                        method,
                        args,
                        ty,
                    } => {
                        let mut ctx = FunctionCompileCtx {
                            builder: &mut builder,
                            module: &mut *module,
                            func_ids,
                            runtime,
                            val_map: &mut val_map,
                            const_int_map: &mut const_int_map,
                            const_float_map: &mut const_float_map,
                            string_vids: &mut string_vids,
                            bool_vids: &mut bool_vids,
                            list_vids: &mut list_vids,
                            map_vids: &mut map_vids,
                            val_to_class: &mut val_to_class,
                            var_to_class: &mut var_to_class,
                            var_map: &mut var_map,
                            string_vars: &mut string_vars,
                            bool_vars: &mut bool_vars,
                            list_vars: &mut list_vars,
                            map_vars: &mut map_vars,
                            class_field_offsets,
                            field_default_offsets,
                            string_fields,
                            string_literal_map,
                            string_return_funcs,
                            dmir_module,
                            current_func: f,
                        };
                        compile_method_call(&mut ctx, dest, object, method, args, ty)?;
                    }
                    Inst::StructInit {
                        dest,
                        class_name,
                        fields,
                    } => {
                        // Conservative escape analysis: if the struct's
                        // pointer can outlive this frame — returned,
                        // stored into another object or variable, passed
                        // to a call/method, or inserted into a map/list
                        // (a runtime call) — it must live on the heap.
                        let escapes = f.return_type == *class_name
                            || f.blocks.iter().any(|b| {
                                let returned = match &b.terminator {
                                    Terminator::Return {
                                        value: Some(ret_val),
                                    } => ret_val == dest,
                                    _ => false,
                                };
                                returned
                                    || b.instructions.iter().any(|inst| match inst {
                                        Inst::SetField { value, .. } => value == dest,
                                        Inst::AssignVar { value, .. } => value == dest,
                                        Inst::Call { args, .. } => args.contains(dest),
                                        Inst::MethodCall { object, args, .. } => {
                                            object == dest || args.contains(dest)
                                        }
                                        _ => false,
                                    })
                            });
                        let byte_size = fields.len().saturating_mul(8).max(16) as u32;
                        let slot_addr = if !escapes {
                            let slot_data =
                                StackSlotData::new(StackSlotKind::ExplicitSlot, byte_size, 3);
                            let slot = builder.create_sized_stack_slot(slot_data);
                            builder.ins().stack_addr(clif_types::I64, slot, 0)
                        } else {
                            let malloc_ref = module.declare_func_in_func(malloc_id, builder.func);
                            let size_val = builder.ins().iconst(clif_types::I64, byte_size as i64);
                            let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                            builder.inst_results(call_inst)[0]
                        };
                        let flags = cranelift_codegen::ir::MachMemFlags::new();
                        for (idx, (fname, fval)) in fields.iter().enumerate() {
                            if let Some(&v) = val_map.get(fval) {
                                // Store at the DECLARED layout offset for this
                                // class/field pair; the literal's field order
                                // can differ from the resolved class layout
                                // (composition merges fields), and GetField
                                // always reads by declared offset.
                                let base_c = class_name
                                    .split('<')
                                    .next()
                                    .unwrap_or(class_name)
                                    .split('_')
                                    .next()
                                    .unwrap_or(class_name);
                                let off = class_field_offsets
                                    .get(class_name)
                                    .or_else(|| class_field_offsets.get(base_c))
                                    .and_then(|m| m.get(fname).copied())
                                    .unwrap_or((idx * 8) as i32);
                                let v_ty = builder.func.dfg.value_type(v);
                                let val_to_store =
                                    if v_ty == clif_types::I8 || v_ty == clif_types::I32 {
                                        builder.ins().sextend(clif_types::I64, v)
                                    } else {
                                        v
                                    };
                                builder.ins().store(flags, val_to_store, slot_addr, off);
                            }
                        }
                        val_map.insert(*dest, slot_addr);
                        val_to_class.insert(*dest, class_name.clone());
                    }
                    Inst::GetField {
                        dest,
                        object,
                        field,
                        ty,
                    } => {
                        let obj_val = val_map.get(object).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: object %{} not found for GetField in function '{}'",
                                    object, f.name
                                )
                            })?;
                        let current_class_name = val_to_class
                            .get(object)
                            .map(|s| s.as_str())
                            .or_else(|| f.params.first().map(|p| p.1.as_str()))
                            .unwrap_or("");
                        let base_c = current_class_name
                            .split('<')
                            .next()
                            .unwrap_or(current_class_name)
                            .split('_')
                            .next()
                            .unwrap_or(current_class_name);
                        let offset = class_field_offsets
                            .get(current_class_name)
                            .or_else(|| class_field_offsets.get(base_c))
                            .and_then(|m| m.get(field).copied())
                            .or_else(|| field_default_offsets.get(field).copied())
                            .unwrap_or(0);
                        if std::env::var("DATARA_CODEGEN_TRACE").is_ok() {
                            eprintln!(
                                "[getfield] fn={} class={:?} field={} offset={} declared_ty={:?} inst_ty={}",
                                f.name,
                                current_class_name,
                                field,
                                offset,
                                dmir_module
                                    .class_field_types
                                    .get(&format!("{}.{}", current_class_name, field)),
                                ty
                            );
                        }
                        let field_key = format!("{}.{}", current_class_name, field);
                        let field_type_declared = dmir_module
                            .class_field_types
                            .get(&field_key)
                            .map(|s| s.as_str())
                            .unwrap_or(ty.as_str());
                        let is_float = field_type_declared == "Float"
                            || ty == "Float"
                            || field_type_declared == "Float64"
                            || field_type_declared == "Float32"
                            || ((ty == "T"
                                || field_type_declared == "T"
                                || ty == "Int"
                                || field_type_declared == "Int")
                                && (current_class_name.contains("<Float>")
                                    || current_class_name.contains("<f64>")
                                    || current_class_name.ends_with("_Float")
                                    || current_class_name.ends_with("_Float64")));
                        let flags = cranelift_codegen::ir::MachMemFlags::new();
                        if is_float {
                            let loaded =
                                builder.ins().load(clif_types::F64, flags, obj_val, offset);
                            val_map.insert(*dest, loaded);
                        } else {
                            let loaded =
                                builder.ins().load(clif_types::I64, flags, obj_val, offset);
                            val_map.insert(*dest, loaded);
                            if field_type_declared.contains("Str")
                                || ty.contains("Str")
                                || string_fields.contains(field)
                                || ((ty == "T" || field_type_declared == "T")
                                    && (current_class_name.contains("<Str>")
                                        || current_class_name.contains("<String>")
                                        || current_class_name.ends_with("_Str")
                                        || current_class_name.ends_with("_String")))
                            {
                                string_vids.insert(*dest);
                            }
                            if field_type_declared == "Bool"
                                || ty == "Bool"
                                || ((ty == "T" || field_type_declared == "T")
                                    && (current_class_name.contains("<Bool>")
                                        || current_class_name.ends_with("_Bool")))
                            {
                                bool_vids.insert(*dest);
                            }
                            if field_type_declared.contains("List")
                                || ty.contains("List")
                                || field_type_declared.starts_with('[')
                                || ty.starts_with('[')
                            {
                                list_vids.insert(*dest);
                            }
                            if field_type_declared.contains("Map") || ty.contains("Map") {
                                map_vids.insert(*dest);
                            }
                            let stripped_field_type = field_type_declared
                                .split('<')
                                .next()
                                .unwrap_or(field_type_declared);
                            if !stripped_field_type.is_empty()
                                && stripped_field_type != "Int"
                                && stripped_field_type != "Float"
                                && stripped_field_type != "Bool"
                                && stripped_field_type != "Str"
                                && stripped_field_type != "String"
                                && stripped_field_type != "List"
                                && stripped_field_type != "Map"
                                && stripped_field_type != "Unit"
                                && !stripped_field_type.starts_with('[')
                            {
                                val_to_class.insert(*dest, stripped_field_type.to_string());
                            }
                        }
                    }
                    Inst::SetField {
                        object,
                        field,
                        value,
                    } => {
                        let obj_val = val_map.get(object).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: object %{} not found for SetField in function '{}'",
                                    object, f.name
                                )
                            })?;
                        let val = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for SetField in function '{}'",
                                    value, f.name
                                )
                            })?;
                        let current_class_name = val_to_class
                            .get(object)
                            .map(|s| s.as_str())
                            .or_else(|| f.params.first().map(|p| p.1.as_str()))
                            .unwrap_or("");
                        let base_c = current_class_name
                            .split('<')
                            .next()
                            .unwrap_or(current_class_name)
                            .split('_')
                            .next()
                            .unwrap_or(current_class_name);
                        let offset = class_field_offsets
                            .get(current_class_name)
                            .or_else(|| class_field_offsets.get(base_c))
                            .and_then(|m| m.get(field).copied())
                            .or_else(|| field_default_offsets.get(field).copied())
                            .unwrap_or(0);
                        let flags = cranelift_codegen::ir::MachMemFlags::new();
                        let val_ty = builder.func.dfg.value_type(val);
                        let val_to_store = if val_ty == clif_types::I8 || val_ty == clif_types::I32
                        {
                            builder.ins().sextend(clif_types::I64, val)
                        } else {
                            val
                        };
                        builder.ins().store(flags, val_to_store, obj_val, offset);
                    }
                    Inst::Out { value } => {
                        let v = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for Out in function '{}'",
                                    value, f.name
                                )
                            })?;
                        let v_ty = builder.func.dfg.value_type(v);
                        if string_vids.contains(value) {
                            let fn_ref = module.declare_func_in_func(rt_out_str_id, builder.func);
                            builder.ins().call(fn_ref, &[v]);
                        } else if v_ty == clif_types::F64 {
                            let fn_ref = module.declare_func_in_func(rt_out_flt_id, builder.func);
                            builder.ins().call(fn_ref, &[v]);
                        } else if bool_vids.contains(value) {
                            let fn_ref = module.declare_func_in_func(rt_out_bool_id, builder.func);
                            builder.ins().call(fn_ref, &[v]);
                        } else {
                            let fn_ref = module.declare_func_in_func(rt_out_int_id, builder.func);
                            builder.ins().call(fn_ref, &[v]);
                        }
                    }
                    Inst::Err { value } => {
                        let v = val_map.get(value).copied().ok_or_else(|| {
                                format!(
                                    "Code generation failed: value %{} not found for Err in function '{}'",
                                    value, f.name
                                )
                            })?;
                        let fn_ref = module.declare_func_in_func(rt_err_id, builder.func);
                        builder.ins().call(fn_ref, &[v]);
                    }
                    Inst::FormatStr {
                        dest,
                        parts,
                        values,
                    } => {
                        let concat_ref = module.declare_func_in_func(rt_concat_id, builder.func);
                        let concat_3_ref =
                            module.declare_func_in_func(rt_concat_3_id, builder.func);
                        let concat_4_ref =
                            module.declare_func_in_func(rt_concat_4_id, builder.func);
                        let concat_5_ref =
                            module.declare_func_in_func(rt_concat_5_id, builder.func);
                        let int_to_str_ref =
                            module.declare_func_in_func(rt_int_to_str_id, builder.func);
                        let bool_to_str_ref =
                            module.declare_func_in_func(rt_bool_to_str_id, builder.func);
                        let flt_to_str_ref =
                            module.declare_func_in_func(rt_flt_to_str_id, builder.func);

                        let empty_id = *string_literal_map.get("").ok_or_else(|| {
                                format!(
                                    "Code generation failed: empty string literal not found in function '{}'",
                                    f.name
                                )
                            })?;
                        let mut pieces: Vec<ClifValue> = Vec::new();

                        for (idx, p) in parts.iter().enumerate() {
                            if !p.is_empty() || (idx == 0 && values.is_empty()) {
                                let pid = string_literal_map
                                    .get(p.as_str())
                                    .copied()
                                    .unwrap_or(empty_id);
                                let pgv = module.declare_data_in_func(pid, builder.func);
                                let p_val = builder.ins().symbol_value(clif_types::I64, pgv);
                                pieces.push(p_val);
                            }
                            if idx < values.len() {
                                let val_id = &values[idx];
                                let raw_val = val_map.get(val_id).copied().ok_or_else(|| {
                                        format!(
                                            "Code generation failed: interpolated value %{} not found in function '{}'",
                                            val_id, f.name
                                        )
                                    })?;
                                let val_is_str = string_vids.contains(val_id);
                                let v_ty = builder.func.dfg.value_type(raw_val);
                                let s_val = if val_is_str {
                                    raw_val
                                } else if bool_vids.contains(val_id) {
                                    let call = builder.ins().call(bool_to_str_ref, &[raw_val]);
                                    builder.inst_results(call)[0]
                                } else if v_ty == clif_types::F64 {
                                    let call = builder.ins().call(flt_to_str_ref, &[raw_val]);
                                    builder.inst_results(call)[0]
                                } else {
                                    let call = builder.ins().call(int_to_str_ref, &[raw_val]);
                                    builder.inst_results(call)[0]
                                };
                                pieces.push(s_val);
                            }
                        }

                        let res_str = match pieces.len() {
                            0 => {
                                let pgv = module.declare_data_in_func(empty_id, builder.func);
                                builder.ins().symbol_value(clif_types::I64, pgv)
                            }
                            1 => pieces[0],
                            2 => {
                                let call = builder.ins().call(concat_ref, &[pieces[0], pieces[1]]);
                                builder.inst_results(call)[0]
                            }
                            3 => {
                                let call = builder
                                    .ins()
                                    .call(concat_3_ref, &[pieces[0], pieces[1], pieces[2]]);
                                builder.inst_results(call)[0]
                            }
                            4 => {
                                let call = builder.ins().call(
                                    concat_4_ref,
                                    &[pieces[0], pieces[1], pieces[2], pieces[3]],
                                );
                                builder.inst_results(call)[0]
                            }
                            5 => {
                                let call = builder.ins().call(
                                    concat_5_ref,
                                    &[pieces[0], pieces[1], pieces[2], pieces[3], pieces[4]],
                                );
                                builder.inst_results(call)[0]
                            }
                            _ => {
                                let mut curr = pieces[0];
                                for piece in &pieces[1..] {
                                    let call = builder.ins().call(concat_ref, &[curr, *piece]);
                                    curr = builder.inst_results(call)[0];
                                }
                                curr
                            }
                        };

                        val_map.insert(*dest, res_str);
                        string_vids.insert(*dest);
                    }
                    Inst::Decide {
                        dest,
                        arms,
                        else_val,
                        ty,
                    } => {
                        let mut current_val = else_val
                            .and_then(|ev| val_map.get(&ev).copied())
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));

                        for (cond, val) in arms.iter().rev() {
                            let raw_cond = val_map
                                .get(cond)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                            let cond_ty = builder.func.dfg.value_type(raw_cond);
                            let cv = if cond_ty == clif_types::I8 {
                                raw_cond
                            } else {
                                builder.ins().icmp_imm_s(
                                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                    raw_cond,
                                    0,
                                )
                            };
                            let vv = val_map
                                .get(val)
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                            current_val = builder.ins().select(cv, vv, current_val);
                        }
                        val_map.insert(*dest, current_val);
                        if ty == "String"
                            || ty.contains("Str")
                            || arms.iter().any(|(_, v)| string_vids.contains(v))
                            || else_val
                                .map(|ev| string_vids.contains(&ev))
                                .unwrap_or(false)
                        {
                            string_vids.insert(*dest);
                        }
                        if ty == "Bool" {
                            bool_vids.insert(*dest);
                        }
                    }
                    Inst::Select {
                        dest,
                        cond,
                        then_val,
                        else_val,
                        ty,
                    } => {
                        let raw_cond = val_map
                            .get(cond)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                        let cond_ty = builder.func.dfg.value_type(raw_cond);
                        let cv = if cond_ty == clif_types::I8 {
                            raw_cond
                        } else {
                            builder.ins().icmp_imm_s(
                                cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                                raw_cond,
                                0,
                            )
                        };
                        let tv = val_map
                            .get(then_val)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                        let ev = val_map
                            .get(else_val)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                        let sel = builder.ins().select(cv, tv, ev);
                        val_map.insert(*dest, sel);
                        if string_vids.contains(then_val) || string_vids.contains(else_val) {
                            string_vids.insert(*dest);
                        }
                        if bool_vids.contains(then_val)
                            || bool_vids.contains(else_val)
                            || ty == "Bool"
                        {
                            bool_vids.insert(*dest);
                        }
                    }
                    Inst::WhileLoop { .. } | Inst::TryCatch { .. } | Inst::Return { .. } => {}
                }
            }
            // Handle terminator
            match &b.terminator {
                Terminator::Branch { target, args } => {
                    if let Some(&target_block) = block_map.get(target) {
                        let arg_vals: Vec<BlockArg> =
                            args.iter()
                                .map(|a| {
                                    let v = val_map.get(a).copied().unwrap_or_else(|| {
                                        builder.ins().iconst(clif_types::I64, 0)
                                    });
                                    BlockArg::Value(v)
                                })
                                .collect();
                        builder.ins().jump(target_block, &arg_vals);
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_block,
                    then_args,
                    else_block,
                    else_args,
                } => {
                    let cond_val = val_map
                        .get(cond)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(clif_types::I8, 0));
                    let cond_ty = builder.func.dfg.value_type(cond_val);
                    let cond_bool = if cond_ty == clif_types::I8 {
                        cond_val
                    } else {
                        builder.ins().icmp_imm_s(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            cond_val,
                            0,
                        )
                    };
                    if let (Some(&tb), Some(&eb)) =
                        (block_map.get(then_block), block_map.get(else_block))
                    {
                        let then_vals: Vec<BlockArg> = then_args
                            .iter()
                            .map(|a| {
                                let v = val_map
                                    .get(a)
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                BlockArg::Value(v)
                            })
                            .collect();
                        let else_vals: Vec<BlockArg> = else_args
                            .iter()
                            .map(|a| {
                                let v = val_map
                                    .get(a)
                                    .copied()
                                    .unwrap_or_else(|| builder.ins().iconst(clif_types::I64, 0));
                                BlockArg::Value(v)
                            })
                            .collect();
                        builder
                            .ins()
                            .brif(cond_bool, tb, &then_vals, eb, &else_vals);
                    }
                }
                Terminator::Return { value } => {
                    if f.return_type == "Unit" {
                        builder.ins().return_(&[]);
                    } else if f.return_type == "Float" {
                        if let Some(v_id) = value {
                            if let Some(&v) = val_map.get(v_id) {
                                let v_ty = builder.func.dfg.value_type(v);
                                let ret_v = if v_ty == clif_types::F64 {
                                    v
                                } else if v_ty == clif_types::I64 {
                                    builder.ins().fcvt_from_sint(clif_types::F64, v)
                                } else {
                                    v
                                };
                                builder.ins().return_(&[ret_v]);
                            } else {
                                let zero = builder.ins().f64const(0.0);
                                builder.ins().return_(&[zero]);
                            }
                        } else {
                            let zero = builder.ins().f64const(0.0);
                            builder.ins().return_(&[zero]);
                        }
                    } else {
                        if let Some(v_id) = value {
                            if let Some(&v) = val_map.get(v_id) {
                                let v_ty = builder.func.dfg.value_type(v);
                                let ret_v = if v_ty == clif_types::I64 {
                                    v
                                } else if v_ty == clif_types::F64 {
                                    builder.ins().fcvt_to_sint(clif_types::I64, v)
                                } else {
                                    v
                                };
                                builder.ins().return_(&[ret_v]);
                            } else {
                                let zero = builder.ins().iconst(clif_types::I64, 0);
                                builder.ins().return_(&[zero]);
                            }
                        } else {
                            let zero = builder.ins().iconst(clif_types::I64, 0);
                            builder.ins().return_(&[zero]);
                        }
                    }
                }
                Terminator::Unreachable => {
                    builder
                        .ins()
                        .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
                }
            }
        }

        builder.seal_all_blocks();
        builder.finalize(frontend_config);

        let mut ctx = cranelift_codegen::Context::for_function(clif_fn.clone());
        if std::env::var("FORGEN_DUMP_CLIF").is_ok() {
            eprintln!("=== CLIF IR FOR {} ===\n{}", name, clif_fn.display());
        }
        if let Err(e) = module.define_function(*func_id, &mut ctx) {
            return Err(format!(
                "Error in {}:\nCLIF:\n{}\nError:\n{}",
                name,
                clif_fn.display(),
                e
            ));
        }
    }

    Ok(())
}

pub fn define_main_entry<M: ClifModule>(
    module: &mut M,
    frontend_config: TargetFrontendConfig,
    func_ids: &HashMap<String, (FuncId, Signature)>,
    main_entry_info: Option<(FuncId, Signature)>,
) -> Result<(), String> {
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    if let Some((main_entry_id, ref main_entry_sig)) = main_entry_info
        && let Some(&(main_fn_id, _)) = func_ids.get("main")
    {
        let mut main_clif_fn = ClifFunction::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, main_entry_id.as_u32()),
            main_entry_sig.clone(),
        );
        let mut builder = FunctionBuilder::new(&mut main_clif_fn, &mut fn_builder_ctx);
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        let argc_val = builder.block_params(entry_block)[0];
        let argv_val = builder.block_params(entry_block)[1];

        if let Some(&(set_args_id, _)) = func_ids.get("datara_rt_set_args") {
            let set_args_ref = module.declare_func_in_func(set_args_id, builder.func);
            builder.ins().call(set_args_ref, &[argc_val, argv_val]);
        }

        let main_fn_ref = module.declare_func_in_func(main_fn_id, builder.func);
        let main_param_count = func_ids
            .get("main")
            .map(|(_, s)| s.params.len())
            .unwrap_or(0);
        let main_call = if main_param_count == 0 {
            builder.ins().call(main_fn_ref, &[])
        } else {
            // Never hand main a bogus pointer: pass NULL. Program args
            // are read through datara_rt_args_* (populated above via
            // datara_rt_set_args), mirroring the JIT entry.
            let null_arg = builder.ins().iconst(clif_types::I64, 0);
            builder.ins().call(main_fn_ref, &[null_arg])
        };
        // Propagate datara_main's result so `main` returning a nonzero
        // Int reaches the process exit code.
        let ret_val = match builder.inst_results(main_call).first().copied() {
            Some(v) if builder.func.dfg.value_type(v) == clif_types::I64 => {
                builder.ins().ireduce(clif_types::I32, v)
            }
            Some(v) if builder.func.dfg.value_type(v) == clif_types::I32 => v,
            _ => builder.ins().iconst(clif_types::I32, 0),
        };
        builder.ins().return_(&[ret_val]);

        builder.seal_all_blocks();
        builder.finalize(frontend_config);

        let mut ctx = cranelift_codegen::Context::for_function(main_clif_fn);
        module
            .define_function(main_entry_id, &mut ctx)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
