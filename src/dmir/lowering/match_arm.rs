use crate::ast::*;
use crate::dmir::ir::*;

use super::Lowering;

/// How a match/decide/select arm's condition is produced during CFG-based
/// arm lowering. `Pattern` arms reuse the eager lowering's pattern logic.
pub(crate) enum ArmCond<'a> {
    Expr(&'a Expr),
    Pattern {
        val: ValueId,
        pattern: &'a Pattern,
        guard: Option<&'a Expr>,
    },
}

impl<'a> Lowering<'a> {
    pub(crate) fn lower_match_arm_cond(
        &mut self,
        val: ValueId,
        pattern: &Pattern,
        guard: Option<&Expr>,
        cur_block: &mut BasicBlockId,
    ) -> ValueId {
        let cond = match pattern {
            Pattern::Literal(lit, span) => {
                let lit_expr = Expr::Literal(lit.clone(), span.clone());
                if let Some(lit_val) = self.lower_expr(&lit_expr, cur_block) {
                    let eq_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::BinOp {
                            dest: eq_dest,
                            op: "==".into(),
                            left: val,
                            right: lit_val,
                            ty: "Bool".into(),
                        });
                    eq_dest
                } else {
                    val
                }
            }
            Pattern::Identifier(name, _) if name == "_" => {
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
            Pattern::Identifier(name, _) => {
                self.symbol_values.insert(name.clone(), val);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: name.clone(),
                        value: val,
                    });
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
            Pattern::Variant {
                enum_name,
                variant_name,
                bindings,
                ..
            } => {
                let expected_tag = if let Some(en) = enum_name {
                    self.enum_variant_tags
                        .get(&format!("{}.{}", en, variant_name))
                        .copied()
                } else {
                    self.enum_variant_tags.get(variant_name).copied()
                };

                if let Some(tag) = expected_tag {
                    let tag_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::GetField {
                            dest: tag_dest,
                            object: val,
                            field: "__tag".to_string(),
                            ty: "Int".to_string(),
                        });
                    let const_tag = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstInt {
                            dest: const_tag,
                            value: tag,
                        });
                    let eq_dest = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::BinOp {
                            dest: eq_dest,
                            op: "==".into(),
                            left: tag_dest,
                            right: const_tag,
                            ty: "Bool".into(),
                        });

