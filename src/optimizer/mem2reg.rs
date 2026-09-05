//! Mem2Reg: promotes named scalar variables (`LoadVar` / `AssignVar`) into
//! real SSA values with block parameters (phis).
//!
//! The lowering emits every local-variable read as `Inst::LoadVar` and every
//! write as `Inst::AssignVar`. That is the classic "mem2reg input" shape: the
//! information needed to turn the named memory traffic into SSA is entirely
//! local, and the result is what every dominance-sensitive optimization
//! (global CSE, scalar evolution, strength reduction) actually requires.
//!
//! Scope of promotion — deliberately narrow and conservative:
//! - Only variables whose every write has type `Int`, `Float` or `Bool`.
//!   Strings, structs and lists stay as named variables because the backend
//!   tracks them by name.
//! - The pass operates per function on the real CFG (`crate::dmir::cfg`).
//! - Any situation the algorithm does not fully understand (compound
//!   `WhileLoop`/`TryCatch` nodes, unreachable blocks, pre-existing block
//!   parameters, a load that no definition dominates, a phi argument that
//!   has no reaching definition) restores the original function from a
//!   clone and promotes nothing. A conservative no-op is always sound.

use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use std::collections::{HashMap, HashSet};

const PROMOTABLE_TYPES: [&str; 3] = ["Int", "Float", "Bool"];

/// Promote scalar variables in every function of the module.
///
/// Fresh `ValueId`s are allocated above the maximum id used anywhere in the
/// module: the lowering assigns ids from a single module-wide counter, so
/// per-function maxima are not a safe ceiling.
pub fn promote_module(module: &mut Module) -> usize {
    let mut max_id = 0usize;
    for function in module.functions.values() {
        scan_max_vids(function, &mut max_id);
    }
    let mut next = max_id + 1;

    let mut total = 0;
    for function in module.functions.values_mut() {
        total += promote_function(function, &mut next);
    }
    total
}

fn scan_max_vids(function: &Function, max: &mut usize) {
    let mut bump = |v: &ValueId| {
        if v.0 > *max {
            *max = v.0;
        }
    };
    for (_, _, v) in &function.params {
        bump(v);
    }
    for block in &function.blocks {
        for p in &block.params {
            bump(&p.val);
        }
        for inst in &block.instructions {
            visit_vids(inst, &mut bump);
        }
        match &block.terminator {
            Terminator::Branch { target: _, args } => args.iter().for_each(&mut bump),
            Terminator::CondBranch {
                cond,
                then_block: _,
                then_args,
                else_block: _,
                else_args,
            } => {
                bump(cond);
                then_args.iter().for_each(&mut bump);
                else_args.iter().for_each(&mut bump);
            }
            Terminator::Return { value } => {
                if let Some(v) = value {
                    bump(v);
                }
            }
            Terminator::Unreachable => {}
        }
    }
}

