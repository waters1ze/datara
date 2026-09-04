use crate::ast::{Expr, LiteralValue, Refinement};
use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::{HashMap, HashSet};

/// Loop optimizations that operate on the **real CFG** (basic blocks joined by
/// `Terminator::Branch` / `Terminator::CondBranch`).
///
/// Historically these passes transformed the compound `Inst::WhileLoop` node.
/// That node was only a cloned snapshot of the CFG blocks and was dropped by the
/// Cranelift backend (`backend.rs`: `Inst::WhileLoop { .. } => {}`), so every
/// transformation applied to it was discarded at codegen time. All passes here
/// now work on the blocks that are actually compiled.
pub struct LoopOptimizer;

/// Facts gathered once per loop, used to decide which instructions may be hoisted.
struct LoopFacts {
    /// Variables written anywhere inside the loop. Loading one yields a
    /// different value on each iteration, so such loads are not invariant.
    assigned: HashSet<String>,
    /// Set when the loop contains a call, method call or field store, i.e.
    /// something that could change locals or memory behind our back.
    may_alias: bool,
}

impl LoopOptimizer {
    pub fn optimize_loops(
        f: &mut Function,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut transformed = 0;
        transformed += Self::licm_pass(f, cost_model, trace);
        transformed += Self::bce_pass(f, cost_model, trace);
        // Detection only. It must not contribute to `transformed`: a non-zero
        // return would make the driver believe the function changed and keep
        // re-running every pass until the iteration cap, re-emitting traces for
        // work that was never performed.
        Self::analyze_vectorization(f, cost_model, trace);
        transformed
    }

    // NOTE: loop unrolling is deliberately NOT implemented.
    //
    // A previous version replicated the loop body inside its basic block while
    // leaving the surrounding control flow untouched. That is unsound: the loop
    // still branched back to the header once per iteration, so the trip count
    // changed, and it duplicated `ValueId` definitions without renaming, which
    // violates the single-assignment form the rest of the pipeline assumes.
    // Measured effect: zero speedup (a later CSE pass collapsed the copies back
    // to a single computation) while inflating the body 3x-6x.
    //
    // A correct implementation needs (a) fresh `ValueId`s for every copied
    // instruction and (b) each copy guarded by the loop condition, so that a
    // trip count that is not a multiple of the unroll factor cannot overshoot.

    /// Recognizes countable while-loops and replaces the whole iteration with
    /// a closed-form computation of the final accumulator value.
    ///
    /// Target shape (produced by lowering + mem2reg for `mut sum = s0; mut i =
    /// 0; while i < n { sum += t; i += 1 }`):
    ///
    /// ```text
    /// preheader: Branch(header, [s0, i0])          i0 must be ConstInt(0)
    /// header(p_sum, p_i): cond = BinOp("<", p_i, n)   n defined outside
    ///           CondBranch(body | exit)             no branch args
    /// body:     sum_next = BinOp("+", p_sum, t)
    ///           i_next   = BinOp("+", p_i, 1)
    ///           Branch(header, [sum_next, i_next])
    /// exit:     ... uses of p_sum ...
    /// ```
    ///
    /// Soundness of the closed form (must be preserved by any future edit):
    ///
    /// 1. Wrapping i64 addition is the group Z/2^64, so the iterated
    ///    `sum += t` produces exactly `s0 + n*t (mod 2^64)` — and for the
    ///    induction term, `sum_{k=0}^{n-1} k = n*(n-1)/2` exactly as integers.
    /// 2. A naive `n*(n-1)/2` is UNSOUND: computing `n*(n-1)` wraps, and
    ///    dividing the wrapped product by 2 loses the parity of the overflow
    ///    bits. The parity-split product avoids division of a wrapped value:
    ///    for even n use `(n/2)*(n-1)`, for odd n use `((n-1)/2)*n` — each
    ///    factor is an exact integer and their wrapping product is the true
    ///    `n*(n-1)/2 (mod 2^64)`.
    /// 3. `n <= 0` means zero trips, guarded by a select. Both select sides are
    ///    computed unconditionally; none of them can trap (the only division
    ///    is by the constant 2).
    ///
    /// Anything that does not match the shape exactly bails: a missed fold
    /// costs speed, a wrong fold costs correctness.
    pub fn fold_loops(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let cfg = ControlFlowGraph::build(f);
        for lp in &cfg.loops {
            if let Some(plan) = Self::match_countable_loop(f, &cfg, lp) {
                Self::apply_fold(f, &plan, trace);
                return 1;
            }
        }
        0
    }

