use super::*;
use crate::dmir::cfg::ControlFlowGraph;
use crate::dmir::{BasicBlockId, Function, Inst, Terminator, ValueId};
use crate::optimizer::cost_model::OptimizationDecisionTrace;
use std::collections::{HashMap, HashSet};

/// The accumulating term of a countable loop, resolved to a form that
/// survives the deletion of the loop blocks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SumTerm {
    /// `sum += i` or `sum += i * k` where `i` is the induction variable:
    /// closed form via parity-split Gaussian product.
    Induction { scale: i64, start: i64, is_le: bool },
    /// `sum += k * i + c`: closed form via parity-split Gaussian product plus linear offset.
    AffineInduction {
        scale: i64,
        offset: i64,
        start: i64,
        is_le: bool,
    },
    /// `sum += i * i` (quadratic sum): closed form via n*(n-1)*(2n-1)/6 or n*(n+1)*(2n+1)/6.
    Quadratic { scale: i64, is_le: bool },
    /// Float induction: `sum += i * k` for Float induction variable.
    FloatInduction { scale: f64, start: f64, is_le: bool },
    /// `sum += x` where `x` is defined outside the loop: closed form `trips*x`.
    InvariantValue {
        val: ValueId,
        start: i64,
        is_le: bool,
    },
    /// `sum += c`: closed form `trips*c`.
    InvariantConst { val: i64, start: i64, is_le: bool },
    /// Float constant `sum += c`: closed form `n*c`.
    FloatInvariantConst(f64),
    /// `sum += if i < threshold { step1 } else { step2 }`: closed form via piecewise linear domain integration.
    PiecewiseLinear {
        threshold: ValueId,
        step1: i64,
        step2: i64,
        start_i: i64,
        is_le: bool,
        cmp_is_le: bool,
    },
}

