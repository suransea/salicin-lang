use crate::ast::{BinaryOp, Expr, ItemOrigin, PassMode, UnaryOp};
use crate::core::LangItemKind;

use super::flow::LowerCtx;
use super::hir::{HirExpr, Ty};
use super::lower::{integer_fits, integer_literal_value, BinaryOperatorCandidate, TypeProbe};
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