    /// Pure instructions whose operands are loop-invariant may be hoisted.
    /// Everything else (calls, I/O, stores, control flow) stays in the loop.
    fn gather_loop_facts(f: &Function, loop_blocks: &HashSet<BasicBlockId>) -> LoopFacts {
        let mut facts = LoopFacts {
            assigned: HashSet::new(),
            may_alias: false,
        };
        for &bid in loop_blocks {
            if let Some(blk) = f.get_block(bid) {
                for inst in &blk.instructions {
                    match inst {
                        Inst::AssignVar { name, .. } => {
                            facts.assigned.insert(name.clone());
                        }
                        Inst::Call { .. } | Inst::MethodCall { .. } | Inst::SetField { .. } => {
                            facts.may_alias = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        facts
    }

    fn is_hoistable(inst: &Inst, facts: &LoopFacts) -> bool {
        match inst {
            // Loading a variable that the loop never writes produces the same
            // value on every iteration, so the load is loop-invariant.
            Inst::LoadVar { name, .. } => !facts.may_alias && !facts.assigned.contains(name),
            Inst::GetField { .. } => !facts.may_alias,
            // Division / modulo may trap on a zero divisor. Moving one out of a
            // loop that may run zero times can introduce a fault the original
            // program never had, so those stay where they are.
            Inst::BinOp { op, .. } => op != "/" && op != "%",
            Inst::ConstInt { .. }
            | Inst::ConstFloat { .. }
            | Inst::ConstBool { .. }
            | Inst::ConstStr { .. }
            | Inst::UnOp { .. } => true,
            _ => false,
        }
    }

    /// Values read by an instruction (i.e. its dependencies).
    fn source_operands(inst: &Inst) -> Vec<ValueId> {
        match inst {
            Inst::BinOp { left, right, .. } => vec![*left, *right],
            Inst::UnOp { operand, .. } => vec![*operand],
            Inst::GetField { object, .. } => vec![*object],
            Inst::AssignVar { value, .. } => vec![*value],
            Inst::LoadVar { .. } => Vec::new(),
            Inst::Call { args, .. } => args.clone(),
            Inst::MethodCall { object, args, .. } => {
                let mut s = vec![*object];
                s.extend(args);
                s
            }
            Inst::StructInit { fields, .. } => fields.iter().map(|(_, v)| *v).collect(),
            Inst::FormatStr { values, .. } => values.clone(),
            _ => Vec::new(),
        }
    }

    fn dest(inst: &Inst) -> Option<ValueId> {
        match inst {
            Inst::ConstInt { dest, .. }
            | Inst::ConstFloat { dest, .. }
            | Inst::ConstStr { dest, .. }
            | Inst::ConstBool { dest, .. }
            | Inst::BinOp { dest, .. }
            | Inst::UnOp { dest, .. }
            | Inst::GetField { dest, .. }
            | Inst::LoadVar { dest, .. }
            | Inst::Call { dest, .. }
            | Inst::MethodCall { dest, .. }
            | Inst::StructInit { dest, .. }
            | Inst::FormatStr { dest, .. }
            | Inst::GetFuncAddr { dest, .. }
            | Inst::Select { dest, .. }
            | Inst::Decide { dest, .. } => Some(*dest),
            Inst::AssignVar { .. }
            | Inst::SetField { .. }
            | Inst::Out { .. }
            | Inst::Err { .. }
            | Inst::Return { .. }
            | Inst::WhileLoop { .. }
            | Inst::TryCatch { .. } => None,
        }
    }

    /// Deterministic traversal order for a loop body: every block that defines
    /// a value precedes every block that consumes it.
    ///
    /// `NaturalLoop::blocks` is a `HashSet`, so iterating it directly yields an
    /// arbitrary order. That is only safe for set-building, not for emitting
    /// instructions: hoisting `t2 = t1 * 2` before `t1 = a + b` would put a use
    /// ahead of its definition in the preheader.
    ///
    /// Dropping the back edges into the header makes the loop body a DAG, so
    /// reversing a DFS post-order yields a topological order over it.
    fn ordered_loop_blocks(
        cfg: &ControlFlowGraph,
        lp: &crate::dmir::cfg::NaturalLoop,
    ) -> Vec<BasicBlockId> {
        let mut post_order: Vec<BasicBlockId> = Vec::new();
        let mut visited: HashSet<BasicBlockId> = HashSet::new();
        let mut stack = vec![(lp.header, false)];

        while let Some((node, processed)) = stack.pop() {
            if processed {
                post_order.push(node);
                continue;
            }
            if !visited.insert(node) {
                continue;
            }
            stack.push((node, true));
            if let Some(succs) = cfg.successors.get(&node) {
                for &s in succs {
                    // Back edges into the header would cycle forever; anything
                    // leaving the loop is not part of the body ordering.
                    if s == lp.header || !lp.blocks.contains(&s) {
                        continue;
                    }
                    if !visited.contains(&s) {
                        stack.push((s, false));
                    }
                }
            }
        }

        post_order.reverse();
        post_order
    }

    /// Loop-Invariant Code Motion over natural loops of the real CFG.
    ///
    /// Hoists pure, dependency-free instructions out of a loop into a dedicated
    /// preheader block (created when necessary), so they execute once instead of
    /// once per iteration.
    pub fn licm_pass(
        f: &mut Function,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut total_hoisted = 0;

        // Each iteration can create a preheader, which changes the CFG, so the
        // CFG is rebuilt every round. Bounded to keep compilation predictable.
        for _round in 0..8 {
            let cfg = ControlFlowGraph::build(f);
            if cfg.loops.is_empty() {
                break;
            }

            // Plan the hoists for the outermost loop first; recompute afterwards.
            let mut plan: Option<(crate::dmir::cfg::NaturalLoop, Vec<Inst>)> = None;

            for lp in &cfg.loops {
                // Values defined anywhere inside the loop. Loading one yields a
                // different value on each iteration, so such loads are not invariant.
                // Block parameters are definitions as well: after mem2reg a loop
                // header parameter is the per-iteration value of a loop-carried
                // variable, and an instruction reading one is not invariant.
                let mut loop_defs: HashSet<ValueId> = HashSet::new();
                for &bid in &lp.blocks {
                    if let Some(blk) = f.get_block(bid) {
                        for param in &blk.params {
                            loop_defs.insert(param.val);
                        }
                        for inst in &blk.instructions {
                            if let Some(d) = Self::dest(inst) {
                                loop_defs.insert(d);
                            }
                        }
                    }
                }

                let facts = Self::gather_loop_facts(f, &lp.blocks);

                // Fixpoint: an instruction is invariant when every operand is
                // either defined outside the loop or already known invariant.
                let mut invariant: HashSet<ValueId> = HashSet::new();
                let mut changed = true;
                while changed {
                    changed = false;
                    for &bid in &lp.blocks {
                        if let Some(blk) = f.get_block(bid) {
                            for inst in &blk.instructions {
                                if !Self::is_hoistable(inst, &facts) {
                                    continue;
                                }
                                if let Some(d) = Self::dest(inst) {
                                    if invariant.contains(&d) {
                                        continue;
                                    }
                                    let srcs = Self::source_operands(inst);
                                    let ok = srcs
                                        .iter()
                                        .all(|s| !loop_defs.contains(s) || invariant.contains(s));
                                    if ok {
                                        invariant.insert(d);
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }

                if invariant.is_empty() {
                    continue;
                }

                // Collect hoistable instructions in a def-before-use order so
                // the preheader never references a value it defines later.
                let mut hoisted: Vec<Inst> = Vec::new();
                for bid in Self::ordered_loop_blocks(&cfg, lp) {
                    if let Some(blk) = f.get_block(bid) {
                        for inst in &blk.instructions {
                            if Self::is_hoistable(inst, &facts)
                                && let Some(d) = Self::dest(inst)
                                && invariant.contains(&d)
                            {
                                hoisted.push(inst.clone());
                            }
                        }
                    }
                }

                if hoisted.is_empty() {
                    continue;
                }

                plan = Some((lp.clone(), hoisted));
                break;
            }

            let (lp, hoisted) = match plan {
                Some(p) => p,
                None => break,
            };

            let count = hoisted.len();
            let preheader = Self::ensure_preheader(f, lp.header, &lp.blocks);

            // Remove the hoisted instructions from the loop blocks.
            let hoisted_dests: HashSet<ValueId> = hoisted.iter().filter_map(Self::dest).collect();
            for bid in &lp.blocks {
                if let Some(blk) = f.get_block_mut(*bid) {
                    blk.instructions.retain(
                        |i| !matches!(Self::dest(i), Some(d) if hoisted_dests.contains(&d)),
                    );
                }
            }

            // Emit them once, in the preheader.
            if let Some(ph) = f.get_block_mut(preheader) {
                ph.instructions.extend(hoisted);
            }

            total_hoisted += count;

            let (apply, benefit, cost, reason) = cost_model.evaluate_licm("loop_invariant", true);
            let decision = if apply { "Applied" } else { "Rejected" };
            trace.record(
                "LICM",
                &format!("{}:bb{}_to_bb{}", f.name, preheader.0, lp.header.0),
                decision,
                &benefit,
                &cost,
                &format!(
                    "{} loop-invariant instruction(s) hoisted to preheader; reason: {}",
                    count, reason
                ),
            );
        }

        total_hoisted
    }

    /// Guarantees a block that dominates the loop header and is outside the loop.
    /// Reuses an existing one when possible, otherwise creates a new preheader
    /// and rewrites the entering edges to point at it.
    fn ensure_preheader(
        f: &mut Function,
        header: BasicBlockId,
        loop_blocks: &HashSet<BasicBlockId>,
    ) -> BasicBlockId {
        // A preheader must *dominate* the header, otherwise the hoisted values
        // would be undefined on any other path into the loop. That only holds
        // when the loop has a single entering edge, so reuse is limited to that
        // case; otherwise a dedicated block is created.
        let entering: Vec<BasicBlockId> = f
            .blocks
            .iter()
            .filter(|b| !loop_blocks.contains(&b.id))
            .filter(|b| match &b.terminator {
                Terminator::Branch { target, .. } => *target == header,
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => *then_block == header || *else_block == header,
                _ => false,
            })
            .map(|b| b.id)
            .collect();

        if entering.len() == 1
            && let Some(b) = f.get_block(entering[0])
            && matches!(&b.terminator, Terminator::Branch { .. })
        {
            return entering[0];
        }

        let next_id = f
            .blocks
            .iter()
            .map(|b| b.id.0)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let new_id = BasicBlockId(next_id);

        // The new preheader inherits the header's block parameters: every
        // redirected edge keeps its arguments (they were written for the
        // header), the preheader receives them as parameters, and its
        // terminator forwards them to the header. Fresh `ValueId`s are
        // required — a parameter is a definition, and reusing the header's
        // ids would define one value twice under strict SSA.
        let header_params: Vec<crate::dmir::BlockParam> = f
            .get_block(header)
            .map(|h| h.params.clone())
            .unwrap_or_default();
        let mut fresh = f
            .blocks
            .iter()
            .flat_map(|b| {
                let mut ids = Vec::new();
                for p in &b.params {
                    ids.push(p.val.0);
                }
                for inst in &b.instructions {
                    Self::for_each_vid(inst, &mut |v: &ValueId| ids.push(v.0));
                }
                match &b.terminator {
                    Terminator::Branch { args, .. } => {
                        ids.extend(args.iter().map(|v| v.0));
                    }
                    Terminator::CondBranch {
                        cond,
                        then_args,
                        else_args,
                        ..
                    } => {
                        ids.push(cond.0);
                        ids.extend(then_args.iter().map(|v| v.0));
                        ids.extend(else_args.iter().map(|v| v.0));
                    }
                    Terminator::Return { value: Some(v) } => ids.push(v.0),
                    _ => {}
                }
                ids
            })
            .max()
            .unwrap_or(0)
            + 1;
        let mut forwarded_args = Vec::with_capacity(header_params.len());
        let mut preheader_params = Vec::with_capacity(header_params.len());
        for p in &header_params {
            let v = ValueId(fresh);
            fresh += 1;
            forwarded_args.push(p.val);
            preheader_params.push(crate::dmir::BlockParam {
                val: v,
                ty: p.ty.clone(),
                name: p.name.clone(),
            });
        }

        // Redirect every edge entering the loop to the new preheader.
        for b in f.blocks.iter_mut() {
            if loop_blocks.contains(&b.id) {
                continue;
            }
            match &mut b.terminator {
                Terminator::Branch { target, .. } => {
                    if *target == header {
                        *target = new_id;
                    }
                }
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => {
                    if *then_block == header {
                        *then_block = new_id;
                    }
                    if *else_block == header {
                        *else_block = new_id;
                    }
                }
                _ => {}
            }
        }

        f.blocks.push(crate::dmir::BasicBlock {
            id: new_id,
            label: format!("loop_preheader_{}", new_id.0),
            params: preheader_params,
            instructions: Vec::new(),
            terminator: Terminator::Branch {
                target: header,
                args: forwarded_args,
            },
        });

        new_id
    }

    /// Visits every `ValueId` mentioned by an instruction (defs and uses).
    fn for_each_vid(inst: &Inst, f: &mut dyn FnMut(&ValueId)) {
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

    /// Detects loops that are candidates for SIMD vectorization.
    ///
    /// This is a **detection-only** pass. It does not emit vector instructions,
    /// because no SIMD lowering exists in the Cranelift backend yet. Records are
    /// therefore logged as `Candidate`, never as `Applied`, and no speedup is
    /// claimed.
    pub fn analyze_vectorization(
        f: &mut Function,
        cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) {
        if cost_model.vectorization_width == 0 {
            return;
        }

        let cfg = ControlFlowGraph::build(f);

        for lp in &cfg.loops {
            let body_blocks: Vec<BasicBlockId> = lp
                .blocks
                .iter()
                .copied()
                .filter(|b| *b != lp.header)
                .collect();

            let arith_ops: usize = body_blocks
                .iter()
                .filter_map(|b| f.get_block(*b))
                .map(|blk| {
                    blk.instructions
                        .iter()
                        .filter(|i| matches!(i, Inst::BinOp { op, .. } if op == "+" || op == "*"))
                        .count()
                })
                .sum();

            // A body must be free of calls / I/O to be safely vectorizable.
            let is_pure = body_blocks
                .iter()
                .filter_map(|b| f.get_block(*b))
                .all(|blk| {
                    blk.instructions.iter().all(|i| {
                        !matches!(
                            i,
                            Inst::Call { .. }
                                | Inst::MethodCall { .. }
                                | Inst::Out { .. }
                                | Inst::Err { .. }
                        )
                    })
                });

            if arith_ops >= 4 && is_pure {
                trace.record(
                    "SIMDVectorize",
                    &format!("{}:bb{}_to_bb{}", f.name, lp.header.0, lp.header.0),
                    "Candidate",
                    "Not yet realized (no SIMD lowering in backend)",
                    "None (analysis only)",
                    &format!(
                        "counted {} '+'/'*' operations in loop body; dependency analysis and \
                         SIMD lowering are both absent, so width {} is NOT emitted",
                        arith_ops, cost_model.vectorization_width
                    ),
                );
            }
        }
    }
}

/// The accumulating term of a countable loop, resolved to a form that
/// survives the deletion of the loop blocks.
#[derive(Debug, Clone, Copy)]
enum SumTerm {
    /// `sum += i` or `sum += i * k` where `i` is the induction variable:
    /// closed form via parity-split Gaussian product.
    Induction { scale: i64, start: i64, is_le: bool },
    /// `sum += i * i` (quadratic sum): closed form via n*(n-1)*(2n-1)/6 or n*(n+1)*(2n+1)/6.
    Quadratic { scale: i64, is_le: bool },
    /// Float induction: `sum += i * k` for Float induction variable.
    FloatInduction { scale: f64, start: f64, is_le: bool },
    /// `sum += x` where `x` is defined outside the loop: closed form `n*x`.
    InvariantValue(ValueId),
    /// `sum += c`: closed form `n*c`.
    InvariantConst(i64),
    /// Float constant `sum += c`: closed form `n*c`.
    FloatInvariantConst(f64),
}

struct FoldPlan {
    header: BasicBlockId,
    body: BasicBlockId,
    exit: BasicBlockId,
    preheader: BasicBlockId,
    /// The accumulator's header parameter.
    p_sum: ValueId,
    /// Initial accumulator value (defined outside the loop).
    s0: ValueId,
    /// The loop bound `n` in `i < n` (defined outside the loop).
    n: ValueId,
    term: SumTerm,
    is_float: bool,
}

impl LoopOptimizer {
    fn match_countable_loop(
        f: &Function,
        cfg: &ControlFlowGraph,
        lp: &crate::dmir::cfg::NaturalLoop,
    ) -> Option<FoldPlan> {
        // v1 shape: exactly one header + one back-edge block.
        if lp.blocks.len() != 2 || lp.back_edges.len() != 1 {
            return None;
        }
        let header = lp.header;
        let body = lp.back_edges[0];
        if body == header || !lp.blocks.contains(&body) {
            return None;
        }
        if f.entry_block == header || f.entry_block == body {
            return None;
        }

        let header_blk = f.get_block(header)?;
        let body_blk = f.get_block(body)?;

        // Header: two parameters (both Int or both Float), one `<` or `<=` comparison whose
        // left operand is the induction variable, and a CondBranch into body | exit.
        let is_float = header_blk.params.iter().all(|p| p.ty == "Float");
        let is_int = header_blk.params.iter().all(|p| p.ty == "Int");
        if header_blk.params.len() != 2 || (!is_int && !is_float) {
            return None;
        }
        if header_blk.instructions.len() != 1 {
            return None;
        }
        let (cond_vid, p_i, n, is_le) = match &header_blk.instructions[0] {
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ..
            } if op == "<" => (*dest, *left, *right, false),
            Inst::BinOp {
                dest,
                op,
                left,
                right,
                ..
            } if op == "<=" => (*dest, *left, *right, true),
            _ => return None,
        };
        let (p_a, p_b) = (header_blk.params[0].val, header_blk.params[1].val);
        if p_i != p_a && p_i != p_b {
            return None;
        }
        let p_sum = if p_i == p_a { p_b } else { p_a };
        let exit = match &header_blk.terminator {
            Terminator::CondBranch {
                cond,
                then_block,
                then_args,
                else_block,
                else_args,
            } if *cond == cond_vid
                && *then_block == body
                && then_args.is_empty()
                && else_args.is_empty() =>
            {
                *else_block
            }
            _ => return None,
        };
        if exit == body || exit == header {
            return None;
        }

        // Body: back-edge additions, optional scaled induction, plus constants feeding them.
        if body_blk.instructions.is_empty() {
            return None;
        }
        let mut scaled_terms: HashMap<ValueId, i64> = HashMap::new();
        let mut quadratic_terms: HashSet<ValueId> = HashSet::new();
        let mut float_scaled_terms: HashMap<ValueId, f64> = HashMap::new();
        for inst in &body_blk.instructions {
            if let Inst::BinOp {
                dest,
                op,
                left,
                right,
                ..
            } = inst
                && op == "*"
            {
                if !is_float && *left == p_i && *right == p_i {
                    quadratic_terms.insert(*dest);
                } else if !is_float && *left == p_i {
                    if let Some(k) = Self::const_int_value(f, *right) {
                        scaled_terms.insert(*dest, k);
                    }
                } else if !is_float
                    && *right == p_i
                    && let Some(k) = Self::const_int_value(f, *left)
                {
                    scaled_terms.insert(*dest, k);
                } else if is_float && *left == p_i {
                    if let Some(k) = Self::const_float_value(f, *right) {
                        float_scaled_terms.insert(*dest, k);
                    }
                } else if is_float
                    && *right == p_i
                    && let Some(k) = Self::const_float_value(f, *left)
                {
                    float_scaled_terms.insert(*dest, k);
                }
            }
        }

        let mut acc: Option<(ValueId, ValueId)> = None; // (sum_next, operand)
        let mut inc: Option<ValueId> = None; // i_next
        for inst in &body_blk.instructions {
            match inst {
                Inst::ConstInt { .. } if !is_float => {}
                Inst::ConstFloat { .. } if is_float => {}
                Inst::BinOp { op, dest, .. } if op == "*" => {
                    if !scaled_terms.contains_key(dest)
                        && !quadratic_terms.contains(dest)
                        && !float_scaled_terms.contains_key(dest)
                    {
                        return None;
                    }
                }
                Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ..
                } if op == "+" => {
                    if *left == p_sum && acc.is_none() {
                        acc = Some((*dest, *right));
                    } else if *left == p_i && inc.is_none() {
                        if (!is_float && Self::const_int_value(f, *right) == Some(1))
                            || (is_float && Self::const_float_value(f, *right) == Some(1.0))
                        {
                            inc = Some(*dest);
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        let (sum_next, x) = acc?;
        let i_next = inc?;

        // Back edge: both additions are passed to the header, each in the
        // parameter slot of the value it redefines.
        let args = match &body_blk.terminator {
            Terminator::Branch { target, args } if *target == header => args,
            _ => return None,
        };
        let idx_sum = header_blk.params.iter().position(|p| p.val == p_sum)?;
        let idx_i = header_blk.params.iter().position(|p| p.val == p_i)?;
        if args.len() != 2 || args[idx_sum] != sum_next || args[idx_i] != i_next {
            return None;
        }

        // Every value defined inside the loop.
        let mut loop_defs: HashSet<ValueId> = HashSet::new();
        loop_defs.insert(p_sum);
        loop_defs.insert(p_i);
        for blk in [header_blk, body_blk] {
            for inst in &blk.instructions {
                if let Some(d) = Self::dest(inst) {
                    loop_defs.insert(d);
                }
            }
        }

        // The loop bound must be loop-invariant.
        if loop_defs.contains(&n) {
            return None;
        }

        // The loop must have exactly one entering edge, unconditional, and no
        // outside edge into the body block.
        let mut preheader: Option<BasicBlockId> = None;
        for b in &f.blocks {
            if lp.blocks.contains(&b.id) {
                continue;
            }
            let (to_header, to_body) = match &b.terminator {
                Terminator::Branch { target, .. } => (*target == header, *target == body),
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => (
                    *then_block == header || *else_block == header,
                    *then_block == body || *else_block == body,
                ),
                _ => (false, false),
            };
            if to_body {
                return None;
            }
            if to_header {
                if preheader.is_some() {
                    return None;
                }
                preheader = Some(b.id);
            }
        }
        let preheader = preheader?;
        let (s0, i0) = match &f.get_block(preheader)?.terminator {
            Terminator::Branch { target, args } if *target == header && args.len() == 2 => {
                (args[idx_sum], args[idx_i])
            }
            _ => return None,
        };
        if loop_defs.contains(&s0) {
            return None;
        }
        // The induction must start at 0 or 1 for the closed form to hold.
        let i0_val = if !is_float {
            match Self::const_int_value(f, i0) {
                Some(0) => 0,
                Some(1) => 1,
                _ => return None,
            }
        } else {
            0
        };
        let i0_float_val = if is_float {
            match Self::const_float_value(f, i0) {
                Some(0.0) => 0.0,
                Some(1.0) => 1.0,
                _ => return None,
            }
        } else {
            0.0
        };

        // The exit block must be reachable only through the loop: then every
        // value defined outside the loop and used inside it (in particular
        // `n`, `s0` and the accumulate operand) dominates the exit block,
        // which is where the closed form is computed.
        let exit_blk = f.get_block(exit)?;
        if !exit_blk.params.is_empty() {
            return None;
        }
        let mut exit_preds = 0;
        for b in &f.blocks {
            let targets_exit = match &b.terminator {
                Terminator::Branch { target, .. } => *target == exit,
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => *then_block == exit || *else_block == exit,
                _ => false,
            };
            if targets_exit {
                exit_preds += 1;
            }
        }
        if exit_preds != 1 {
            return None;
        }

        // Classify the accumulate operand.
        let term = if is_float {
            if x == p_i {
                SumTerm::FloatInduction {
                    scale: 1.0,
                    start: i0_float_val,
                    is_le,
                }
            } else if let Some(&k) = float_scaled_terms.get(&x) {
                SumTerm::FloatInduction {
                    scale: k,
                    start: i0_float_val,
                    is_le,
                }
            } else {
                let v = Self::const_float_value(f, x)?;
                SumTerm::FloatInvariantConst(v)
            }
        } else if x == p_i {
            SumTerm::Induction {
                scale: 1,
                start: i0_val,
                is_le,
            }
        } else if quadratic_terms.contains(&x) {
            SumTerm::Quadratic { scale: 1, is_le }
        } else if let Some(&k) = scaled_terms.get(&x) {
            SumTerm::Induction {
                scale: k,
                start: i0_val,
                is_le,
            }
        } else if let Some(v) = Self::const_int_value(f, x) {
            SumTerm::InvariantConst(v)
        } else if !loop_defs.contains(&x) {
            SumTerm::InvariantValue(x)
        } else {
            return None;
        };

        // Uses outside the loop: only the accumulator's may survive (rewritten
        // to the closed form), and only in blocks the exit dominates.
        if !Self::outside_uses_ok(f, cfg, lp, exit, &loop_defs, p_sum) {
            return None;
        }

        Some(FoldPlan {
            header,
            body,
            exit,
            preheader,
            p_sum,
            s0,
            n,
            term,
            is_float,
        })
    }

    /// The value of a `ConstFloat` definition, wherever it lives in the function.
    fn const_float_value(f: &Function, vid: ValueId) -> Option<f64> {
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::ConstFloat { dest, value } = inst
                    && *dest == vid
                {
                    return Some(*value);
                }
            }
        }
        None
    }

    /// The value of a `ConstInt` definition, wherever it lives in the function.
    fn const_int_value(f: &Function, vid: ValueId) -> Option<i64> {
        for b in &f.blocks {
            for inst in &b.instructions {
                if let Inst::ConstInt { dest, value } = inst
                    && *dest == vid
                {
                    return Some(*value);
                }
            }
        }
        None
    }

    /// A use of `v` outside the loop is allowed when it is the accumulator
    /// (whose value the closed form reproduces) living in a block the exit
    /// dominates, or when `v` is not defined by the loop at all.
    fn outside_use_allowed(
        v: &ValueId,
        loop_defs: &HashSet<ValueId>,
        p_sum: ValueId,
        exit: BasicBlockId,
        use_block: BasicBlockId,
        cfg: &ControlFlowGraph,
    ) -> bool {
        if *v == p_sum {
            return cfg.dominates(exit, use_block);
        }
        !loop_defs.contains(v)
    }

    fn outside_uses_ok(
        f: &Function,
        cfg: &ControlFlowGraph,
        lp: &crate::dmir::cfg::NaturalLoop,
        exit: BasicBlockId,
        loop_defs: &HashSet<ValueId>,
        p_sum: ValueId,
    ) -> bool {
        for b in &f.blocks {
            if lp.blocks.contains(&b.id) {
                continue;
            }
            // Legacy nested-instruction wrappers hide their operands from the
            // visitor below; their presence bails.
            for inst in &b.instructions {
                if matches!(inst, Inst::WhileLoop { .. } | Inst::TryCatch { .. }) {
                    return false;
                }
                let mut ok = true;
                Self::for_each_vid(inst, &mut |v: &ValueId| {
                    if !Self::outside_use_allowed(v, loop_defs, p_sum, exit, b.id, cfg) {
                        ok = false;
                    }
                });
                if !ok {
                    return false;
                }
            }
            let mut ok = true;
            let mut check = |v: &ValueId| {
                if !Self::outside_use_allowed(v, loop_defs, p_sum, exit, b.id, cfg) {
                    ok = false;
                }
            };
            match &b.terminator {
                Terminator::Branch { args, .. } => args.iter().for_each(&mut check),
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    check(cond);
                    then_args.iter().for_each(&mut check);
                    else_args.iter().for_each(&mut check);
                }
                Terminator::Return { value: Some(v) } => check(v),
                _ => {}
            }
            if !ok {
                return false;
            }
        }
        true
    }

    fn apply_fold(f: &mut Function, plan: &FoldPlan, trace: &mut OptimizationDecisionTrace) {
        fn push_const(insts: &mut Vec<Inst>, next: &mut usize, value: i64) -> ValueId {
            let dest = ValueId(*next);
            *next += 1;
            insts.push(Inst::ConstInt { dest, value });
            dest
        }
        fn push_bin(
            insts: &mut Vec<Inst>,
            next: &mut usize,
            op: &str,
            l: ValueId,
            r: ValueId,
            ty: &str,
        ) -> ValueId {
            let dest = ValueId(*next);
            *next += 1;
            insts.push(Inst::BinOp {
                dest,
                op: op.to_string(),
                left: l,
                right: r,
                ty: ty.to_string(),
            });
            dest
        }
        fn push_decide(
            insts: &mut Vec<Inst>,
            next: &mut usize,
            arms: Vec<(ValueId, ValueId)>,
            else_val: Option<ValueId>,
            ty: &str,
        ) -> ValueId {
            let dest = ValueId(*next);
            *next += 1;
            insts.push(Inst::Decide {
                dest,
                arms,
                else_val,
                ty: ty.to_string(),
            });
            dest
        }

        fn push_const_float(insts: &mut Vec<Inst>, next: &mut usize, value: f64) -> ValueId {
            let dest = ValueId(*next);
            *next += 1;
            insts.push(Inst::ConstFloat { dest, value });
            dest
        }

        let mut next = Self::max_vid(f) + 1;
        let mut insts: Vec<Inst> = Vec::new();

        let (_closed, s_final) = if plan.is_float {
            let c0 = push_const_float(&mut insts, &mut next, 0.0);
            let c1 = push_const_float(&mut insts, &mut next, 1.0);
            let c2 = push_const_float(&mut insts, &mut next, 2.0);

            let closed = match plan.term {
                SumTerm::FloatInduction {
                    scale,
                    start,
                    is_le,
                } => {
                    let use_plus_one = (start == 1.0 && is_le) || (start == 0.0 && is_le);
                    let adj_op = if use_plus_one { "+" } else { "-" };
                    let n_adj = push_bin(&mut insts, &mut next, adj_op, plan.n, c1, "Float");
                    let prod = push_bin(&mut insts, &mut next, "*", plan.n, n_adj, "Float");
                    let half = push_bin(&mut insts, &mut next, "/", prod, c2, "Float");
                    if scale != 1.0 {
                        let k_val = push_const_float(&mut insts, &mut next, scale);
                        push_bin(&mut insts, &mut next, "*", half, k_val, "Float")
                    } else {
                        half
                    }
                }
                SumTerm::FloatInvariantConst(c) => {
                    let cv = push_const_float(&mut insts, &mut next, c);
                    push_bin(&mut insts, &mut next, "*", plan.n, cv, "Float")
                }
                _ => unreachable!(),
            };

            let neg = push_bin(&mut insts, &mut next, "<=", plan.n, c0, "Bool");
            let total = push_bin(&mut insts, &mut next, "+", plan.s0, closed, "Float");
            let s_fin = push_decide(
                &mut insts,
                &mut next,
                vec![(neg, plan.s0)],
                Some(total),
                "Float",
            );
            (closed, s_fin)
        } else {
            let c0 = push_const(&mut insts, &mut next, 0);
            let c1 = push_const(&mut insts, &mut next, 1);
            let c2 = push_const(&mut insts, &mut next, 2);

            let closed = match plan.term {
                SumTerm::Induction {
                    scale,
                    start,
                    is_le,
                } => {
                    let use_plus_one = (start == 1 && is_le) || (start == 0 && is_le);
                    let adj_op = if use_plus_one { "+" } else { "-" };
                    let n_adj = push_bin(&mut insts, &mut next, adj_op, plan.n, c1, "Int");

                    let half_e = push_bin(&mut insts, &mut next, "/", plan.n, c2, "Int");
                    let prod_e = push_bin(&mut insts, &mut next, "*", half_e, n_adj, "Int");
                    let half_o = push_bin(&mut insts, &mut next, "/", n_adj, c2, "Int");
                    let prod_o = push_bin(&mut insts, &mut next, "*", half_o, plan.n, "Int");
                    let mod2 = push_bin(&mut insts, &mut next, "%", plan.n, c2, "Int");
                    let is_even = push_bin(&mut insts, &mut next, "==", mod2, c0, "Bool");
                    let unscaled = push_decide(
                        &mut insts,
                        &mut next,
                        vec![(is_even, prod_e)],
                        Some(prod_o),
                        "Int",
                    );

                    if scale != 1 {
                        let k_val = push_const(&mut insts, &mut next, scale);
                        push_bin(&mut insts, &mut next, "*", unscaled, k_val, "Int")
                    } else {
                        unscaled
                    }
                }
                SumTerm::Quadratic { scale, is_le } => {
                    let c6 = push_const(&mut insts, &mut next, 6);
                    let adj_op = if is_le { "+" } else { "-" };
                    let n_adj = push_bin(&mut insts, &mut next, adj_op, plan.n, c1, "Int");
                    let two_n = push_bin(&mut insts, &mut next, "*", plan.n, c2, "Int");
                    let two_n_adj = push_bin(&mut insts, &mut next, adj_op, two_n, c1, "Int");
                    let prod1 = push_bin(&mut insts, &mut next, "*", plan.n, n_adj, "Int");
                    let num = push_bin(&mut insts, &mut next, "*", prod1, two_n_adj, "Int");
                    let unscaled = push_bin(&mut insts, &mut next, "/", num, c6, "Int");
                    if scale != 1 {
                        let k_val = push_const(&mut insts, &mut next, scale);
                        push_bin(&mut insts, &mut next, "*", unscaled, k_val, "Int")
                    } else {
                        unscaled
                    }
                }
                SumTerm::InvariantValue(x) => {
                    push_bin(&mut insts, &mut next, "*", plan.n, x, "Int")
                }
                SumTerm::InvariantConst(v) => {
                    let cv = push_const(&mut insts, &mut next, v);
                    push_bin(&mut insts, &mut next, "*", plan.n, cv, "Int")
                }
                _ => unreachable!(),
            };

            let neg = push_bin(&mut insts, &mut next, "<", plan.n, c0, "Bool");
            let total = push_bin(&mut insts, &mut next, "+", plan.s0, closed, "Int");
            let s_fin = push_decide(
                &mut insts,
                &mut next,
                vec![(neg, plan.s0)],
                Some(total),
                "Int",
            );
            (closed, s_fin)
        };

        // Prepend the closed form to the exit block.
        {
            let exit_blk = f.get_block_mut(plan.exit).expect("exit block must exist");
            let tail = std::mem::take(&mut exit_blk.instructions);
            insts.extend(tail);
            exit_blk.instructions = insts;
        }

        // Redirect the preheader straight to the exit.
        {
            let ph = f
                .get_block_mut(plan.preheader)
                .expect("preheader block must exist");
            ph.terminator = Terminator::Branch {
                target: plan.exit,
                args: Vec::new(),
            };
        }

        // Delete the loop blocks, then rewrite the surviving uses of the
        // accumulator to the closed-form value.
        f.blocks
            .retain(|b| b.id != plan.header && b.id != plan.body);
        for b in f.blocks.iter_mut() {
            for inst in b.instructions.iter_mut() {
                Self::rewrite_uses(inst, plan.p_sum, s_final);
            }
            Self::rewrite_term_uses(&mut b.terminator, plan.p_sum, s_final);
        }

        let reason = match plan.term {
            SumTerm::Induction { .. } => {
                "countable while-loop: final value proven in wrapping arithmetic \
                 (parity-split Gaussian closed form); zero trips guarded by select"
            }
            SumTerm::Quadratic { .. } => {
                "countable while-loop with quadratic accumulation (sum += i*i): final value \
                 proven via sum of squares closed form n*(n-1)*(2n-1)/6; zero trips guarded by select"
            }
            SumTerm::FloatInduction { .. } => {
                "countable while-loop with Float induction: final value proven via analytical \
                 closed form; zero trips guarded by select"
            }
            SumTerm::FloatInvariantConst(_) => {
                "countable while-loop (Float sum += constant): final value proven == s0 + n*c; \
                 n <= 0 guarded by select"
            }
            SumTerm::InvariantValue(_) => {
                "countable while-loop (i from 0 step 1, i < n, sum += loop-invariant): final \
                 value proven == s0 + n*inv in wrapping arithmetic; n <= 0 guarded by select"
            }
            SumTerm::InvariantConst(_) => {
                "countable while-loop (i from 0 step 1, i < n, sum += constant): final value \
                 proven == s0 + n*c in wrapping arithmetic; n <= 0 guarded by select"
            }
        };
        trace.record(
            "LoopFold",
            &format!("{}:bb{}_loop_folded", f.name, plan.header.0),
            "Applied",
            "O(n) loop iterations replaced by O(1) closed-form arithmetic",
            "a handful of extra integer ops in the exit block",
            reason,
        );
    }

    /// Largest `ValueId` allocated inside this function. Ids are
    /// function-local, so cross-function collisions are irrelevant.
    fn max_vid(f: &Function) -> usize {
        let mut max = 0;
        for (_, _, v) in &f.params {
            max = max.max(v.0);
        }
        for b in &f.blocks {
            for p in &b.params {
                max = max.max(p.val.0);
            }
            for inst in &b.instructions {
                Self::for_each_vid(inst, &mut |v: &ValueId| max = max.max(v.0));
            }
            match &b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        max = max.max(a.0);
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    max = max.max(cond.0);
                    for a in then_args.iter().chain(else_args.iter()) {
                        max = max.max(a.0);
                    }
                }
                Terminator::Return { value: Some(v) } => max = max.max(v.0),
                _ => {}
            }
        }
        max
    }

    /// Rewrites every use of `from` to `to` inside one instruction.
    /// Definition positions are untouched: `from` is a block parameter of a
    /// deleted block, so no surviving instruction can define it.
    fn rewrite_uses(inst: &mut Inst, from: ValueId, to: ValueId) {
        match inst {
            Inst::BinOp { left, right, .. } => {
                if *left == from {
                    *left = to;
                }
                if *right == from {
                    *right = to;
                }
            }
            Inst::UnOp { operand, .. } => {
                if *operand == from {
                    *operand = to;
                }
            }
            Inst::Call { args, .. } => {
                for a in args {
                    if *a == from {
                        *a = to;
                    }
                }
            }
            Inst::MethodCall { object, args, .. } => {
                if *object == from {
                    *object = to;
                }
                for a in args {
                    if *a == from {
                        *a = to;
                    }
                }
            }
            Inst::StructInit { fields, .. } => {
                for (_, v) in fields {
                    if *v == from {
                        *v = to;
                    }
                }
            }
            Inst::GetField { object, .. } => {
                if *object == from {
                    *object = to;
                }
            }
            Inst::SetField { object, value, .. } => {
                if *object == from {
                    *object = to;
                }
                if *value == from {
                    *value = to;
                }
            }
            Inst::FormatStr { values, .. } => {
                for v in values {
                    if *v == from {
                        *v = to;
                    }
                }
            }
            Inst::Decide { arms, else_val, .. } => {
                for (c, v) in arms {
                    if *c == from {
                        *c = to;
                    }
                    if *v == from {
                        *v = to;
                    }
                }
                if let Some(v) = else_val
                    && *v == from
                {
                    *v = to;
                }
            }
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => {
                if *cond == from {
                    *cond = to;
                }
                if *then_val == from {
                    *then_val = to;
                }
                if *else_val == from {
                    *else_val = to;
                }
            }
            Inst::AssignVar { value, .. } => {
                if *value == from {
                    *value = to;
                }
            }
            Inst::Out { value } | Inst::Err { value } => {
                if *value == from {
                    *value = to;
                }
            }
            Inst::Return { value: Some(v) } if *v == from => {
                *v = to;
            }
            _ => {}
        }
    }

    fn rewrite_term_uses(term: &mut Terminator, from: ValueId, to: ValueId) {
        match term {
            Terminator::Branch { args, .. } => {
                for a in args {
                    if *a == from {
                        *a = to;
                    }
                }
            }
            Terminator::CondBranch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                if *cond == from {
                    *cond = to;
                }
                for a in then_args.iter_mut().chain(else_args.iter_mut()) {
                    if *a == from {
                        *a = to;
                    }
                }
            }
            Terminator::Return { value: Some(v) } => {
                if *v == from {
                    *v = to;
                }
            }
            Terminator::Unreachable => {}
            _ => {}
        }
    }

    /// Bounds Check Elimination (BCE) pass.
    /// Proves `idx < len` for list accesses inside a loop *before* removing the
    /// runtime bounds check. A proof requires ALL of:
    ///   1. the index is the loop induction variable `i`,
    ///   2. the loop is canonical: `i` starts at a constant 0, steps by +1 on
    ///      the back edge, and the header test is `i < bound` (strict — `<=`
    ///      allows `idx == bound`, which is out of range for `len == bound`),
    ///   3. the list operand resolves to a `datara_rt_list_create_repeat`
    ///      allocation whose count value is the same SSA value (or constant)
    ///      as the header bound.
    ///
    /// Anything unproven keeps the checked runtime call.
    pub fn bce_pass(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let mut eliminated = 0;
        let cfg = ControlFlowGraph::build(f);

        // --- Whole-function value lattice (order-insensitive and therefore
        // deliberately conservative: a second binding disqualifies a name). ---
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum LenVal {
            Const(i64),
            Vid(ValueId),
        }

        // Whole-function constant table (precomputed so later mutation
        // phases don't conflict with immutable borrows of `f`).
        let mut consts: HashMap<ValueId, i64> = HashMap::new();

        // value -> list length provenance; name -> defining value; value -> name.
        let mut list_len: HashMap<ValueId, LenVal> = HashMap::new();
        let mut val_to_name: HashMap<ValueId, String> = HashMap::new();
        let mut name_to_val: HashMap<String, ValueId> = HashMap::new();
        let mut copy_of: HashMap<ValueId, ValueId> = HashMap::new();
        let mut assigned: HashSet<String> = HashSet::new();

        for (p_name, _ty, p_val) in &f.params {
            name_to_val.insert(p_name.clone(), *p_val);
            val_to_name.insert(*p_val, p_name.clone());
        }

        for block in &f.blocks {
            for inst in &block.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        consts.insert(*dest, *value);
                    }
                    Inst::Call {
                        func, args, dest, ..
                    } if func == "datara_rt_list_create_repeat" && args.len() == 2 => {
                        list_len.insert(*dest, LenVal::Vid(args[1]));
                    }
                    Inst::AssignVar { name, value } => {
                        assigned.insert(name.clone());
                        if let Some(prev) = name_to_val.get(name) {
                            if *prev != *value {
                                // Rebinding: invalidate the value direction.
                                val_to_name.remove(prev);
                                name_to_val.remove(name);
                            }
                        } else {
                            name_to_val.insert(name.clone(), *value);
                            val_to_name.insert(*value, name.clone());
                        }
                    }
                    Inst::LoadVar { dest, name } => {
                        // Load dests carry the name forward for SSA chasing.
                        val_to_name.insert(*dest, name.clone());
                        if let Some(&v) = name_to_val.get(name) {
                            copy_of.insert(*dest, v);
                        }
                    }
                    Inst::UnOp {
                        dest, op, operand, ..
                    } if op == "copy" => {
                        copy_of.insert(*dest, *operand);
                    }
                    _ => {}
                }
            }
        }

        // Chase copy/name chains; a dead end is itself a valid identity key.
        fn resolve(
            mut v: ValueId,
            copy_of: &HashMap<ValueId, ValueId>,
            list_len: &HashMap<ValueId, LenVal>,
            consts: &HashMap<ValueId, i64>,
        ) -> LenVal {
            for _ in 0..32 {
                if let Some(l) = list_len.get(&v) {
                    return *l;
                }
                if let Some(c) = consts.get(&v) {
                    return LenVal::Const(*c);
                }
                match copy_of.get(&v) {
                    Some(&next) if next != v => v = next,
                    _ => return LenVal::Vid(v),
                }
            }
            LenVal::Vid(v)
        }

        // Resolve a value to the variable name it was loaded from.
        fn resolve_name(
            mut v: ValueId,
            copy_of: &HashMap<ValueId, ValueId>,
            val_to_name: &HashMap<ValueId, String>,
        ) -> Option<String> {
            for _ in 0..32 {
                if let Some(n) = val_to_name.get(&v) {
                    return Some(n.clone());
                }
                match copy_of.get(&v) {
                    Some(&next) if next != v => v = next,
                    _ => return None,
                }
            }
            None
        }

        let const_val = |vid: ValueId| -> Option<i64> { consts.get(&vid).copied() };

        // --- Evidence Gate Bounds Check Elimination (BCE) ---
        // Proves 0 <= idx < len(arr) from parameter refinement types (e.g. idx: Int in 0..<arr.len())
        // or contract preconditions (require 0 <= idx && idx < arr.len()).
        let mut proven_bounds: HashMap<String, String> = HashMap::new();

        for (p_name, refn_opt) in &f.param_refinements {
            if let Some(refn) = refn_opt {
                match refn {
                    Refinement::Range {
                        start,
                        end,
                        inclusive,
                    } => {
                        if !inclusive
                            && Self::is_zero_expr(start)
                            && let Some(arr_name) = Self::extract_len_target(end)
                        {
                            proven_bounds.insert(p_name.clone(), arr_name);
                        }
                    }
                    Refinement::Predicate {
                        var_name,
                        predicate,
                    } => {
                        if let Some(arr_name) =
                            Self::extract_predicate_len_target(var_name, predicate)
                        {
                            proven_bounds.insert(p_name.clone(), arr_name);
                        }
                    }
                }
            }
        }

        for req in &f.requires {
            if let Some((idx_name, arr_name)) =
                Self::extract_index_bound_from_contract(&req.condition)
            {
                proven_bounds.insert(idx_name, arr_name);
            }
        }

        let mut evidence_gate_count = 0;
        if !proven_bounds.is_empty() {
            for block in &mut f.blocks {
                for inst in &mut block.instructions {
                    if let Inst::Call { func, args, .. } = inst
                        && (func == "datara_rt_list_get" || func == "datara_rt_list_set")
                        && args.len() >= 2
                    {
                        let arr_name = resolve_name(args[0], &copy_of, &val_to_name);
                        let idx_name = resolve_name(args[1], &copy_of, &val_to_name);
                        if let (Some(arr), Some(idx)) = (arr_name, idx_name)
                            && proven_bounds.get(&idx) == Some(&arr)
                            && !assigned.contains(&idx)
                            && !assigned.contains(&arr)
                        {
                            *func = format!("{}_unchecked", func);
                            eliminated += 1;
                            evidence_gate_count += 1;
                        }
                    }
                }
            }
        }

        if evidence_gate_count > 0 {
            trace.record(
                "EvidenceGate:BCE",
                &format!("{}:param_refinement", f.name),
                "Applied",
                &format!("+{} proven unchecked access", evidence_gate_count),
                "0",
                &format!(
                    "Evidence Gate proved 0 <= idx < len(arr) via refinement/contract for {} accesses: bypassed runtime bounds check",
                    evidence_gate_count
                ),
            );
        }

        for lp in &cfg.loops {
            let header_block = match f.get_block(lp.header) {
                Some(b) => b,
                None => continue,
            };

            // Canonical header: `cond = (i < bound)`. `<=` is rejected: it
            // allows `idx == bound`, which is out of range when len == bound.
            let (induction_var, bound_val) = match &header_block.terminator {
                Terminator::CondBranch { cond, .. } => {
                    let mut found = None;
                    for inst in &header_block.instructions {
                        if let Inst::BinOp {
                            dest,
                            op,
                            left,
                            right,
                            ..
                        } = inst
                            && dest == cond
                            && op == "<"
                        {
                            found = Some((*left, *right));
                        }
                    }
                    match found {
                        Some(v) => v,
                        None => continue,
                    }
                }
                _ => continue,
            };

            // The induction value must be a load of a named counter.
            let counter_name = match resolve_name(induction_var, &copy_of, &val_to_name) {
                Some(n) => n,
                None => continue,
            };

            let loop_blocks: HashSet<_> = lp.blocks.iter().copied().collect();

            // Prove init-to-0 in the preheader, step-by-1 on the back edge,
            // and that the counter is never rebound to anything else in-loop.
            let mut init_zero = false;
            let mut step_one = false;
            let mut rebound = false;
            for block in &f.blocks {
                let in_loop = loop_blocks.contains(&block.id);
                for inst in &block.instructions {
                    match inst {
                        Inst::AssignVar { name, value } if name == &counter_name => {
                            let is_step = if in_loop {
                                // `store i = (i + 1)` — the only legal in-loop
                                // rebinding of the counter.
                                match Self::binop_add_one_source(f, *value) {
                                    Some(lhs) => {
                                        resolve_name(lhs, &copy_of, &val_to_name).as_deref()
                                            == Some(counter_name.as_str())
                                    }
                                    None => false,
                                }
                            } else {
                                false
                            };
                            if is_step {
                                step_one = true;
                            } else if in_loop {
                                rebound = true;
                            } else if const_val(*value) == Some(0) {
                                init_zero = true;
                            } else {
                                // A non-zero init: cannot prove [0, bound).
                                rebound = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !init_zero || !step_one || rebound {
                continue;
            }

            // Prove the header bound equals the indexed list's length.
            let bound_len = resolve(bound_val, &copy_of, &list_len, &consts);

            for &block_id in &lp.blocks {
                if let Some(block) = f.get_block_mut(block_id) {
                    for inst in &mut block.instructions {
                        if let Inst::Call { func, args, .. } = inst
                            && (func == "datara_rt_list_get" || func == "datara_rt_list_set")
                            && args.len() >= 2
                        {
                            // The index must be the counter variable.
                            if resolve_name(args[1], &copy_of, &val_to_name).as_deref()
                                != Some(counter_name.as_str())
                            {
                                continue;
                            }
                            let list_len_val = resolve(args[0], &copy_of, &list_len, &consts);
                            let proven = match (bound_len, list_len_val) {
                                // Same allocation-length value.
                                (LenVal::Vid(a), LenVal::Vid(b)) => a == b,
                                // Static bound below a static length.
                                (LenVal::Const(a), LenVal::Const(b)) => a <= b,
                                _ => false,
                            };
                            if proven {
                                *func = format!("{}_unchecked", func);
                                eliminated += 1;
                            }
                        }
                    }
                }
            }
        }

        if eliminated > 0 {
            trace.record(
                "BCE",
                &format!("{}:loop_bounds", f.name),
                "Applied",
                &format!("+{} proven unchecked access", eliminated),
                "0",
                &format!(
                    "Proved idx < len for {} accesses (canonical 0-based +1 loop bound tied to allocation length)",
                    eliminated
                ),
            );
        }

        eliminated
    }

    /// If `vid` is the dest of `a + 1` (BinOp), return the left operand.
    fn binop_add_one_source(f: &Function, vid: ValueId) -> Option<ValueId> {
        for block in &f.blocks {
            for inst in &block.instructions {
                if let Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ..
                } = inst
                    && *dest == vid
                    && op == "+"
                    && Self::const_int_value(f, *right) == Some(1)
                {
                    return Some(*left);
                }
            }
        }
        None
    }

    fn is_zero_expr(expr: &Expr) -> bool {
        matches!(expr, Expr::Literal(LiteralValue::Int(0), _))
    }

    fn extract_len_target(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, args, .. } if args.is_empty() => {
                if let Expr::MemberAccess { object, member, .. } = callee.as_ref()
                    && (member == "len" || member == "length")
                    && let Expr::Identifier(arr_name, _) = object.as_ref()
                {
                    return Some(arr_name.clone());
                }
                if let Expr::Identifier(fn_name, _) = callee.as_ref()
                    && (fn_name == "len" || fn_name == "length")
                    && let Some(Expr::Identifier(arr_name, _)) = args.first()
                {
                    return Some(arr_name.clone());
                }
                None
            }
            Expr::Call { callee, args, .. } if args.len() == 1 => {
                if let Expr::Identifier(fn_name, _) = callee.as_ref()
                    && (fn_name == "len" || fn_name == "length")
                    && let Some(Expr::Identifier(arr_name, _)) = args.first()
                {
                    return Some(arr_name.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn extract_predicate_len_target(var_name: &str, predicate: &Expr) -> Option<String> {
        match predicate {
            Expr::Binary {
                op, left, right, ..
            } if op == "&&" => {
                let left_ok = Self::is_non_negative_check(var_name, left);
                let right_target = Self::is_less_than_len_check(var_name, right);
                if left_ok && right_target.is_some() {
                    return right_target;
                }
                let right_ok = Self::is_non_negative_check(var_name, right);
                let left_target = Self::is_less_than_len_check(var_name, left);
                if right_ok && left_target.is_some() {
                    return left_target;
                }
                None
            }
            _ => None,
        }
    }

    fn is_non_negative_check(var_name: &str, expr: &Expr) -> bool {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == ">=" {
                    if let Expr::Identifier(name, _) = left.as_ref()
                        && name == var_name
                        && Self::is_zero_expr(right)
                    {
                        return true;
                    }
                } else if op == "<="
                    && let Expr::Identifier(name, _) = right.as_ref()
                    && name == var_name
                    && Self::is_zero_expr(left)
                {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn is_less_than_len_check(var_name: &str, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "<" => {
                if let Expr::Identifier(name, _) = left.as_ref()
                    && name == var_name
                {
                    return Self::extract_len_target(right);
                }
                None
            }
            _ => None,
        }
    }

    fn extract_index_bound_from_contract(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "&&" => {
                if let (Some(idx1), Some((idx2, arr))) = (
                    Self::extract_non_negative_var(left),
                    Self::extract_less_than_len(right),
                ) && idx1 == idx2
                {
                    return Some((idx1, arr));
                }
                if let (Some((idx1, arr)), Some(idx2)) = (
                    Self::extract_less_than_len(left),
                    Self::extract_non_negative_var(right),
                ) && idx1 == idx2
                {
                    return Some((idx1, arr));
                }
                None
            }
            _ => None,
        }
    }

    fn extract_non_negative_var(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                if op == ">=" {
                    if let Expr::Identifier(name, _) = left.as_ref()
                        && Self::is_zero_expr(right)
                    {
                        return Some(name.clone());
                    }
                } else if op == "<="
                    && let Expr::Identifier(name, _) = right.as_ref()
                    && Self::is_zero_expr(left)
                {
                    return Some(name.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn extract_less_than_len(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } if op == "<" => {
                if let Expr::Identifier(idx_name, _) = left.as_ref()
                    && let Some(arr_name) = Self::extract_len_target(right)
                {
                    return Some((idx_name.clone(), arr_name));
                }
                None
            }
            _ => None,
        }
    }
}
