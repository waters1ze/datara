use super::{DataraType, TypeChecker};
use crate::ast::*;
use crate::diagnostics::span::SourceSpan;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};

impl<'a> TypeChecker<'a> {
    pub fn check_refinement(
        &self,
        tn: &TypeNode,
        init: &Expr,
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        let Some(refinement) = self.get_refinement(tn) else {
            return;
        };

        match refinement {
            Refinement::Range {
                start,
                end,
                inclusive,
            } => match init {
                Expr::Literal(LiteralValue::Int(val), _) => {
                    let s_val = Self::extract_int_lit(start);
                    let e_val = Self::extract_int_lit(end);
                    if let (Some(s), Some(e)) = (s_val, e_val) {
                        let in_bounds = if *inclusive {
                            *val >= s && *val <= e
                        } else {
                            *val >= s && *val < e
                        };
                        if !in_bounds {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Refinement type violation: value {} is outside allowed range {}{}{}",
                                    val,
                                    s,
                                    if *inclusive { "..=" } else { "..<" },
                                    e
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }
                Expr::Literal(LiteralValue::Float(val), _) => {
                    let s_val = Self::extract_float_lit(start);
                    let e_val = Self::extract_float_lit(end);
                    if let (Some(s), Some(e)) = (s_val, e_val) {
                        let in_bounds = if *inclusive {
                            *val >= s && *val <= e
                        } else {
                            *val >= s && *val < e
                        };
                        if !in_bounds {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!(
                                    "Refinement type violation: value {} is outside allowed range {}{}{}",
                                    val,
                                    s,
                                    if *inclusive { "..=" } else { "..<" },
                                    e
                                ),
                                Some(span.clone()),
                            );
                        }
                    }
                }
                _ => {}
            },
            Refinement::Predicate {
                var_name,
                predicate,
            } => {
                if let Expr::Literal(lit, _) = init
                    && let Some(false) = Self::eval_predicate(predicate, var_name, lit)
                {
                    let lit_str = match lit {
                        LiteralValue::Int(i) => i.to_string(),
                        LiteralValue::Float(f) => f.to_string(),
                        LiteralValue::String(s) => format!("\"{}\"", s),
                        LiteralValue::Bool(b) => b.to_string(),
                        LiteralValue::Char(c) => format!("'{}'", c),
                        LiteralValue::None => "none".to_string(),
                    };
                    diag.error(
                        ErrorCode::TypeMismatch,
                        format!(
                            "Refinement type violation: value {} violates predicate",
                            lit_str
                        ),
                        Some(span.clone()),
                    );
                }
            }
        }
    }

    fn extract_int_lit(expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Literal(LiteralValue::Int(v), _) => Some(*v),
            Expr::Unary { op, expr, .. } if op == "-" => Self::extract_int_lit(expr).map(|v| -v),
            _ => None,
        }
    }

