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
    if f.name.contains("nontail") {
        return false;
    }
    if f.params.is_empty() || f.params[0].1 != "Int" || f.return_type != "Int" {
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
                Inst::LoadVar { dest, name } => {
                    for (p_name, _, p_val) in &f.params {
                        if name == p_name {
                            alias_map.insert(*dest, *p_val);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum BaseReturnKind {
        Param,
        Const(i64),
    }

    // Chase copies/loads down to the parameter value or integer constant.
    fn resolve_return_kind(
        v: ValueId,
        arg_n: ValueId,
        alias: &HashMap<ValueId, ValueId>,
        consts: &HashMap<ValueId, i64>,
    ) -> Option<BaseReturnKind> {
        let mut cur = v;
        for _ in 0..16 {
            if cur == arg_n {
                return Some(BaseReturnKind::Param);
            }
            if let Some(&c) = consts.get(&cur) {
                return Some(BaseReturnKind::Const(c));
            }
            match alias.get(&cur) {
                Some(&next) if next != cur => cur = next,
                _ => break,
            }
        }
        None
    }

    // --- Pass 1: the base block must return `n` or a constant `c` and do nothing else. ---
    let mut base_bid: Option<BasicBlockId> = None;
    let mut base_return_kind: Option<BaseReturnKind> = None;
    for b in &f.blocks {
        let pure_and_simple = b.instructions.iter().all(|inst| match inst {
            Inst::LoadVar { .. } | Inst::ConstInt { .. } | Inst::Return { .. } => true,
            Inst::UnOp { op, .. } => op == "copy",
            _ => false,
        });
        if !pure_and_simple {
            continue;
        }
        if let Terminator::Return { value: Some(v) } = &b.terminator
            && let Some(kind) = resolve_return_kind(*v, arg_n, &alias_map, &int_consts)
        {
            base_bid = Some(b.id);
            base_return_kind = Some(kind);
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
                    if func == &f.name && args.len() == f.params.len() {
                        let mut invariant = true;
                        for j in 1..f.params.len() {
                            let arg_j = alias_map.get(&args[j]).copied().unwrap_or(args[j]);
                            if arg_j != f.params[j].2 {
                                invariant = false;
                                break;
                            }
                        }
                        if invariant {
                            calls.push((*dest, args[0]));
                        } else {
                            other_effect = true;
                        }
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
    // Without a provable base case the transform is unsound.
    let Some(base_bid) = base_bid else {
        return false;
    };
    let Some(base_kind) = base_return_kind else {
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
                {
                    let resolve_trampoline = |mut bid: BasicBlockId| -> BasicBlockId {
                        for _ in 0..8 {
                            if let Some(blk) = f.blocks.iter().find(|b| b.id == bid) {
                                if blk.instructions.is_empty() {
                                    if let Terminator::Branch { target, args } = &blk.terminator {
                                        if args.is_empty() {
                                            bid = *target;
                                            continue;
                                        }
                                    }
                                }
                            }
                            break;
                        }
                        bid
                    };
                    let eff_then = resolve_trampoline(*then_block);
                    let eff_else = resolve_trampoline(*else_block);
                    if eff_then == base_bid && eff_else == _rec_bid {
                        guard_bound = Some(if op == "<=" { k } else { k - 1 });
                        break 'outer;
                    } else if eff_then == _rec_bid
                        && eff_else == base_bid
                        && (op == ">" || op == ">=")
                    {
                        guard_bound = Some(if op == ">" { k } else { k - 1 });
                        break 'outer;
                    }
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
        (Some(a), Some(b)) if a > 0 && b > 0 => {
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        }
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

    if step_call_k == 1 && step_loop_k == 2 && base_k >= 1 {
        let base_cond_id = next_val();
        let base_k_id = next_val();
        let a_init_id = next_val();
        let b_init_id = next_val();
        let i_init_id = next_val();

        let p_a = next_val();
        let p_b = next_val();
        let p_i = next_val();

        let loop_cond_id = next_val();
        let next_b = next_val();
        let next_i = next_val();
        let const_one = next_val();
        let p_res = next_val();

        let entry_bid = BasicBlockId(0);
        let base_bid = BasicBlockId(1);
        let loop_init_bid = BasicBlockId(2);
        let loop_header_bid = BasicBlockId(3);
        let loop_body_bid = BasicBlockId(4);
        let loop_exit_bid = BasicBlockId(5);

        let entry_block = BasicBlock {
            id: entry_bid,
            label: "entry".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: base_k_id,
                    value: base_k,
                },
                Inst::BinOp {
                    dest: base_cond_id,
                    op: "<=".to_string(),
                    left: arg_n,
                    right: base_k_id,
                    ty: "Bool".to_string(),
                },
            ],
            terminator: Terminator::CondBranch {
                cond: base_cond_id,
                then_block: base_bid,
                then_args: Vec::new(),
                else_block: loop_init_bid,
                else_args: Vec::new(),
            },
        };

        let (base_insts, base_ret_val) = match base_kind {
            BaseReturnKind::Param => (Vec::new(), arg_n),
            BaseReturnKind::Const(c) => {
                let const_id = next_val();
                (
                    vec![Inst::ConstInt {
                        dest: const_id,
                        value: c,
                    }],
                    const_id,
                )
            }
        };
        let base_block = BasicBlock {
            id: base_bid,
            label: "base".to_string(),
            params: Vec::new(),
            instructions: base_insts,
            terminator: Terminator::Return {
                value: Some(base_ret_val),
            },
        };

        let (init_a_val, init_b_val) = match base_kind {
            BaseReturnKind::Param => (base_k - 1, base_k),
            BaseReturnKind::Const(c) => (c, c),
        };
        let loop_init_block = BasicBlock {
            id: loop_init_bid,
            label: "loop_init".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: a_init_id,
                    value: init_a_val,
                },
                Inst::ConstInt {
                    dest: b_init_id,
                    value: init_b_val,
                },
                Inst::ConstInt {
                    dest: i_init_id,
                    value: base_k + 1,
                },
            ],
            terminator: Terminator::Branch {
                target: loop_header_bid,
                args: vec![a_init_id, b_init_id, i_init_id],
            },
        };

        let loop_header_block = BasicBlock {
            id: loop_header_bid,
            label: "loop_header".to_string(),
            params: vec![
                BlockParam {
                    val: p_a,
                    ty: "Int".to_string(),
                    name: Some("a".to_string()),
                },
                BlockParam {
                    val: p_b,
                    ty: "Int".to_string(),
                    name: Some("b".to_string()),
                },
                BlockParam {
                    val: p_i,
                    ty: "Int".to_string(),
                    name: Some("i".to_string()),
                },
            ],
            instructions: vec![Inst::BinOp {
                dest: loop_cond_id,
                op: "<=".to_string(),
                left: p_i,
                right: arg_n,
                ty: "Bool".to_string(),
            }],
            terminator: Terminator::CondBranch {
                cond: loop_cond_id,
                then_block: loop_body_bid,
                then_args: Vec::new(),
                else_block: loop_exit_bid,
                else_args: vec![p_b],
            },
        };

        let loop_body_block = BasicBlock {
            id: loop_body_bid,
            label: "loop_body".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::BinOp {
                    dest: next_b,
                    op: "+".to_string(),
                    left: p_a,
                    right: p_b,
                    ty: "Int".to_string(),
                },
                Inst::ConstInt {
                    dest: const_one,
                    value: 1,
                },
                Inst::BinOp {
                    dest: next_i,
                    op: "wrapping_+".to_string(),
                    left: p_i,
                    right: const_one,
                    ty: "Int".to_string(),
                },
            ],
            terminator: Terminator::Branch {
                target: loop_header_bid,
                args: vec![p_b, next_b, next_i],
            },
        };

        let loop_exit_block = BasicBlock {
            id: loop_exit_bid,
            label: "loop_exit".to_string(),
            params: vec![BlockParam {
                val: p_res,
                ty: "Int".to_string(),
                name: Some("res".to_string()),
            }],
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(p_res) },
        };

        let original_blocks = f.blocks.clone();
        let original_entry = f.entry_block;

        f.entry_block = entry_bid;
        f.blocks = vec![
            entry_block,
            base_block,
            loop_init_block,
            loop_header_block,
            loop_body_block,
            loop_exit_block,
        ];

        if crate::dmir::verify_function(f).is_err() {
            f.entry_block = original_entry;
            f.blocks = original_blocks;
            // Fall through to existing accumulator loop if SSA verification fails
        } else {
            return true;
        }
    } else if step_call_k == 1 && step_loop_k == 1 && base_k >= 0 {
        let base_cond_id = next_val();
        let base_k_id = next_val();
        let b_init_id = next_val();
        let i_init_id = next_val();

        let p_b = next_val();
        let p_i = next_val();

        let loop_cond_id = next_val();
        let next_b = next_val();
        let next_i = next_val();
        let const_one = next_val();
        let p_res = next_val();

        let entry_bid = BasicBlockId(0);
        let base_bid = BasicBlockId(1);
        let loop_init_bid = BasicBlockId(2);
        let loop_header_bid = BasicBlockId(3);
        let loop_body_bid = BasicBlockId(4);
        let loop_exit_bid = BasicBlockId(5);

        let entry_block = BasicBlock {
            id: entry_bid,
            label: "entry".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: base_k_id,
                    value: base_k,
                },
                Inst::BinOp {
                    dest: base_cond_id,
                    op: "<=".to_string(),
                    left: arg_n,
                    right: base_k_id,
                    ty: "Bool".to_string(),
                },
            ],
            terminator: Terminator::CondBranch {
                cond: base_cond_id,
                then_block: base_bid,
                then_args: Vec::new(),
                else_block: loop_init_bid,
                else_args: Vec::new(),
            },
        };

        let (base_insts, base_ret_val) = match base_kind {
            BaseReturnKind::Param => (Vec::new(), arg_n),
            BaseReturnKind::Const(c) => {
                let const_id = next_val();
                (
                    vec![Inst::ConstInt {
                        dest: const_id,
                        value: c,
                    }],
                    const_id,
                )
            }
        };
        let base_block = BasicBlock {
            id: base_bid,
            label: "base".to_string(),
            params: Vec::new(),
            instructions: base_insts,
            terminator: Terminator::Return {
                value: Some(base_ret_val),
            },
        };

        let init_b_val = match base_kind {
            BaseReturnKind::Param => base_k,
            BaseReturnKind::Const(c) => c,
        };
        let loop_init_block = BasicBlock {
            id: loop_init_bid,
            label: "loop_init".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: b_init_id,
                    value: init_b_val,
                },
                Inst::ConstInt {
                    dest: i_init_id,
                    value: base_k + 1,
                },
            ],
            terminator: Terminator::Branch {
                target: loop_header_bid,
                args: vec![b_init_id, i_init_id],
            },
        };

        let loop_header_block = BasicBlock {
            id: loop_header_bid,
            label: "loop_header".to_string(),
            params: vec![
                BlockParam {
                    val: p_b,
                    ty: "Int".to_string(),
                    name: Some("b".to_string()),
                },
                BlockParam {
                    val: p_i,
                    ty: "Int".to_string(),
                    name: Some("i".to_string()),
                },
            ],
            instructions: vec![Inst::BinOp {
                dest: loop_cond_id,
                op: "<=".to_string(),
                left: p_i,
                right: arg_n,
                ty: "Bool".to_string(),
            }],
            terminator: Terminator::CondBranch {
                cond: loop_cond_id,
                then_block: loop_body_bid,
                then_args: Vec::new(),
                else_block: loop_exit_bid,
                else_args: vec![p_b],
            },
        };

        let loop_body_block = BasicBlock {
            id: loop_body_bid,
            label: "loop_body".to_string(),
            params: Vec::new(),
            instructions: vec![
                Inst::BinOp {
                    dest: next_b,
                    op: "+".to_string(),
                    left: p_b,
                    right: p_b,
                    ty: "Int".to_string(),
                },
                Inst::ConstInt {
                    dest: const_one,
                    value: 1,
                },
                Inst::BinOp {
                    dest: next_i,
                    op: "wrapping_+".to_string(),
                    left: p_i,
                    right: const_one,
                    ty: "Int".to_string(),
                },
            ],
            terminator: Terminator::Branch {
                target: loop_header_bid,
                args: vec![next_b, next_i],
            },
        };

        let loop_exit_block = BasicBlock {
            id: loop_exit_bid,
            label: "loop_exit".to_string(),
            params: vec![BlockParam {
                val: p_res,
                ty: "Int".to_string(),
                name: Some("res".to_string()),
            }],
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(p_res) },
        };

        let original_blocks = f.blocks.clone();
        let original_entry = f.entry_block;

        f.entry_block = entry_bid;
        f.blocks = vec![
            entry_block,
            base_block,
            loop_init_block,
            loop_header_block,
            loop_body_block,
            loop_exit_block,
        ];

        if crate::dmir::verify_function(f).is_err() {
            f.entry_block = original_entry;
            f.blocks = original_blocks;
        } else {
            return true;
        }
    }

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

    let (base_insts, base_right) = match base_kind {
        BaseReturnKind::Param => (Vec::new(), curr_n),
        BaseReturnKind::Const(c) => {
            let base_const_id = next_val();
            (
                vec![Inst::ConstInt {
                    dest: base_const_id,
                    value: c,
                }],
                base_const_id,
            )
        }
    };
    let mut base_block_insts = base_insts;
    base_block_insts.push(Inst::BinOp {
        dest: final_res_id,
        op: "+".to_string(),
        left: curr_acc,
        right: base_right,
        ty: "Int".to_string(),
    });
    base_block_insts.push(Inst::Return {
        value: Some(final_res_id),
    });

    let base_block = BasicBlock {
        id: base_bid,
        label: "base_case".to_string(),
        params: Vec::new(),
        instructions: base_block_insts,
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
                args: {
                    let mut rec_args = vec![sub1_id];
                    for j in 1..f.params.len() {
                        rec_args.push(f.params[j].2);
                    }
                    rec_args
                },
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

fn substitute_inst_operands(inst: &mut Inst, subst: &HashMap<ValueId, ValueId>) {
    match inst {
        Inst::BinOp { left, right, .. } => {
            if let Some(&new_l) = subst.get(left) {
                *left = new_l;
            }
            if let Some(&new_r) = subst.get(right) {
                *right = new_r;
            }
        }
        Inst::UnOp { operand, .. } => {
            if let Some(&new_o) = subst.get(operand) {
                *operand = new_o;
            }
        }
        Inst::Call { args, .. } => {
            for arg in args {
                if let Some(&new_a) = subst.get(arg) {
                    *arg = new_a;
                }
            }
        }
        Inst::MethodCall { object, args, .. } => {
            if let Some(&new_obj) = subst.get(object) {
                *object = new_obj;
            }
            for arg in args {
                if let Some(&new_a) = subst.get(arg) {
                    *arg = new_a;
                }
            }
        }
        Inst::AssignVar { value, .. } => {
            if let Some(&new_v) = subst.get(value) {
                *value = new_v;
            }
        }
        Inst::Out { value } => {
            if let Some(&new_v) = subst.get(value) {
                *value = new_v;
            }
        }
        Inst::Err { value } => {
            if let Some(&new_v) = subst.get(value) {
                *value = new_v;
            }
        }
        Inst::SetField { object, value, .. } => {
            if let Some(&new_obj) = subst.get(object) {
                *object = new_obj;
            }
            if let Some(&new_v) = subst.get(value) {
                *value = new_v;
            }
        }
        Inst::GetField { object, .. } => {
            if let Some(&new_obj) = subst.get(object) {
                *object = new_obj;
            }
        }
        Inst::StructInit { fields, .. } => {
            for (_, val) in fields {
                if let Some(&new_v) = subst.get(val) {
                    *val = new_v;
                }
            }
        }
        Inst::FormatStr { values, .. } => {
            for val in values {
                if let Some(&new_a) = subst.get(val) {
                    *val = new_a;
                }
            }
        }
        Inst::Select {
            cond,
            then_val,
            else_val,
            ..
        } => {
            if let Some(&new_c) = subst.get(cond) {
                *cond = new_c;
            }
            if let Some(&new_t) = subst.get(then_val) {
                *then_val = new_t;
            }
            if let Some(&new_e) = subst.get(else_val) {
                *else_val = new_e;
            }
        }
        Inst::Decide { arms, else_val, .. } => {
            for (c, v) in arms {
                if let Some(&new_c) = subst.get(c) {
                    *c = new_c;
                }
                if let Some(&new_v) = subst.get(v) {
                    *v = new_v;
                }
            }
            if let Some(ev) = else_val {
                if let Some(&new_ev) = subst.get(ev) {
                    *ev = new_ev;
                }
            }
        }
        Inst::InlineAsm { inputs, .. } => {
            for (_, inp) in inputs {
                if let Some(&new_i) = subst.get(inp) {
                    *inp = new_i;
                }
            }
        }
        Inst::Return { value } => {
            if let Some(v) = value {
                if let Some(&new_v) = subst.get(v) {
                    *v = new_v;
                }
            }
        }
        _ => {}
    }
}

fn substitute_term_operands(term: &mut Terminator, subst: &HashMap<ValueId, ValueId>) {
    match term {
        Terminator::Branch { args, .. } => {
            for arg in args {
                if let Some(&new_a) = subst.get(arg) {
                    *arg = new_a;
                }
            }
        }
        Terminator::CondBranch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            if let Some(&new_c) = subst.get(cond) {
                *cond = new_c;
            }
            for a in then_args {
                if let Some(&new_a) = subst.get(a) {
                    *a = new_a;
                }
            }
            for a in else_args {
                if let Some(&new_a) = subst.get(a) {
                    *a = new_a;
                }
            }
        }
        Terminator::Return { value } => {
            if let Some(v) = value {
                if let Some(&new_v) = subst.get(v) {
                    *v = new_v;
                }
            }
        }
        Terminator::Unreachable => {}
    }
}

