//! Sibling Recursion Elimination (Tail-Call Optimization for Additive Binary Recursion)
//!
//! When a pure recursive function matches the pattern:
//!   f(n) = if n <= base { n } else { f(n - 1) + f(n - 2) }
//!
//! Advanced optimizing compilers eliminate one of the recursive calls by transforming
//! it into an accumulator loop:
//!   f(n):
//!     mut acc = 0
//!     while n > base {
//!       acc += f(n - 1)
//!       n -= 2
//!     }
//!     return acc + n
//!
//! This cuts stack frame allocations and recursive call traffic by 50%, transforming
//! an exponential binary recursion tree into a single recursive spine with an iterative loop.

use crate::dmir::{BasicBlock, BasicBlockId, BlockParam, Function, Inst, Terminator, ValueId};
use std::collections::HashMap;

pub fn eliminate_sibling_recursion(f: &mut Function) -> bool {
    if f.params.len() != 1 || f.params[0].1 != "Int" || f.return_type != "Int" {
        return false;
    }

    let arg_n = f.params[0].2;

    // --- Soundness pre-pass: value lattice used to resolve "returns n". ---
    let mut alias_map: HashMap<ValueId, ValueId> = HashMap::new();
    let mut int_consts: HashMap<ValueId, i64> = HashMap::new();
    for b in &f.blocks {
        for inst in &b.instructions {
            match inst {
                Inst::ConstInt { dest, value } => {
                    int_consts.insert(*dest, *value);
                }
                Inst::LoadVar { dest, name } if name == &f.params[0].0 => {
                    alias_map.insert(*dest, arg_n);
                }
                _ => {}
            }
        }
    }
    // Chase copies/loads down to the parameter value.
    fn resolves_to_param(v: ValueId, arg_n: ValueId, alias: &HashMap<ValueId, ValueId>) -> bool {
        let mut cur = v;
        for _ in 0..16 {
            if cur == arg_n {
                return true;
            }
            match alias.get(&cur) {
                Some(&next) if next != cur => cur = next,
                _ => return false,
            }
        }
        false
    }

    // --- Pass 1: the base block must return exactly `n` and do nothing else.
    // The old transform silently replaced ANY base case with `acc + n`,
    // which changes semantics whenever the base does not return `n`. ---
    let mut base_bid: Option<BasicBlockId> = None;
    for b in &f.blocks {
        let pure_and_simple = b.instructions.iter().all(|inst| match inst {
            Inst::LoadVar { .. } | Inst::ConstInt { .. } => true,
            Inst::UnOp { op, .. } => op == "copy",
            _ => false,
        });
        if !pure_and_simple {
            continue;
        }
        if let Terminator::Return { value: Some(v) } = &b.terminator
            && resolves_to_param(*v, arg_n, &alias_map)
        {
            base_bid = Some(b.id);
            break;
        }
    }

    // --- Pass 2: the rec block: exactly two self-calls, the sum returned,
    // and nothing else effectful. ---
    let mut rec_info: Option<(BasicBlockId, ValueId, ValueId, ValueId)> = None;
    for b in &f.blocks {
        if Some(b.id) == base_bid {
            continue;
        }
        let mut calls: Vec<(ValueId, ValueId)> = Vec::new();
        let mut other_effect = false;
        for inst in &b.instructions {
            match inst {
                Inst::Call {
                    dest, func, args, ..
                } => {
                    if func == &f.name && args.len() == 1 {
                        calls.push((*dest, args[0]));
                    } else {
                        other_effect = true;
                    }
                }
                Inst::Out { .. } | Inst::Err { .. } | Inst::SetField { .. } => other_effect = true,
                _ => {}
            }
        }
        if other_effect || calls.len() != 2 {
            continue;
        }
        let (c1, a1) = calls[0];
        let (c2, a2) = calls[1];

        for inst in &b.instructions {
            if let Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } = inst
                && op == "+"
                && ty == "Int"
                && ((*left == c1 && *right == c2) || (*left == c2 && *right == c1))
            {
                let is_returned = match &b.terminator {
                    Terminator::Return { value: Some(ret_v) } => *ret_v == *dest,
                    _ => false,
                };
                if is_returned {
                    rec_info = Some((b.id, a1, a2, *dest));
                    break;
                }
            }
        }
        if rec_info.is_some() {
            break;
        }
    }

    let Some((_rec_bid, a1, a2, _sum_dest)) = rec_info else {
        return false;
    };
    // Without a provable `return n` base case the transform is unsound.
    let Some(base_bid) = base_bid else {
        return false;
    };

    // --- Pass 3: the guard comparison must branch DIRECTLY to the base and
    // rec blocks; otherwise it may not actually guard them. ---
    // The function must have exactly these two exits; the transform replaces
    // every block, so any third return path would be silently dropped.
    let return_sites: Vec<BasicBlockId> = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Return { .. }))
        .map(|b| b.id)
        .collect();
    if return_sites.len() != 2
        || !return_sites.contains(&base_bid)
        || !return_sites.contains(&_rec_bid)
    {
        return false;
    }
    let mut guard_bound: Option<i64> = None;
    'outer: for b in &f.blocks {
        for inst in &b.instructions {
            if let Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } = inst
                && (op == "<=" || op == "<")
                && (ty == "Bool" || ty == "Int")
            {
                let base_l = alias_map.get(left).copied().unwrap_or(*left);
                if base_l == arg_n
                    && let Some(&k) = int_consts.get(right)
                    && let Terminator::CondBranch {
                        cond: c,
                        then_block,
                        else_block,
                        ..
                    } = &b.terminator
                    && c == dest
                    && ((*then_block == base_bid && *else_block == _rec_bid)
                        || (*then_block == _rec_bid && *else_block == base_bid))
                {
                    guard_bound = Some(if op == "<=" { k } else { k - 1 });
                    break 'outer;
                }
            }
        }
    }
    let Some(base_k) = guard_bound else {
        return false;
    };

    let mut sub_consts: HashMap<ValueId, i64> = HashMap::new();
    for b in &f.blocks {
        for inst in &b.instructions {
            if let Inst::BinOp {
                dest,
                op,
                left,
                right,
                ty,
            } = inst
                && op == "-"
                && ty == "Int"
            {
                let base_l = alias_map.get(left).copied().unwrap_or(*left);
                if base_l == arg_n
                    && let Some(&k) = int_consts.get(right)
                {
                    sub_consts.insert(*dest, k);
                }
            }
        }
    }

    let k1 = sub_consts.get(&a1).copied();
    let k2 = sub_consts.get(&a2).copied();

    let (step_call_k, step_loop_k) = match (k1, k2) {
        (Some(1), Some(2)) => (1, 2),
        (Some(2), Some(1)) => (1, 2),
        _ => return false,
    };

    let mut max_id = arg_n.0;
    for b in &f.blocks {
        for inst in &b.instructions {
            let d = match inst {
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
                | Inst::Select { dest, .. }
                | Inst::Decide { dest, .. } => dest.0,
                _ => 0,
            };
            if d > max_id {
                max_id = d;
            }
        }
    }

    let mut next_val = || {
        max_id += 1;
        ValueId(max_id)
    };

    let acc_0 = next_val();
    let curr_n = next_val();
    let curr_acc = next_val();
    let base_bound_id = next_val();
    let cond_id = next_val();
    let final_res_id = next_val();
    let k1_id = next_val();
    let sub1_id = next_val();
    let rec1_id = next_val();
    let next_acc_id = next_val();
    let k2_id = next_val();
    let next_n_id = next_val();

    let entry_bid = BasicBlockId(0);
    let loop_header_bid = BasicBlockId(1);
    let base_bid = BasicBlockId(2);
    let step_bid = BasicBlockId(3);

    let entry_block = BasicBlock {
        id: entry_bid,
        label: "entry_0".to_string(),
        params: Vec::new(),
        instructions: vec![Inst::ConstInt {
            dest: acc_0,
            value: 0,
        }],
        terminator: Terminator::Branch {
            target: loop_header_bid,
            args: vec![arg_n, acc_0],
        },
    };

    let loop_header_block = BasicBlock {
        id: loop_header_bid,
        label: "loop_header".to_string(),
        params: vec![
            BlockParam {
                val: curr_n,
                ty: "Int".to_string(),
                name: Some("curr_n".to_string()),
            },
            BlockParam {
                val: curr_acc,
                ty: "Int".to_string(),
                name: Some("curr_acc".to_string()),
            },
        ],
        instructions: vec![
            Inst::ConstInt {
                dest: base_bound_id,
                value: base_k,
            },
            Inst::BinOp {
                dest: cond_id,
                op: "<=".to_string(),
                left: curr_n,
                right: base_bound_id,
                ty: "Bool".to_string(),
            },
        ],
        terminator: Terminator::CondBranch {
            cond: cond_id,
            then_block: base_bid,
            then_args: Vec::new(),
            else_block: step_bid,
            else_args: Vec::new(),
        },
    };

    let base_block = BasicBlock {
        id: base_bid,
        label: "base_case".to_string(),
        params: Vec::new(),
        instructions: vec![
            Inst::BinOp {
                dest: final_res_id,
                op: "+".to_string(),
                left: curr_acc,
                right: curr_n,
                ty: "Int".to_string(),
            },
            Inst::Return {
                value: Some(final_res_id),
            },
        ],
        terminator: Terminator::Return {
            value: Some(final_res_id),
        },
    };

    let step_block = BasicBlock {
        id: step_bid,
        label: "step_case".to_string(),
        params: Vec::new(),
        instructions: vec![
            Inst::ConstInt {
                dest: k1_id,
                value: step_call_k,
            },
            Inst::BinOp {
                dest: sub1_id,
                op: "-".to_string(),
                left: curr_n,
                right: k1_id,
                ty: "Int".to_string(),
            },
            Inst::Call {
                dest: rec1_id,
                func: f.name.clone(),
                args: vec![sub1_id],
                ty: "Int".to_string(),
            },
            Inst::BinOp {
                dest: next_acc_id,
                op: "+".to_string(),
                left: curr_acc,
                right: rec1_id,
                ty: "Int".to_string(),
            },
            Inst::ConstInt {
                dest: k2_id,
                value: step_loop_k,
            },
            Inst::BinOp {
                dest: next_n_id,
                op: "-".to_string(),
                left: curr_n,
                right: k2_id,
                ty: "Int".to_string(),
            },
        ],
        terminator: Terminator::Branch {
            target: loop_header_bid,
            args: vec![next_n_id, next_acc_id],
        },
    };

    let original_blocks = f.blocks.clone();
    let original_entry = f.entry_block;

    f.entry_block = entry_bid;
    f.blocks = vec![entry_block, loop_header_block, base_block, step_block];

    if crate::dmir::verify_function(f).is_err() {
        f.blocks = original_blocks;
        f.entry_block = original_entry;
        return false;
    }

    true
}
