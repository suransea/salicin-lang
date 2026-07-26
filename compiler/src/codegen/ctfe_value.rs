use super::hir::{LayoutQueryKind, Ty};

/// One normalized runtime-typed value produced by compile-time evaluation.
///
/// Static metadata such as `type`, `effect`, and finite-sort members remains
/// in `StaticValue`; it must not be encoded in this runtime value domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CtfeValue {
    pub(super) ty: Ty,
    pub(super) kind: CtfeValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CtfeValueKind {
    Integer(i128),
    Bool(bool),
    Unit,
    Tuple(Vec<CtfeValue>),
    Array(Vec<CtfeValue>),
    Struct {
        name: String,
        fields: Vec<CtfeValue>,
    },
    Enum {
        name: String,
        variant: usize,
        fields: Vec<CtfeValue>,
    },
    LayoutQuery {
        queried: Ty,
        kind: LayoutQueryKind,
    },
}

impl CtfeValue {
    pub(super) fn integer(ty: Ty, value: i128) -> Self {
        debug_assert!(ty.is_integer());
        Self {
            ty,
            kind: CtfeValueKind::Integer(value),
        }
    }

    pub(super) fn usize(value: u64) -> Self {
        Self::integer(Ty::USize, i128::from(value))
    }

    pub(super) fn bool(value: bool) -> Self {
        Self {
            ty: Ty::Bool,
            kind: CtfeValueKind::Bool(value),
        }
    }

    pub(super) fn unit() -> Self {
        Self {
            ty: Ty::Unit,
            kind: CtfeValueKind::Unit,
        }
    }

    pub(super) fn integer_value(&self) -> Option<i128> {
        match self.kind {
            CtfeValueKind::Integer(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn usize_value(&self) -> Option<u64> {
        (self.ty == Ty::USize)
            .then(|| self.integer_value())
            .flatten()
            .and_then(|value| u64::try_from(value).ok())
    }

    pub(super) fn bool_value(&self) -> Option<bool> {
        match self.kind {
            CtfeValueKind::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn projection(&self, index: usize) -> Option<&CtfeValue> {
        match &self.kind {
            CtfeValueKind::Tuple(fields) | CtfeValueKind::Array(fields) => fields.get(index),
            CtfeValueKind::Struct { fields, .. } => fields.get(index),
            CtfeValueKind::Enum { fields, .. } => fields.get(index),
            CtfeValueKind::Integer(_)
            | CtfeValueKind::Bool(_)
            | CtfeValueKind::Unit
            | CtfeValueKind::LayoutQuery { .. } => None,
        }
    }
}