/// Visitor over every `ValueId` mentioned by an instruction (uses and defs).
fn visit_vids(inst: &Inst, f: &mut dyn FnMut(&ValueId)) {
    match inst {
        Inst::ConstInt { dest, .. }
        | Inst::ConstFloat { dest, .. }
        | Inst::ConstStr { dest, .. }
        | Inst::ConstBool { dest, .. }
        | Inst::GetFuncAddr { dest, .. }
        | Inst::LoadVar { dest, .. } => f(dest),
        Inst::BinOp {
            dest, left, right, ..
        } => {
            f(dest);
            f(left);
            f(right);
        }
        Inst::UnOp { dest, operand, .. } => {
            f(dest);
            f(operand);
        }
        Inst::Call { dest, args, .. } => {
            f(dest);
            args.iter().for_each(f);
        }
        Inst::MethodCall {
            dest, object, args, ..
        } => {
            f(dest);
            f(object);
            args.iter().for_each(f);
        }
        Inst::StructInit { dest, fields, .. } => {
            f(dest);
            fields.iter().for_each(|(_, v)| f(v));
        }
        Inst::GetField { dest, object, .. } => {
            f(dest);
            f(object);
        }
        Inst::SetField { object, value, .. } => {
            f(object);
            f(value);
        }
        Inst::FormatStr { dest, values, .. } => {
            f(dest);
            values.iter().for_each(f);
        }
        Inst::Decide {
            dest,
            arms,
            else_val,
            ..
        } => {
            f(dest);
            arms.iter().for_each(|(c, v)| {
                f(c);
                f(v);
            });
            if let Some(v) = else_val {
                f(v);
            }
        }
        Inst::Select {
            dest,
            cond,
            then_val,
            else_val,
            ..
        } => {
            f(dest);
            f(cond);
            f(then_val);
            f(else_val);
        }
        Inst::AssignVar { value, .. } => f(value),
        Inst::Out { value } | Inst::Err { value } => f(value),
        Inst::Return { value: Some(v) } => f(v),
        Inst::Return { value: None } => {}
        Inst::WhileLoop { .. } | Inst::TryCatch { .. } => {}
    }
}

/// Promote one function. Returns the number of promoted variables (0 when the
/// function was restored unchanged because some condition was not provable).
pub fn promote_function(function: &mut Function, next: &mut usize) -> usize {
    let mut original = Some(function.clone());

    match promote_function_inner(function, next) {
        Ok(count) => {
            if count > 0 {
                // Self-check before committing: a pass that cannot prove its
                // own output well-formed must not hand it to the rest of the
                // pipeline.
                if crate::dmir::verify_function(function).is_err() {
                    *function = original.take().unwrap();
                    return 0;
                }
            }
            count
        }
        Err(_) => {
            *function = original.take().unwrap();
            0
        }
    }
}

