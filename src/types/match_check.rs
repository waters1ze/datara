use super::{DataraType, TypeChecker};
use crate::ast::*;
use crate::diagnostics::span::SourceSpan;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use std::collections::HashSet;

impl<'a> TypeChecker<'a> {
    pub(crate) fn check_match_exhaustiveness(
        &self,
        val_ty: &DataraType,
        arms: &[MatchArm],
        match_span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        if arms.is_empty() {
            diag.error(
                ErrorCode::NonExhaustiveMatch,
                "Match expression has no arms and is non-exhaustive".to_string(),
                Some(match_span.clone()),
            );
            return;
        }

        enum Domain {
            Option,
            Result,
            Bool,
            Enum(Vec<String>),
            Infinite,
        }

        let domain = match val_ty {
            DataraType::Option(_) => Domain::Option,
            DataraType::Result(_, _) => Domain::Result,
            DataraType::Bool => Domain::Bool,
            DataraType::Class(cls_name) => {
                if cls_name == "Maybe" || cls_name == "Option" {
                    Domain::Option
                } else if cls_name == "Outcome" || cls_name == "Result" {
                    Domain::Result
                } else if let Some(e) = self.resolver.enums.get(cls_name) {
                    Domain::Enum(e.variants.iter().map(|v| v.name.clone()).collect())
                } else {
                    Domain::Infinite
                }
            }
            DataraType::GenericInstance { name, .. } => {
                if name == "Maybe" || name == "Option" {
                    Domain::Option
                } else if name == "Outcome" || name == "Result" {
                    Domain::Result
                } else if let Some(e) = self.resolver.enums.get(name) {
                    Domain::Enum(e.variants.iter().map(|v| v.name.clone()).collect())
                } else {
                    Domain::Infinite
                }
            }
            _ => Domain::Infinite,
        };

        let mut covered_variants: HashSet<String> = HashSet::new();
        let mut catch_all_hit = false;

        for arm in arms {
            let arm_span = arm.pattern.span().clone();

            if catch_all_hit {
                diag.error(
                    ErrorCode::UnreachablePattern,
                    "Unreachable pattern: arm is preceded by an unconditional catch-all"
                        .to_string(),
                    Some(arm_span),
                );
                continue;
            }

            match &arm.pattern {
                Pattern::Wildcard(_) => {
                    if arm.guard.is_none() {
                        catch_all_hit = true;
                    }
                }
                Pattern::Identifier(name, _) if name == "_" => {
                    if arm.guard.is_none() {
                        catch_all_hit = true;
                    }
                }
                Pattern::Identifier(name, _) => {
                    let is_enum_variant = match &domain {
                        Domain::Option => name == "None" || name == "Some",
                        Domain::Result => name == "Ok" || name == "Err",
                        Domain::Enum(vars) => vars.contains(name),
                        _ => false,
                    };

                    if is_enum_variant {
                        if covered_variants.contains(name) {
                            diag.error(
                                ErrorCode::UnreachablePattern,
                                format!(
                                    "Unreachable pattern: variant '{}' is already covered",
                                    name
                                ),
                                Some(arm_span.clone()),
                            );
                        }
                        if arm.guard.is_none() {
                            covered_variants.insert(name.clone());
                        }
                    } else if arm.guard.is_none() {
                        catch_all_hit = true;
                    }
                }
                Pattern::Literal(lit, _) => match lit {
                    LiteralValue::None => {
                        if !matches!(&domain, Domain::Option) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!("Cannot match 'None' against type '{}'", val_ty),
                                Some(arm_span.clone()),
                            );
                        }
                        if covered_variants.contains("None") {
                            diag.error(
                                ErrorCode::UnreachablePattern,
                                "Unreachable pattern: 'None' is already covered".to_string(),
                                Some(arm_span.clone()),
                            );
                        }
                        if arm.guard.is_none() {
                            covered_variants.insert("None".to_string());
                        }
                    }
                    LiteralValue::Bool(b) => {
                        if !matches!(&domain, Domain::Bool) {
                            diag.error(
                                ErrorCode::TypeMismatch,
                                format!("Cannot match boolean literal against type '{}'", val_ty),
                                Some(arm_span.clone()),
                            );
                        }
                        let key = if *b { "true" } else { "false" };
                        if covered_variants.contains(key) {
                            diag.error(
                                ErrorCode::UnreachablePattern,
                                format!("Unreachable pattern: '{}' is already covered", key),
                                Some(arm_span.clone()),
                            );
                        }
                        if arm.guard.is_none() {
                            covered_variants.insert(key.to_string());
                        }
                    }
                    _ => {}
                },
                Pattern::Variant { variant_name, .. } => {
                    let is_known_variant = match &domain {
                        Domain::Option => variant_name == "Some" || variant_name == "None",
                        Domain::Result => variant_name == "Ok" || variant_name == "Err",
                        Domain::Enum(vars) => vars.contains(variant_name),
                        _ => true,
                    };

                    if !is_known_variant {
                        diag.error(
                            ErrorCode::TypeMismatch,
                            format!(
                                "Variant '{}' does not belong to type '{}'",
                                variant_name, val_ty
                            ),
                            Some(arm_span.clone()),
                        );
                    }

                    if covered_variants.contains(variant_name) {
                        diag.error(
                            ErrorCode::UnreachablePattern,
                            format!(
                                "Unreachable pattern: variant '{}' is already covered",
                                variant_name
                            ),
                            Some(arm_span.clone()),
                        );
                    }
                    if arm.guard.is_none() {
                        covered_variants.insert(variant_name.clone());
                    }
                }
            }
        }

        if !catch_all_hit {
            match &domain {
                Domain::Option => {
                    let mut missing = Vec::new();
                    if !covered_variants.contains("Some") {
                        missing.push("Some(_)");
                    }
                    if !covered_variants.contains("None") {
                        missing.push("None");
                    }
                    if !missing.is_empty() {
                        diag.error_with_help(
                            ErrorCode::NonExhaustiveMatch,
                            format!(
                                "Non-exhaustive patterns in match: missing {}. Missing pattern: {}",
                                missing.join(", "),
                                missing[0]
                            ),
                            Some(match_span.clone()),
                            Some(format!("Add missing arm(s): {}", missing.join(", "))),
                        );
                    }
                }
                Domain::Result => {
                    let mut missing = Vec::new();
                    if !covered_variants.contains("Ok") {
                        missing.push("Ok(_)");
                    }
                    if !covered_variants.contains("Err") {
                        missing.push("Err(_)");
                    }
                    if !missing.is_empty() {
                        diag.error_with_help(
                            ErrorCode::NonExhaustiveMatch,
                            format!(
                                "Non-exhaustive patterns in match: missing {}. Missing pattern: {}",
                                missing.join(", "),
                                missing[0]
                            ),
                            Some(match_span.clone()),
                            Some(format!("Add missing arm(s): {}", missing.join(", "))),
                        );
                    }
                }
                Domain::Bool => {
                    let mut missing = Vec::new();
                    if !covered_variants.contains("true") {
                        missing.push("true");
                    }
                    if !covered_variants.contains("false") {
                        missing.push("false");
                    }
                    if !missing.is_empty() {
                        diag.error_with_help(
                            ErrorCode::NonExhaustiveMatch,
                            format!(
                                "Non-exhaustive patterns in match: missing {}. Missing pattern: {}",
                                missing.join(", "),
                                missing[0]
                            ),
                            Some(match_span.clone()),
                            Some(format!("Add missing arm(s): {}", missing.join(", "))),
                        );
                    }
                }
                Domain::Enum(vars) => {
                    let missing: Vec<&str> = vars
                        .iter()
                        .filter(|v| !covered_variants.contains(v.as_str()))
                        .map(|s| s.as_str())
                        .collect();
                    if !missing.is_empty() {
                        diag.error_with_help(
                            ErrorCode::NonExhaustiveMatch,
                            format!(
                                "Non-exhaustive patterns in match for enum: missing {}. Missing pattern: {}",
                                missing.join(", "),
                                missing[0]
                            ),
                            Some(match_span.clone()),
                            Some(format!("Add missing arm(s): {}", missing.join(", "))),
                        );
                    }
                }
                Domain::Infinite => {
                    diag.error_with_help(
                        ErrorCode::NonExhaustiveMatch,
                        format!(
                            "Non-exhaustive patterns in match for type '{}'. Missing pattern: _",
                            val_ty
                        ),
                        Some(match_span.clone()),
                        Some("Add a wildcard '_' arm to cover all remaining cases".to_string()),
                    );
                }
            }
        }
    }

    pub(crate) fn check_decide_exhaustiveness(
        &self,
        arms: &[DecideArm],
        else_arm: Option<&Expr>,
        unified_ty: &DataraType,
        decide_span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        if arms.is_empty() && else_arm.is_none() {
            diag.error(
                ErrorCode::NonExhaustiveMatch,
                "Decide expression has no arms and is non-exhaustive".to_string(),
                Some(decide_span.clone()),
            );
            return;
        }

        let mut hit_unconditional = false;
        for arm in arms {
            if hit_unconditional {
                diag.error(
                    ErrorCode::UnreachablePattern,
                    "Unreachable decide arm: preceded by an unconditional 'true' branch"
                        .to_string(),
                    Some(arm.condition.span().clone()),
                );
            } else if matches!(&arm.condition, Expr::Literal(LiteralValue::Bool(true), _)) {
                hit_unconditional = true;
            }
        }
        if hit_unconditional && else_arm.is_some() {
            if let Some(eb) = else_arm {
                diag.error(
                    ErrorCode::UnreachablePattern,
                    "Unreachable decide else branch: preceded by an unconditional 'true' arm"
                        .to_string(),
                    Some(eb.span().clone()),
                );
            }
        }

        // A value-producing decide expression (producing a non-Unit type)
        // must be exhaustive: either it has an else branch, or an unconditional true arm.
        if *unified_ty != DataraType::Unit && else_arm.is_none() && !hit_unconditional {
            diag.error_with_help(
                ErrorCode::NonExhaustiveMatch,
                format!(
                    "Value-producing 'decide' expression returning '{}' is non-exhaustive; missing 'else' branch",
                    unified_ty
                ),
                Some(decide_span.clone()),
                Some("Add an 'else => <default>' branch to cover all remaining conditions".to_string()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::driver::ForgenCompiler;

    #[test]
    fn test_exhaustiveness_full_coverage() {
        let code = r#"
enum Direction { North, South, East, West }

fn dir_to_int(d: Direction) -> Int {
    match d {
        Direction.North => 1,
        Direction.South => 2,
        Direction.East => 3,
        Direction.West => 4,
    }
}

fn bool_to_int(b: Bool) -> Int {
    match b {
        true => 10,
        false => 20,
    }
}

fn decide_with_else(x: Int) -> Str {
    decide {
        x > 0 => "positive",
        x < 0 => "negative",
        else => "zero",
    }
}

fn main() {
    out dir_to_int(Direction.North)
    out bool_to_int(true)
    out decide_with_else(5)
}
"#;
        let compiler = ForgenCompiler::new("release");
        let res = compiler.compile_source(code, "full_coverage.dtr", None);
        assert!(
            res.success,
            "Full coverage match/decide must compile cleanly: {:?}",
            res.error
        );
    }

    #[test]
    fn test_exhaustiveness_hole_missing_variant() {
        let compiler = ForgenCompiler::new("release");

        // 1. Enum missing variant
        let code1 = r#"
enum Color { Red, Green, Blue }
fn check(c: Color) -> Int {
    match c {
        Color.Red => 1,
        Color.Green => 2,
    }
}
fn main() {}
"#;
        let res1 = compiler.compile_source(code1, "missing_enum.dtr", None);
        assert!(
            !res1.success,
            "Match with missing enum variant must fail compilation"
        );
        assert!(res1.error.unwrap_or_default().contains("E0310"));

        // 2. Bool missing false
        let code2 = r#"
fn check(b: Bool) -> Int {
    match b {
        true => 1,
    }
}
fn main() {}
"#;
        let res2 = compiler.compile_source(code2, "missing_bool.dtr", None);
        assert!(
            !res2.success,
            "Match with missing false must fail compilation"
        );
        assert!(res2.error.unwrap_or_default().contains("E0310"));

        // 3. Value-producing decide missing else
        let code3 = r#"
fn check(x: Int) -> Str {
    decide {
        x > 0 => "pos",
        x < 0 => "neg",
    }
}
fn main() {}
"#;
        let res3 = compiler.compile_source(code3, "missing_else.dtr", None);
        assert!(
            !res3.success,
            "Value-producing decide without else must fail compilation"
        );
        assert!(res3.error.unwrap_or_default().contains("E0310"));
    }

    #[test]
    fn test_exhaustiveness_wildcard_coverage() {
        let code = r#"
enum Priority { Low, Medium, High, Critical }

fn priority_weight(p: Priority) -> Int {
    match p {
        Priority.Critical => 100,
        _ => 10,
    }
}

fn int_match_wildcard(n: Int) -> Str {
    match n {
        0 => "zero",
        1 => "one",
        _ => "many",
    }
}

fn decide_unconditional_true(x: Int) -> Str {
    decide {
        x > 100 => "huge",
        true => "default",
    }
}

fn main() {
    out priority_weight(Priority.Low)
    out int_match_wildcard(42)
    out decide_unconditional_true(5)
}
"#;
        let compiler = ForgenCompiler::new("release");
        let res = compiler.compile_source(code, "wildcard.dtr", None);
        assert!(
            res.success,
            "Wildcard coverage must compile cleanly: {:?}",
            res.error
        );
    }
}
