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
                if let Some(e) = self.resolver.enums.get(cls_name) {
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
                        if covered_variants.contains(name) && arm.guard.is_none() {
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
                        if covered_variants.contains("None") && arm.guard.is_none() {
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
                        let key = if *b { "true" } else { "false" };
                        if covered_variants.contains(key) && arm.guard.is_none() {
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
                    if covered_variants.contains(variant_name) && arm.guard.is_none() {
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
                                "Non-exhaustive patterns in match: missing {}",
                                missing.join(", ")
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
                                "Non-exhaustive patterns in match: missing {}",
                                missing.join(", ")
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
                                "Non-exhaustive patterns in match: missing {}",
                                missing.join(", ")
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
                                "Non-exhaustive patterns in match for enum: missing {}",
                                missing.join(", ")
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
                            "Non-exhaustive patterns in match for type '{}'. A wildcard '_' or variable pattern is required.",
                            val_ty
                        ),
                        Some(match_span.clone()),
                        Some("Add a wildcard '_' arm to cover all remaining cases".to_string()),
                    );
                }
            }
        }
    }
}