fn promote_function_inner(function: &mut Function, next: &mut usize) -> Result<usize, String> {
    // Legacy compound nodes duplicate instruction lists inside a single
    // instruction; renaming through them is not worth the complexity and the
    // lowering no longer produces them.
    let has_compound = function.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Inst::WhileLoop { .. } | Inst::TryCatch { .. }))
    });
    if has_compound {
        return Err("legacy compound nodes present".to_string());
    }
    // Pre-existing block parameters would make phi bookkeeping ambiguous.
    if function.blocks.iter().any(|b| !b.params.is_empty()) {
        return Err("function already has block parameters".to_string());
    }

    let cfg = ControlFlowGraph::build(function);

    // Blocks unreachable from the entry can never execute (nothing branches
    // to them). They are typically `if { return a; } else { return b; }`
    // merge placeholders with an `Unreachable` terminator. Deleting them
    // makes the dom-tree walk total instead of bailing out on the whole
    // function — a provably behavior-preserving cleanup.
    let reachable = reachable_blocks(function, &cfg);
    if !reachable.contains(&function.entry_block) {
        return Err("entry block unreachable".to_string());
    }
    function.blocks.retain(|b| reachable.contains(&b.id));

    // ---- 0. Function parameters are definitions at entry -------------------
    // A parameter dominates every reachable block, so reads of a scalar
    // parameter are promotable even when the parameter is never assigned.
    // Seed each scalar param as a definition living at the start of the
    // entry block; the renaming pass then behaves exactly as if the value
    // had been assigned before the first instruction.
    let entry_id = function.entry_block;
    let mut param_seeds: HashMap<String, (ValueId, String)> = HashMap::new();
    for (name, ty, val) in &function.params {
        if PROMOTABLE_TYPES.contains(&ty.as_str()) {
            param_seeds.insert(name.clone(), (*val, ty.clone()));
        }
    }

    // ---- 1. Type analysis --------------------------------------------------
    // def_type: known result type of a ValueId; var_type: known type of a
    // named variable; conflicts mark a name as unpromotable.
    let mut def_type: HashMap<ValueId, String> = HashMap::new();
    let mut var_type: HashMap<String, String> = HashMap::new();
    let mut bad_names: HashSet<String> = HashSet::new();
    for (name, (_, ty)) in &param_seeds {
        var_type.entry(name.clone()).or_insert_with(|| ty.clone());
    }

    let seed_def_types = |inst: &Inst,
                          def_type: &mut HashMap<ValueId, String>,
                          var_type: &HashMap<String, String>| {
        match inst {
            Inst::ConstInt { dest, .. } => {
                def_type.insert(*dest, "Int".to_string());
            }
            Inst::ConstFloat { dest, .. } => {
                def_type.insert(*dest, "Float".to_string());
            }
            Inst::ConstBool { dest, .. } => {
                def_type.insert(*dest, "Bool".to_string());
            }
            Inst::ConstStr { dest, .. } | Inst::FormatStr { dest, .. } => {
                def_type.insert(*dest, "Str".to_string());
            }
            Inst::BinOp { dest, ty, .. }
            | Inst::UnOp { dest, ty, .. }
            | Inst::Call { dest, ty, .. }
            | Inst::MethodCall { dest, ty, .. }
            | Inst::GetField { dest, ty, .. } => {
                def_type.insert(*dest, ty.clone());
            }
            Inst::StructInit {
                dest, class_name, ..
            } => {
                def_type.insert(*dest, class_name.clone());
            }
            Inst::LoadVar { dest, name } => {
                if let Some(t) = var_type.get(name) {
                    def_type.insert(*dest, t.clone());
                }
            }
            _ => {}
        }
    };

    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::AssignVar { name, value } => {
                        if let Some(t) = def_type.get(value) {
                            match var_type.get(name) {
                                None => {
                                    var_type.insert(name.clone(), t.clone());
                                    changed = true;
                                }
                                Some(prev) if prev != t && !bad_names.contains(name) => {
                                    bad_names.insert(name.clone());
                                    changed = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(t) = var_type.get(name)
                            && !def_type.contains_key(dest)
                        {
                            def_type.insert(*dest, t.clone());
                            changed = true;
                        }
                    }
                    other => seed_def_types(other, &mut def_type, &var_type),
                }
            }
        }
    }

    // ---- 2. Candidate selection -------------------------------------------
    let mut assigns: HashMap<String, Vec<(BasicBlockId, ValueId)>> = HashMap::new();
    let mut loads: Vec<(BasicBlockId, usize, ValueId, String)> = Vec::new();
    for block in &function.blocks {
        for (idx, inst) in block.instructions.iter().enumerate() {
            match inst {
                Inst::AssignVar { name, value } => {
                    assigns
                        .entry(name.clone())
                        .or_default()
                        .push((block.id, *value));
                }
                Inst::LoadVar { dest, name } => {
                    loads.push((block.id, idx, *dest, name.clone()));
                }
                _ => {}
            }
        }
    }

    // Seed the entry-block parameter definitions into the assigns map so the
    // dominance check and phi placement see them (a param def at entry
    // dominates every load).
    for (name, (val, _ty)) in &param_seeds {
        assigns
            .entry(name.clone())
            .or_default()
            .insert(0, (entry_id, *val));
    }

    let mut promotable: Vec<String> = Vec::new();
    // Iterate deterministically: a bare HashMap iteration makes the promotable
    // order — and therefore phi block-param order and fresh ValueIds — vary
    // run to run. Sort the candidate names before visiting them.
    let mut assign_names: Vec<&String> = assigns.keys().collect();
    assign_names.sort();
    for name in assign_names {
        let sites = &assigns[name];
        if bad_names.contains(name) {
            continue;
        }
        let has_body_access = loads.iter().any(|(_, _, _, lname)| lname == name)
            || function.blocks.iter().any(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i, Inst::AssignVar { name: n, .. } if n == name))
            });
        if !has_body_access {
            continue;
        }
        let Some(t) = var_type.get(name) else {
            continue;
        };
        if !PROMOTABLE_TYPES.contains(&t.as_str()) {
            continue;
        }
        // Every load must be dominated by some definition of the same name
        // (in a dominating block, or earlier in the same block). Otherwise a
        // load could execute before any assignment — an uninitialized read
        // that promotion would silently turn into a garbage SSA value.
        let def_blocks: HashSet<BasicBlockId> = sites.iter().map(|(b, _)| *b).collect();
        let dominated = loads.iter().all(|(lb, li, _, lname)| {
            lname != name
                || def_blocks.contains(lb)
                || sites
                    .iter()
                    .any(|(db, _)| *db != *lb && cfg.dominates(*db, *lb))
                || sites.iter().any(|(db, _)| {
                    *db == *lb
                        && function
                            .get_block(*lb)
                            .map(|blk| {
                                blk.instructions[..*li].iter().any(
                                    |i| matches!(i, Inst::AssignVar { name: n, .. } if n == name),
                                )
                            })
                            .unwrap_or(false)
                })
        });
        if dominated {
            promotable.push(name.clone());
        }
    }
    if promotable.is_empty() {
        return Err("no promotable candidates".to_string());
    }
    let promotable_set: HashSet<&str> = promotable.iter().map(|s| s.as_str()).collect();

    // ---- 3. Phi placement (iterated dominance frontier) --------------------
    // phi_names[block] = ordered list of names receiving a phi in that block.
    let mut phi_names: HashMap<BasicBlockId, Vec<String>> = HashMap::new();
    for name in &promotable {
        let mut def_blocks: HashSet<BasicBlockId> =
            assigns.get(name).unwrap().iter().map(|(b, _)| *b).collect();
        let mut worklist: Vec<BasicBlockId> = def_blocks.iter().copied().collect();
        while let Some(d) = worklist.pop() {
            let Some(frontier) = cfg.dominance_frontiers.get(&d) else {
                continue;
            };
            for y in frontier.iter().copied().collect::<Vec<_>>() {
                let entry = phi_names.entry(y).or_default();
                if !entry.iter().any(|n| n == name) {
                    entry.push(name.clone());
                    if def_blocks.insert(y) {
                        worklist.push(y);
                    }
                }
            }
        }
    }

    // ---- 4. Create phi block params ----------------------------------------
    let mut block_ids: Vec<BasicBlockId> = phi_names.keys().copied().collect();
    block_ids.sort_by_key(|b| b.0);
    for bid in block_ids {
        let names = phi_names[&bid].clone();
        let mut params = Vec::new();
        for name in names {
            *next += 1;
            let val = ValueId(*next - 1);
            let ty = var_type.get(&name).cloned().unwrap_or_else(|| "Int".into());
            params.push(crate::dmir::BlockParam {
                val,
                ty,
                name: Some(name.clone()),
            });
        }
        function.get_block_mut(bid).unwrap().params = params;
    }

    // ---- 5. Renaming (dom-tree DFS) ----------------------------------------
    let entry = function.entry_block;
    let mut subst: HashMap<ValueId, ValueId> = HashMap::new();
    let mut removed: HashSet<(BasicBlockId, usize)> = HashSet::new();
    let mut visited: HashSet<BasicBlockId> = HashSet::new();

    // Frame: (block, stacks snapshot taken at entry, next child index)
    // The initial frame's stacks start pre-seeded with the parameter values:
    // parameters are defined before the first instruction of the entry.
    let mut initial_stacks: HashMap<String, Vec<ValueId>> = HashMap::new();
    for (name, (val, _)) in &param_seeds {
        initial_stacks.insert(name.clone(), vec![*val]);
    }
    let mut stack: Vec<(BasicBlockId, HashMap<String, Vec<ValueId>>, usize)> =
        vec![(entry, initial_stacks, 0)];

    while let Some((bid, mut stacks, _)) = stack.pop() {
        if !visited.insert(bid) {
            continue;
        }
        let block = function
            .get_block(bid)
            .ok_or_else(|| format!("missing block {}", bid.0))?
            .clone();

        // Push phi parameters for this block.
        for p in &block.params {
            if let Some(name) = &p.name {
                stacks.entry(name.clone()).or_default().push(p.val);
            }
        }

        // Walk instructions in order, building the successor stack snapshot.
        for (idx, inst) in block.instructions.iter().enumerate() {
            match inst {
                Inst::LoadVar { dest, name } if promotable_set.contains(name.as_str()) => {
                    let v = stacks
                        .get(name)
                        .and_then(|s| s.last().copied())
                        .ok_or_else(|| format!("load of {} with no reaching def", name))?;
                    subst.insert(*dest, v);
                    removed.insert((bid, idx));
                }
                Inst::AssignVar { name, value } if promotable_set.contains(name.as_str()) => {
                    stacks.entry(name.clone()).or_default().push(*value);
                    removed.insert((bid, idx));
                }
                _ => {}
            }
        }

        // For every successor edge, forward the current value of each name
        // that has a phi in the successor.
        let mut terminator = block.terminator.clone();
        match &mut terminator {
            Terminator::Branch { target, args } => {
                let names = phi_names.get(target).cloned().unwrap_or_default();
                for name in names {
                    let v = stacks
                        .get(&name)
                        .and_then(|s| s.last().copied())
                        .ok_or_else(|| format!("phi argument of {} undefined", name))?;
                    args.push(v);
                }
            }
            Terminator::CondBranch {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                for (target, args) in [(*then_block, then_args), (*else_block, else_args)] {
                    let names = phi_names.get(&target).cloned().unwrap_or_default();
                    for name in names {
                        let v = stacks
                            .get(&name)
                            .and_then(|s| s.last().copied())
                            .ok_or_else(|| format!("phi argument of {} undefined", name))?;
                        args.push(v);
                    }
                }
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
        function.get_block_mut(bid).unwrap().terminator = terminator;

        // Descend into dominance-tree children with a snapshot of the stacks.
        if let Some(children) = cfg.dom_tree_children.get(&bid) {
            for &child in children {
                stack.push((child, stacks.clone(), 0));
            }
        }
    }

    if visited.len() != function.blocks.len() {
        return Err("dom-tree walk missed blocks".to_string());
    }

    // ---- 6. Rewrite: drop promoted memory traffic, resolve substitutions ---
    // Resolve substitution chains transitively (load of a load of ...).
    let resolve = |map: &HashMap<ValueId, ValueId>, mut v: ValueId| -> ValueId {
        let mut hops = 0;
        while let Some(&n) = map.get(&v) {
            if n == v || hops > 1024 {
                break;
            }
            v = n;
            hops += 1;
        }
        v
    };

    for block in function.blocks.iter_mut() {
        let mut kept: Vec<Inst> = Vec::with_capacity(block.instructions.len());
        for (idx, inst) in block.instructions.iter().enumerate() {
            if removed.contains(&(block.id, idx)) {
                continue;
            }
            let mut inst = inst.clone();
            substitute_inst(&mut inst, &subst, &resolve);
            kept.push(inst);
        }
        block.instructions = kept;
        let mut terminator = block.terminator.clone();
        substitute_terminator(&mut terminator, &subst, &resolve);
        block.terminator = terminator;
    }

    // ---- 7. Structural self-check ------------------------------------------
    for block in &function.blocks {
        match &block.terminator {
            Terminator::Return { .. } | Terminator::Unreachable => {}
            Terminator::Branch { target, args } => {
                let have = function
                    .get_block(*target)
                    .map(|b| b.params.len())
                    .ok_or("branch to missing block")?;
                if args.len() != have {
                    return Err("branch arg count mismatch".to_string());
                }
            }
            Terminator::CondBranch {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                let then_have = function
                    .get_block(*then_block)
                    .map(|b| b.params.len())
                    .ok_or("condbranch to missing block")?;
                let else_have = function
                    .get_block(*else_block)
                    .map(|b| b.params.len())
                    .ok_or("condbranch to missing block")?;
                if then_args.len() != then_have || else_args.len() != else_have {
                    return Err("condbranch arg count mismatch".to_string());
                }
            }
        }
    }
    // No promoted variable may still be read or written by name.
    for block in &function.blocks {
        for inst in &block.instructions {
            let offenders = match inst {
                Inst::LoadVar { name, .. } | Inst::AssignVar { name, .. } => {
                    promotable_set.contains(name.as_str())
                }
                _ => false,
            };
            if offenders {
                return Err("promoted variable still accessed by name".to_string());
            }
        }
    }

    Ok(promotable.len())
}

fn reachable_blocks(function: &Function, cfg: &ControlFlowGraph) -> HashSet<BasicBlockId> {
    let mut seen: HashSet<BasicBlockId> = HashSet::new();
    let mut queue = vec![function.entry_block];
    while let Some(b) = queue.pop() {
        if !seen.insert(b) {
            continue;
        }
        if let Some(succs) = cfg.successors.get(&b) {
            queue.extend(succs.iter().copied());
        }
    }
    seen
}

fn substitute_inst(
    inst: &mut Inst,
    subst: &HashMap<ValueId, ValueId>,
    resolve: &dyn Fn(&HashMap<ValueId, ValueId>, ValueId) -> ValueId,
) {
    let map = |v: &mut ValueId| {
        *v = resolve(subst, *v);
    };
    match inst {
        Inst::ConstInt { .. }
        | Inst::ConstFloat { .. }
        | Inst::ConstStr { .. }
        | Inst::ConstBool { .. }
        | Inst::GetFuncAddr { .. }
        | Inst::LoadVar { .. } => {}
        Inst::AssignVar { value, .. } => map(value),
        Inst::BinOp { left, right, .. } => {
            map(left);
            map(right);
        }
        Inst::UnOp { operand, .. } => map(operand),
        Inst::Call { args, .. } => args.iter_mut().for_each(map),
        Inst::MethodCall { object, args, .. } => {
            map(object);
            args.iter_mut().for_each(map);
        }
        Inst::StructInit { fields, .. } => fields.iter_mut().for_each(|(_, v)| map(v)),
        Inst::GetField { object, .. } => map(object),
        Inst::SetField { object, value, .. } => {
            map(object);
            map(value);
        }
        Inst::FormatStr { values, .. } => values.iter_mut().for_each(map),
        Inst::Decide { arms, else_val, .. } => {
            arms.iter_mut().for_each(|(c, v)| {
                map(c);
                map(v);
            });
            if let Some(v) = else_val {
                map(v);
            }
        }
        Inst::Select {
            cond,
            then_val,
            else_val,
            ..
        } => {
            map(cond);
            map(then_val);
            map(else_val);
        }
        Inst::Out { value } | Inst::Err { value } => map(value),
        Inst::Return { value } => {
            if let Some(v) = value {
                map(v);
            }
        }
        Inst::WhileLoop { .. } | Inst::TryCatch { .. } => {}
    }
}

fn substitute_terminator(
    terminator: &mut Terminator,
    subst: &HashMap<ValueId, ValueId>,
    resolve: &dyn Fn(&HashMap<ValueId, ValueId>, ValueId) -> ValueId,
) {
    let map = |v: &mut ValueId| {
        *v = resolve(subst, *v);
    };
    match terminator {
        Terminator::Branch { args, .. } => args.iter_mut().for_each(map),
        Terminator::CondBranch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            map(cond);
            then_args.iter_mut().for_each(map);
            else_args.iter_mut().for_each(map);
        }
        Terminator::Return { value } => {
            if let Some(v) = value {
                map(v);
            }
        }
        Terminator::Unreachable => {}
    }
}
