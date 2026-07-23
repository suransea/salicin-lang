use std::collections::HashSet;

use crate::ast::PassMode;
use crate::core::LangItemKind;

use super::hir::{HirPlace, Ty};
use super::registry::NominalKind;
use super::Analyzer;

impl Analyzer {
    pub(super) fn copy_layout_is_valid(&self, target: &Ty, valid: &HashSet<Ty>) -> bool {
        match target {
            Ty::Struct(name) => self.struct_layouts.get(name).is_some_and(|layout| {
                layout
                    .fields
                    .iter()
                    .all(|field| self.type_is_copy_with_nominals(&field.ty, valid))
            }),
            Ty::Enum(name) => self.enum_layouts.get(name).is_some_and(|layout| {
                layout.variants.iter().all(|variant| {
                    variant
                        .fields
                        .iter()
                        .all(|field| self.type_is_copy_with_nominals(&field.ty, valid))
                })
            }),
            _ => false,
        }
    }

    pub(super) fn first_non_copy_member(
        &self,
        target: &Ty,
        valid: &HashSet<Ty>,
    ) -> Option<(String, Ty)> {
        let target_name = self.diagnostic_type_name(target);
        match target {
            Ty::Struct(name) => self
                .struct_layouts
                .get(name)?
                .fields
                .iter()
                .find_map(|field| {
                    (!self.type_is_copy_with_nominals(&field.ty, valid)).then(|| {
                        (
                            format!("field `{target_name}.{}`", field.name),
                            field.ty.clone(),
                        )
                    })
                }),
            Ty::Enum(name) => self
                .enum_layouts
                .get(name)?
                .variants
                .iter()
                .find_map(|variant| {
                    variant.fields.iter().find_map(|field| {
                        (!self.type_is_copy_with_nominals(&field.ty, valid)).then(|| {
                            (
                                format!("field `{target_name}.{}.{}`", variant.name, field.name),
                                field.ty.clone(),
                            )
                        })
                    })
                }),
            _ => None,
        }
    }

    pub(super) fn type_is_copy_with_nominals(&self, ty: &Ty, valid: &HashSet<Ty>) -> bool {
        match ty {
            Ty::I32
            | Ty::I64
            | Ty::U32
            | Ty::U64
            | Ty::Bool
            | Ty::Unit
            | Ty::Pointer { .. }
            | Ty::Function(_)
            | Ty::EffectRow { .. }
            | Ty::Never
            | Ty::Error => true,
            Ty::Reference { mutable, .. } => !mutable,
            Ty::Array(element, _) => self.type_is_copy_with_nominals(element, valid),
            Ty::Enum(name) if name == self.lang_item_name(LangItemKind::Never) => true,
            Ty::Struct(_) | Ty::Enum(_) => valid.contains(ty),
            Ty::Callable(_) | Ty::Continuation { .. } | Ty::EffectCallable { .. } => false,
        }
    }

    pub(super) fn is_copy_type(&self, ty: &Ty) -> bool {
        self.type_is_copy_with_nominals(ty, &self.copy_nominals)
    }

    fn box_pointee_type(&self, name: &str) -> Option<&Ty> {
        self.nominal_instances.get(name).and_then(|instance| {
            (instance.key.kind == NominalKind::Struct
                && instance.key.template == "alloc::boxed::Box"
                && instance.key.arguments.len() == 1)
                .then(|| &instance.key.arguments[0])
        })
    }

    pub(super) fn type_needs_drop(&self, ty: &Ty) -> bool {
        self.type_needs_drop_inner(ty, &mut HashSet::new())
    }

    fn type_needs_drop_inner(&self, ty: &Ty, visiting: &mut HashSet<Ty>) -> bool {
        if !visiting.insert(ty.clone()) {
            return true;
        }
        let has_custom_drop = self.trait_impls.keys().any(|key| {
            key.self_ty == *ty
                && key.trait_ref.name == self.lang_item_name(LangItemKind::Drop)
                && key.trait_ref.arguments.is_empty()
        });
        let result = has_custom_drop
            || match ty {
                Ty::Array(element, _) => self.type_needs_drop_inner(element, visiting),
                Ty::Pointer { .. } | Ty::Reference { .. } => false,
                Ty::Struct(name) => {
                    self.box_pointee_type(name).is_some()
                        || self.struct_layouts.get(name).is_some_and(|layout| {
                            layout
                                .fields
                                .iter()
                                .any(|field| self.type_needs_drop_inner(&field.ty, visiting))
                        })
                }
                Ty::Enum(name) => self.enum_layouts.get(name).is_some_and(|layout| {
                    layout.variants.iter().any(|variant| {
                        variant
                            .fields
                            .iter()
                            .any(|field| self.type_needs_drop_inner(&field.ty, visiting))
                    })
                }),
                Ty::Function(_) | Ty::EffectRow { .. } => false,
                Ty::Callable(callable) => callable.captures.iter().any(|capture| {
                    !matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)
                        && self.type_needs_drop_inner(&capture.ty, visiting)
                }),
                Ty::Continuation { .. } | Ty::EffectCallable { .. } => true,
                Ty::I32
                | Ty::I64
                | Ty::U32
                | Ty::U64
                | Ty::Bool
                | Ty::Unit
                | Ty::Never
                | Ty::Error => false,
            };
        visiting.remove(ty);
        result
    }

    pub(super) fn type_has_custom_drop(&self, ty: &Ty) -> bool {
        self.trait_impls.keys().any(|key| {
            key.self_ty == *ty
                && key.trait_ref.name == self.lang_item_name(LangItemKind::Drop)
                && key.trait_ref.arguments.is_empty()
        })
    }

    pub(super) fn projected_place_crosses_custom_drop(&self, place: &HirPlace) -> bool {
        let mut ty = &place.root_ty;
        for projection in &place.projections {
            if self.type_has_custom_drop(ty) {
                return true;
            }
            ty = match ty {
                Ty::Struct(name) => {
                    let Some(field) = self
                        .struct_layouts
                        .get(name)
                        .and_then(|layout| layout.fields.get(*projection))
                    else {
                        return false;
                    };
                    &field.ty
                }
                Ty::Array(element, length)
                    if u64::try_from(*projection).is_ok_and(|index| index < *length) =>
                {
                    element
                }
                _ => return false,
            };
        }
        false
    }

    pub(super) fn effective_pass_mode(&self, mode: PassMode, ty: &Ty) -> PassMode {
        match mode {
            PassMode::Inferred if self.is_copy_type(ty) => PassMode::Copy,
            PassMode::Inferred => PassMode::Move,
            mode => mode,
        }
    }
}
