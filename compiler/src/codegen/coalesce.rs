use crate::ast::{Binding, CallArg, Expr, MatchArm, Pattern, PatternFields, Stmt};

use super::fallible::{CoalescePayloadHint, InferredEnumHints, StandardFallibleKind};
use super::flow::LowerCtx;
use super::hir::{HirExpr, Ty};
use super::lower::{error_expr, TypeProbe};
use super::registry::NominalKind;
use super::Analyzer;

impl Analyzer {
    pub(super) fn probe_coalesce_ty(
        &self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let left_probe = self.probe_expr_ty(left, None, context);
        let payload = if let Some(info) = self.standard_fallible_info_for_probe(&left_probe) {
            match info.payload_source {
                Some(source) => TypeProbe::KnownSource(info.payload, source),
                None => TypeProbe::Known(info.payload),
            }
        } else {
            let Some(inferred) = self.inferred_standard_coalesce_lhs(left, context) else {
                return TypeProbe::Unsupported;
            };
            self.inferred_coalesce_payload_probe(
                inferred.kind,
                &inferred.type_groups,
                hint,
                right,
                context,
            )
        };
        let payload_ty = match &payload {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) | TypeProbe::Defaultable(ty) => ty,
            TypeProbe::Unsupported => return TypeProbe::Unsupported,
        };
        let right = self.probe_expr_ty(right, Some(payload_ty), context);
        if Self::probe_matches_type(&right, payload_ty) {
            payload
        } else {
            TypeProbe::Unsupported
        }
    }

    pub(super) fn lower_coalesce(
        &mut self,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let inferred_left = self.inferred_standard_coalesce_lhs(left, context);
        if inferred_left.is_none() {
            let left_probe = self.probe_expr_ty(left, None, context);
            if Self::nominal_ty_from_probe(&left_probe).is_some()
                && self.standard_fallible_info_for_probe(&left_probe).is_none()
            {
                return self.lower_custom_coalesce(left, right, expected, context);
            }
        }
        let payload_hint = inferred_left.as_ref().and_then(|inferred| {
            match self.inferred_coalesce_payload_probe(
                inferred.kind,
                &inferred.type_groups,
                expected.filter(|ty| **ty != Ty::Error && !self.is_uninhabited_type(ty)),
                right,
                context,
            ) {
                TypeProbe::Known(ty) | TypeProbe::Defaultable(ty) => Some(CoalescePayloadHint {
                    source: self.source_type_for_ty(&ty),
                    ty,
                }),
                TypeProbe::KnownSource(ty, source) => Some(CoalescePayloadHint {
                    ty,
                    source: Some(source),
                }),
                TypeProbe::Unsupported => None,
            }
        });
        let scrutinee = if let (Some(inferred), Some(hint)) = (inferred_left, payload_hint.as_ref())
        {
            let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                &inferred.name,
                &inferred.type_groups,
                inferred.variant,
                &inferred.value_groups,
                InferredEnumHints {
                    payload: Some(hint),
                    result: None,
                },
                context,
            ) else {
                return error_expr();
            };
            if inferred.value_groups.is_empty() {
                self.lower_nominal_type_member_value(
                    &canonical,
                    NominalKind::Enum,
                    inferred.variant,
                )
            } else {
                self.lower_nominal_type_member_call(
                    &canonical,
                    NominalKind::Enum,
                    inferred.variant,
                    &inferred.value_groups,
                    None,
                    context,
                )
            }
        } else {
            self.lower_expr(left, None, context)
        };
        if scrutinee.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.standard_fallible_info_for_ty(&scrutinee.ty) else {
            self.error(format!(
                "operator `??` requires `Option(T)` or `Result(E)(T)` on the left, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };

        const PAYLOAD_BINDING: &str = "$coalesce$payload";
        let payload_arm = |variant: &str| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields: PatternFields::Positional(vec![Pattern::Binding(
                    PAYLOAD_BINDING.to_owned(),
                )]),
            },
            guard: None,
            body: Expr::Name(PAYLOAD_BINDING.to_owned()),
        };
        let fallback_arm = |variant: &str, fields: PatternFields| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields,
            },
            guard: None,
            body: right.clone(),
        };
        let arms = match info.kind {
            StandardFallibleKind::Option => vec![
                payload_arm("Some"),
                fallback_arm("None", PatternFields::Unit),
            ],
            StandardFallibleKind::Result => vec![
                payload_arm("Ok"),
                fallback_arm("Err", PatternFields::Positional(vec![Pattern::Wildcard])),
            ],
        };
        self.lower_match_with_scrutinee(scrutinee, &arms, Some(&info.payload), context)
    }

    pub(super) fn lower_custom_coalesce(
        &mut self,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        const SCRUTINEE_BINDING: &str = "$coalesce$scrutinee";
        let lowered = Expr::Block(
            vec![Stmt::Let(Binding {
                mutable: false,
                name: SCRUTINEE_BINDING.to_owned(),
                annotation: None,
                value: left.clone(),
            })],
            Some(Box::new(Expr::Call(
                Box::new(Expr::Call(
                    Box::new(Expr::Member(
                        Box::new(Expr::Name(SCRUTINEE_BINDING.to_owned())),
                        "$lang$coalesce".to_owned(),
                    )),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name("pure".to_owned()),
                    }],
                )),
                vec![CallArg {
                    label: None,
                    value: Expr::Closure(Vec::new(), Box::new(right.clone())),
                }],
            ))),
        );
        self.lower_expr(&lowered, expected, context)
    }

    pub(super) fn lower_handler_coalesce(
        &mut self,
        source_scrutinee: &Expr,
        payload: &str,
        success: &Expr,
        fallback: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let scrutinee = self.lower_expr(source_scrutinee, None, context);
        if scrutinee.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.standard_fallible_info_for_ty(&scrutinee.ty) else {
            self.error(format!(
                "operator `??` requires `Option(T)` or `Result(E)(T)` on the left, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let payload_arm = |variant: &str| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields: PatternFields::Positional(vec![Pattern::Binding(payload.to_owned())]),
            },
            guard: None,
            body: success.clone(),
        };
        let fallback_arm = |variant: &str, fields: PatternFields| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields,
            },
            guard: None,
            body: fallback.clone(),
        };
        let arms = match info.kind {
            StandardFallibleKind::Option => vec![
                payload_arm("Some"),
                fallback_arm("None", PatternFields::Unit),
            ],
            StandardFallibleKind::Result => vec![
                payload_arm("Ok"),
                fallback_arm("Err", PatternFields::Positional(vec![Pattern::Wildcard])),
            ],
        };
        self.lower_match_with_scrutinee(scrutinee, &arms, expected, context)
    }
}
