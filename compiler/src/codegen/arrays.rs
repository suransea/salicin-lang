use crate::ast::Expr;

use super::flow::LowerCtx;
use super::hir::{HirExpr, HirExprKind, HirIndex, Ty};
use super::lower::{error_expr, integer_literal_value};
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let expected_array = match expected {
            Some(Ty::Array(element, length)) => Some((element.as_ref().clone(), *length)),
            Some(Ty::Error) => None,
            Some(other) => {
                self.error(format!(
                    "array literal cannot be used where `{other}` is expected"
                ));
                None
            }
            None => None,
        };

        if let Some((element_ty, length)) = expected_array {
            if element_ty == Ty::Unit {
                self.error("array element type `()` is not supported in the first version");
            }
            if elements.len() as u64 != length {
                self.error(format!(
                    "array literal length mismatch: expected {length}, found {}",
                    elements.len()
                ));
            }
            let elements = elements
                .iter()
                .map(|element| self.lower_expr(element, Some(&element_ty), context))
                .collect();
            let array_ty = Ty::Array(Box::new(element_ty), length);
            self.array_types.insert(array_ty.clone());
            return HirExpr {
                ty: array_ty,
                kind: HirExprKind::Array(elements),
            };
        }

        let Some((first, rest)) = elements.split_first() else {
            self.error("empty array literal requires an expected array type");
            return error_expr();
        };
        let first = self.lower_expr(first, None, context);
        let element_ty = first.ty.clone();
        if element_ty == Ty::Unit {
            self.error("array element type `()` is not supported in the first version");
        }
        let mut lowered = vec![first];
        lowered.extend(
            rest.iter()
                .map(|element| self.lower_expr(element, Some(&element_ty), context)),
        );
        let array_ty = Ty::Array(Box::new(element_ty), elements.len() as u64);
        self.array_types.insert(array_ty.clone());
        HirExpr {
            ty: array_ty,
            kind: HirExprKind::Array(lowered),
        }
    }

    pub(super) fn lower_index(
        &mut self,
        base: &Expr,
        index: &Expr,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let base = self.lower_expr(base, None, context);
        let Ty::Array(element, length) = &base.ty else {
            self.error(format!(
                "array index requires an array value, found `{}`",
                base.ty
            ));
            let _ = self.lower_expr(index, None, context);
            return error_expr();
        };
        let element_ty = element.as_ref().clone();
        let length = *length;
        let lowered_index = self.lower_expr(index, None, context);
        self.require_same_type(&lowered_index.ty, &Ty::I32, "array index");

        let moves = !self.is_copy_type(&element_ty);
        if moves && integer_literal_value(index).is_none() {
            self.error(format!(
                "dynamic indexing requires Copy elements, found `{}`; use a constant index to move a resource element",
                self.diagnostic_type_name(&element_ty)
            ));
            return error_expr();
        }

        let index = match integer_literal_value(index) {
            Some(value) => {
                if value < 0 || u64::try_from(value).map_or(true, |value| value >= length) {
                    self.error(format!(
                        "array index {value} is out of bounds for length {length}"
                    ));
                    HirIndex::Static(0)
                } else {
                    HirIndex::Static(value as u64)
                }
            }
            None => HirIndex::Dynamic(Box::new(lowered_index)),
        };
        HirExpr {
            ty: element_ty,
            kind: HirExprKind::Index {
                base: Box::new(base),
                index,
                length,
                moves,
            },
        }
    }
}
