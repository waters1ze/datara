use super::*;
use crate::dmir::{Function, Inst, ValueId};
use std::collections::HashMap;

impl Optimizer {
    pub(crate) fn constant_fold(&mut self, f: &mut Function) -> bool {
        let mut changed = false;
        let mut int_constants: HashMap<ValueId, i64> = HashMap::new();
        let mut str_constants: HashMap<ValueId, String> = HashMap::new();
        let mut bool_constants: HashMap<ValueId, bool> = HashMap::new();

        for block in &mut f.blocks {
            let mut block_var_ints: HashMap<String, i64> = HashMap::new();
            let mut block_var_strs: HashMap<String, String> = HashMap::new();
            let mut block_var_bools: HashMap<String, bool> = HashMap::new();
            let mut new_instructions = Vec::new();

            for inst in &block.instructions {
                match inst {
                    Inst::ConstInt { dest, value } => {
                        int_constants.insert(*dest, *value);
                        new_instructions.push(inst.clone());
                    }
                    Inst::ConstStr { dest, value } => {
                        str_constants.insert(*dest, value.clone());
                        new_instructions.push(inst.clone());
                    }
                    Inst::ConstBool { dest, value } => {
                        bool_constants.insert(*dest, *value);
                        new_instructions.push(inst.clone());
                    }
                    Inst::AssignVar { name, value } => {
                        if let Some(v) = int_constants.get(value) {
                            block_var_ints.insert(name.clone(), *v);
                        } else {
                            block_var_ints.remove(name);
                        }
                        if let Some(v) = str_constants.get(value) {
                            block_var_strs.insert(name.clone(), v.clone());
                        } else {
                            block_var_strs.remove(name);
                        }
                        if let Some(v) = bool_constants.get(value) {
                            block_var_bools.insert(name.clone(), *v);
                        } else {
                            block_var_bools.remove(name);
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::LoadVar { dest, name } => {
                        if let Some(v) = block_var_ints.get(name) {
                            int_constants.insert(*dest, *v);
                        }
                        if let Some(v) = block_var_strs.get(name) {
                            str_constants.insert(*dest, v.clone());
                        }
                        if let Some(v) = block_var_bools.get(name) {
                            bool_constants.insert(*dest, *v);
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::UnOp {
                        dest,
                        op,
                        operand,
                        ty: _,
                    } => {
                        if op == "copy" {
                            if let Some(i) = int_constants.get(operand).copied() {
                                int_constants.insert(*dest, i);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: i,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(s) = str_constants.get(operand).cloned() {
                                str_constants.insert(*dest, s.clone());
                                new_instructions.push(Inst::ConstStr {
                                    dest: *dest,
                                    value: s,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(b) = bool_constants.get(operand).copied() {
                                bool_constants.insert(*dest, b);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: b,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        } else if op == "-" {
                            if let Some(i) = int_constants.get(operand).copied() {
                                let res = i.wrapping_neg();
                                int_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        } else if op == "!" || op == "not" {
                            if let Some(b) = bool_constants.get(operand).copied() {
                                let res = !b;
                                bool_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::BinOp {
                        dest,
                        op,
                        left,
                        right,
                        ty: _,
                    } => {
                        if let (Some(l_val), Some(r_val)) =
                            (int_constants.get(left), int_constants.get(right))
                        {
                            let folded = match op.as_str() {
                                "+" => l_val.checked_add(*r_val),
                                "-" => l_val.checked_sub(*r_val),
                                "*" => l_val.checked_mul(*r_val),
                                "wrapping_+" => Some(l_val.wrapping_add(*r_val)),
                                "wrapping_-" => Some(l_val.wrapping_sub(*r_val)),
                                "wrapping_*" => Some(l_val.wrapping_mul(*r_val)),
                                "saturating_+" => Some(l_val.saturating_add(*r_val)),
                                "saturating_-" => Some(l_val.saturating_sub(*r_val)),
                                "saturating_*" => Some(l_val.saturating_mul(*r_val)),
                                "/" if *r_val != 0 && !(*l_val == i64::MIN && *r_val == -1) => {
                                    l_val.checked_div(*r_val)
                                }
                                "%" if *r_val != 0 && !(*l_val == i64::MIN && *r_val == -1) => {
                                    l_val.checked_rem(*r_val)
                                }
                                _ => None,
                            };
                            if let Some(res) = folded {
                                int_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }

                            let bool_folded = match op.as_str() {
                                "<" => Some(l_val < r_val),
                                "<=" => Some(l_val <= r_val),
                                ">" => Some(l_val > r_val),
                                ">=" => Some(l_val >= r_val),
                                "==" => Some(l_val == r_val),
                                "!=" => Some(l_val != r_val),
                                _ => None,
                            };
                            if let Some(res) = bool_folded {
                                bool_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }

                        if let (Some(l_val), Some(r_val)) =
                            (bool_constants.get(left), bool_constants.get(right))
                        {
                            let bool_folded = match op.as_str() {
                                "==" => Some(l_val == r_val),
                                "!=" => Some(l_val != r_val),
                                "&&" | "and" => Some(*l_val && *r_val),
                                "||" | "or" => Some(*l_val || *r_val),
                                _ => None,
                            };
                            if let Some(res) = bool_folded {
                                bool_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }

                        if let (Some(l_val), Some(r_val)) =
                            (str_constants.get(left), str_constants.get(right))
                        {
                            let bool_folded = match op.as_str() {
                                "==" => Some(l_val == r_val),
                                "!=" => Some(l_val != r_val),
                                _ => None,
                            };
                            if let Some(res) = bool_folded {
                                bool_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::Decide {
                        dest,
                        arms,
                        else_val,
                        ty: _,
                    } => {
                        let mut resolved: Option<ValueId> = None;
                        let mut can_resolve = true;
                        for (cond, val) in arms {
                            if let Some(b) = bool_constants.get(cond) {
                                if *b {
                                    resolved = Some(*val);
                                    break;
                                }
                            } else if let Some(i) = int_constants.get(cond) {
                                if *i != 0 {
                                    resolved = Some(*val);
                                    break;
                                }
                            } else {
                                can_resolve = false;
                                break;
                            }
                        }
                        if resolved.is_none() && can_resolve {
                            resolved = *else_val;
                        }

                        if let Some(res_vid) = resolved {
                            if let Some(ival) = int_constants.get(&res_vid).copied() {
                                int_constants.insert(*dest, ival);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: ival,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(sval) = str_constants.get(&res_vid).cloned() {
                                str_constants.insert(*dest, sval.clone());
                                new_instructions.push(Inst::ConstStr {
                                    dest: *dest,
                                    value: sval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(bval) = bool_constants.get(&res_vid).copied() {
                                bool_constants.insert(*dest, bval);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: bval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::Select {
                        dest,
                        cond,
                        then_val,
                        else_val,
                        ty: _,
                    } => {
                        if let Some(b) = bool_constants.get(cond) {
                            let chosen = if *b { *then_val } else { *else_val };
                            if let Some(ival) = int_constants.get(&chosen).copied() {
                                int_constants.insert(*dest, ival);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: ival,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(sval) = str_constants.get(&chosen).cloned() {
                                str_constants.insert(*dest, sval.clone());
                                new_instructions.push(Inst::ConstStr {
                                    dest: *dest,
                                    value: sval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            } else if let Some(bval) = bool_constants.get(&chosen).copied() {
                                bool_constants.insert(*dest, bval);
                                new_instructions.push(Inst::ConstBool {
                                    dest: *dest,
                                    value: bval,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::FormatStr {
                        dest,
                        parts,
                        values,
                    } => {
                        let all_known = values.iter().all(|v| {
                            int_constants.contains_key(v)
                                || str_constants.contains_key(v)
                                || bool_constants.contains_key(v)
                        });
                        if all_known {
                            let mut res_str = String::new();
                            for (i, p) in parts.iter().enumerate() {
                                res_str.push_str(p);
                                if i < values.len() {
                                    let v_id = &values[i];
                                    if let Some(c) = int_constants.get(v_id) {
                                        res_str.push_str(&c.to_string());
                                    } else if let Some(c) = str_constants.get(v_id) {
                                        res_str.push_str(c);
                                    } else if let Some(c) = bool_constants.get(v_id) {
                                        res_str.push_str(if *c { "true" } else { "false" });
                                    }
                                }
                            }
                            str_constants.insert(*dest, res_str.clone());
                            new_instructions.push(Inst::ConstStr {
                                dest: *dest,
                                value: res_str,
                            });
                            self.report.constants_folded += 1;
                            changed = true;
                            continue;
                        }
                        new_instructions.push(inst.clone());
                    }
                    Inst::Call {
                        dest,
                        func,
                        args,
                        ty: _,
                    } => {
                        let int_args: Vec<Option<i64>> =
                            args.iter().map(|a| int_constants.get(a).copied()).collect();
                        let all_int_consts =
                            !int_args.is_empty() && int_args.iter().all(|a| a.is_some());

                        if all_int_consts {
                            let vals: Vec<i64> = int_args.into_iter().map(|a| a.unwrap()).collect();
                            let folded: Option<i64> = match func.as_str() {
                                "abs" | "math_abs" if vals.len() == 1 => vals[0].checked_abs(),
                                "min" if vals.len() == 2 => Some(vals[0].min(vals[1])),
                                "max" if vals.len() == 2 => Some(vals[0].max(vals[1])),
                                _ => None,
                            };

                            if let Some(res) = folded {
                                int_constants.insert(*dest, res);
                                new_instructions.push(Inst::ConstInt {
                                    dest: *dest,
                                    value: res,
                                });
                                self.report.constants_folded += 1;
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(inst.clone());
                    }
                    _ => {
                        new_instructions.push(inst.clone());
                    }
                }
            }

            block.instructions = new_instructions;
        }

        changed
    }
}
