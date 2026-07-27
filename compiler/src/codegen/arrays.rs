use crate::ast::{CallArg, Expr, Type};
use crate::core::LangItemKind;
use std::collections::HashSet;

use super::compile_time::usize_value_marker;
use super::flow::LowerCtx;
use super::hir::{
    HirArgument, HirBinding, HirExpr, HirExprKind, HirIndex, HirPlace, HirStmt, LoanId,
    LocalCapability, Ty,
};
use super::lower::{error_expr, integer_literal_value, BoundMethodConstraint, TypeProbe};
use super::Analyzer;

const ARRAY_LITERAL_TRAIT: &str = "core::literal::array_literal";

impl Analyzer {
    pub(super) fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let expected_array = expected.and_then(|expected| match expected {
            Ty::Array(element, length) => Some((element.as_ref().clone(), *length)),
            Ty::Slice(element) => Some((element.as_ref().clone(), elements.len() as u64)),
            _ => None,
        });
        let protocol_element = expected
            .filter(|expected| !matches!(expected, Ty::Array(_, _) | Ty::Error))
            .and_then(|expected| {
                self.literal_protocol_element(ARRAY_LITERAL_TRAIT, expected)
                    .or_else(|| match expected {
                        Ty::Reference {
                            pointee,
                            mutable: false,
                            ..
                        } => match pointee.as_ref() {
                            Ty::Slice(element) => Some(element.as_ref().clone()),
                            _ => None,
                        },
                        _ => None,
                    })
            });

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
            let backing = HirExpr {
                ty: array_ty,
                kind: HirExprKind::Array(elements),
            };
            self.ensure_array_trait_extensions(&backing.ty);
            self.require_literal_protocol_impl(
                ARRAY_LITERAL_TRAIT,
                "from_array_literal",
                &backing.ty,
                &backing.ty,
            );
            return backing;
        }

        let (element_ty, lowered) = if let Some(element_ty) = protocol_element {
            (
                element_ty.clone(),
                elements
                    .iter()
                    .map(|element| self.lower_expr(element, Some(&element_ty), context))
                    .collect::<Vec<_>>(),
            )
        } else {
            let Some((first, rest)) = elements.split_first() else {
                self.error("empty array literal requires an expected literal target");
                return error_expr();
            };
            let first = self.lower_expr(first, None, context);
            let element_ty = first.ty.clone();
            let mut lowered = vec![first];
            lowered.extend(
                rest.iter()
                    .map(|element| self.lower_expr(element, Some(&element_ty), context)),
            );
            (element_ty, lowered)
        };
        if element_ty == Ty::Unit {
            self.error("array element type `()` is not supported in the first version");
        }
        let array_ty = Ty::Array(Box::new(element_ty), elements.len() as u64);
        self.array_types.insert(array_ty.clone());
        let backing = HirExpr {
            ty: array_ty,
            kind: HirExprKind::Array(lowered),
        };
        self.ensure_array_trait_extensions(&backing.ty);
        match expected {
            Some(expected) if *expected != backing.ty && *expected != Ty::Error => self
                .lower_literal_protocol_call(
                    ARRAY_LITERAL_TRAIT,
                    "from_array_literal",
                    backing,
                    expected,
                    context,
                ),
            _ => backing,
        }
    }

    pub(super) fn literal_protocol_element(&self, trait_name: &str, output: &Ty) -> Option<Ty> {
        self.trait_impls.values().find_map(|implementation| {
            (implementation.key.trait_ref.name == trait_name
                && implementation.associated_types.get("output") == Some(output))
            .then(|| implementation.key.trait_ref.arguments.first().cloned())
            .flatten()
        })
    }

    pub(super) fn lower_literal_protocol_call(
        &mut self,
        trait_name: &str,
        member: &str,
        backing: HirExpr,
        expected: &Ty,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Ty::Reference {
            pointee, mutable, ..
        } = expected
        {
            if !*mutable && matches!(pointee.as_ref(), Ty::Slice(_)) {
                self.ensure_slice_inherent_extensions(pointee);
                self.require_literal_protocol_impl(trait_name, member, &backing.ty, pointee);
                return self.lower_literal_slice_borrow(backing, expected, context);
            }
        }
        let candidates = self
            .trait_impls
            .values()
            .filter(|implementation| {
                implementation.key.trait_ref.name == trait_name
                    && implementation.associated_types.get("output") == Some(expected)
                    && self.literal_trait_arguments_match(
                        trait_name,
                        &implementation.key.trait_ref.arguments,
                        &backing.ty,
                    )
            })
            .filter_map(|implementation| implementation.methods.get(member).cloned())
            .collect::<Vec<_>>();
        let method = match candidates.as_slice() {
            [method] => method.clone(),
            [] => {
                self.error(format!(
                    "type `{}` does not implement `{}` for this literal",
                    self.diagnostic_type_name(expected),
                    trait_name.rsplit("::").next().unwrap_or(trait_name)
                ));
                return error_expr();
            }
            _ => {
                self.error(format!(
                    "literal construction for `{}` is ambiguous",
                    self.diagnostic_type_name(expected)
                ));
                return error_expr();
            }
        };
        let length = match &backing.ty {
            Ty::Array(_, length) => *length,
            _ => unreachable!("literal backing is always a fixed-size array"),
        };
        let canonical = if self.function_templates.contains_key(&method) {
            let marker = Ty::Struct(usize_value_marker(length));
            let Some(canonical) = self.ensure_function_instance(
                &method,
                vec![Type::CompileUSize(length)],
                vec![marker],
            ) else {
                return error_expr();
            };
            canonical
        } else {
            method
        };
        self.require_function_effects(&canonical, context);
        let signature = self.signatures[&canonical].clone();
        if !matches!(signature.groups.as_slice(), [group] if matches!(group.as_slice(), [parameter] if parameter.ty == backing.ty))
            || signature.result.as_ref() != Some(expected)
        {
            self.error(format!(
                "literal trait implementation `{canonical}` does not preserve its source-backed signature"
            ));
            return error_expr();
        }
        HirExpr {
            ty: expected.clone(),
            kind: HirExprKind::Call {
                function: canonical,
                arguments: vec![HirArgument::Move(backing)],
                consumed_callable: None,
                diverges: self.is_uninhabited_type(expected),
            },
        }
    }

    pub(super) fn require_literal_protocol_impl(
        &mut self,
        trait_name: &str,
        member: &str,
        backing: &Ty,
        output: &Ty,
    ) {
        let exists = self.trait_impls.values().any(|implementation| {
            implementation.key.trait_ref.name == trait_name
                && implementation.associated_types.get("output") == Some(output)
                && implementation.methods.contains_key(member)
                && self.literal_trait_arguments_match(
                    trait_name,
                    &implementation.key.trait_ref.arguments,
                    backing,
                )
        });
        if !exists {
            self.error(format!(
                "core literal backing `{}` is missing its source-backed `{}` implementation",
                self.diagnostic_type_name(output),
                trait_name.rsplit("::").next().unwrap_or(trait_name)
            ));
        }
    }

    fn literal_trait_arguments_match(
        &self,
        trait_name: &str,
        arguments: &[Ty],
        backing: &Ty,
    ) -> bool {
        match (trait_name, backing) {
            (ARRAY_LITERAL_TRAIT, Ty::Array(element, _)) => {
                arguments == std::slice::from_ref(element.as_ref())
            }
            ("core::literal::string_literal", Ty::Array(element, _)) => {
                element.as_ref() == &Ty::U8 && arguments.is_empty()
            }
            _ => false,
        }
    }

    fn lower_literal_slice_borrow(
        &mut self,
        backing: HirExpr,
        expected: &Ty,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let id = context.fresh_local();
        let backing_ty = backing.ty.clone();
        let place = HirPlace {
            local: id,
            root_ty: backing_ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
            ty: backing_ty.clone(),
            capability: LocalCapability::Owned,
            root_mutable: false,
            loan: None,
            indirect: false,
        };
        HirExpr {
            ty: expected.clone(),
            kind: HirExprKind::Block(
                vec![HirStmt::Let(HirBinding {
                    id,
                    name: "$literal backing".to_owned(),
                    ty: backing_ty,
                    mutable: false,
                    value: backing,
                })],
                Some(Box::new(HirExpr {
                    ty: expected.clone(),
                    kind: HirExprKind::Borrow {
                        place,
                        mutable: false,
                    },
                })),
            ),
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
                && implementation.trait_ref.arguments == [Ty::USize]
        });
        if !implements_index {
            self.error(format!(
                "type `{}` does not implement `Index(usize)` required by array brackets",
                self.diagnostic_type_name(&base.ty)
            ));
            let _ = self.lower_expr(index, Some(&Ty::USize), context);
            return error_expr();
        }
        let Ty::Array(element, length) = &base.ty else {
            return error_expr();
        };
        let element_ty = element.as_ref().clone();
        let length = *length;
        let index_hint = matches!(
            self.probe_expr_ty(index, None, context),
            TypeProbe::Defaultable(_)
        )
        .then_some(&Ty::USize);
        let lowered_index = self.lower_expr(index, index_hint, context);
        self.require_same_type(&lowered_index.ty, &Ty::USize, "array index");

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
