pub(crate) mod bce;
pub(crate) mod engine_v2;
pub(crate) mod fold;
pub mod polyhedral;

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
        bce_proven: &mut usize,
    ) -> usize {
        let mut transformed = 0;
        transformed += Self::licm_pass(f, cost_model, trace);
        transformed += Self::forward_invariants_pass(f, cost_model, trace);
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
        let clean = func
            .strip_prefix("datara_rt_math_")
            .or_else(|| func.strip_prefix("math_"))
            .unwrap_or(func);
        let pure_prefixes = [
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
            "round",
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
        pure_prefixes.iter().any(|p| clean.starts_with(p))
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
            // Integer division / modulo may trap on a zero divisor. Floating-point
            // division under IEEE-754 produces inf/nan and never traps on x86_64, so
            // float division is safe to hoist.
            Inst::BinOp { op, ty, .. } => {
                if op == "/" {
                    ty == "Float" || ty == "f64"
                } else {
                    op != "%"
                }
            }
            Inst::ConstInt { .. }
            | Inst::ConstFloat { .. }
            | Inst::ConstBool { .. }
            | Inst::ConstStr { .. } => true,
            // Plain copy and negation (arithmetic/float) are pure and non-trapping.
            Inst::UnOp { op, .. } => op == "copy" || op == "-",
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
            Inst::Select {
                cond,
                then_val,
                else_val,
                ..
            } => vec![*cond, *then_val, *else_val],
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

    /// Forwards loop-invariant values through loop header block parameters.
    ///
    /// When values computed before a loop (e.g. vector components, division results,
    /// or pure arithmetic) are used inside a loop, Cranelift's egraph elaboration may
    /// lazily sink their computation into the loop body (especially if any operand is
    /// a constant marked `remat`), re-evaluating high-latency instructions (like `fdiv`)
    /// on every iteration.
    ///
    /// By threading these invariants as block parameters through the loop header:
    /// 1. The preheader passes the invariant into the loop header, forcing its evaluation
    ///    before the loop.
    /// 2. The loop header receives the invariant as an SSA phi / block parameter.
    /// 3. The loop latch passes the parameter back around the backedge.
    /// 4. Inside the loop, instructions read the invariant directly from the block parameter
    ///    (which Cranelift maps to a callee-saved register).
    pub fn forward_invariants_pass(
        f: &mut Function,
        _cost_model: &CostModel,
        trace: &mut OptimizationDecisionTrace,
    ) -> usize {
        let cfg = ControlFlowGraph::build(f);
        if cfg.loops.is_empty() {
            return 0;
        }

        let mut val_ty: HashMap<ValueId, String> = HashMap::new();
        let mut def_cost: HashMap<ValueId, u32> = HashMap::new();

        for (_, p_type, p_val) in &f.params {
            val_ty.insert(*p_val, p_type.clone());
        }

        for blk in &f.blocks {
            for p in &blk.params {
                val_ty.insert(p.val, p.ty.clone());
            }
            for inst in &blk.instructions {
                match inst {
                    Inst::ConstInt { dest, .. } => {
                        val_ty.insert(*dest, "Int".into());
                    }
                    Inst::ConstFloat { dest, .. } => {
                        val_ty.insert(*dest, "Float".into());
                    }
                    Inst::ConstBool { dest, .. } => {
                        val_ty.insert(*dest, "Bool".into());
                    }
                    Inst::ConstStr { dest, .. } => {
                        val_ty.insert(*dest, "String".into());
                    }
                    Inst::BinOp { dest, op, ty, .. } => {
                        val_ty.insert(*dest, ty.clone());
                        if op == "/" {
                            def_cost.insert(*dest, 30);
                        } else if op == "*" {
                            def_cost.insert(*dest, 10);
                        }
                    }
                    Inst::GetField { dest, ty, .. }
                    | Inst::Select { dest, ty, .. }
                    | Inst::UnOp { dest, ty, .. } => {
                        val_ty.insert(*dest, ty.clone());
                    }
                    Inst::Call { dest, ty, .. } | Inst::MethodCall { dest, ty, .. } => {
                        val_ty.insert(*dest, ty.clone());
                    }
                    _ => {}
                }
            }
        }

        let mut max_vid = 0usize;
        let mut bump = |v: &ValueId| {
            if v.0 > max_vid {
                max_vid = v.0;
            }
        };
        for (_, _, v) in &f.params {
            bump(v);
        }
        for b in &f.blocks {
            for p in &b.params {
                bump(&p.val);
            }
            for inst in &b.instructions {
                Self::for_each_vid(inst, &mut bump);
            }
            match &b.terminator {
                Terminator::Branch { args, .. } => {
                    for a in args {
                        bump(a);
                    }
                }
                Terminator::CondBranch {
                    cond,
                    then_args,
                    else_args,
                    ..
                } => {
                    bump(cond);
                    for a in then_args.iter().chain(else_args.iter()) {
                        bump(a);
                    }
                }
                Terminator::Return { value } => {
                    if let Some(v) = value {
                        bump(v);
                    }
                }
                Terminator::Unreachable => {}
            }
        }

        let mut next_vid = max_vid + 1;
        let mut total_forwarded = 0;

        for lp in &cfg.loops {
            if lp.header == f.entry_block {
                continue;
            }

            let mut defined_in_loop: HashSet<ValueId> = HashSet::new();
            for &bid in &lp.blocks {
                if let Some(blk) = f.get_block(bid) {
                    for p in &blk.params {
                        defined_in_loop.insert(p.val);
                    }
                    for inst in &blk.instructions {
                        if let Some(d) = Self::dest(inst) {
                            defined_in_loop.insert(d);
                        }
                    }
                }
            }

            let header_param_vids: HashSet<ValueId> = f
                .get_block(lp.header)
                .map(|h| h.params.iter().map(|p| p.val).collect())
                .unwrap_or_default();

            let mut invariants_to_forward: Vec<ValueId> = Vec::new();
            let mut seen_invariants: HashSet<ValueId> = HashSet::new();

            for &bid in &lp.blocks {
                if let Some(blk) = f.get_block(bid) {
                    for inst in &blk.instructions {
                        for src in Self::source_operands(inst) {
                            if !defined_in_loop.contains(&src)
                                && def_cost.contains_key(&src)
                                && val_ty.contains_key(&src)
                                && !seen_invariants.contains(&src)
                                && !header_param_vids.contains(&src)
                            {
                                seen_invariants.insert(src);
                                invariants_to_forward.push(src);
                            }
                        }
                    }

                    if !lp.back_edges.contains(&bid) {
                        match &blk.terminator {
                            Terminator::Branch { args, .. } => {
                                for a in args {
                                    if !defined_in_loop.contains(a)
                                        && def_cost.contains_key(a)
                                        && val_ty.contains_key(a)
                                        && !seen_invariants.contains(a)
                                        && !header_param_vids.contains(a)
                                    {
                                        seen_invariants.insert(*a);
                                        invariants_to_forward.push(*a);
                                    }
                                }
                            }
                            Terminator::CondBranch {
                                cond,
                                then_args,
                                else_args,
                                ..
                            } => {
                                for a in std::iter::once(cond)
                                    .chain(then_args.iter())
                                    .chain(else_args.iter())
                                {
                                    if !defined_in_loop.contains(a)
                                        && def_cost.contains_key(a)
                                        && val_ty.contains_key(a)
                                        && !seen_invariants.contains(a)
                                        && !header_param_vids.contains(a)
                                    {
                                        seen_invariants.insert(*a);
                                        invariants_to_forward.push(*a);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Sort candidate invariants by cost descending (e.g. division before multiplication)
            invariants_to_forward.sort_by(|a, b| {
                let cost_a = def_cost.get(a).copied().unwrap_or(0);
                let cost_b = def_cost.get(b).copied().unwrap_or(0);
                cost_b.cmp(&cost_a)
            });

            // Cap invariants per loop to 3 to ensure zero register spilling on Windows x64
            if invariants_to_forward.len() > 3 {
                invariants_to_forward.truncate(3);
            }

            if invariants_to_forward.is_empty() {
                continue;
            }

            let mut subst: HashMap<ValueId, ValueId> = HashMap::new();
            let mut new_params: Vec<crate::dmir::BlockParam> = Vec::new();

            for &inv in &invariants_to_forward {
                let ty = val_ty[&inv].clone();
                let new_param_val = ValueId(next_vid);
                next_vid += 1;
                new_params.push(crate::dmir::BlockParam {
                    val: new_param_val,
                    ty,
                    name: None,
                });
                subst.insert(inv, new_param_val);
            }

            // 1. Add parameters to header
            if let Some(header_blk) = f.get_block_mut(lp.header) {
                header_blk.params.extend(new_params);
            }

            // 2. Update predecessors: entering edges pass `inv`, backedges pass `new_param_val`
            let preds: Vec<BasicBlockId> = cfg
                .predecessors
                .get(&lp.header)
                .cloned()
                .unwrap_or_default();

            for pred_id in preds {
                let is_backedge = lp.blocks.contains(&pred_id);
                if let Some(pred_blk) = f.get_block_mut(pred_id) {
                    if is_backedge {
                        Self::subst_term_operands(&mut pred_blk.terminator, &subst);
                    }

                    let mut backedge_args: Vec<ValueId> = Vec::new();
                    if is_backedge {
                        let bool_cond = ValueId(next_vid);
                        next_vid += 1;
                        pred_blk.instructions.push(Inst::ConstBool {
                            dest: bool_cond,
                            value: true,
                        });
                        for &inv in &invariants_to_forward {
                            let p_val = subst[&inv];
                            let ty = val_ty[&inv].clone();
                            let sel_dest = ValueId(next_vid);
                            next_vid += 1;
                            pred_blk.instructions.push(Inst::Select {
                                dest: sel_dest,
                                cond: bool_cond,
                                then_val: p_val,
                                else_val: p_val,
                                ty,
                            });
                            backedge_args.push(sel_dest);
                        }
                    }

                    match &mut pred_blk.terminator {
                        Terminator::Branch { target, args } if *target == lp.header => {
                            if is_backedge {
                                args.extend(backedge_args);
                            } else {
                                args.extend(invariants_to_forward.iter().copied());
                            }
                        }
                        Terminator::CondBranch {
                            then_block,
                            then_args,
                            else_block,
                            else_args,
                            ..
                        } => {
                            if *then_block == lp.header {
                                if is_backedge {
                                    then_args.extend(backedge_args.iter().copied());
                                } else {
                                    then_args.extend(invariants_to_forward.iter().copied());
                                }
                            }
                            if *else_block == lp.header {
                                if is_backedge {
                                    else_args.extend(backedge_args.iter().copied());
                                } else {
                                    else_args.extend(invariants_to_forward.iter().copied());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // 3. Substitute uses in all loop blocks
            for &bid in &lp.blocks {
                if let Some(blk) = f.get_block_mut(bid) {
                    for inst in &mut blk.instructions {
                        Self::subst_inst_operands(inst, &subst);
                    }
                    if !lp.back_edges.contains(&bid) {
                        Self::subst_term_operands(&mut blk.terminator, &subst);
                    }
                }
            }

            total_forwarded += invariants_to_forward.len();
            trace.record(
                "ForwardInvariants",
                &format!("{}:bb{}", f.name, lp.header.0),
                "Applied",
                "high",
                "zero",
                &format!(
                    "forwarded {} invariant(s) through loop header",
                    invariants_to_forward.len()
                ),
            );
        }

        total_forwarded
    }

    fn subst_inst_operands(inst: &mut Inst, subst: &HashMap<ValueId, ValueId>) {
        let map = |v: &mut ValueId| {
            if let Some(&replacement) = subst.get(v) {
                *v = replacement;
            }
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
                for (c, v) in arms {
                    map(c);
                    map(v);
                }
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
            Inst::InlineAsm { inputs, .. } => {
                for (_, v) in inputs {
                    map(v);
                }
            }
            Inst::Return { value } => {
                if let Some(v) = value {
                    map(v);
                }
            }
            Inst::WhileLoop { .. } | Inst::TryCatch { .. } => {}
        }
    }

    fn subst_term_operands(term: &mut Terminator, subst: &HashMap<ValueId, ValueId>) {
        let map = |v: &mut ValueId| {
            if let Some(&replacement) = subst.get(v) {
                *v = replacement;
            }
        };
        match term {
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
