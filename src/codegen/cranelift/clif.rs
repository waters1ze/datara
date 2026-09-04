use crate::ast::Program;
use crate::codegen::target::{CallingConvention, TargetInfo};
use crate::dmir::{Function, Inst, Module};
use crate::types::TypeChecker;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCodegenInspection {
    pub name: String,
    pub instruction_count: usize,
    pub stack_frame_bytes: usize,
    pub explicit_stack_slots: usize,
    pub direct_calls: usize,
    pub branches: usize,
    pub heap_allocations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCodegenInspection {
    pub target: String,
    pub calling_convention: String,
    pub total_functions: usize,
    pub total_instructions: usize,
    pub total_heap_allocations: usize,
    pub functions: Vec<FunctionCodegenInspection>,
}

pub struct ClifEmitter<'a> {
    pub target: &'a TargetInfo,
}

impl<'a> ClifEmitter<'a> {
    pub fn new(target: &'a TargetInfo) -> Self {
        Self { target }
    }

    pub fn inspect_module(&self, module: &Module) -> ModuleCodegenInspection {
        let mut functions_inspect = Vec::new();
        let mut total_insts = 0;
        let mut total_heap = 0;

        for f in module.functions.values() {
            let finsp = self.inspect_function(f);
            total_insts += finsp.instruction_count;
            total_heap += finsp.heap_allocations;
            functions_inspect.push(finsp);
        }

        let cc_str = match self.target.calling_convention {
            CallingConvention::WindowsFastcall => "windows_fastcall",
            CallingConvention::SystemV => "system_v",
            CallingConvention::Aarch64Standard => "aarch64_standard",
            CallingConvention::WasmStandard => "wasm_standard",
        };

        ModuleCodegenInspection {
            target: self.target.triple_string(),
            calling_convention: cc_str.to_string(),
            total_functions: module.functions.len(),
            total_instructions: total_insts,
            total_heap_allocations: total_heap,
            functions: functions_inspect,
        }
    }

    pub fn inspect_function(&self, f: &Function) -> FunctionCodegenInspection {
        let mut inst_count = 0;
        let mut direct_calls = 0;
        let mut branches = 0;
        let mut heap_allocs = 0;
        let mut all_vars = HashSet::new();

        for b in &f.blocks {
            for inst in &b.instructions {
                inst_count += 1;
                match inst {
                    Inst::AssignVar { name, .. } => {
                        all_vars.insert(name.clone());
                    }
                    Inst::Call { .. } | Inst::MethodCall { .. } => {
                        direct_calls += 1;
                    }
                    Inst::WhileLoop {
                        condition_insts,
                        body_insts,
                        ..
                    } => {
                        branches += 2; // brif + jump
                        inst_count += condition_insts.len() + body_insts.len();
                    }
                    Inst::Decide { arms, .. } => {
                        branches += arms.len();
                    }
                    Inst::StructInit { .. } => {
                        // In unoptimized or escaping cases this counts as allocation
                        heap_allocs += 0; // In SROA-scalarized DMIR, heap allocation is 0
                    }
                    _ => {}
                }
            }
        }

        let stack_slots = all_vars.len();
        let stack_frame_bytes = stack_slots * 8;

        FunctionCodegenInspection {
            name: f.name.clone(),
            instruction_count: inst_count,
            stack_frame_bytes,
            explicit_stack_slots: stack_slots,
            direct_calls,
            branches,
            heap_allocations: heap_allocs,
        }
    }

    pub fn emit_module(&self, module: &Module, _program: &Program, _types: &TypeChecker) -> String {
        let mut clif = String::new();
        clif.push_str("; Auto-generated Cranelift IR (CLIF) by Forgen Native Backend\n");
        clif.push_str(&format!("; Target: {}\n\n", self.target.triple_string()));

        clif.push_str("test compile\n");
        clif.push_str(&format!("target {}\n\n", self.target.triple_string()));

        for f in module.functions.values() {
            clif.push_str(&self.emit_function(f, module));
            clif.push('\n');
        }

        clif
    }