                    for (idx, b_name) in bindings.iter().enumerate() {
                        let field_dest = self.next_val();
                        let field_name = format!("f{}", idx);
                        let field_ty = self
                            .class_field_types
                            .get(&field_name)
                            .cloned()
                            .unwrap_or_else(|| "Int".into());
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::GetField {
                                dest: field_dest,
                                object: val,
                                field: field_name,
                                ty: field_ty,
                            });
                        self.symbol_values.insert(b_name.clone(), field_dest);
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::AssignVar {
                                name: b_name.clone(),
                                value: field_dest,
                            });
                    }
                    eq_dest
                } else {
                    let true_val = self.next_val();
                    self.get_block_mut(*cur_block)
                        .instructions
                        .push(Inst::ConstBool {
                            dest: true_val,
                            value: true,
                        });
                    true_val
                }
            }
            _ => {
                let true_val = self.next_val();
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::ConstBool {
                        dest: true_val,
                        value: true,
                    });
                true_val
            }
        };

        if let Some(g) = guard
            && let Some(g_val) = self.lower_expr(g, cur_block)
        {
            let and_dest = self.next_val();
            self.get_block_mut(*cur_block)
                .instructions
                .push(Inst::BinOp {
                    dest: and_dest,
                    op: "&&".into(),
                    left: cond,
                    right: g_val,
                    ty: "Bool".into(),
                });
            return and_dest;
        }
        cond
    }

    /// CFG-based lowering for a `match` whose arms have block bodies.
    pub(crate) fn lower_match_arms_cfg(
        &mut self,
        val: ValueId,
        arms: &[MatchArm],
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let cfg_arms: Vec<(ArmCond<'_>, &Expr)> = arms
            .iter()
            .map(|arm| {
                (
                    ArmCond::Pattern {
                        val,
                        pattern: &arm.pattern,
                        guard: arm.guard.as_ref(),
                    },
                    &arm.body,
                )
            })
            .collect();
        self.lower_arm_conds_cfg(&cfg_arms, None, cur_block)
    }

    /// Lowers match/decide/select arms as real control flow: each arm body is
    /// lowered into its own block and its value stored to a temp, so block
    /// bodies with side effects only execute when their arm is selected.
    /// Control falls through the condition chain into the merge block, which
    /// falls through to whatever the surrounding lowering emits next (the same
    /// pattern as Stmt::If).
    pub(crate) fn lower_arm_conds_cfg(
        &mut self,
        arms: &[(ArmCond<'_>, &Expr)],
        else_body: Option<&Expr>,
        cur_block: &mut BasicBlockId,
    ) -> Option<ValueId> {
        let temp = format!("__match_val_{}", self.val_counter);
        // Zero-init so a fall-through (no arm taken, no else) yields 0.
        let zero = self.next_val();
        self.get_block_mut(*cur_block)
            .instructions
            .push(Inst::ConstInt {
                dest: zero,
                value: 0,
            });
        self.symbol_values.insert(temp.clone(), zero);
        self.get_block_mut(*cur_block)
            .instructions
            .push(Inst::AssignVar {
                name: temp.clone(),
                value: zero,
            });

        let mut arm_ends: Vec<BasicBlockId> = Vec::new();
        for (cond_src, body) in arms {
            let cond = match cond_src {
                ArmCond::Expr(cond_expr) => {
                    self.lower_expr(cond_expr, cur_block).unwrap_or_else(|| {
                        let t = self.next_val();
                        self.get_block_mut(*cur_block)
                            .instructions
                            .push(Inst::ConstBool {
                                dest: t,
                                value: true,
                            });
                        t
                    })
                }
                ArmCond::Pattern {
                    val,
                    pattern,
                    guard,
                } => self.lower_match_arm_cond(*val, pattern, *guard, cur_block),
            };
            let arm_id = self.create_block("match_arm");
            let next_id = self.create_block("match_next");
            self.get_block_mut(*cur_block).terminator = Terminator::CondBranch {
                cond,
                then_block: arm_id,
                then_args: Vec::new(),
                else_block: next_id,
                else_args: Vec::new(),
            };
            let mut body_block = arm_id;
            let body_val = self.lower_expr(body, &mut body_block);
            if let Some(bv) = body_val {
                self.symbol_values.insert(temp.clone(), bv);
                self.get_block_mut(body_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: temp.clone(),
                        value: bv,
                    });
            }
            if self.block_falls_through(body_block) {
                arm_ends.push(body_block);
            }
            *cur_block = next_id;
        }
        if let Some(eb) = else_body {
            // The else body lowers into the final "next" block, which only
            // runs when no arm matched — never into the merge itself, or it
            // would overwrite the selected arm's value.
            let else_val = self.lower_expr(eb, cur_block);
            if let Some(bv) = else_val {
                self.symbol_values.insert(temp.clone(), bv);
                self.get_block_mut(*cur_block)
                    .instructions
                    .push(Inst::AssignVar {
                        name: temp.clone(),
                        value: bv,
                    });
            }
        }

        // With an else body the last "next" block is the else block, so the
        // merge point is a fresh block; without one the fall-through block is
        // the merge. Arm blocks branch forward into it (back-fixup now that
        // its id is known).
        let merge_id = if else_body.is_some() {
            let m = self.create_block("match_merge");
            if self.block_falls_through(*cur_block) {
                self.get_block_mut(*cur_block).terminator = Terminator::Branch {
                    target: m,
                    args: Vec::new(),
                };
            }
            *cur_block = m;
            m
        } else {
            *cur_block
        };
        for b in arm_ends {
            self.get_block_mut(b).terminator = Terminator::Branch {
                target: merge_id,
                args: Vec::new(),
            };
        }
        let dest = self.next_val();
        self.get_block_mut(merge_id)
            .instructions
            .push(Inst::LoadVar {
                dest,
                name: temp.clone(),
            });
        Some(dest)
    }
}