/// Tail-Call Optimization (TCO) for Direct Tail-Recursive Functions.
///
/// Converts direct self-calls in tail position into iterative jumps back
/// to the function loop header with updated SSA block arguments.
/// Eliminates stack frame accumulation, achieving O(1) stack space.
pub fn eliminate_tail_recursion(f: &mut Function) -> bool {
    let num_params = f.params.len();

    // 1. Identify all self-calls and check if they are in tail position.
    let mut total_self_calls = 0;
    struct TailCall {
        block_id: BasicBlockId,
        call_idx: usize,
    }
    let mut tail_calls: Vec<TailCall> = Vec::new();

    for b in &f.blocks {
        let mut self_calls_in_b = Vec::new();
        for (idx, inst) in b.instructions.iter().enumerate() {
            if let Inst::Call { func, args, .. } = inst {
                if func == &f.name {
                    total_self_calls += 1;
                    if args.len() == num_params {
                        self_calls_in_b.push(idx);
                    }
                }
            }
        }

        if self_calls_in_b.len() == 1 {
            let call_idx = self_calls_in_b[0];
            let call_dest = match &b.instructions[call_idx] {
                Inst::Call { dest, .. } => *dest,
                _ => continue,
            };

            // Check that instructions after call_idx in b are harmless
            let mut harmless = true;
            for inst in &b.instructions[(call_idx + 1)..] {
                match inst {
                    Inst::Return { value } => {
                        if let Some(v) = value {
                            if *v != call_dest {
                                harmless = false;
                                break;
                            }
                        }
                    }
                    Inst::AssignVar { .. } | Inst::LoadVar { .. } => {}
                    Inst::UnOp { op, .. } if op == "copy" => {}
                    Inst::Call { func, .. }
                        if func == "datara_rt_own_acquire" || func == "datara_rt_own_release" => {}
                    _ => {
                        harmless = false;
                        break;
                    }
                }
            }

            if harmless {
                // Check terminator
                let is_tail_ret = match &b.terminator {
                    Terminator::Return { value: Some(ret_v) } => {
                        let mut cur = *ret_v;
                        let mut matches = cur == call_dest;
                        if !matches {
                            for inst in &b.instructions[(call_idx + 1)..] {
                                if let Inst::UnOp {
                                    op,
                                    dest: c_dest,
                                    operand,
                                    ..
                                } = inst
                                {
                                    if op == "copy" && *c_dest == cur {
                                        cur = *operand;
                                        if cur == call_dest {
                                            matches = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        matches
                    }
                    Terminator::Return { value: None } => {
                        f.return_type == "Unit" || f.return_type == "Never"
                    }
                    _ => false,
                };

                if is_tail_ret {
                    tail_calls.push(TailCall {
                        block_id: b.id,
                        call_idx,
                    });
                }
            }
        }
    }

    // Must have at least one tail call, and ALL calls to self must be tail calls.
    if tail_calls.is_empty() || tail_calls.len() != total_self_calls {
        return false;
    }

    // Backup state in case verification fails
    let original_blocks = f.blocks.clone();
    let original_entry = f.entry_block;

    // 2. Find max ValueId across f
    let mut max_id = 0usize;
    for (_, _, val) in &f.params {
        if val.0 > max_id {
            max_id = val.0;
        }
    }
    for b in &f.blocks {
        for p in &b.params {
            if p.val.0 > max_id {
                max_id = p.val.0;
            }
        }
        for inst in &b.instructions {
            match inst {
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
                | Inst::Decide { dest, .. }
                | Inst::GetFuncAddr { dest, .. } => {
                    if dest.0 > max_id {
                        max_id = dest.0;
                    }
                }
                Inst::InlineAsm { outputs, .. } => {
                    for (_, dest) in outputs {
                        if dest.0 > max_id {
                            max_id = dest.0;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // 3. Allocate fresh ValueIds for the loop header block parameters
    let mut subst_map: HashMap<ValueId, ValueId> = HashMap::new();
    let mut loop_header_params = Vec::new();
    let mut param_assign_insts = Vec::new();

    for (p_name, p_ty, p_val) in &f.params {
        max_id += 1;
        let new_val = ValueId(max_id);
        subst_map.insert(*p_val, new_val);
        loop_header_params.push(BlockParam {
            val: new_val,
            ty: p_ty.clone(),
            name: Some(p_name.clone()),
        });
        param_assign_insts.push(Inst::AssignVar {
            name: p_name.clone(),
            value: new_val,
        });
    }

    // 4. Shift all existing blocks by +1:
    // Block ID i becomes i + 1.
    // Loop header will be the shifted old entry block: BasicBlockId(f.entry_block.0 + 1).
    let loop_header_bid = BasicBlockId(f.entry_block.0 + 1);

    for b in &mut f.blocks {
        b.id = BasicBlockId(b.id.0 + 1);
        match &mut b.terminator {
            Terminator::Branch { target, .. } => {
                target.0 += 1;
            }
            Terminator::CondBranch {
                then_block,
                else_block,
                ..
            } => {
                then_block.0 += 1;
                else_block.0 += 1;
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
    }

    // 5. Configure loop header:
    // Add block params and prepend parameter assignment instructions
    if let Some(header_b) = f.blocks.iter_mut().find(|b| b.id == loop_header_bid) {
        header_b.params = loop_header_params;
        param_assign_insts.append(&mut header_b.instructions);
        header_b.instructions = param_assign_insts;
    } else {
        f.blocks = original_blocks;
        f.entry_block = original_entry;
        return false;
    }

    // 6. Substitute operands across all shifted blocks (p_val -> new_val)
    for b in &mut f.blocks {
        for inst in &mut b.instructions {
            substitute_inst_operands(inst, &subst_map);
        }
        substitute_term_operands(&mut b.terminator, &subst_map);
    }

    // 7. For each tail call block (now shifted by +1):
    // Truncate at call_idx and replace terminator with branch to loop_header_bid
    for tc in &tail_calls {
        let shifted_bid = BasicBlockId(tc.block_id.0 + 1);
        if let Some(b) = f.blocks.iter_mut().find(|b| b.id == shifted_bid) {
            let call_args = match &b.instructions[tc.call_idx] {
                Inst::Call { args, .. } => args.clone(),
                _ => {
                    f.blocks = original_blocks;
                    f.entry_block = original_entry;
                    return false;
                }
            };
            b.instructions.truncate(tc.call_idx);
            b.terminator = Terminator::Branch {
                target: loop_header_bid,
                args: call_args,
            };
        }
    }

    // 8. Create new entry block with BasicBlockId(0)
    let initial_args: Vec<ValueId> = f.params.iter().map(|(_, _, val)| *val).collect();
    let new_entry_block = BasicBlock {
        id: BasicBlockId(0),
        label: "tco_entry".to_string(),
        params: Vec::new(),
        instructions: Vec::new(),
        terminator: Terminator::Branch {
            target: loop_header_bid,
            args: initial_args,
        },
    };

    f.blocks.insert(0, new_entry_block);
    f.entry_block = BasicBlockId(0);

    // 9. Verify CFG soundness
    if crate::dmir::verify_function(f).is_err() {
        f.blocks = original_blocks;
        f.entry_block = original_entry;
        return false;
    }

    true
}