    fn extract_float_lit(expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Literal(LiteralValue::Float(v), _) => Some(*v),
            Expr::Literal(LiteralValue::Int(v), _) => Some(*v as f64),
            Expr::Unary { op, expr, .. } if op == "-" => Self::extract_float_lit(expr).map(|v| -v),
            _ => None,
        }
    }

    fn eval_predicate(expr: &Expr, var_name: &str, lit: &LiteralValue) -> Option<bool> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                let left_val = Self::eval_expr_for_pred(left, var_name, lit)?;
                let right_val = Self::eval_expr_for_pred(right, var_name, lit)?;
                match op.as_str() {
                    "!=" => Some(left_val != right_val),
                    "==" => Some(left_val == right_val),
                    "<" => Some(left_val < right_val),
                    "<=" => Some(left_val <= right_val),
                    ">" => Some(left_val > right_val),
                    ">=" => Some(left_val >= right_val),
                    "&&" => {
                        let l = Self::eval_predicate(left, var_name, lit)?;
                        let r = Self::eval_predicate(right, var_name, lit)?;
                        Some(l && r)
                    }
                    "||" => {
                        let l = Self::eval_predicate(left, var_name, lit)?;
                        let r = Self::eval_predicate(right, var_name, lit)?;
                        Some(l || r)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn eval_expr_for_pred(expr: &Expr, var_name: &str, lit: &LiteralValue) -> Option<f64> {
        match expr {
            Expr::Identifier(name, _) if name == var_name || name == "val" || name == "it" => {
                match lit {
                    LiteralValue::Int(i) => Some(*i as f64),
                    LiteralValue::Float(f) => Some(*f),
                    _ => None,
                }
            }
            Expr::Literal(LiteralValue::Int(i), _) => Some(*i as f64),
            Expr::Literal(LiteralValue::Float(f), _) => Some(*f),
            Expr::Unary { op, expr, .. } if op == "-" => {
                Self::eval_expr_for_pred(expr, var_name, lit).map(|v| -v)
            }
            _ => None,
        }
    }

    pub fn suggest_type_fix(expected: &DataraType, found: &DataraType) -> Option<String> {
        match (expected, found) {
            (DataraType::Int, DataraType::Float) => Some(
                "use explicit cast 'val as Int' or call 'datara_rt_math_floor(val)' to convert Float to Int".into(),
            ),
            (DataraType::Float, DataraType::Int) => Some(
                "use a floating-point literal (e.g. '10.0') or cast with 'val as Float'".into(),
            ),
            (DataraType::String, DataraType::Int) => Some(
                "convert Int to String using string interpolation '\"{val}\"' or 'int_to_str(val)'".into(),
            ),
            (DataraType::String, DataraType::Float) => Some(
                "convert Float to String using string interpolation '\"{val}\"' or 'float_to_str(val)'".into(),
            ),
            (DataraType::String, DataraType::Bool) => Some(
                "convert Bool to String using 'bool_to_str(val)' or '\"{val}\"'".into(),
            ),
            (DataraType::Int, DataraType::String) => Some(
                "parse String to Int using 'str_to_int(val)'".into(),
            ),
            (DataraType::Float, DataraType::String) => Some(
                "parse String to Float using 'str_to_float(val)'".into(),
            ),
            (DataraType::Bool, DataraType::Int) => Some(
                "in Datara, integers are not implicitly boolean; use an explicit comparison like 'val != 0'".into(),
            ),
            (DataraType::Bool, _) => Some(
                "expression must evaluate to a Bool; use comparison operators (==, !=, <, >)".into(),
            ),
            _ => None,
        }
    }
}

impl<'a> TypeChecker<'a> {
    /// The `T!E` sugar is represented at runtime by the stdlib `Outcome<T>`
    /// class whose error channel is the fixed `error_msg: String` field. An
    /// error type of anything but String has no representation, so reject it
    /// loudly instead of silently coercing (no JS-style magic).
    pub(crate) fn validate_error_channels(&self, tn: &TypeNode, diag: &mut DiagnosticEngine) {
        let err_node = if let Some(err) = &tn.error_type {
            Some((err.as_ref(), tn.full_type_name()))
        } else if tn.name == "Result" && tn.generic_args.len() == 2 {
            Some((&tn.generic_args[1], tn.full_type_name()))
        } else {
            None
        };
        if let Some((err_node, type_str)) = err_node {
            let err_ty = self.resolve_type_node(err_node, diag);
            if err_ty != DataraType::String {
                diag.error(
                    ErrorCode::TypeMismatch,
                    format!(
                        "the error channel of '{}' must be String (Outcome representation), got '{}'",
                        type_str, err_ty
                    ),
                    Some(tn.span.clone()),
                );
            }
        }
        for a in &tn.generic_args {
            self.validate_error_channels(a, diag);
        }
    }

    pub(crate) fn check_range_and_measure_assignment(
        &self,
        declared: &DataraType,
        init_type: &DataraType,
        init: &Expr,
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        // Range check
        if let DataraType::Range { min, max, .. } = declared {
            let lit_val = match init {
                Expr::Literal(LiteralValue::Int(n), _) => Some(*n as i128),
                Expr::Unary { op, expr, .. } if op == "-" => {
                    if let Expr::Literal(LiteralValue::Int(n), _) = &**expr {
                        Some(-(*n as i128))
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(v) = lit_val {
                if v < *min || v > *max {
                    diag.error_with_help(
                        ErrorCode::RangeViolation,
                        format!(
                            "Value {} is out of bounds for range type '{}' (allowed [{}..{}])",
                            v, declared, min, max
                        ),
                        Some(span.clone()),
                        Some(format!("Ensure value is between {} and {}", min, max)),
                    );
                }
            } else if let DataraType::Range {
                min: r_min,
                max: r_max,
                ..
            } = init_type
                && (*r_min < *min || *r_max > *max)
            {
                diag.error_with_help(
                    ErrorCode::RangeViolation,
                    format!(
                        "Range [{}..{}] does not fit within target range [{}..{}]",
                        r_min, r_max, min, max
                    ),
                    Some(span.clone()),
                    Some(format!("Ensure range is within [{}..{}]", min, max)),
                );
            }
        }

        // Measure check
        if let DataraType::Measure {
            unit: decl_unit, ..
        } = declared
            && let DataraType::Measure {
                unit: init_unit, ..
            } = init_type
            && decl_unit != init_unit
        {
            diag.error_with_help(
                ErrorCode::DimensionMismatch,
                format!(
                    "Cannot assign value of unit '{}' to variable of unit '{}'",
                    init_unit, decl_unit
                ),
                Some(span.clone()),
                Some(format!("Convert or adjust unit to '{}'", decl_unit)),
            );
        }
    }
}
