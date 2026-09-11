pub(crate) mod bce;
pub(crate) mod engine_v2;
pub(crate) mod fold;

use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::{CostModel, OptimizationDecisionTrace};
use std::collections::HashSet;

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
        bce_proven: &mut usize,
    ) -> usize {
        let mut transformed = 0;
        transformed += Self::licm_pass(f, cost_model, trace);
        let bce_count = Self::bce_pass(f, cost_model, trace);
        *bce_proven += bce_count;
        transformed += bce_count;
        let v2_count = engine_v2::LoopEngineV2::optimize(f, cost_model, trace);
        transformed += v2_count;
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

    pub fn is_pure_call(func: &str) -> bool {
        let pure_prefixes = [
            "math_",
            "abs",
            "min",
            "max",
            "sqrt",
            "pow",
            "hypot",
            "sin",
            "cos",
            "tan",
            "floor",
            "ceil",
            "log",
            "exp",
            "fib",
            "calc",
            "compute",
            "factorial",
            "gcd",
            "lcm",
            "float4",
            "float8",
            "dot",
            "pure_",
        ];
        pure_prefixes.iter().any(|p| func.starts_with(p))
    }

    /// Pure instructions whose operands are loop-invariant may be hoisted.
    /// Everything else (non-pure calls, I/O, stores, control flow) stays in the loop.
    fn gather_loop_facts(f: &Function, loop_blocks: &HashSet<BasicBlockId>) -> LoopFacts {
        let mut facts = LoopFacts {
            assigned: HashSet::new(),
            may_alias: false,
        };
        fn inspect_inst(inst: &Inst, facts: &mut LoopFacts) {
            match inst {
                Inst::AssignVar { name, .. } => {
                    facts.assigned.insert(name.clone());
                }
                Inst::Call { func, .. } => {
                    if !LoopOptimizer::is_pure_call(func) {
                        facts.may_alias = true;
                    }
                }
                Inst::MethodCall { .. } | Inst::SetField { .. } => {
                    facts.may_alias = true;
                }
                Inst::WhileLoop {
                    condition_insts,
                    body_insts,
                    ..
                } => {
                    for i in condition_insts.iter().chain(body_insts.iter()) {
                        inspect_inst(i, facts);
                    }
                }
                Inst::TryCatch {
                    try_insts,
                    catch_insts,
                    ..
                } => {
                    for i in try_insts.iter().chain(catch_insts.iter()) {
                        inspect_inst(i, facts);
                    }
                }
                _ => {}
            }
        }
        for &bid in loop_blocks {
            if let Some(blk) = f.get_block(bid) {
                for inst in &blk.instructions {
                    inspect_inst(inst, &mut facts);
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
            // GetField dereferences its object: hoisting it out of a zero-trip
            // loop could introduce a fault the original never had. Stay
            // conservative and never hoist it.
            Inst::GetField { .. } => false,
            // Division / modulo may trap on a zero divisor. Moving one out of a
            // loop that may run zero times can introduce a fault the original
            // program never had, so those stay where they are.
            Inst::BinOp { op, .. } => op != "/" && op != "%",
            Inst::ConstInt { .. }
            | Inst::ConstFloat { .. }
            | Inst::ConstBool { .. }
            | Inst::ConstStr { .. } => true,
            // Only a plain copy is known non-trapping; other unops are not
            // proven pure.
            Inst::UnOp { op, .. } => op == "copy",
            Inst::Call { func, .. } => Self::is_pure_call(func),
            Inst::InlineAsm { options, .. } => options.iter().any(|o| o == "pure"),
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
            Inst::InlineAsm { inputs, .. } => inputs.iter().map(|(_, v)| *v).collect(),
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
            Inst::InlineAsm { outputs, .. } => outputs.first().map(|(_, d)| *d),
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
                if lp.header == f.entry_block {
                    continue;
                }

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

            let (apply, benefit, cost, reason) = cost_model.evaluate_licm("loop_invariant", true);
            if !apply {
                break;
            }

            let preheader = match Self::ensure_preheader(f, lp.header, &lp.blocks) {
                Some(ph) => ph,
                None => break,
            };

            let count = hoisted.len();

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

            trace.record(
                "LICM",
                &format!("{}:bb{}_to_bb{}", f.name, preheader.0, lp.header.0),
                "Applied",
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
    ) -> Option<BasicBlockId> {
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

        if entering.is_empty() {
            return None;
        }

        if entering.len() == 1
            && let Some(b) = f.get_block(entering[0])
            && matches!(&b.terminator, Terminator::Branch { .. })
        {
            return Some(entering[0]);
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
        let mut fresh = Self::max_vid(f) + 1;
        let mut forwarded_args = Vec::with_capacity(header_params.len());
        let mut preheader_params = Vec::with_capacity(header_params.len());
        for p in &header_params {
            let v = ValueId(fresh);
            fresh += 1;
            forwarded_args.push(v);
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

        Some(new_id)
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
            Inst::InlineAsm {
                outputs, inputs, ..
            } => {
                for (_, d) in outputs {
                    f(d);
                }
                for (_, i) in inputs {
                    f(i);
                }
            }
            Inst::Out { value } | Inst::Err { value } => f(value),
            Inst::Return { value: Some(v) } => f(v),
            Inst::Return { value: None } => {}
            Inst::WhileLoop {
                condition_insts,
                cond_val,
                body_insts,
            } => {
                for i in condition_insts {
                    Self::for_each_vid(i, f);
                }
                f(cond_val);
                for i in body_insts {
                    Self::for_each_vid(i, f);
                }
            }
            Inst::TryCatch {
                try_insts,
                catch_insts,
                ..
            } => {
                for i in try_insts {
                    Self::for_each_vid(i, f);
                }
                for i in catch_insts {
                    Self::for_each_vid(i, f);
                }
            }
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