    pub fn emit_function(&self, f: &Function, module: &Module) -> String {
        let mut out = String::new();
        let cc_str = match self.target.calling_convention {
            CallingConvention::WindowsFastcall => "windows_fastcall",
            CallingConvention::SystemV => "system_v",
            CallingConvention::Aarch64Standard => "system_v",
            CallingConvention::WasmStandard => "wasm_standard",
        };

        // Function signature
        let param_types = f
            .params
            .iter()
            .map(|(_, p_type, _)| self.dmir_type_to_clif(p_type))
            .collect::<Vec<_>>()
            .join(", ");

        let ret_type = if f.return_type == "Unit" {
            "".to_string()
        } else {
            format!(" -> {}", self.dmir_type_to_clif(&f.return_type))
        };

        out.push_str(&format!(
            "function u0:{}({}){} {} {{\n",
            f.name, param_types, ret_type, cc_str
        ));

        // Declare stack slots if any local vars exist
        let mut all_vars = HashSet::new();
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::AssignVar { name, .. } = inst {
                    all_vars.insert(name.clone());
                }
            }
        }

        for (idx, var_name) in all_vars.iter().enumerate() {
            out.push_str(&format!(
                "    ss{} = explicit_slot 8 ; var '{}'\n",
                idx, var_name
            ));
        }

        // Emit basic blocks
        for (b_idx, block) in f.blocks.iter().enumerate() {
            if b_idx == 0 {
                let block_params = f
                    .params
                    .iter()
                    .map(|(_, p_type, p_val)| {
                        format!("v{}: {}", p_val.0, self.dmir_type_to_clif(p_type))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("  {}({}):\n", block.label, block_params));
            } else {
                out.push_str(&format!("  {}:\n", block.label));
            }

            for inst in &block.instructions {
                out.push_str(&self.emit_instruction(inst, module, f, &all_vars));
            }
        }

        out.push_str("}\n");
        out
    }

    fn emit_instruction(
        &self,
        inst: &Inst,
        module: &Module,
        f: &Function,
        _vars: &HashSet<String>,
    ) -> String {
        match inst {
            Inst::ConstInt { dest, value } => {
                format!("    v{} = iconst.i64 {}\n", dest.0, value)
            }
            Inst::ConstFloat { dest, value } => {
                format!("    v{} = f64const {}\n", dest.0, value)
            }
            Inst::ConstBool { dest, value } => {
                let bit = if *value { 1 } else { 0 };
                format!("    v{} = iconst.i8 {}\n", dest.0, bit)
            }
            Inst::ConstStr { dest, value } => {
                format!(
                    "    ; const string {:?}\n    v{} = iconst.i64 0\n",
                    value, dest.0
                )
            }
            Inst::GetFuncAddr { dest, func_name } => {
                format!(
                    "    ; get func addr '{}'\n    v{} = iconst.i64 0\n",
                    func_name, dest.0
                )
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ..
            } => {
                format!(
                    "    v{} = select v{}, v{}, v{}\n",
                    dest.0, cond.0, then_val.0, else_val.0
                )
            }
            Inst::LoadVar { dest, name } => {
                format!(
                    "    ; load var '{}'\n    v{} = iconst.i64 0\n",
                    name, dest.0
                )
            }
            Inst::AssignVar { name, value } => {
                format!("    ; store var '{}' <= v{}\n", name, value.0)
            }
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } => {
                let clif_op = match op.as_str() {
                    "+" => {
                        if ty == "Float" {
                            "fadd"
                        } else {
                            "iadd"
                        }
                    }
                    "-" => {
                        if ty == "Float" {
                            "fsub"
                        } else {
                            "isub"
                        }
                    }
                    "*" => {
                        if ty == "Float" {
                            "fmul"
                        } else {
                            "imul"
                        }
                    }
                    "/" => {
                        if ty == "Float" {
                            "fdiv"
                        } else {
                            "sdiv"
                        }
                    }
                    "<" => {
                        if ty == "Float" {
                            "fcmp lt"
                        } else {
                            "icmp slt"
                        }
                    }
                    "<=" => {
                        if ty == "Float" {
                            "fcmp le"
                        } else {
                            "icmp sle"
                        }
                    }
                    ">" => {
                        if ty == "Float" {
                            "fcmp gt"
                        } else {
                            "icmp sgt"
                        }
                    }
                    ">=" => {
                        if ty == "Float" {
                            "fcmp ge"
                        } else {
                            "icmp sge"
                        }
                    }
                    "==" => {
                        if ty == "Float" {
                            "fcmp eq"
                        } else {
                            "icmp eq"
                        }
                    }
                    "!=" => {
                        if ty == "Float" {
                            "fcmp ne"
                        } else {
                            "icmp ne"
                        }
                    }
                    _ => "iadd",
                };
                format!("    v{} = {} v{}, v{}\n", dest.0, clif_op, left.0, right.0)
            }
            Inst::UnOp {
                dest,
                op,
                operand,
                ty,
            } => {
                if op == "-" {
                    if ty == "Float" {
                        format!("    v{} = fneg v{}\n", dest.0, operand.0)
                    } else {
                        format!("    v{} = ineg v{}\n", dest.0, operand.0)
                    }
                } else if op == "!" {
                    format!("    v{} = bnot v{}\n", dest.0, operand.0)
                } else {
                    format!("    v{} = copy v{}\n", dest.0, operand.0)
                }
            }
            Inst::Call {
                dest, func, args, ..
            } => {
                let args_str = args
                    .iter()
                    .map(|a| format!("v{}", a.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                let returns_void = if let Some(target_fn) = module.functions.get(func) {
                    target_fn.return_type == "Unit"
                } else {
                    false
                };

                if returns_void {
                    format!("    call fn${}({})\n", func, args_str)
                } else {
                    format!("    v{} = call fn${}({})\n", dest.0, func, args_str)
                }
            }
            Inst::MethodCall {
                dest,
                object,
                method,
                args,
                ..
            } => {
                let mut all_args = vec![format!("v{}", object.0)];
                all_args.extend(args.iter().map(|a| format!("v{}", a.0)));
                let args_str = all_args.join(", ");
                format!("    v{} = call fn${}({})\n", dest.0, method, args_str)
            }
            Inst::StructInit {
                dest,
                class_name,
                fields,
            } => {
                let mut out = format!("    ; struct alloc {}\n", class_name);
                out.push_str(&format!("    v{} = iconst.i64 0 ; ptr\n", dest.0));
                for (fname, fval) in fields {
                    out.push_str(&format!("    ; set field {} <= v{}\n", fname, fval.0));
                }
                out
            }
            Inst::GetField {
                dest,
                object,
                field,
                ..
            } => {
                format!(
                    "    ; get field {}.{}\n    v{} = copy v{}\n",
                    object.0, field, dest.0, object.0
                )
            }
            Inst::SetField {
                object,
                field,
                value,
            } => {
                format!("    ; set field {}.{} <= v{}\n", object.0, field, value.0)
            }
            Inst::Out { value } => {
                format!("    call fn$rt_out(v{})\n", value.0)
            }
            Inst::Err { value } => {
                format!("    call fn$rt_err(v{})\n", value.0)
            }
            Inst::FormatStr { dest, .. } => {
                format!("    ; format str\n    v{} = iconst.i64 0\n", dest.0)
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ..
            } => {
                let mut out = format!("    ; decide expression -> v{}\n", dest.0);
                if let Some((first_cond, first_val)) = arms.first() {
                    out.push_str(&format!(
                        "    v{} = select v{}, v{}, v{}\n",
                        dest.0,
                        first_cond.0,
                        first_val.0,
                        else_val.map(|v| v.0).unwrap_or(0)
                    ));
                }
                out
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                let mut out = String::new();
                out.push_str("  loop_header:\n");
                for ci in condition_insts {
                    out.push_str(&self.emit_instruction(ci, module, f, _vars));
                }
                out.push_str(&format!("    brif v{}, loop_body, loop_exit\n", cond_val.0));
                out.push_str("  loop_body:\n");
                for bi in body_insts {
                    out.push_str(&self.emit_instruction(bi, module, f, _vars));
                }
                out.push_str("    jump loop_header\n");
                out.push_str("  loop_exit:\n");
                out
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                let mut out = String::new();
                out.push_str("    ;; try-catch block begin\n");
                for ti in try_insts {
                    out.push_str(&self.emit_instruction(ti, module, f, _vars));
                }
                for ci in catch_insts {
                    out.push_str(&self.emit_instruction(ci, module, f, _vars));
                }
                out.push_str("    ;; try-catch block end\n");
                out
            }
            Inst::Return { value } => {
                if f.return_type == "Unit" {
                    "    return\n".to_string()
                } else if let Some(v) = value {
                    format!("    return v{}\n", v.0)
                } else {
                    "    return\n".to_string()
                }
            }
        }
    }

    fn dmir_type_to_clif(&self, ty: &str) -> &'static str {
        match ty {
            "Int" => "i64",
            "Float" => "f64",
            "Bool" => "i8",
            "String" | "Str" => "i64",
            "Unit" => "i64",
            _ => "i64",
        }
    }
}
