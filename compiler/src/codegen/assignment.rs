use crate::ast::{BinaryOp, CallArg, Expr};

use super::flow::{InitializationStatus, LowerCtx};
use super::hir::{AccessKind, HirExpr, HirExprKind, Ty};
use super::lower::{error_expr, BoundMethodConstraint, TypeProbe};
use super::operators::assignment_operator_trait;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_compound_assign(
        &mut self,
        place: &Expr,
        operator: BinaryOp,
        value: &Expr,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(lang_item) = assignment_operator_trait(operator) else {
            self.error("unsupported compound assignment operator");
            return error_expr();
        };
        match self.probe_expr_ty(place, None, context) {
            TypeProbe::Known(Ty::Struct(_))
            | TypeProbe::Known(Ty::Enum(_))
            | TypeProbe::KnownSource(Ty::Struct(_), _)
            | TypeProbe::KnownSource(Ty::Enum(_), _) => {
                let arguments = [CallArg {
                    label: None,
                    value: value.clone(),
                }];
                return self.lower_bound_method_call(
                    place,
                    lang_item
                        .assignment_operator_method()
                        .expect("assignment lang item has a method"),
                    &[arguments.as_slice()],
                    BoundMethodConstraint::LangItem(lang_item),
                    Some(&Ty::Unit),
                    context,
                );
            }
            _ => {}
        }

        let Some(place) = self.lower_place(place, context) else {
            return error_expr();
        };
        self.ensure_writable(&place);
        if !place.ty.is_integer() {
            self.error(format!(
                "compound assignment requires an integer or an implementation of `{}`, found `{}`",
                lang_item.source_name(),
                self.diagnostic_type_name(&place.ty),
            ));
            let _ = self.lower_expr(value, None, context);
            return error_expr();
        }
        let left = self.access_place(place.clone(), AccessKind::Copy, context);
        let right = self.lower_expr(value, Some(&place.ty), context);
        self.require_same_type(&right.ty, &place.ty, "right operand of compound assignment");
        let implemented = self.trait_impls.keys().any(|key| {
            key.self_ty == place.ty
                && key.trait_ref.name == self.lang_item_name(lang_item)
                && key.trait_ref.arguments.as_slice() == [right.ty.clone()]
        });
        if !implemented {
            self.error(format!(
                "type `{}` does not implement `{}` required by compound assignment",
                self.diagnostic_type_name(&place.ty),
                lang_item.source_name(),
            ));
            return error_expr();
        }
        let binary = HirExpr {
            ty: place.ty.clone(),
            kind: HirExprKind::Binary(Box::new(left), operator, Box::new(right)),
        };
        let assignment = self.mark_initialized(&place, context);
        let mut root = place.clone();
        root.projections.clear();
        root.ty = root.root_ty.clone();
        let root_initialized = context
            .flow
            .initialization_status(&self.place_leaf_keys(&root))
            == InitializationStatus::Initialized;
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::Assign {
                place,
                value: Box::new(binary),
                assignment,
                root_initialized,
            },
        }
    }
}
