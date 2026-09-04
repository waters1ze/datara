use std::collections::{HashMap, HashSet};

use super::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};

/// Verifies the structural invariants required by the DMIR optimizer and
/// native backend. Since real SSA (mem2reg + block parameters) this checks:
/// - CFG integrity: unique block ids, existing entry and branch targets,
///   block-argument/parameter count agreement on every edge;
/// - single assignment: every `ValueId` is defined exactly once (the entry
///   block mirroring of function parameters counts as one logical definition);
/// - use-before-def: operands (including terminator operands and block
///   arguments) are used only after their definition in the same block, or in
///   a block dominated by the defining block.
pub fn verify_module(module: &Module) -> Result<(), String> {
    for (name, function) in &module.functions {
        verify_function(function).map_err(|error| format!("{}: {}", name, error))?;
    }
    Ok(())
}

pub fn verify_function(function: &Function) -> Result<(), String> {
    let block_ids: HashSet<BasicBlockId> = function.blocks.iter().map(|block| block.id).collect();
    if block_ids.len() != function.blocks.len() {
        return Err("duplicate basic-block id".to_string());
    }
    if !block_ids.contains(&function.entry_block) {
        return Err(format!(
            "entry block {} does not exist",
            function.entry_block
        ));
    }

    // Definition positions for use-before-def checking: block + index (None
    // index = end of block, i.e. a block parameter or terminator operand).
    let mut definitions: HashMap<ValueId, (BasicBlockId, Option<usize>)> = HashMap::new();

    for (_, _, value) in &function.params {
        definitions.insert(*value, (function.entry_block, None));
    }
    let function_params: HashSet<ValueId> =
        function.params.iter().map(|(_, _, value)| *value).collect();

    for block in &function.blocks {
        for param in &block.params {
            // The current lowering mirrors function parameters in the entry
            // block. That is one logical definition, not a duplicate SSA
            // definition. Other block parameters must remain unique.
            if block.id == function.entry_block && function_params.contains(&param.val) {
                continue;
            }
            define_value(
                &mut definitions,
                param.val,
                block.id,
                None,
                "block parameter",
            )?;
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(dest) = instruction_dest(instruction) {
                define_value(
                    &mut definitions,
                    dest,
                    block.id,
                    Some(index),
                    "instruction result",
                )?;
            }
        }
    }

    // Use-before-def with dominance. An operand may be used: (1) later in the
    // same block than its definition; (2) in any block dominated by the
    // defining block (block parameters / terminator operands count as uses at
    // the end of the block); (3) function parameters, defined at entry.
    let cfg = crate::dmir::cfg::ControlFlowGraph::build_dominance_only(function);

    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            for value in instruction_uses(instruction) {
                check_use(&definitions, &cfg, value, block.id, Some(index))?;
            }
        }
        for value in terminator_uses(&block.terminator) {
            check_use(&definitions, &cfg, value, block.id, None)?;
        }
        // Edge agreement: every outgoing branch must supply exactly one
        // argument per target block parameter.
        check_edge_arity(&block.terminator, function)?;
    }

    Ok(())
}

fn define_value(
    definitions: &mut HashMap<ValueId, (BasicBlockId, Option<usize>)>,
    value: ValueId,
    block: BasicBlockId,
    index: Option<usize>,
    kind: &str,
) -> Result<(), String> {
    if definitions.insert(value, (block, index)).is_some() {
        return Err(format!(
            "value {} defined more than once ({} redefinition)",
            value.0, kind
        ));
    }
    Ok(())
}

fn check_use(
    definitions: &HashMap<ValueId, (BasicBlockId, Option<usize>)>,
    cfg: &crate::dmir::cfg::ControlFlowGraph,
    value: ValueId,
    block: BasicBlockId,
    index: Option<usize>,
) -> Result<(), String> {
    let Some(&(def_block, def_index)) = definitions.get(&value) else {
        return Err(format!("use of undefined value {}", value.0));
    };
    if def_block == block {
        // Same block: the use must come after the definition. Function
        // parameters (def_index None) are usable anywhere in entry.
        let ok = match (def_index, index) {
            (None, _) => true,
            (Some(d), Some(u)) => u > d,
            // Terminator operands use end-of-block: anything defined
            // in the block counts as defined by then.
            (Some(_), None) => true,
        };
        if ok {
            return Ok(());
        }
    }
    if cfg.dominates(def_block, block) {
        return Ok(());
    }
    Err(format!(
        "value {} used in block {} before its definition dominates it",
        value.0, block.0
    ))
}