pub(crate) struct FoldPlan {
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
    pub(crate) fn match_countable_loop(
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
        // Float induction/invariant-constant folding is disabled pending
        // bit-exactness analysis: the closed forms are not bit-exact with
        // IEEE-754 sequential addition.
        if is_float {
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
        // For `<=` the bound itself is the last tested value. When it is
        // i64::MAX the induction variable wraps (MAX + 1 == MIN, and MIN is
        // still <= MAX), so the original loop never terminates and the
        // closed form is invalid.
        if is_le && Self::const_int_value(f, n) == Some(i64::MAX) {
            return None;
        }
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

        // Back edge: both additions are passed to the header, each in the
        // parameter slot of the value it redefines.
        let args = match &body_blk.terminator {
            Terminator::Branch { target, args } if *target == header => args,
            _ => return None,
        };
        let idx_sum = header_blk.params.iter().position(|p| p.val == p_sum)?;
        let idx_i = header_blk.params.iter().position(|p| p.val == p_i)?;
        if args.len() != 2 {
            return None;
        }
        let expected_sum = args[idx_sum];
        let expected_i = args[idx_i];

        let mut affine_terms: HashMap<ValueId, (i64, i64)> = HashMap::new();
        for inst in &body_blk.instructions {
            if let Inst::BinOp {
                dest,
                op,
                left,
                right,
                ..
            } = inst
            {
                if *dest == expected_i || *dest == expected_sum {
                    continue;
                }
                if op == "+" || op == "wrapping_+" {
                    if let Some(&k) = scaled_terms.get(left) {
                        if let Some(c) = Self::const_int_value(f, *right) {
                            affine_terms.insert(*dest, (k, c));
                        }
                    } else if let Some(&k) = scaled_terms.get(right) {
                        if let Some(c) = Self::const_int_value(f, *left) {
                            affine_terms.insert(*dest, (k, c));
                        }
                    } else if *left == p_i {
                        if let Some(c) = Self::const_int_value(f, *right) {
                            affine_terms.insert(*dest, (1, c));
                        }
                    } else if *right == p_i {
                        if let Some(c) = Self::const_int_value(f, *left) {
                            affine_terms.insert(*dest, (1, c));
                        }
                    }
                } else if op == "-" || op == "wrapping_-" {
                    if let Some(&k) = scaled_terms.get(left) {
                        if let Some(c) = Self::const_int_value(f, *right) {
                            affine_terms.insert(*dest, (k, -c));
                        }
                    } else if *left == p_i {
                        if let Some(c) = Self::const_int_value(f, *right) {
                            affine_terms.insert(*dest, (1, -c));
                        }
                    }
                }
            }
        }

        let mut acc: Option<(ValueId, ValueId)> = None; // (sum_next, operand)
        let mut inc: Option<ValueId> = None; // i_next
        let mut piecewise: Option<(ValueId, ValueId, i64, i64, bool)> = None; // (sum_next, threshold_vid, step1, step2, cmp_is_le)

        let mut all_standard = true;
        for inst in &body_blk.instructions {
            match inst {
                Inst::ConstInt { .. } if !is_float => {}
                Inst::ConstFloat { .. } if is_float => {}
                Inst::BinOp { op, dest, .. } if op == "*" => {
                    if !scaled_terms.contains_key(dest)
                        && !quadratic_terms.contains(dest)
                        && !float_scaled_terms.contains_key(dest)
                    {
                        all_standard = false;
                        break;
                    }
                }
                Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ..
                } if op == "+" || op == "wrapping_+" => {
                    if *dest == expected_i && inc.is_none() {
                        if (!is_float
                            && ((*left == p_i && Self::const_int_value(f, *right) == Some(1))
                                || (*right == p_i && Self::const_int_value(f, *left) == Some(1))))
                            || (is_float
                                && ((*left == p_i
                                    && Self::const_float_value(f, *right) == Some(1.0))
                                    || (*right == p_i
                                        && Self::const_float_value(f, *left) == Some(1.0))))
                        {
                            inc = Some(*dest);
                        } else {
                            all_standard = false;
                            break;
                        }
                    } else if *dest == expected_sum && acc.is_none() {
                        if *left == p_sum {
                            acc = Some((*dest, *right));
                        } else if *right == p_sum {
                            acc = Some((*dest, *left));
                        } else {
                            all_standard = false;
                            break;
                        }
                    } else if affine_terms.contains_key(dest) {
                        // Standard affine term instruction
                    } else if *left == p_sum && acc.is_none() {
                        acc = Some((*dest, *right));
                    } else if *right == p_sum && acc.is_none() {
                        acc = Some((*dest, *left));
                    } else if *left == p_i && inc.is_none() {
                        if (!is_float && Self::const_int_value(f, *right) == Some(1))
                            || (is_float && Self::const_float_value(f, *right) == Some(1.0))
                        {
                            inc = Some(*dest);
                        } else {
                            all_standard = false;
                            break;
                        }
                    } else if *right == p_i && inc.is_none() {
                        if (!is_float && Self::const_int_value(f, *left) == Some(1))
                            || (is_float && Self::const_float_value(f, *left) == Some(1.0))
                        {
                            inc = Some(*dest);
                        } else {
                            all_standard = false;
                            break;
                        }
                    } else {
                        all_standard = false;
                        break;
                    }
                }
                Inst::BinOp { dest, op, .. }
                    if (op == "-" || op == "wrapping_-") && affine_terms.contains_key(dest) => {}
                _ => {
                    all_standard = false;
                    break;
                }
            }
        }

        if (!all_standard || acc.is_none() || inc.is_none()) && !is_float {
            acc = None;
            inc = None;
            for inst in &body_blk.instructions {
                if let Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ..
                } = inst
                    && (op == "+" || op == "wrapping_+")
                    && ((*left == p_i && Self::const_int_value(f, *right) == Some(1))
                        || (*right == p_i && Self::const_int_value(f, *left) == Some(1)))
                {
                    inc = Some(*dest);
                    break;
                }
            }

            let mut pw_cmp: Option<(ValueId, ValueId, bool)> = None;
            for inst in &body_blk.instructions {
                if let Inst::BinOp {
                    dest,
                    op,
                    left,
                    right,
                    ty,
                } = inst
                    && (op == "<" || op == "<=")
                    && *left == p_i
                    && ty == "Int"
                {
                    pw_cmp = Some((*dest, *right, op == "<="));
                    break;
                }
            }

            if let Some((cmp_dest, threshold_vid, cmp_is_le)) = pw_cmp {
                let mut pw_select: Option<(ValueId, ValueId, ValueId)> = None;
                for inst in &body_blk.instructions {
                    if let Inst::Select {
                        dest,
                        cond,
                        then_val,
                        else_val,
                        ..
                    } = inst
                        && *cond == cmp_dest
                    {
                        pw_select = Some((*dest, *then_val, *else_val));
                        break;
                    }
                }

                if let Some((sel_dest, then_val, else_val)) = pw_select {
                    let find_add = |v: ValueId| -> Option<i64> {
                        for inst in &body_blk.instructions {
                            if let Inst::BinOp {
                                dest,
                                op,
                                left,
                                right,
                                ..
                            } = inst
                                && *dest == v
                                && (op == "+" || op == "wrapping_+")
                            {
                                if *left == p_sum {
                                    return Self::const_int_value(f, *right);
                                } else if *right == p_sum {
                                    return Self::const_int_value(f, *left);
                                }
                            }
                        }
                        None
                    };

                    if let (Some(s1), Some(s2)) = (find_add(then_val), find_add(else_val)) {
                        piecewise = Some((sel_dest, threshold_vid, s1, s2, cmp_is_le));
                    } else {
                        let s1 = Self::const_int_value(f, then_val);
                        let s2 = Self::const_int_value(f, else_val);
                        if let (Some(s1), Some(s2)) = (s1, s2) {
                            for inst in &body_blk.instructions {
                                if let Inst::BinOp {
                                    dest,
                                    op,
                                    left,
                                    right,
                                    ..
                                } = inst
                                    && (op == "+" || op == "wrapping_+")
                                    && ((*left == p_sum && *right == sel_dest)
                                        || (*right == p_sum && *left == sel_dest))
                                {
                                    piecewise = Some((*dest, threshold_vid, s1, s2, cmp_is_le));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let sum_next = if let Some((sn, ..)) = piecewise {
            sn
        } else if let Some((sn, _)) = acc {
            sn
        } else {
            return None;
        };
        let i_next = inc?;

        // Back edge: both additions must match the values passed to the header.
        if sum_next != expected_sum || i_next != expected_i {
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
        if let Some((_, threshold_vid, ..)) = piecewise {
            if loop_defs.contains(&threshold_vid) {
                return None;
            }
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
        let term = if let Some((_, threshold_vid, s1, s2, cmp_is_le)) = piecewise {
            SumTerm::PiecewiseLinear {
                threshold: threshold_vid,
                step1: s1,
                step2: s2,
                start_i: i0_val,
                is_le,
                cmp_is_le,
            }
        } else if is_float {
            let (_, x) = acc?;
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
        } else {
            let (_, x) = acc?;
            if x == p_i {
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
            } else if let Some(&(k, c)) = affine_terms.get(&x) {
                SumTerm::AffineInduction {
                    scale: k,
                    offset: c,
                    start: i0_val,
                    is_le,
                }
            } else if let Some(v) = Self::const_int_value(f, x) {
                SumTerm::InvariantConst {
                    val: v,
                    start: i0_val,
                    is_le,
                }
            } else if !loop_defs.contains(&x) {
                SumTerm::InvariantValue {
                    val: x,
                    start: i0_val,
                    is_le,
                }
            } else {
                return None;
            }
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
    pub(crate) fn const_int_value(f: &Function, vid: ValueId) -> Option<i64> {
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

    pub(crate) fn apply_fold(
        f: &mut Function,
        plan: &FoldPlan,
        trace: &mut OptimizationDecisionTrace,
    ) {
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
                _ => c0,
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
                SumTerm::AffineInduction {
                    scale,
                    offset,
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

                    let gauss_part = if scale != 1 {
                        let k_val = push_const(&mut insts, &mut next, scale);
                        push_bin(&mut insts, &mut next, "*", unscaled, k_val, "Int")
                    } else {
                        unscaled
                    };

                    let trips = if start == 0 && !is_le {
                        plan.n
                    } else {
                        let start_v = push_const(&mut insts, &mut next, start);
                        let diff = push_bin(&mut insts, &mut next, "-", plan.n, start_v, "Int");
                        if is_le {
                            push_bin(&mut insts, &mut next, "+", diff, c1, "Int")
                        } else {
                            diff
                        }
                    };
                    let off_val = push_const(&mut insts, &mut next, offset);
                    let off_part = push_bin(&mut insts, &mut next, "*", trips, off_val, "Int");

                    push_bin(&mut insts, &mut next, "+", gauss_part, off_part, "Int")
                }
                SumTerm::InvariantValue { val, start, is_le } => {
                    let trips = if start == 0 && !is_le {
                        plan.n
                    } else {
                        let start_v = push_const(&mut insts, &mut next, start);
                        let diff = push_bin(&mut insts, &mut next, "-", plan.n, start_v, "Int");
                        if is_le {
                            push_bin(&mut insts, &mut next, "+", diff, c1, "Int")
                        } else {
                            diff
                        }
                    };
                    push_bin(&mut insts, &mut next, "*", trips, val, "Int")
                }
                SumTerm::InvariantConst { val, start, is_le } => {
                    let trips = if start == 0 && !is_le {
                        plan.n
                    } else {
                        let start_v = push_const(&mut insts, &mut next, start);
                        let diff = push_bin(&mut insts, &mut next, "-", plan.n, start_v, "Int");
                        if is_le {
                            push_bin(&mut insts, &mut next, "+", diff, c1, "Int")
                        } else {
                            diff
                        }
                    };
                    let cv = push_const(&mut insts, &mut next, val);
                    push_bin(&mut insts, &mut next, "*", trips, cv, "Int")
                }
                SumTerm::PiecewiseLinear {
                    threshold,
                    step1,
                    step2,
                    start_i,
                    is_le,
                    cmp_is_le,
                } => {
                    let start_v = push_const(&mut insts, &mut next, start_i);
                    let raw_trips = push_bin(&mut insts, &mut next, "-", plan.n, start_v, "Int");
                    let total_t = if is_le {
                        push_bin(&mut insts, &mut next, "+", raw_trips, c1, "Int")
                    } else {
                        raw_trips
                    };
                    let is_t_pos = push_bin(&mut insts, &mut next, ">", total_t, c0, "Bool");
                    let t_clamped = push_decide(
                        &mut insts,
                        &mut next,
                        vec![(is_t_pos, total_t)],
                        Some(c0),
                        "Int",
                    );

                    let raw_k = push_bin(&mut insts, &mut next, "-", threshold, start_v, "Int");
                    let adj_k = if cmp_is_le {
                        push_bin(&mut insts, &mut next, "+", raw_k, c1, "Int")
                    } else {
                        raw_k
                    };
                    let is_k_pos = push_bin(&mut insts, &mut next, ">", adj_k, c0, "Bool");
                    let k_clamped = push_decide(
                        &mut insts,
                        &mut next,
                        vec![(is_k_pos, adj_k)],
                        Some(c0),
                        "Int",
                    );

                    let is_t_smaller =
                        push_bin(&mut insts, &mut next, "<", t_clamped, k_clamped, "Bool");
                    let t1 = push_decide(
                        &mut insts,
                        &mut next,
                        vec![(is_t_smaller, t_clamped)],
                        Some(k_clamped),
                        "Int",
                    );
                    let t2 = push_bin(&mut insts, &mut next, "-", t_clamped, t1, "Int");

                    let s1_val = push_const(&mut insts, &mut next, step1);
                    let part1 = push_bin(&mut insts, &mut next, "*", t1, s1_val, "Int");

                    let s2_val = push_const(&mut insts, &mut next, step2);
                    let part2 = push_bin(&mut insts, &mut next, "*", t2, s2_val, "Int");

                    push_bin(&mut insts, &mut next, "+", part1, part2, "Int")
                }
                _ => c0,
            };

            let neg = match plan.term {
                SumTerm::PiecewiseLinear { .. } => {
                    push_bin(&mut insts, &mut next, "<", plan.n, c0, "Bool")
                }
                SumTerm::Induction { start, is_le, .. }
                | SumTerm::AffineInduction { start, is_le, .. }
                | SumTerm::InvariantConst { start, is_le, .. }
                | SumTerm::InvariantValue { start, is_le, .. } => {
                    let min_n = if is_le { start - 1 } else { start };
                    if min_n == 0 {
                        push_bin(&mut insts, &mut next, "<=", plan.n, c0, "Bool")
                    } else {
                        let min_v = push_const(&mut insts, &mut next, min_n);
                        push_bin(&mut insts, &mut next, "<=", plan.n, min_v, "Bool")
                    }
                }
                _ => push_bin(&mut insts, &mut next, "<=", plan.n, c0, "Bool"),
            };
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
            SumTerm::AffineInduction { .. } => {
                "countable while-loop with affine induction (sum += k*i + c): final value \
                 proven via parity-split Gaussian closed form and linear offset; zero trips guarded by select"
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
            SumTerm::InvariantValue { .. } => {
                "countable while-loop (sum += loop-invariant): final value \
                 proven == s0 + trips*inv in wrapping arithmetic; zero trips guarded by select"
            }
            SumTerm::InvariantConst { .. } => {
                "countable while-loop (sum += constant): final value \
                 proven == s0 + trips*c in wrapping arithmetic; zero trips guarded by select"
            }
            SumTerm::PiecewiseLinear { .. } => {
                "countable while-loop with piecewise step (if i < K { +c1 } else { +c2 }): \
                 final value proven via analytical domain splitting; zero trips guarded by select"
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
    pub(crate) fn max_vid(f: &Function) -> usize {
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
    pub(crate) fn rewrite_uses(inst: &mut Inst, from: ValueId, to: ValueId) {
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
}
