use crate::ast::Expr;

use super::fallible::InferredEnumHints;
use super::flow::LowerCtx;
use super::hir::{AccessKind, HirExpr, HirExprKind, Ty};
use super::lower::error_expr;
use super::registry::NominalKind;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_member(
        &mut self,
        base: &Expr,
        member: &str,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Some((name, type_groups)) = self.inferred_generic_enum_type_head(base, context) {
            let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                &name,
                &type_groups,
                member,
                &[],
                InferredEnumHints {
                    payload: None,
                    result: expected,
                },
                context,
            ) else {
                return error_expr();
            };
            return self.lower_nominal_type_member_value(&canonical, NominalKind::Enum, member);
        }
        match self.resolve_nominal_type_head(base, context) {
            Ok(Some((target, kind))) => {
                return self.lower_nominal_type_member_value(&target, kind, member);
            }
            Err(()) => return error_expr(),
            Ok(None) => {}
        }
        if let Expr::Name(target) = base {
            if !context.shadows_top_level_name(target)
                && (self.struct_layouts.contains_key(target)
                    || self.enum_layouts.contains_key(target))
            {
                if let Some(canonical) = self
                    .inherent_members
                    .get(target)
                    .and_then(|members| members.constants.get(member))
                    .cloned()
                {
                    return HirExpr {
                        ty: self.global_type(&canonical),
                        kind: HirExprKind::Global(canonical),
                    };
                }
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
            }
            if !context.shadows_top_level_name(target) {
                if let Some(layout) = self.enum_layouts.get(target).cloned() {
                    if let Some((variant, variant_layout)) = layout
                        .variants
                        .iter()
                        .enumerate()
                        .find(|(_, variant)| variant.name == member)
                    {
                        if !variant_layout.fields.is_empty() {
                            self.error(format!(
                                "variant `{target}.{member}` requires constructor arguments"
                            ));
                            return error_expr();
                        }
                        return HirExpr {
                            ty: Ty::Enum(target.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: target.clone(),
                                variant,
                                fields: Vec::new(),
                            },
                        };
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member or variant `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
                if self.struct_layouts.contains_key(target) {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
            }
        }

        if let Some(place) = self.lower_place_without_diagnostic(base, context) {
            if let Ty::Struct(target) | Ty::Enum(target) = &place.ty {
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.methods.contains_key(member))
                {
                    self.error(format!(
                        "inherent method `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
                let has_field = self
                    .struct_layouts
                    .get(target)
                    .is_some_and(|layout| layout.fields.iter().any(|field| field.name == member));
                if !has_field {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.functions.contains_key(member))
                    {
                        self.error(format!(
                            "associated function `{target}.{member}` must be called on the type"
                        ));
                        return error_expr();
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.constants.contains_key(member))
                    {
                        self.error(format!(
                            "associated constant `{target}.{member}` must be accessed on the type"
                        ));
                        return error_expr();
                    }
                }
            }
            let Ty::Struct(struct_name) = &place.ty else {
                self.error(format!(
                    "member access requires a struct value, found `{}`",
                    place.ty
                ));
                return error_expr();
            };
            let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
                return error_expr();
            };
            let Some((index, field)) = layout
                .fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == member)
            else {
                self.error(format!(
                    "unknown field `{member}` on struct `{struct_name}`"
                ));
                return error_expr();
            };
            if !self.require_field_access(struct_name, field, &context.origin) {
                return error_expr();
            }
            let mut field_place = place;
            field_place.projections.push(index);
            field_place.ty = field.ty.clone();
            return self.access_place(field_place, AccessKind::Auto, context);
        }

        let base = self.lower_expr(base, None, context);
        let Ty::Struct(struct_name) = &base.ty else {
            self.error(format!(
                "member access requires a struct value, found `{}`",
                base.ty
            ));
            return error_expr();
        };
        let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
            return error_expr();
        };
        let Some((index, field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == member)
        else {
            self.error(format!(
                "unknown field `{member}` on struct `{struct_name}`"
            ));
            return error_expr();
        };
        if !self.require_field_access(struct_name, field, &context.origin) {
            return error_expr();
        }
        if self.type_needs_drop(&base.ty) {
            self.error(
                "taking a field from a temporary value that needs drop is not supported until partial-drop LLVM lowering is complete",
            );
        }
        HirExpr {
            ty: field.ty.clone(),
            kind: HirExprKind::Field {
                base: Box::new(base),
                index,
            },
        }
    }

    pub(super) fn lower_nominal_type_member_value(
        &mut self,
        target: &str,
        kind: NominalKind,
        member: &str,
    ) -> HirExpr {
        if let Some(canonical) = self
            .inherent_members
            .get(target)
            .and_then(|members| members.constants.get(member))
            .cloned()
        {
            return HirExpr {
                ty: self.global_type(&canonical),
                kind: HirExprKind::Global(canonical),
            };
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.functions.contains_key(member))
        {
            self.error(format!(
                "associated function `{target}.{member}` must be called"
            ));
            return error_expr();
        }
        if kind == NominalKind::Enum {
            let Some(layout) = self.enum_layout_or_diagnostic(target) else {
                return error_expr();
            };
            if let Some((variant, variant_layout)) = layout
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name == member)
            {
                if !variant_layout.fields.is_empty() {
                    self.error(format!(
                        "variant `{target}.{member}` requires constructor arguments"
                    ));
                    return error_expr();
                }
                return HirExpr {
                    ty: Ty::Enum(target.to_owned()),
                    kind: HirExprKind::ConstructEnum {
                        name: target.to_owned(),
                        variant,
                        fields: Vec::new(),
                    },
                };
            }
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.methods.contains_key(member))
        {
            self.error(format!(
                "inherent method `{target}.{member}` requires an instance receiver and must be called"
            ));
        } else if kind == NominalKind::Enum {
            self.error(format!(
                "unknown associated member or variant `{member}` on `{target}`"
            ));
        } else {
            self.error(format!(
                "unknown associated member `{member}` on `{target}`"
            ));
        }
        error_expr()
    }
}
