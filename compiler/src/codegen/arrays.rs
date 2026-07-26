use crate::ast::{CallArg, Expr};
use crate::core::LangItemKind;
use std::collections::HashSet;

use super::flow::LowerCtx;
use super::hir::{HirExpr, HirExprKind, HirIndex, LoanId, Ty};
use super::lower::{error_expr, integer_literal_value, BoundMethodConstraint};
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
        if !matches!(base, Expr::Array(_))
            && !matches!(
                self.probe_expr_ty(base, None, context),
                super::lower::TypeProbe::Known(Ty::Array(_, _))
                    | super::lower::TypeProbe::KnownSource(Ty::Array(_, _), _)
            )
        {
            return self.lower_protocol_index(base, index, context);
        }
        let base = self.lower_expr(base, None, context);
        self.ensure_array_trait_extensions(&base.ty);
        let implements_index = self.trait_impls.keys().any(|implementation| {
            implementation.self_ty == base.ty
                && implementation.trait_ref.name == self.lang_item_name(LangItemKind::Index)
                && implementation.trait_ref.arguments == [Ty::I32]
        });
        if !implements_index {
            self.error(format!(
                "type `{}` does not implement `Index(i32)` required by array brackets",
                self.diagnostic_type_name(&base.ty)
            ));
            let _ = self.lower_expr(index, Some(&Ty::I32), context);
            return error_expr();
        }
        let Ty::Array(element, length) = &base.ty else {
            return error_expr();
        };
        let element_ty = element.as_ref().clone();
        let length = *length;
        let lowered_index = self.lower_expr(index, None, context);
        self.require_same_type(&lowered_index.ty, &Ty::I32, "array index");

        let moves = !self.is_copy_type(&element_ty);
        if moves && integer_literal_value(index).is_none() {
            self.error(format!(
                "dynamic indexing requires copyable elements, found `{}`; use a constant index to move a resource element",
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

    fn lower_protocol_index(
        &mut self,
        base: &Expr,
        index: &Expr,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let loans = Self::loan_snapshot(context);
        let reference = self.lower_protocol_index_reference(base, index, false, context);
        self.release_loans_since(&loans, context);
        let Ty::Reference { pointee, .. } = &reference.ty else {
            return error_expr();
        };
        let element = pointee.as_ref().clone();
        if !self.is_copy_type(&element) {
            self.error(format!(
                "indexed value access requires copyable output, found `{}`; borrow the indexed place instead",
                self.diagnostic_type_name(&element)
            ));
            return error_expr();
        }
        HirExpr {
            ty: element,
            kind: HirExprKind::ReferenceRead(Box::new(reference)),
        }
    }

    pub(super) fn lower_protocol_index_reference(
        &mut self,
        base: &Expr,
        index: &Expr,
        mutable: bool,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let access_group = [CallArg {
            label: None,
            value: Expr::Name(if mutable { "mut" } else { "shared" }.to_owned()),
        }];
        let key_group = [CallArg {
            label: None,
            value: index.clone(),
        }];
        let reference = self.lower_bound_method_call(
            base,
            "index",
            &[access_group.as_slice(), key_group.as_slice()],
            BoundMethodConstraint::LangItem(LangItemKind::Index),
            None,
            context,
        );
        let Ty::Reference {
            mutable: actual_mutable,
            ..
        } = &reference.ty
        else {
            if reference.ty != Ty::Error {
                self.error(format!(
                    "`Index.index` must return a borrow, found `{}`",
                    reference.ty
                ));
            }
            return error_expr();
        };
        if *actual_mutable != mutable {
            self.error(format!(
                "`Index.index({})` must return a {} borrow, found `{}`",
                if mutable { "mut" } else { "shared" },
                if mutable { "mutable" } else { "shared" },
                reference.ty
            ));
            return error_expr();
        }
        reference
    }

    pub(super) fn loan_snapshot(context: &LowerCtx) -> HashSet<LoanId> {
        context.flow.loans.keys().copied().collect()
    }

    pub(super) fn release_loans_since(
        &mut self,
        snapshot: &HashSet<LoanId>,
        context: &mut LowerCtx,
    ) {
        let loans = context
            .flow
            .loans
            .keys()
            .copied()
            .filter(|loan| !snapshot.contains(loan))
            .collect::<Vec<_>>();
        self.release_loans(&loans, context);
        for scope in &mut context.scopes {
            scope.lexical_loans.retain(|loan| !loans.contains(loan));
        }
    }
}