fn instruction_dest(instruction: &Inst) -> Option<ValueId> {
    match instruction {
        Inst::ConstInt { dest, .. }
        | Inst::ConstFloat { dest, .. }
        | Inst::ConstStr { dest, .. }
        | Inst::ConstBool { dest, .. }
        | Inst::LoadVar { dest, .. }
        | Inst::BinOp { dest, .. }
        | Inst::UnOp { dest, .. }
        | Inst::Call { dest, .. }
        | Inst::MethodCall { dest, .. }
        | Inst::StructInit { dest, .. }
        | Inst::GetField { dest, .. }
        | Inst::FormatStr { dest, .. }
        | Inst::GetFuncAddr { dest, .. }
        | Inst::Select { dest, .. }
        | Inst::Decide { dest, .. } => Some(*dest),
        _ => None,
    }
}

fn instruction_uses(instruction: &Inst) -> Vec<ValueId> {
    let mut uses = Vec::new();
    match instruction {
        Inst::ConstInt { .. }
        | Inst::ConstFloat { .. }
        | Inst::ConstStr { .. }
        | Inst::ConstBool { .. }
        | Inst::GetFuncAddr { .. }
        | Inst::LoadVar { .. } => {}
        Inst::AssignVar { value, .. } | Inst::Out { value } | Inst::Err { value } => {
            uses.push(*value)
        }
        Inst::Return { value: Some(value) } => uses.push(*value),
        Inst::Return { value: None } => {}
        Inst::BinOp { left, right, .. } => {
            uses.push(*left);
            uses.push(*right);
        }
        Inst::UnOp { operand, .. } => uses.push(*operand),
        Inst::Call { args, .. } => uses.extend(args.iter().copied()),
        Inst::MethodCall { object, args, .. } => {
            uses.push(*object);
            uses.extend(args.iter().copied());
        }
        Inst::StructInit { fields, .. } => uses.extend(fields.iter().map(|(_, v)| *v)),
        Inst::GetField { object, .. } => uses.push(*object),
        Inst::SetField { object, value, .. } => {
            uses.push(*object);
            uses.push(*value);
        }
        Inst::FormatStr { values, .. } => uses.extend(values.iter().copied()),
        Inst::Select {
            cond,
            then_val,
            else_val,
            ..
        } => {
            uses.push(*cond);
            uses.push(*then_val);
            uses.push(*else_val);
        }
        Inst::Decide { arms, else_val, .. } => {
            for (condition, value) in arms {
                uses.push(*condition);
                uses.push(*value);
            }
            if let Some(value) = else_val {
                uses.push(*value);
            }
        }
        Inst::WhileLoop {
            condition_insts,
            cond_val,
            body_insts,
        } => {
            // Legacy compound node: verify nested instruction lists so a
            // hand-written bad node is still rejected.
            for nested in condition_insts.iter().chain(body_insts) {
                uses.extend(instruction_uses(nested));
            }
            uses.push(*cond_val);
        }
        Inst::TryCatch {
            try_insts,
            catch_insts,
            ..
        } => {
            for nested in try_insts.iter().chain(catch_insts) {
                uses.extend(instruction_uses(nested));
            }
        }
    }
    uses
}

fn terminator_uses(terminator: &Terminator) -> Vec<ValueId> {
    match terminator {
        Terminator::Branch { args, .. } => args.clone(),
        Terminator::CondBranch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            let mut uses = vec![*cond];
            uses.extend(then_args.iter().copied());
            uses.extend(else_args.iter().copied());
            uses
        }
        Terminator::Return { value } => value.iter().copied().collect(),
        Terminator::Unreachable => Vec::new(),
    }
}

fn check_edge_arity(terminator: &Terminator, function: &Function) -> Result<(), String> {
    let param_count = |target: BasicBlockId| -> Result<usize, String> {
        function
            .get_block(target)
            .map(|b| b.params.len())
            .ok_or_else(|| format!("branch targets missing block {}", target.0))
    };
    match terminator {
        Terminator::Branch { target, args } => {
            let expected = param_count(*target)?;
            if args.len() != expected {
                return Err(format!(
                    "branch to block {} supplies {} arguments but target declares {} parameters",
                    target.0,
                    args.len(),
                    expected
                ));
            }
        }
        Terminator::CondBranch {
            then_block,
            then_args,
            else_block,
            else_args,
            ..
        } => {
            let then_expected = param_count(*then_block)?;
            let else_expected = param_count(*else_block)?;
            if then_args.len() != then_expected {
                return Err(format!(
                    "then-edge to block {} supplies {} arguments but target declares {} parameters",
                    then_block.0,
                    then_args.len(),
                    then_expected
                ));
            }
            if else_args.len() != else_expected {
                return Err(format!(
                    "else-edge to block {} supplies {} arguments but target declares {} parameters",
                    else_block.0,
                    else_args.len(),
                    else_expected
                ));
            }
        }
        Terminator::Return { .. } | Terminator::Unreachable => {}
    }
    Ok(())
}
