//! Global common-subexpression elimination.
//!
//! With real SSA (mem2reg + block parameters) every operand of a `BinOp` is a
//! single-assignment value, so an expression can be shared across blocks once
//! its definition is proven to dominate the use: the dominating definition
//! executes on every path that reaches the use, so reusing its result cannot
//! change behavior. This is the standard sufficient condition for non-
//! speculative CSE of pure operations.
//!
//! Division/modulo need no special guard here: CSE replaces a *later*
//! computation with an *earlier dominating* one. If the earlier one traps,
//! the later one would have executed anyway on that path — trap behavior is
//! unchanged. (Hoisting a trap *into* a zero-trip loop is LICM's concern and
//! is guarded there.)

use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::{HashMap, HashSet};

/// Expression identity. `ty` is part of the key so an `Int` and a `Float`
/// expression with the same operands are never merged.
type ExprKey = (String, String, ValueId, ValueId);

/// Ops whose result is bit-identical under operand swap on two's-complement
/// integer semantics. Floats are deliberately excluded: IEEE `+`/`*` are
/// commutative, but keeping the rewrite strictly to `Int` removes any doubt
/// about NaN/rounding edge cases.
fn canonical_key(op: &str, ty: &str, left: ValueId, right: ValueId) -> ExprKey {
    let key = (op.to_string(), ty.to_string(), left, right);
    if ty == "Int" && matches!(op, "+" | "*" | "==" | "!=") && left.0 > right.0 {
        (key.0, key.1, key.3, key.2)
    } else {
        key
    }
}

pub struct ScalarOptimizer;

impl ScalarOptimizer {
    /// Dominance-based global CSE.
    ///
    /// Phase 1 counts expression occurrences across the whole function so the
    /// cost model can judge global benefit. Phase 2 walks blocks in dominator
    /// preorder (every dominator is visited before the blocks it dominates),
    /// maintaining one map from expression key to its surviving definition
    /// block. A later occurrence is replaced by a zero-cost `copy` of the
    /// earlier value when the cost model approves and the definition block
    /// dominates the use block.
    pub fn eliminate_common_subexpressions(
        f: &mut Function,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let cfg = ControlFlowGraph::build(f);

        // ---- Phase 1: global occurrence counting --------------------------
        let mut counts: HashMap<ExprKey, usize> = HashMap::new();
        for block in &f.blocks {
            for inst in &block.instructions {
                if let Inst::BinOp {
                    op,
                    left,
                    right,
                    ty,
                    ..
                } = inst
                {
                    *counts
                        .entry(canonical_key(op, ty, *left, *right))
                        .or_insert(0) += 1;
                }
            }
        }

        // ---- Phase 2: dominator-ordered rewrite ---------------------------
        let mut order: Vec<BasicBlockId> = Vec::with_capacity(f.blocks.len());
        {
            let mut visited: HashSet<BasicBlockId> = HashSet::new();
            let mut stack = vec![f.entry_block];
            while let Some(b) = stack.pop() {
                if !visited.insert(b) {
                    continue;
                }
                order.push(b);
                if let Some(children) = cfg.dom_tree_children.get(&b) {
                    // Push in reverse so children are visited in ascending id
                    // order (determinism only; correctness is order-agnostic).
                    for &c in children.iter().rev() {
                        stack.push(c);
                    }
                }
            }
            // Blocks outside the dominator tree are unreachable from entry:
            // their instructions can never execute, so they are left untouched
            // (DCE removes them).
        }

        let mut def_block: HashMap<ExprKey, BasicBlockId> = HashMap::new();
        let mut approved: HashMap<ExprKey, bool> = HashMap::new();
        let mut eliminated = 0usize;

        for bid in &order {
            let Some(bi) = f.blocks.iter().position(|b| &b.id == bid) else {
                continue;
            };
            for ii in 0..f.blocks[bi].instructions.len() {
                let (key, dest, ty) = match &f.blocks[bi].instructions[ii] {
                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ty,
                    } => (canonical_key(op, ty, *left, *right), *dest, ty.clone()),
                    _ => continue,
                };

                let hit = match def_block.get(&key) {
                    // Same block: the surviving definition was recorded while
                    // walking this block, so it is textually earlier.
                    Some(&db) if db == *bid => Some(db),
                    // Cross block: requires a dominance proof.
                    Some(&db) if cfg.dominates(db, *bid) => Some(db),
                    _ => None,
                };

                let Some(_db) = hit else {
                    // No reusable definition: record this instance as the
                    // candidate definition for its key (first one wins; never
                    // overwrite an earlier definition).
                    def_block.entry(key).or_insert(*bid);
                    continue;
                };

                let apply = *approved.entry(key.clone()).or_insert_with(|| {
                    let count = counts.get(&key).copied().unwrap_or(1);
                    let expr_repr = format!("v{} {} v{}", key.2.0, key.0, key.3.0);
                    let (apply, _benefit, _cost, _reason) =
                        cost_model.evaluate_cse(&expr_repr, count);
                    apply
                });
                if !apply {
                    // Cost model rejected this key: remember the verdict
                    // and drop the map entry so later hits skip cleanly.
                    def_block.remove(&key);
                    continue;
                }

                // Find the surviving value for this key: the dest of the
                // first BinOp with this key in the def block. Re-derive it
                // from the block to avoid storing stale ids across rewrites.
                let first_val = f.blocks[bi]
                    .instructions
                    .iter()
                    .find_map(|i| match i {
                        Inst::BinOp {
                            dest,
                            op,
                            left,
                            right,
                            ty,
                        } if *bid == _db && canonical_key(op, ty, *left, *right) == key => {
                            Some(*dest)
                        }
                        _ => None,
                    })
                    .or_else(|| {
                        f.blocks.iter().find(|b| b.id == _db).and_then(|b| {
                            b.instructions.iter().find_map(|i| match i {
                                Inst::BinOp {
                                    dest,
                                    op,
                                    left,
                                    right,
                                    ty,
                                } if canonical_key(op, ty, *left, *right) == key => Some(*dest),
                                _ => None,
                            })
                        })
                    });

                if let Some(first_val) = first_val {
                    f.blocks[bi].instructions[ii] = Inst::UnOp {
                        dest,
                        op: "copy".to_string(),
                        operand: first_val,
                        ty,
                    };
                    eliminated += 1;
                    trace.record(
                        "CSE",
                        &format!(
                            "{}:bb{}:v{} {} v{}",
                            f.name, bid.0, first_val.0, key.0, key.3.0
                        ),
                        "Applied",
                        &format!(
                            "reuse of dominating definition %{} (bb{})",
                            first_val.0, _db.0
                        ),
                        "None (SSA value forwarding)",
                        "global CSE with dominance proof: definition block \
                          dominates use block, operands are single-assignment",
                    );
                } else {
                    // Defensive: the definition vanished (should not happen).
                    def_block.remove(&key);
                }
            }
        }

        eliminated
    }
}
