use crate::ast::{BinaryOp, Expr, ItemOrigin, PassMode, UnaryOp};
use crate::core::LangItemKind;

use super::flow::{FlowState, LowerCtx};
use super::hir::{HirArgument, HirExpr, HirExprKind, HirMatchArm, HirMatcher, Ty};
use super::lower::{
    error_expr, integer_fits, integer_literal_value, BinaryOperatorCandidate, TypeProbe,
};
use super::names::canonical_type_encoding;
use super::Analyzer;

#[derive(Debug, Clone, Copy)]
pub(super) struct BinaryOperatorTrait {
    pub(super) operator: BinaryOp,
    pub(super) lang_item: LangItemKind,
    pub(super) parameter_mode: PassMode,
    pub(super) method_output: OperatorMethodOutput,
    pub(super) result_transform: OperatorResultTransform,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UnaryOperatorTrait {
    pub(super) operator: UnaryOp,
    pub(super) lang_item: LangItemKind,
}

impl UnaryOperatorTrait {
    pub(super) fn method(self) -> &'static str {
        self.lang_item
            .operator_method()
            .expect("unary operator lang items have a method")
    }
}

pub(super) const UNARY_OPERATOR_TRAITS: [UnaryOperatorTrait; 2] = [
    UnaryOperatorTrait {
        operator: UnaryOp::Neg,
        lang_item: LangItemKind::Neg,
    },
    UnaryOperatorTrait {
        operator: UnaryOp::Not,
        lang_item: LangItemKind::Not,
    },
];

