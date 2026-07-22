use crate::ast::{BinaryOp, Expr, PassMode, UnaryOp};
use crate::core::LangItemKind;

use super::hir::{HirExpr, Ty};

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
