use crate::ast::{CallArg, Type};

use super::hir::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StandardFallibleKind {
    Option,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StandardFallibleInfo {
    pub(super) kind: StandardFallibleKind,
    pub(super) payload: Ty,
    pub(super) payload_source: Option<Type>,
    pub(super) error: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReturnBoundary {
    pub(super) kind: Option<StandardFallibleKind>,
    pub(super) container: Ty,
    pub(super) success: Ty,
    pub(super) error: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CoalescePayloadHint {
    pub(super) ty: Ty,
    pub(super) source: Option<Type>,
}

pub(super) struct InferredCoalesceLhs<'a> {
    pub(super) kind: StandardFallibleKind,
    pub(super) name: String,
    pub(super) type_groups: Vec<&'a [CallArg]>,
    pub(super) variant: &'a str,
    pub(super) value_groups: Vec<&'a [CallArg]>,
}

#[derive(Clone, Copy)]
pub(super) struct InferredEnumHints<'a> {
    pub(super) payload: Option<&'a CoalescePayloadHint>,
    pub(super) result: Option<&'a Ty>,
}
