use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::types::{DataraType, TypeChecker};

impl<'a> TypeChecker<'a> {
    pub(crate) fn check_binary(
        &mut self,
        op: &str,
        left: &Box<Expr>,
        right: &Box<Expr>,
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) -> DataraType {
        let lt = self.check_expr(left, diag);
        let rt = self.check_expr(right, diag);

        // --- Units of Measure Dimensional Analysis ---
        if let (
            DataraType::Measure { base: b1, unit: u1 },
            DataraType::Measure { base: b2, unit: u2 },
        ) = (&lt, &rt)
        {
            let base = if **b1 == DataraType::Float || **b2 == DataraType::Float {
                DataraType::Float
            } else {
                DataraType::Int
            };
            match op {
                "+" | "-" => {
                    if u1 != u2 {
                        diag.error(
                                    ErrorCode::DimensionMismatch,
                                    format!(
                                        "Cannot perform '{}' on incompatible units of measure '{}' and '{}'",
                                        op, u1, u2
                                    ),
                                    Some(span.clone()),
                                );
                    }
                    return DataraType::Measure {
                        base: Box::new(base),
                        unit: u1.clone(),
                    };
                }
                "*" => {
                    let unit = format!("{}*{}", u1, u2);
                    return DataraType::Measure {
                        base: Box::new(base),
                        unit,
                    };
                }
                "/" => {
                    if u1 == u2 {
                        return base;
                    } else {
                        let unit = format!("{}/{}", u1, u2);
                        return DataraType::Measure {
                            base: Box::new(base),
                            unit,
                        };
                    }
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    if u1 != u2 {
                        diag.error(
                            ErrorCode::DimensionMismatch,
                            format!(
                                "Cannot compare incompatible units of measure '{}' and '{}'",
                                u1, u2
                            ),
                            Some(span.clone()),
                        );
                    }
                    return DataraType::Bool;
                }
                _ => {}
            }
        } else if let DataraType::Measure { base, unit } = &lt {
            match op {
                "*" => {
                    let b = if **base == DataraType::Float || rt == DataraType::Float {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    return DataraType::Measure {
                        base: Box::new(b),
                        unit: unit.clone(),
                    };
                }
                "/" => {
                    let b = if **base == DataraType::Float || rt == DataraType::Float {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    return DataraType::Measure {
                        base: Box::new(b),
                        unit: unit.clone(),
                    };
                }
                "+" | "-" => {
                    diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot perform '{}' between unit of measure '{}' and dimensionless quantity",
                                    op, unit
                                ),
                                Some(span.clone()),
                            );
                    return DataraType::Measure {
                        base: base.clone(),
                        unit: unit.clone(),
                    };
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    if matches!(
                        &**right,
                        Expr::Literal(LiteralValue::Int(_) | LiteralValue::Float(_), _)
                    ) {
                        return DataraType::Bool;
                    }
                    diag.error(
                        ErrorCode::DimensionMismatch,
                        format!(
                            "Cannot compare unit of measure '{}' with dimensionless quantity",
                            unit
                        ),
                        Some(span.clone()),
                    );
                    return DataraType::Bool;
                }
                _ => {}
            }
        } else if let DataraType::Measure { base, unit } = &rt {
            match op {
                "*" => {
                    let b = if lt == DataraType::Float || **base == DataraType::Float {
                        DataraType::Float
                    } else {
                        DataraType::Int
                    };
                    return DataraType::Measure {
                        base: Box::new(b),
                        unit: unit.clone(),
                    };
                }
                "+" | "-" => {
                    diag.error(
                                ErrorCode::DimensionMismatch,
                                format!(
                                    "Cannot perform '{}' between dimensionless quantity and unit of measure '{}'",
                                    op, unit
                                ),
                                Some(span.clone()),
                            );
                    return DataraType::Measure {
                        base: base.clone(),
                        unit: unit.clone(),
                    };
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    if matches!(
                        &**left,
                        Expr::Literal(LiteralValue::Int(_) | LiteralValue::Float(_), _)
                    ) {
                        return DataraType::Bool;
                    }
                    diag.error(
                        ErrorCode::DimensionMismatch,
                        format!(
                            "Cannot compare dimensionless quantity with unit of measure '{}'",
                            unit
                        ),
                        Some(span.clone()),
                    );
                    return DataraType::Bool;
                }
                _ => {}
            }
        }

        // --- Range Interval Arithmetic ---
        if let (
            DataraType::Range {
                base: b1,
                min: min1,
                max: max1,
            },
            DataraType::Range {
                base: b2,
                min: min2,
                max: max2,
            },
        ) = (&lt, &rt)
        {
            let base = if **b1 == DataraType::Float || **b2 == DataraType::Float {
                DataraType::Float
            } else {
                DataraType::Int
            };
            match op {
                "+" => {
                    return DataraType::Range {
                        base: Box::new(base),
                        min: min1.saturating_add(*min2),
                        max: max1.saturating_add(*max2),
                    };
                }
                "-" => {
                    return DataraType::Range {
                        base: Box::new(base),
                        min: min1.saturating_sub(*max2),
                        max: max1.saturating_sub(*min2),
                    };
                }
                "*" => {
                    let p1 = min1.saturating_mul(*min2);
                    let p2 = min1.saturating_mul(*max2);
                    let p3 = max1.saturating_mul(*min2);
                    let p4 = max1.saturating_mul(*max2);
                    let min = p1.min(p2).min(p3).min(p4);
                    let max = p1.max(p2).max(p3).max(p4);
                    return DataraType::Range {
                        base: Box::new(base),
                        min,
                        max,
                    };
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                    return DataraType::Bool;
                }
                _ => {}
            }
        } else if let DataraType::Range { base, min, max } = &lt {
            if let Expr::Literal(LiteralValue::Int(n), _) = &**right {
                let val = *n as i128;
                match op {
                    "+" => {
                        return DataraType::Range {
                            base: base.clone(),
                            min: min.saturating_add(val),
                            max: max.saturating_add(val),
                        };
                    }
                    "-" => {
                        return DataraType::Range {
                            base: base.clone(),
                            min: min.saturating_sub(val),
                            max: max.saturating_sub(val),
                        };
                    }
                    "*" => {
                        let p1 = min.saturating_mul(val);
                        let p2 = max.saturating_mul(val);
                        return DataraType::Range {
                            base: base.clone(),
                            min: p1.min(p2),
                            max: p1.max(p2),
                        };
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                        return DataraType::Bool;
                    }
                    _ => {}
                }
            }
        } else if let DataraType::Range { base, min, max } = &rt
            && let Expr::Literal(LiteralValue::Int(n), _) = &**left
        {
            let val = *n as i128;
            match op {
                "+" => {
                    return DataraType::Range {
                        base: base.clone(),
                        min: val.saturating_add(*min),
                        max: val.saturating_add(*max),
                    };
                }
                "-" => {
                    return DataraType::Range {
                        base: base.clone(),
                        min: val.saturating_sub(*max),
                        max: val.saturating_sub(*min),
                    };
                }
                "*" => {
                    let p1 = val.saturating_mul(*min);
                    let p2 = val.saturating_mul(*max);
                    return DataraType::Range {
                        base: base.clone(),
                        min: p1.min(p2),
                        max: p1.max(p2),
                    };
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => {
                    return DataraType::Bool;
                }
                _ => {}
            }
        }

        // --- Operator operand validation ---
        // `Val` (the dynamic type), `TypeParam`s, `Measure`/`Range`
        // quantities and `Never` are intentionally left permissive:
        // they carry their own rules (dimensional analysis and
        // interval arithmetic above) or are resolved dynamically.
        let is_open = |t: &DataraType| {
            matches!(
                t,
                DataraType::Val
                    | DataraType::TypeParam(_)
                    | DataraType::Never
                    | DataraType::Measure { .. }
                    | DataraType::Range { .. }
            )
        };
        let is_numeric = |t: &DataraType| {
            matches!(
                t,
                DataraType::Int | DataraType::Float | DataraType::Dec64 | DataraType::Dec128
            )
        };
        let is_orderable = |t: &DataraType| {
            matches!(
                t,
                DataraType::Int
                    | DataraType::Float
                    | DataraType::Dec64
                    | DataraType::Dec128
                    | DataraType::String
                    | DataraType::Char
            )
        };
        let report_bad_operands = |diag: &mut DiagnosticEngine| {
            diag.error(
                ErrorCode::TypeInvalidBinaryOp,
                format!(
                    "Operator '{}' cannot be applied to operands of type '{}' and '{}'",
                    op, lt, rt
                ),
                Some(span.clone()),
            );
        };

        if !is_open(&lt) && !is_open(&rt) {
            match op {
                "+" | "-" | "*" | "/" | "%" => {
                    // Str concatenation via `+` is an intended
                    // language feature and stays permissive.
                    let concat =
                        op == "+" && (lt == DataraType::String || rt == DataraType::String);
                    if !concat && (!is_numeric(&lt) || !is_numeric(&rt)) {
                        report_bad_operands(diag);
                    }
                }
                "==" | "!=" => {
                    // Bool/Int cross-comparisons are intended
                    // dynamic behavior in Datara (truthy scalars).
                    let is_truthy_scalar = |t: &DataraType| {
                        matches!(
                            t,
                            DataraType::Int
                                | DataraType::Float
                                | DataraType::Dec64
                                | DataraType::Dec128
                                | DataraType::Bool
                        )
                    };
                    if !(is_truthy_scalar(&lt) && is_truthy_scalar(&rt))
                        && !lt.is_compatible_with_refined_with_args(&rt, Some(self.resolver))
                        && !rt.is_compatible_with_refined_with_args(&lt, Some(self.resolver))
                    {
                        report_bad_operands(diag);
                    }
                }
                "<" | "<=" | ">" | ">=" => {
                    if !is_orderable(&lt) || !is_orderable(&rt) {
                        report_bad_operands(diag);
                    }
                }
                "&&" | "||" => {
                    // Ints participate in logical ops as truthy
                    // values (C-style); that is intended behavior.
                    let is_logical =
                        |t: &DataraType| matches!(t, DataraType::Bool | DataraType::Int);
                    if !is_logical(&lt) || !is_logical(&rt) {
                        report_bad_operands(diag);
                    }
                }
                _ => {}
            }
        }

        match op {
            "+" if lt == DataraType::String || rt == DataraType::String => DataraType::String,
            "+" | "-" | "*" | "/" | "%" => {
                if lt == DataraType::Float || rt == DataraType::Float {
                    DataraType::Float
                } else {
                    DataraType::Int
                }
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => DataraType::Bool,
            _ => lt,
        }
    }
}
