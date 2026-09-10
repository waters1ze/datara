use crate::ast::{ContractClause, Refinement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ValueId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BasicBlockId(pub usize);

impl std::fmt::Display for ValueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}", self.0)
    }
}

impl std::fmt::Display for BasicBlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terminator {
    Branch {
        target: BasicBlockId,
        args: Vec<ValueId>,
    },
    CondBranch {
        cond: ValueId,
        then_block: BasicBlockId,
        then_args: Vec<ValueId>,
        else_block: BasicBlockId,
        else_args: Vec<ValueId>,
    },
    Return {
        value: Option<ValueId>,
    },
    Unreachable,
}

impl Default for Terminator {
    fn default() -> Self {
        Terminator::Return { value: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockParam {
    pub val: ValueId,
    pub ty: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Inst {
    ConstInt {
        dest: ValueId,
        value: i64,
    },
    ConstFloat {
        dest: ValueId,
        value: f64,
    },
    ConstStr {
        dest: ValueId,
        value: String,
    },
    ConstBool {
        dest: ValueId,
        value: bool,
    },
    LoadVar {
        dest: ValueId,
        name: String,
    },
    AssignVar {
        name: String,
        value: ValueId,
    },
    BinOp {
        dest: ValueId,
        op: String,
        left: ValueId,
        right: ValueId,
        ty: String,
    },
    UnOp {
        dest: ValueId,
        op: String,
        operand: ValueId,
        ty: String,
    },
    Call {
        dest: ValueId,
        func: String,
        args: Vec<ValueId>,
        ty: String,
    },
    MethodCall {
        dest: ValueId,
        object: ValueId,
        method: String,
        args: Vec<ValueId>,
        ty: String,
    },
    StructInit {
        dest: ValueId,
        class_name: String,
        fields: Vec<(String, ValueId)>,
    },
    GetField {
        dest: ValueId,
        object: ValueId,
        field: String,
        ty: String,
    },
    SetField {
        object: ValueId,
        field: String,
        value: ValueId,
    },
    Out {
        value: ValueId,
    },
    Err {
        value: ValueId,
    },
    FormatStr {
        dest: ValueId,
        parts: Vec<String>,
        values: Vec<ValueId>,
    },
    Decide {
        dest: ValueId,
        arms: Vec<(ValueId, ValueId)>,
        else_val: Option<ValueId>,
        ty: String,
    },
    WhileLoop {
        condition_insts: Vec<Inst>,
        cond_val: ValueId,
        body_insts: Vec<Inst>,
    },
    TryCatch {
        try_insts: Vec<Inst>,
        err_var: String,
        catch_insts: Vec<Inst>,
    },
    Return {
        value: Option<ValueId>,
    },
    GetFuncAddr {
        dest: ValueId,
        func_name: String,
    },
    Select {
        dest: ValueId,
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
        ty: String,
    },
    InlineAsm {
        template: String,
        outputs: Vec<(String, ValueId)>,
        inputs: Vec<(String, ValueId)>,
        clobbers: Vec<String>,
        options: Vec<String>,
    },
}

impl std::hash::Hash for Inst {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Inst::ConstInt { dest, value } => {
                dest.hash(state);
                value.hash(state);
            }
            Inst::ConstFloat { dest, value } => {
                dest.hash(state);
                value.to_bits().hash(state);
            }
            Inst::ConstStr { dest, value } => {
                dest.hash(state);
                value.hash(state);
            }
            Inst::ConstBool { dest, value } => {
                dest.hash(state);
                value.hash(state);
            }
            Inst::LoadVar { dest, name } => {
                dest.hash(state);
                name.hash(state);
            }
            Inst::AssignVar { name, value } => {
                name.hash(state);
                value.hash(state);
            }
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } => {
                dest.hash(state);
                op.hash(state);
                left.hash(state);
                right.hash(state);
                ty.hash(state);
            }
            Inst::UnOp {
                dest,
                op,
                operand,
                ty,
            } => {
                dest.hash(state);
                op.hash(state);
                operand.hash(state);
                ty.hash(state);
            }
            Inst::Call {
                dest,
                func,
                args,
                ty,
            } => {
                dest.hash(state);
                func.hash(state);
                args.hash(state);
                ty.hash(state);
            }
            Inst::MethodCall {
                dest,
                object,
                method,
                args,
                ty,
            } => {
                dest.hash(state);
                object.hash(state);
                method.hash(state);
                args.hash(state);
                ty.hash(state);
            }
            Inst::StructInit {
                dest,
                class_name,
                fields,
            } => {
                dest.hash(state);
                class_name.hash(state);
                fields.hash(state);
            }
            Inst::GetField {
                dest,
                object,
                field,
                ty,
            } => {
                dest.hash(state);
                object.hash(state);
                field.hash(state);
                ty.hash(state);
            }
            Inst::SetField {
                object,
                field,
                value,
            } => {
                object.hash(state);
                field.hash(state);
                value.hash(state);
            }
            Inst::Out { value } => {
                value.hash(state);
            }
            Inst::Err { value } => {
                value.hash(state);
            }
            Inst::FormatStr {
                dest,
                parts,
                values,
            } => {
                dest.hash(state);
                parts.hash(state);
                values.hash(state);
            }
            Inst::Decide {
                dest,
                arms,
                else_val,
                ty,
            } => {
                dest.hash(state);
                arms.hash(state);
                else_val.hash(state);
                ty.hash(state);
            }
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                condition_insts.hash(state);
                cond_val.hash(state);
                body_insts.hash(state);
            }
            Inst::TryCatch {
                try_insts,
                err_var,
                catch_insts,
            } => {
                try_insts.hash(state);
                err_var.hash(state);
                catch_insts.hash(state);
            }
            Inst::Return { value } => {
                value.hash(state);
            }
            Inst::GetFuncAddr { dest, func_name } => {
                dest.hash(state);
                func_name.hash(state);
            }
            Inst::Select {
                dest,
                cond,
                then_val,
                else_val,
                ty,
            } => {
                dest.hash(state);
                cond.hash(state);
                then_val.hash(state);
                else_val.hash(state);
                ty.hash(state);
            }
            Inst::InlineAsm {
                template,
                outputs,
                inputs,
                clobbers,
                options,
            } => {
                template.hash(state);
                outputs.hash(state);
                inputs.hash(state);
                clobbers.hash(state);
                options.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    pub label: String,
    pub params: Vec<BlockParam>,
    pub instructions: Vec<Inst>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, String, ValueId)>,
    #[serde(default)]
    pub param_refinements: Vec<(String, Option<Refinement>)>,
    #[serde(default)]
    pub requires: Vec<ContractClause>,
    pub return_type: String,
    pub entry_block: BasicBlockId,
    pub blocks: Vec<BasicBlock>,
}

impl Default for Function {
    fn default() -> Self {
        Self {
            name: String::new(),
            params: Vec::new(),
            param_refinements: Vec::new(),
            requires: Vec::new(),
            return_type: String::new(),
            entry_block: BasicBlockId(0),
            blocks: Vec::new(),
        }
    }
}

impl Function {
    pub fn get_block(&self, id: BasicBlockId) -> Option<&BasicBlock> {
        if id.0 < self.blocks.len() && self.blocks[id.0].id == id {
            return Some(&self.blocks[id.0]);
        }
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn get_block_mut(&mut self, id: BasicBlockId) -> Option<&mut BasicBlock> {
        if id.0 < self.blocks.len() && self.blocks[id.0].id == id {
            return Some(&mut self.blocks[id.0]);
        }
        self.blocks.iter_mut().find(|b| b.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub functions: HashMap<String, Function>,
    pub extern_functions: HashMap<String, (Vec<String>, String)>,
    pub class_fields: HashMap<String, Vec<String>>,
    pub class_field_types: HashMap<String, String>,
    #[serde(default)]
    pub function_spans: HashMap<String, crate::diagnostics::SourceSpan>,
    #[serde(default)]
    pub function_line_spans: HashMap<String, Vec<crate::diagnostics::SourceSpan>>,
    #[serde(default)]
    pub link_libraries: Vec<String>,
}

impl Module {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            functions: HashMap::new(),
            extern_functions: HashMap::new(),
            class_fields: HashMap::new(),
            class_field_types: HashMap::new(),
            function_spans: HashMap::new(),
            function_line_spans: HashMap::new(),
            link_libraries: Vec::new(),
        }
    }
}