pub(super) fn unary_operator_trait(operator: UnaryOp) -> Option<UnaryOperatorTrait> {
    UNARY_OPERATOR_TRAITS
        .iter()
        .copied()
        .find(|candidate| candidate.operator == operator)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatorMethodOutput {
    Associated,
    Bool,
    PartialOrdering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatorResultTransform {
    Direct,
    NegateBool,
    PartialOrdering(u8),
}

impl BinaryOperatorTrait {
    pub(super) fn method(self) -> &'static str {
        self.lang_item
            .operator_method()
            .expect("binary operator lang items have a method")
    }

    pub(super) fn expression_output(self, method_output: &Ty) -> Ty {
        match self.result_transform {
            OperatorResultTransform::Direct => method_output.clone(),
            OperatorResultTransform::NegateBool | OperatorResultTransform::PartialOrdering(_) => {
                Ty::Bool
            }
        }
    }
}

pub(super) enum BinaryOperatorLeft<'a> {
    Source(&'a Expr),
    Lowered(Box<HirExpr>),
}

impl Analyzer {
    pub(super) fn builtin_unary_operator_output(
        &self,
        trait_name: &str,
        subject: &Ty,
        arguments: &[Ty],
    ) -> Option<Ty> {
        if !arguments.is_empty() {
            return None;
        }
        if trait_name == self.lang_item_name(LangItemKind::Neg)
            && subject.is_integer()
            && subject.is_signed()
        {
            Some(subject.clone())
        } else if trait_name == self.lang_item_name(LangItemKind::Not) && *subject == Ty::Bool {
            Some(Ty::Bool)
        } else {
            None
        }
    }

    pub(super) fn binary_operator_candidates(
        &self,
        operator_trait: BinaryOperatorTrait,
        receiver: &Ty,
        right: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Vec<BinaryOperatorCandidate> {
        let trait_name = self.lang_item_name(operator_trait.lang_item);
        if !self
            .traits
            .get(trait_name)
            .is_some_and(|schema| schema.valid)
        {
            return Vec::new();
        }

        let mut candidates = self
            .trait_impls
            .values()
            .filter_map(|implementation| {
                if implementation.key.self_ty != *receiver
                    || implementation.key.trait_ref.name != trait_name
                    || !Self::access_boundary_allows(&context.origin, &implementation.access)
                {
                    return None;
                }
                let [rhs] = implementation.key.trait_ref.arguments.as_slice() else {
                    return None;
                };
                let method = implementation.methods.get(operator_trait.method())?;
                let fixed_output;
                let output = match operator_trait.method_output {
                    OperatorMethodOutput::Associated => {
                        implementation.associated_types.get("Output")?
                    }
                    OperatorMethodOutput::Bool => {
                        fixed_output = Ty::Bool;
                        &fixed_output
                    }
                    OperatorMethodOutput::PartialOrdering => {
                        fixed_output = Ty::Enum(
                            self.lang_item_name(LangItemKind::PartialOrdering)
                                .to_owned(),
                        );
                        &fixed_output
                    }
                };
                if integer_literal_value(right).is_some_and(|value| !integer_fits(value, rhs)) {
                    return None;
                }
                let right_probe = self.probe_expr_ty(right, Some(rhs), context);
                Self::probe_matches_type(&right_probe, rhs).then(|| BinaryOperatorCandidate {
                    method: method.clone(),
                    rhs: rhs.clone(),
                    output: output.clone(),
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            canonical_type_encoding(&left.rhs)
                .cmp(&canonical_type_encoding(&right.rhs))
                .then_with(|| {
                    canonical_type_encoding(&left.output)
                        .cmp(&canonical_type_encoding(&right.output))
                })
                .then_with(|| left.method.cmp(&right.method))
        });

        if let Some(expected) = expected.filter(|ty| **ty != Ty::Error) {
            let has_exact_output = candidates
                .iter()
                .any(|candidate| operator_trait.expression_output(&candidate.output) == *expected);
            if has_exact_output {
                candidates.retain(|candidate| {
                    operator_trait.expression_output(&candidate.output) == *expected
                });
            } else {
                candidates.retain(|candidate| {
                    operator_trait.result_transform == OperatorResultTransform::Direct
                        && self.is_uninhabited_type(&candidate.output)
                });
            }
        }
        candidates
    }

    pub(super) fn unary_operator_candidate(
        &self,
        operator: UnaryOperatorTrait,
        receiver: &Ty,
        expected: Option<&Ty>,
        origin: &ItemOrigin,
    ) -> Option<(String, Ty)> {
        let trait_name = self.lang_item_name(operator.lang_item);
        let implementation = self.trait_impls.values().find(|implementation| {
            implementation.key.self_ty == *receiver
                && implementation.key.trait_ref.name == trait_name
                && implementation.key.trait_ref.arguments.is_empty()
                && Self::access_boundary_allows(origin, &implementation.access)
        })?;
        let method = implementation.methods.get(operator.method())?.clone();
        let output = implementation.associated_types.get("Output")?.clone();
        expected
            .filter(|expected| **expected != Ty::Error)
            .is_none_or(|expected| output == *expected || self.is_uninhabited_type(&output))
            .then_some((method, output))
    }

    pub(super) fn probe_unary_ty(
        &self,
        operator: UnaryOp,
        operand: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let operand_probe = self.probe_expr_ty(operand, None, context);
        if let Some(receiver) = Self::nominal_ty_from_probe(&operand_probe) {
            let operator = unary_operator_trait(operator)
                .expect("overloadable unary operators have a core trait specification");
            return self
                .unary_operator_candidate(operator, &receiver, expected, &context.origin)
                .map_or(TypeProbe::Unsupported, |(_, output)| {
                    TypeProbe::Known(output)
                });
        }
        match operator {
            UnaryOp::Neg => {
                self.probe_expr_ty(operand, expected.filter(|ty| ty.is_signed()), context)
            }
            UnaryOp::Not => TypeProbe::Known(Ty::Bool),
            UnaryOp::Deref => TypeProbe::Unsupported,
        }
    }

    fn probe_numeric_binary_ty(
        &self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let numeric_hint = hint.filter(|ty| ty.is_integer());
        match self.probe_expr_ty(left, numeric_hint, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) if ty.is_integer() => {
                TypeProbe::Known(ty)
            }
            _ => match self.probe_expr_ty(right, numeric_hint, context) {
                TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) if ty.is_integer() => {
                    TypeProbe::Known(ty)
                }
                TypeProbe::Defaultable(ty) if ty.is_integer() => TypeProbe::Defaultable(ty),
                _ => TypeProbe::Unsupported,
            },
        }
    }

    pub(super) fn probe_arithmetic_ty(
        &self,
        operator: BinaryOp,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let left_probe = self.probe_expr_ty(left, None, context);
        if let Some(receiver) = Self::nominal_ty_from_probe(&left_probe) {
            let operator_trait = binary_operator_trait(operator)
                .expect("arithmetic operators have a core trait specification");
            let candidates = self.binary_operator_candidates(
                operator_trait,
                &receiver,
                right,
                expected,
                context,
            );
            return match candidates.as_slice() {
                [candidate] => TypeProbe::Known(candidate.output.clone()),
                [] | [_, _, ..] => TypeProbe::Unsupported,
            };
        }
        self.probe_numeric_binary_ty(left, right, expected, context)
    }

    pub(super) fn lower_trait_binary(
        &mut self,
        operator_trait: BinaryOperatorTrait,
        left: BinaryOperatorLeft<'_>,
        right: &Expr,
        receiver: &Ty,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let trait_name = self.lang_item_name(operator_trait.lang_item).to_owned();
        let trait_display_name = operator_trait.lang_item.source_name();
        let method_name = operator_trait.method();
        let spelling = binary_spelling(operator_trait.operator);
        let Some(schema) = self.traits.get(&trait_name) else {
            self.error(format!(
                "operator `{spelling}` for `{receiver}` requires the core `{trait_display_name}` trait"
            ));
            return error_expr();
        };
        if !schema.valid {
            return error_expr();
        }

        let candidates =
            self.binary_operator_candidates(operator_trait, receiver, right, expected, context);
        let candidate = match candidates.as_slice() {
            [candidate] => candidate.clone(),
            [] => {
                let right_probe = self.probe_expr_ty(right, None, context);
                let right_ty = match right_probe {
                    TypeProbe::Known(ty)
                    | TypeProbe::KnownSource(ty, _)
                    | TypeProbe::Defaultable(ty) => Some(ty),
                    TypeProbe::Unsupported => None,
                };
                let expected_output = expected
                    .filter(|ty| **ty != Ty::Error)
                    .map(|ty| format!(" producing `{ty}`"))
                    .unwrap_or_default();
                if let Some(right_ty) = right_ty {
                    self.error(format!(
                        "no matching `{trait_display_name}` implementation for `{receiver}` with right operand `{right_ty}`{expected_output}"
                    ));
                } else {
                    self.error(format!(
                        "no matching `{trait_display_name}` implementation for `{receiver}` with an unresolved right operand{expected_output}"
                    ));
                }
                return error_expr();
            }
            [_, _, ..] => {
                let descriptions = candidates
                    .iter()
                    .map(|candidate| {
                        if operator_trait.method_output == OperatorMethodOutput::Associated {
                            format!(
                                "`{trait_display_name}({}, Output = {})`",
                                candidate.rhs, candidate.output
                            )
                        } else {
                            format!("`{trait_display_name}({})`", candidate.rhs)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error(format!(
                    "ambiguous `{spelling}` for `{receiver}`; matching implementations: {descriptions}"
                ));
                return error_expr();
            }
        };

        let Some(signature) = self.signatures.get(&candidate.method).cloned() else {
            self.error(format!(
                "internal error: `{trait_display_name}.{method_name}` implementation has no function signature"
            ));
            return error_expr();
        };
        let valid_signature = signature.groups.len() == 2
            && signature.groups[0].len() == 1
            && signature.groups[1].len() == 1
            && signature.groups[0][0].ty == *receiver
            && signature.groups[0][0].mode == operator_trait.parameter_mode
            && signature.groups[1][0].ty == candidate.rhs
            && signature.groups[1][0].mode == operator_trait.parameter_mode
            && signature.result.as_ref() == Some(&candidate.output);
        if !valid_signature {
            self.error(format!(
                "internal error: invalid registered `{trait_display_name}.{method_name}` signature"
            ));
            return error_expr();
        }
        let receiver_parameter = signature.groups[0][0].clone();
        let rhs_parameter = signature.groups[1][0].clone();
        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        let left = match left {
            BinaryOperatorLeft::Lowered(left) => {
                self.require_same_type(
                    &left.ty,
                    &receiver_parameter.ty,
                    format!("left operand of overloaded `{spelling}`"),
                );
                HirArgument::Move(*left)
            }
            BinaryOperatorLeft::Source(left) => self.lower_call_argument(
                left,
                &receiver_parameter,
                context,
                &mut temporary_loans,
                &mut temporary_bindings,
            ),
        };
        let right = self.lower_call_argument(
            right,
            &rhs_parameter,
            context,
            &mut temporary_loans,
            &mut temporary_bindings,
        );
        self.release_loans(&temporary_loans, context);
        let mut arguments = vec![left, right];
        let call = HirExpr {
            ty: candidate.output.clone(),
            kind: HirExprKind::Call {
                function: candidate.method,
                arguments: arguments.clone(),
                consumed_callable: None,
                diverges: self.is_uninhabited_type(&candidate.output),
            },
        };
        let call =
            self.wrap_call_argument_temporaries(call, &mut arguments, temporary_bindings, context);
        match operator_trait.result_transform {
            OperatorResultTransform::Direct => call,
            OperatorResultTransform::NegateBool => HirExpr {
                ty: Ty::Bool,
                kind: HirExprKind::Unary(UnaryOp::Not, Box::new(call)),
            },
            OperatorResultTransform::PartialOrdering(mask) => HirExpr {
                ty: Ty::Bool,
                kind: HirExprKind::Match {
                    scrutinee: Box::new(call),
                    arms: (0..4)
                        .map(|variant| HirMatchArm {
                            matcher: HirMatcher::Variant(variant),
                            bindings: Vec::new(),
                            guard: None,
                            body: HirExpr {
                                ty: Ty::Bool,
                                kind: HirExprKind::Bool(variant < 3 && mask & (1 << variant) != 0),
                            },
                        })
                        .collect(),
                },
            },
        }
    }

    pub(super) fn lower_trait_unary(
        &mut self,
        operator: UnaryOperatorTrait,
        operand: &Expr,
        receiver: &Ty,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let trait_name = operator.lang_item.source_name();
        let spelling = unary_spelling(operator.operator);
        let Some((method, output)) =
            self.unary_operator_candidate(operator, receiver, expected, &context.origin)
        else {
            self.error(format!(
                "no matching `{trait_name}` implementation for unary `{spelling}` on `{receiver}`"
            ));
            return error_expr();
        };
        let Some(signature) = self.signatures.get(&method).cloned() else {
            self.error(format!(
                "internal error: `{trait_name}.{}` implementation has no function signature",
                operator.method()
            ));
            return error_expr();
        };
        let valid_signature = signature.groups.len() == 2
            && signature.groups[0].len() == 1
            && signature.groups[1].is_empty()
            && signature.groups[0][0].ty == *receiver
            && signature.groups[0][0].mode == PassMode::Move
            && signature.result.as_ref() == Some(&output);
        if !valid_signature {
            self.error(format!(
                "internal error: invalid registered `{trait_name}.{}` signature",
                operator.method()
            ));
            return error_expr();
        }
        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        let argument = self.lower_call_argument(
            operand,
            &signature.groups[0][0],
            context,
            &mut temporary_loans,
            &mut temporary_bindings,
        );
        self.release_loans(&temporary_loans, context);
        let mut arguments = vec![argument];
        let call = HirExpr {
            ty: output.clone(),
            kind: HirExprKind::Call {
                function: method,
                arguments: arguments.clone(),
                consumed_callable: None,
                diverges: self.is_uninhabited_type(&output),
            },
        };
        self.wrap_call_argument_temporaries(call, &mut arguments, temporary_bindings, context)
    }

    pub(super) fn lower_binary(
        &mut self,
        left: &Expr,
        operator: BinaryOp,
        right: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        use BinaryOp::*;
        if let Some(operator_trait) = binary_operator_trait(operator) {
            let left_probe = self.probe_expr_ty(left, None, context);
            if let Some(receiver) = Self::nominal_ty_from_probe(&left_probe) {
                return self.lower_trait_binary(
                    operator_trait,
                    BinaryOperatorLeft::Source(left),
                    right,
                    &receiver,
                    expected,
                    context,
                );
            }
            if left_probe == TypeProbe::Unsupported
                && operator_trait.parameter_mode == PassMode::Move
            {
                let lowered_left = self.lower_expr(left, None, context);
                if matches!(lowered_left.ty, Ty::Struct(_) | Ty::Enum(_)) {
                    let receiver = lowered_left.ty.clone();
                    return self.lower_trait_binary(
                        operator_trait,
                        BinaryOperatorLeft::Lowered(Box::new(lowered_left)),
                        right,
                        &receiver,
                        expected,
                        context,
                    );
                }

                let right_hint = lowered_left.ty.is_integer().then_some(&lowered_left.ty);
                let lowered_right = self.lower_expr(right, right_hint, context);
                if !lowered_left.ty.is_integer() {
                    self.error(format!(
                        "operator `{}` requires integer operands, found `{}`",
                        binary_spelling(operator),
                        lowered_left.ty,
                    ));
                }
                self.require_same_type(
                    &lowered_left.ty,
                    &lowered_right.ty,
                    format!("operands of `{}`", binary_spelling(operator)),
                );
                let ty = lowered_left.ty.clone();
                return HirExpr {
                    ty,
                    kind: HirExprKind::Binary(
                        Box::new(lowered_left),
                        operator,
                        Box::new(lowered_right),
                    ),
                };
            }
        }
        let (left, right, ty) = match operator {
            And | Or => {
                let left = self.lower_expr(left, Some(&Ty::Bool), context);
                let skip_flow = context.flow.clone();
                let (right, right_flow) =
                    self.lower_expr_from_flow(right, Some(&Ty::Bool), &skip_flow, context);
                context.flow = FlowState::join(&[skip_flow, right_flow]);
                (left, right, Ty::Bool)
            }
            Add | Sub | Mul | Div | Rem | BitAnd | BitOr | BitXor | Shl | Shr | Lt | Le | Gt
            | Ge => {
                let numeric_hint = expected.filter(|ty| ty.is_integer());
                let (left, right) = self.lower_numeric_pair(left, right, numeric_hint, context);
                if !left.ty.is_integer() {
                    self.error(format!(
                        "operator `{}` requires integer operands, found `{}`",
                        binary_spelling(operator),
                        left.ty
                    ));
                }
                self.require_same_type(
                    &left.ty,
                    &right.ty,
                    format!("operands of `{}`", binary_spelling(operator)),
                );
                let ty = if matches!(operator, Lt | Le | Gt | Ge) {
                    Ty::Bool
                } else {
                    left.ty.clone()
                };
                (left, right, ty)
            }
            Eq | Ne => {
                let (left, right) = self.lower_numeric_or_bool_pair(left, right, context);
                if !(left.ty.is_integer() || left.ty == Ty::Bool || left.ty == Ty::Error) {
                    self.error(format!(
                        "operator `{}` does not support `{}`",
                        binary_spelling(operator),
                        left.ty
                    ));
                }
                self.require_same_type(
                    &left.ty,
                    &right.ty,
                    format!("operands of `{}`", binary_spelling(operator)),
                );
                (left, right, Ty::Bool)
            }
        };
        HirExpr {
            ty,
            kind: HirExprKind::Binary(Box::new(left), operator, Box::new(right)),
        }
    }

    fn lower_numeric_pair(
        &mut self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> (HirExpr, HirExpr) {
        if let Some(hint) = hint {
            return (
                self.lower_expr(left, Some(hint), context),
                self.lower_expr(right, Some(hint), context),
            );
        }
        match (left, right) {
            (Expr::Integer(_), Expr::Integer(_)) => (
                self.lower_expr(left, Some(&Ty::I32), context),
                self.lower_expr(right, Some(&Ty::I32), context),
            ),
            (Expr::Integer(_), _) => {
                let right = self.lower_expr(right, None, context);
                let left = self.lower_expr(left, Some(&right.ty), context);
                (left, right)
            }
            (_, Expr::Integer(_)) => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
            _ => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
        }
    }

    fn lower_numeric_or_bool_pair(
        &mut self,
        left: &Expr,
        right: &Expr,
        context: &mut LowerCtx,
    ) -> (HirExpr, HirExpr) {
        match (left, right) {
            (Expr::Integer(_), Expr::Integer(_)) => (
                self.lower_expr(left, Some(&Ty::I32), context),
                self.lower_expr(right, Some(&Ty::I32), context),
            ),
            (Expr::Integer(_), _) => {
                let right = self.lower_expr(right, None, context);
                let left = self.lower_expr(left, Some(&right.ty), context);
                (left, right)
            }
            _ => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
        }
    }
}

const ORDER_LESS: u8 = 1 << 0;
const ORDER_EQUAL: u8 = 1 << 1;
const ORDER_GREATER: u8 = 1 << 2;

pub(super) const BINARY_OPERATOR_TRAITS: [BinaryOperatorTrait; 16] = [
    BinaryOperatorTrait {
        operator: BinaryOp::Add,
        lang_item: LangItemKind::Add,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Sub,
        lang_item: LangItemKind::Sub,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Mul,
        lang_item: LangItemKind::Mul,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Div,
        lang_item: LangItemKind::Div,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Rem,
        lang_item: LangItemKind::Rem,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::BitAnd,
        lang_item: LangItemKind::BitAnd,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::BitOr,
        lang_item: LangItemKind::BitOr,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::BitXor,
        lang_item: LangItemKind::BitXor,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Shl,
        lang_item: LangItemKind::Shl,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Shr,
        lang_item: LangItemKind::Shr,
        parameter_mode: PassMode::Move,
        method_output: OperatorMethodOutput::Associated,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Eq,
        lang_item: LangItemKind::Eq,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::Bool,
        result_transform: OperatorResultTransform::Direct,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Ne,
        lang_item: LangItemKind::Eq,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::Bool,
        result_transform: OperatorResultTransform::NegateBool,
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Lt,
        lang_item: LangItemKind::PartialOrd,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::PartialOrdering,
        result_transform: OperatorResultTransform::PartialOrdering(ORDER_LESS),
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Le,
        lang_item: LangItemKind::PartialOrd,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::PartialOrdering,
        result_transform: OperatorResultTransform::PartialOrdering(ORDER_LESS | ORDER_EQUAL),
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Gt,
        lang_item: LangItemKind::PartialOrd,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::PartialOrdering,
        result_transform: OperatorResultTransform::PartialOrdering(ORDER_GREATER),
    },
    BinaryOperatorTrait {
        operator: BinaryOp::Ge,
        lang_item: LangItemKind::PartialOrd,
        parameter_mode: PassMode::Borrow,
        method_output: OperatorMethodOutput::PartialOrdering,
        result_transform: OperatorResultTransform::PartialOrdering(ORDER_EQUAL | ORDER_GREATER),
    },
];

pub(super) fn binary_operator_trait(operator: BinaryOp) -> Option<BinaryOperatorTrait> {
    BINARY_OPERATOR_TRAITS
        .iter()
        .copied()
        .find(|candidate| candidate.operator == operator)
}

pub(super) fn assignment_operator_trait(operator: BinaryOp) -> Option<LangItemKind> {
    Some(match operator {
        BinaryOp::Add => LangItemKind::AddAssign,
        BinaryOp::Sub => LangItemKind::SubAssign,
        BinaryOp::Mul => LangItemKind::MulAssign,
        BinaryOp::Div => LangItemKind::DivAssign,
        BinaryOp::Rem => LangItemKind::RemAssign,
        BinaryOp::BitAnd => LangItemKind::BitAndAssign,
        BinaryOp::BitOr => LangItemKind::BitOrAssign,
        BinaryOp::BitXor => LangItemKind::BitXorAssign,
        BinaryOp::Shl => LangItemKind::ShlAssign,
        BinaryOp::Shr => LangItemKind::ShrAssign,
        _ => return None,
    })
}

pub(super) fn binary_spelling(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

pub(super) fn unary_spelling(operator: UnaryOp) -> &'static str {
    match operator {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Deref => "*",
    }
}
