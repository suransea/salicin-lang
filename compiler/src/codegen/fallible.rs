use crate::ast::{CallArg, CompileParam, Expr, Type, VariantFields};
use crate::core::LangItemKind;

use super::flow::LowerCtx;
use super::hir::{HirExpr, HirExprKind, Ty};
use super::lower::{error_expr, flatten_call, TypeProbe};
use super::registry::NominalKind;
use super::Analyzer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StandardFallibleKind {
    Option,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StandardFallibleInfo {
    pub(super) kind: StandardFallibleKind,
    pub(super) payload: Ty,
    pub(super) payload_source: Option<Type>,
    pub(super) error: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReturnBoundary {
    pub(super) kind: Option<StandardFallibleKind>,
    pub(super) container: Ty,
    pub(super) success: Ty,
    pub(super) error: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CoalescePayloadHint {
    pub(super) ty: Ty,
    pub(super) source: Option<Type>,
}

pub(super) struct InferredCoalesceLhs<'a> {
    pub(super) kind: StandardFallibleKind,
    pub(super) name: String,
    pub(super) type_groups: Vec<&'a [CallArg]>,
    pub(super) variant: &'a str,
    pub(super) value_groups: Vec<&'a [CallArg]>,
}

#[derive(Clone, Copy)]
pub(super) struct InferredEnumHints<'a> {
    pub(super) payload: Option<&'a CoalescePayloadHint>,
    pub(super) result: Option<&'a Ty>,
}

impl Analyzer {
    pub(super) fn fallible_type_name(&self, kind: StandardFallibleKind) -> &str {
        match kind {
            StandardFallibleKind::Option => self.lang_item_name(LangItemKind::Option),
            StandardFallibleKind::Result => self.lang_item_name(LangItemKind::Result),
        }
    }

    pub(super) fn standard_fallible_info_for_ty(&self, ty: &Ty) -> Option<StandardFallibleInfo> {
        let Ty::Enum(canonical) = ty else {
            return None;
        };
        let instance = self.nominal_instances.get(canonical)?;
        if instance.key.kind != NominalKind::Enum {
            return None;
        }
        let template = instance.key.template.as_str();
        let arguments = instance.key.arguments.as_slice();
        if template == self.lang_item_name(LangItemKind::Option) {
            let [payload] = arguments else {
                return None;
            };
            Some(StandardFallibleInfo {
                kind: StandardFallibleKind::Option,
                payload: payload.clone(),
                payload_source: self.source_type_for_ty(payload),
                error: None,
            })
        } else if template == self.lang_item_name(LangItemKind::Result) {
            let [error, payload] = arguments else {
                return None;
            };
            Some(StandardFallibleInfo {
                kind: StandardFallibleKind::Result,
                payload: payload.clone(),
                payload_source: self.source_type_for_ty(payload),
                error: Some(error.clone()),
            })
        } else {
            None
        }
    }

    pub(super) fn standard_fallible_info_for_probe(
        &self,
        probe: &TypeProbe,
    ) -> Option<StandardFallibleInfo> {
        let ty = match probe {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty,
            TypeProbe::Defaultable(_) | TypeProbe::Unsupported => return None,
        };
        if let Some(info) = self.standard_fallible_info_for_ty(ty) {
            return Some(info);
        }
        let TypeProbe::KnownSource(Ty::Enum(_), Type::Named(template, arguments)) = probe else {
            return None;
        };
        if template == self.lang_item_name(LangItemKind::Option) {
            let [payload] = arguments.as_slice() else {
                return None;
            };
            Some(StandardFallibleInfo {
                kind: StandardFallibleKind::Option,
                payload: self.probe_source_ty(payload)?,
                payload_source: Some(payload.clone()),
                error: None,
            })
        } else if template == self.lang_item_name(LangItemKind::Result) {
            let [error, payload] = arguments.as_slice() else {
                return None;
            };
            Some(StandardFallibleInfo {
                kind: StandardFallibleKind::Result,
                payload: self.probe_source_ty(payload)?,
                payload_source: Some(payload.clone()),
                error: Some(self.probe_source_ty(error)?),
            })
        } else {
            None
        }
    }

    pub(super) fn throws_boundary_for_ty(&self, ty: &Ty, error: &Ty) -> Option<ReturnBoundary> {
        let standard = self.standard_fallible_info_for_ty(ty)?;
        if standard.kind != StandardFallibleKind::Result || standard.error.as_ref() != Some(error) {
            return None;
        }
        Some(ReturnBoundary {
            kind: Some(StandardFallibleKind::Result),
            container: ty.clone(),
            success: standard.payload,
            error: Some(error.clone()),
        })
    }

    pub(super) fn lower_return_value(
        &mut self,
        expression: &Expr,
        boundary: &ReturnBoundary,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let value = self.lower_expr(expression, Some(&boundary.success), context);
        self.finish_return_value(value, boundary)
    }

    pub(super) fn finish_return_value(
        &mut self,
        value: HirExpr,
        boundary: &ReturnBoundary,
    ) -> HirExpr {
        if value.ty == Ty::Error || self.is_uninhabited_type(&value.ty) {
            return value;
        }
        if value.ty == boundary.success {
            return self.construct_boundary_variant(boundary, true, Some(value));
        }
        self.error(format!(
            "throws function has logical result `{}`, found `{}`",
            boundary.success, value.ty
        ));
        error_expr()
    }

    pub(super) fn construct_boundary_variant(
        &mut self,
        boundary: &ReturnBoundary,
        success: bool,
        value: Option<HirExpr>,
    ) -> HirExpr {
        let Ty::Enum(enum_name) = &boundary.container else {
            self.error("internal error: non-enum return boundary");
            return error_expr();
        };
        let variant_name = match (boundary.kind, success) {
            (Some(StandardFallibleKind::Option), true) => "Some",
            (Some(StandardFallibleKind::Option), false) => "None",
            (Some(StandardFallibleKind::Result), true) => "Ok",
            (Some(StandardFallibleKind::Result), false) => "Err",
            (None, _) => {
                self.error("internal error: custom return boundary requires protocol conversion");
                return error_expr();
            }
        };
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
        let Some(variant) = layout
            .variants
            .iter()
            .position(|variant| variant.name == variant_name)
        else {
            self.error(format!(
                "internal error: `{enum_name}` has no `{variant_name}` variant"
            ));
            return error_expr();
        };
        let fields = match value {
            Some(value) => vec![(0, value)],
            None => Vec::new(),
        };
        HirExpr {
            ty: boundary.container.clone(),
            kind: HirExprKind::ConstructEnum {
                name: enum_name.clone(),
                variant,
                fields,
            },
        }
    }

    pub(super) fn ensure_throws_result_type(&mut self, payload: Ty, error: Ty) -> Option<Ty> {
        let arguments = vec![error, payload];
        let Some(source_arguments) = arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>()
        else {
            self.error(
                "throws result type cannot be represented by the current source type system",
            );
            return None;
        };
        let template = self.lang_item_name(LangItemKind::Result).to_owned();
        self.ensure_nominal_instance(NominalKind::Enum, &template, source_arguments, arguments)
            .map(Ty::Enum)
    }

    pub(super) fn standard_fallible_payload_parameter(
        &self,
        kind: StandardFallibleKind,
        enum_name: &str,
    ) -> Option<CompileParam> {
        let template = self.enum_templates.get(enum_name)?;
        let payload_variant = match kind {
            StandardFallibleKind::Option => "Some",
            StandardFallibleKind::Result => "Ok",
        };
        let payload_name = template
            .variants
            .iter()
            .find(|variant| variant.name == payload_variant)
            .and_then(|variant| match &variant.fields {
                VariantFields::Positional(types) => types.first(),
                VariantFields::Unit | VariantFields::Named(_) => None,
            })
            .and_then(|ty| match ty {
                Type::Named(name, arguments) if arguments.is_empty() => Some(name.as_str()),
                _ => None,
            })?;
        template
            .compile_groups
            .iter()
            .flatten()
            .find(|parameter| parameter.name == payload_name)
            .cloned()
    }

    fn explicit_type_argument_for_parameter<'a>(
        &self,
        owner: &str,
        type_groups: &[&'a [CallArg]],
        parameter_name: &str,
    ) -> Option<&'a CallArg> {
        if type_groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            return type_groups
                .iter()
                .flat_map(|group| group.iter())
                .find(|argument| argument.label.as_deref() == Some(parameter_name));
        }
        let position = self.enum_templates[owner]
            .compile_groups
            .iter()
            .flatten()
            .position(|parameter| parameter.name == parameter_name)?;
        type_groups
            .iter()
            .flat_map(|group| group.iter())
            .nth(position)
    }

    pub(super) fn inferred_try_payload(
        &self,
        inferred: &InferredCoalesceLhs<'_>,
        context: &LowerCtx,
    ) -> Option<Ty> {
        let payload_parameter =
            self.standard_fallible_payload_parameter(inferred.kind, &inferred.name)?;
        let explicit_payload = self.explicit_type_argument_for_parameter(
            &inferred.name,
            &inferred.type_groups,
            &payload_parameter.name,
        );
        if let Some(argument) = explicit_payload {
            let source =
                self.probe_type_argument_source(&argument.value, &context.type_substitutions)?;
            return self.probe_source_ty(&source);
        }

        let success_variant = match inferred.kind {
            StandardFallibleKind::Option => "Some",
            StandardFallibleKind::Result => "Ok",
        };
        if inferred.variant != success_variant {
            return None;
        }
        let [values] = inferred.value_groups.as_slice() else {
            return None;
        };
        let [value] = *values else {
            return None;
        };
        if value.label.is_some() {
            return None;
        }
        match self.probe_expr_ty(&value.value, None, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) | TypeProbe::Defaultable(ty) => {
                Some(ty)
            }
            TypeProbe::Unsupported => None,
        }
    }

    pub(super) fn inferred_standard_coalesce_lhs<'a>(
        &self,
        expression: &'a Expr,
        context: &LowerCtx,
    ) -> Option<InferredCoalesceLhs<'a>> {
        let mut value_groups = Vec::new();
        let Expr::Member(base, variant) = flatten_call(expression, &mut value_groups) else {
            return None;
        };
        let (name, type_groups) = self.inferred_generic_enum_type_head(base, context)?;
        let kind = if name == self.lang_item_name(LangItemKind::Option) {
            StandardFallibleKind::Option
        } else if name == self.lang_item_name(LangItemKind::Result) {
            StandardFallibleKind::Result
        } else {
            return None;
        };
        Some(InferredCoalesceLhs {
            kind,
            name,
            type_groups,
            variant: variant.as_str(),
            value_groups,
        })
    }

    pub(super) fn inferred_coalesce_payload_probe(
        &self,
        kind: StandardFallibleKind,
        type_groups: &[&[CallArg]],
        hint: Option<&Ty>,
        right: &Expr,
        context: &LowerCtx,
    ) -> TypeProbe {
        let enum_name = match kind {
            StandardFallibleKind::Option => self.lang_item_name(LangItemKind::Option),
            StandardFallibleKind::Result => self.lang_item_name(LangItemKind::Result),
        };
        let payload_parameter = self
            .standard_fallible_payload_parameter(kind, enum_name)
            .expect("standard fallible container has a payload type parameter");
        let explicit_payload = self.explicit_type_argument_for_parameter(
            enum_name,
            type_groups,
            &payload_parameter.name,
        );
        if let Some(argument) = explicit_payload {
            let Some(source) =
                self.probe_type_argument_source(&argument.value, &context.type_substitutions)
            else {
                return TypeProbe::Unsupported;
            };
            let Some(ty) = self.probe_source_ty(&source) else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::KnownSource(ty, source);
        }
        if let Some(hint) = hint.filter(|ty| **ty != Ty::Error) {
            return self.source_type_for_ty(hint).map_or_else(
                || TypeProbe::Known(hint.clone()),
                |source| TypeProbe::KnownSource(hint.clone(), source),
            );
        }
        self.probe_expr_ty(right, None, context)
    }
}
