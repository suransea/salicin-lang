//! Edition-pinned Salicin `core` sources and their language-item contract.
//!
//! The declarations live in ordinary Salicin source. This module only owns
//! bootstrapping: selecting the source for an edition, parsing it, and
//! rejecting a toolchain bundle whose public surface does not have the exact
//! shape required by the compiler.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::ast::{
    AssociatedKind, CompileParam, CompileParamDefault, CompileParamKind, EnumDef, Function, Item,
    ItemOrigin, PassMode, Program, TraitDef, TraitMember, Type, TypeFormDef, VariantDef,
    VariantFields, Visibility,
};
use crate::manifest::Edition;
use crate::modules::{self, PackageId, SourceUnit};
use crate::parser;

const EDITION_2026_LIB: &str = include_str!("../../library/core/src/lib.sc");
const EDITION_2026_PRELUDE: &str = include_str!("../../library/core/src/prelude.sc");
const EDITION_2026_NEVER: &str = include_str!("../../library/core/src/never.sc");
const EDITION_2026_MARKER: &str = include_str!("../../library/core/src/marker.sc");
const EDITION_2026_PRIMITIVES: &str = include_str!("../../library/core/src/primitives.sc");
const EDITION_2026_OPTION: &str = include_str!("../../library/core/src/option.sc");
const EDITION_2026_RESULT: &str = include_str!("../../library/core/src/result.sc");
const EDITION_2026_CMP: &str = include_str!("../../library/core/src/cmp.sc");
const EDITION_2026_FLOW: &str = include_str!("../../library/core/src/flow.sc");
const EDITION_2026_OPS: &str = include_str!("../../library/core/src/ops.sc");
const EDITION_2026_OPS_ARITH: &str = include_str!("../../library/core/src/ops/arith.sc");
const EDITION_2026_OPS_BIT: &str = include_str!("../../library/core/src/ops/bit.sc");
const EDITION_2026_OPS_ASSIGN: &str = include_str!("../../library/core/src/ops/assign.sc");
const EDITION_2026_EFFECT: &str = include_str!("../../library/core/src/effect.sc");
const EDITION_2026_EFFECT_HANDLER: &str = include_str!("../../library/core/src/effect/handler.sc");
const EDITION_2026_DOMAINS: &str = include_str!("../../library/core/src/domains.sc");
const EDITION_2026_CONTROL: &str = include_str!("../../library/core/src/control.sc");
const EDITION_2026_ITER: &str = include_str!("../../library/core/src/iter.sc");
const EDITION_2026_ALGEBRA: &str = include_str!("../../library/core/src/algebra.sc");
const EDITION_2026_FUNCTIONAL: &str = include_str!("../../library/core/src/functional.sc");

const NON_LANG_ITEM_CORE_MODULES: &[&str] =
    &["primitives", "effect", "control", "algebra", "functional"];

#[cfg(test)]
const TEST_ASSIGNMENT_OPS: &str = r#"
pub let AddAssign(Rhs: type) = trait { let add_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let SubAssign(Rhs: type) = trait { let sub_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let MulAssign(Rhs: type) = trait { let mul_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let DivAssign(Rhs: type) = trait { let div_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let RemAssign(Rhs: type) = trait { let rem_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let BitAndAssign(Rhs: type) = trait { let bit_and_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let BitOrAssign(Rhs: type) = trait { let bit_or_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let BitXorAssign(Rhs: type) = trait { let bit_xor_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let ShlAssign(Rhs: type) = trait { let shl_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
pub let ShrAssign(Rhs: type) = trait { let shr_assign(self: borrow(mut)(Self))
  (rhs: Rhs): () }
"#;

#[cfg(test)]
const TEST_CHAIN_OPS: &str = r#"
pub let Chain = trait {
  let Item: type
  let Rebind(Value: type): type

  let chain(E: effect, U: type)
    (self)
    (transform: (Item): U with(E)): Rebind(U) with(E)
}
pub let Coalesce = trait {
  let Item: type

  let coalesce(E: effect)
    (self)
    (fallback: (): Item with(E)): Item with(E)
}
pub let Unwrap = trait {
  let Output: type
  let unwrap(move self): Output
}
pub let Raise = trait {
  let Output: type
  let Error: type
  let raise(move self): Output with(Throws(Error))
}
"#;

#[cfg(test)]
const TEST_EFFECT: &str = r#"
pub let Unsafe = effect {}
pub let Throws(Error: type) = effect { let raise(move error: Error): Never }
"#;

#[cfg(test)]
const TEST_EFFECT_HANDLER: &str = r#"
pub let Continuation(Input: type, Output: type) = struct {}
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}
pub let Handle = trait(Self: effect) {
  let Clauses(Value: type, Answer: type): parameters
  let handle(Value: type, Answer: type, Rest: effect)
    (...move clauses: Clauses(Value, Answer))
    (move action: (): Value with(Self, Rest)): Answer with(Rest)
}
"#;

/// A stable logical role fulfilled by one declaration in the edition's
/// `core` bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LangItemKind {
    Option,
    Result,
    Never,
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    Copy,
    Drop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
    Eq,
    PartialOrdering,
    PartialOrd,
    Neg,
    Not,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Chain,
    Coalesce,
    Unwrap,
    Raise,
    UnsafeEffect,
    ThrowsEffect,
    TypeDomain,
    RegionDomain,
    AccessDomain,
    PassingDomain,
    EffectDomain,
    ParametersDomain,
    BorrowTypeForm,
    BorrowValueForm,
    Continuation,
    EffectCallable,
    Handle,
    Do,
    DoWhile,
    Try,
    Throw,
    Unsafe,
    Loop,
    While,
    If,
    Match,
    For,
    Iterator,
    IntoIterator,
}

impl LangItemKind {
    const ALL: [Self; 72] = [
        Self::Option,
        Self::Result,
        Self::Never,
        Self::Bool,
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::I128,
        Self::ISize,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::USize,
        Self::Copy,
        Self::Drop,
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Rem,
        Self::AddAssign,
        Self::SubAssign,
        Self::MulAssign,
        Self::DivAssign,
        Self::RemAssign,
        Self::BitAndAssign,
        Self::BitOrAssign,
        Self::BitXorAssign,
        Self::ShlAssign,
        Self::ShrAssign,
        Self::Eq,
        Self::PartialOrdering,
        Self::PartialOrd,
        Self::Neg,
        Self::Not,
        Self::BitAnd,
        Self::BitOr,
        Self::BitXor,
        Self::Shl,
        Self::Shr,
        Self::Chain,
        Self::Coalesce,
        Self::Unwrap,
        Self::Raise,
        Self::UnsafeEffect,
        Self::ThrowsEffect,
        Self::TypeDomain,
        Self::RegionDomain,
        Self::AccessDomain,
        Self::PassingDomain,
        Self::EffectDomain,
        Self::ParametersDomain,
        Self::BorrowTypeForm,
        Self::BorrowValueForm,
        Self::Continuation,
        Self::EffectCallable,
        Self::Handle,
        Self::Do,
        Self::DoWhile,
        Self::Try,
        Self::Throw,
        Self::Unsafe,
        Self::Loop,
        Self::While,
        Self::If,
        Self::Match,
        Self::For,
        Self::Iterator,
        Self::IntoIterator,
    ];

    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Option => "Option",
            Self::Result => "Result",
            Self::Never => "Never",
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::ISize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::USize => "usize",
            Self::Copy => "Copy",
            Self::Drop => "Drop",
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Rem => "Rem",
            Self::AddAssign => "AddAssign",
            Self::SubAssign => "SubAssign",
            Self::MulAssign => "MulAssign",
            Self::DivAssign => "DivAssign",
            Self::RemAssign => "RemAssign",
            Self::BitAndAssign => "BitAndAssign",
            Self::BitOrAssign => "BitOrAssign",
            Self::BitXorAssign => "BitXorAssign",
            Self::ShlAssign => "ShlAssign",
            Self::ShrAssign => "ShrAssign",
            Self::Eq => "Eq",
            Self::PartialOrdering => "PartialOrdering",
            Self::PartialOrd => "PartialOrd",
            Self::Neg => "Neg",
            Self::Not => "Not",
            Self::BitAnd => "BitAnd",
            Self::BitOr => "BitOr",
            Self::BitXor => "BitXor",
            Self::Shl => "Shl",
            Self::Shr => "Shr",
            Self::Chain => "Chain",
            Self::Coalesce => "Coalesce",
            Self::Unwrap => "Unwrap",
            Self::Raise => "Raise",
            Self::UnsafeEffect => "Unsafe",
            Self::ThrowsEffect => "Throws",
            Self::TypeDomain => "type",
            Self::RegionDomain => "region",
            Self::AccessDomain => "access",
            Self::PassingDomain => "passing",
            Self::EffectDomain => "effect",
            Self::ParametersDomain => "parameters",
            Self::BorrowTypeForm => "borrow",
            Self::BorrowValueForm => "borrow",
            Self::Continuation => "Continuation",
            Self::EffectCallable => "EffectCallable",
            Self::Handle => "Handle",
            Self::Do => "do",
            Self::DoWhile => "do",
            Self::Try => "try",
            Self::Throw => "throw",
            Self::Unsafe => "unsafe",
            Self::Loop => "loop",
            Self::While => "while",
            Self::If => "if",
            Self::Match => "match",
            Self::For => "for",
            Self::Iterator => "Iterator",
            Self::IntoIterator => "IntoIterator",
        }
    }

    const fn expected_kind(self) -> &'static str {
        match self {
            Self::Option | Self::Result | Self::Never | Self::PartialOrdering => "enum",
            Self::Continuation | Self::EffectCallable => "struct",
            Self::UnsafeEffect | Self::ThrowsEffect => "effect",
            Self::TypeDomain
            | Self::RegionDomain
            | Self::AccessDomain
            | Self::PassingDomain
            | Self::EffectDomain
            | Self::ParametersDomain => "domain",
            Self::BorrowTypeForm
            | Self::Bool
            | Self::I8
            | Self::I16
            | Self::I32
            | Self::I64
            | Self::I128
            | Self::ISize
            | Self::U8
            | Self::U16
            | Self::U32
            | Self::U64
            | Self::U128
            | Self::USize => "type form",
            Self::BorrowValueForm => "function",
            Self::Do
            | Self::DoWhile
            | Self::Try
            | Self::Throw
            | Self::Unsafe
            | Self::Loop
            | Self::While
            | Self::If
            | Self::Match
            | Self::For => "function",
            Self::Handle
            | Self::Copy
            | Self::Drop
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Rem
            | Self::AddAssign
            | Self::SubAssign
            | Self::MulAssign
            | Self::DivAssign
            | Self::RemAssign
            | Self::BitAndAssign
            | Self::BitOrAssign
            | Self::BitXorAssign
            | Self::ShlAssign
            | Self::ShrAssign
            | Self::Eq
            | Self::PartialOrd
            | Self::Neg
            | Self::Not
            | Self::BitAnd
            | Self::BitOr
            | Self::BitXor
            | Self::Shl
            | Self::Shr
            | Self::Chain
            | Self::Coalesce
            | Self::Unwrap
            | Self::Raise
            | Self::Iterator
            | Self::IntoIterator => "trait",
        }
    }

    pub(crate) const fn operator_method(self) -> Option<&'static str> {
        match self {
            Self::Add => Some("add"),
            Self::Sub => Some("sub"),
            Self::Mul => Some("mul"),
            Self::Div => Some("div"),
            Self::Rem => Some("rem"),
            Self::Eq => Some("eq"),
            Self::PartialOrd => Some("partial_cmp"),
            Self::Neg => Some("neg"),
            Self::Not => Some("not"),
            Self::BitAnd => Some("bit_and"),
            Self::BitOr => Some("bit_or"),
            Self::BitXor => Some("bit_xor"),
            Self::Shl => Some("shl"),
            Self::Shr => Some("shr"),
            Self::Option
            | Self::Result
            | Self::Never
            | Self::Bool
            | Self::I8
            | Self::I16
            | Self::I32
            | Self::I64
            | Self::I128
            | Self::ISize
            | Self::U8
            | Self::U16
            | Self::U32
            | Self::U64
            | Self::U128
            | Self::USize
            | Self::Copy
            | Self::Drop
            | Self::PartialOrdering
            | Self::AddAssign
            | Self::SubAssign
            | Self::MulAssign
            | Self::DivAssign
            | Self::RemAssign
            | Self::BitAndAssign
            | Self::BitOrAssign
            | Self::BitXorAssign
            | Self::ShlAssign
            | Self::ShrAssign
            | Self::Chain
            | Self::Coalesce
            | Self::Unwrap
            | Self::Raise
            | Self::UnsafeEffect
            | Self::ThrowsEffect
            | Self::TypeDomain
            | Self::RegionDomain
            | Self::AccessDomain
            | Self::PassingDomain
            | Self::EffectDomain
            | Self::ParametersDomain
            | Self::BorrowTypeForm
            | Self::BorrowValueForm
            | Self::Continuation
            | Self::EffectCallable
            | Self::Handle
            | Self::Do
            | Self::DoWhile
            | Self::Try
            | Self::Throw
            | Self::Unsafe
            | Self::Loop
            | Self::While
            | Self::If
            | Self::Match
            | Self::For => None,
            Self::Iterator | Self::IntoIterator => None,
        }
    }

    pub(crate) const fn assignment_operator_method(self) -> Option<&'static str> {
        match self {
            Self::AddAssign => Some("add_assign"),
            Self::SubAssign => Some("sub_assign"),
            Self::MulAssign => Some("mul_assign"),
            Self::DivAssign => Some("div_assign"),
            Self::RemAssign => Some("rem_assign"),
            Self::BitAndAssign => Some("bit_and_assign"),
            Self::BitOrAssign => Some("bit_or_assign"),
            Self::BitXorAssign => Some("bit_xor_assign"),
            Self::ShlAssign => Some("shl_assign"),
            Self::ShrAssign => Some("shr_assign"),
            _ => None,
        }
    }
}

impl fmt::Display for LangItemKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.source_name())
    }
}

/// Identity of a validated lang item within [`CoreBundle::program`].
///
/// Keeping the item index alongside its logical role avoids rediscovering
/// lang items later by an untrusted user-facing spelling. Semantic lowering
/// consumes the canonical declaration key derived from that indexed item.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LangItem {
    kind: LangItemKind,
    item_index: usize,
    canonical_name: String,
}

impl LangItem {
    pub const fn kind(&self) -> LangItemKind {
        self.kind
    }

    pub const fn source_name(&self) -> &'static str {
        self.kind.source_name()
    }

    pub const fn item_index(&self) -> usize {
        self.item_index
    }

    /// Canonical declaration key consumed by semantic lowering.
    pub fn canonical_name(&self) -> &str {
        &self.canonical_name
    }
}

/// All declarations whose identities are interpreted specially by this
/// compiler edition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LangItems {
    option: LangItem,
    result: LangItem,
    never: LangItem,
    bool_type: LangItem,
    i8_type: LangItem,
    i16_type: LangItem,
    i32_type: LangItem,
    i64_type: LangItem,
    i128_type: LangItem,
    isize_type: LangItem,
    u8_type: LangItem,
    u16_type: LangItem,
    u32_type: LangItem,
    u64_type: LangItem,
    u128_type: LangItem,
    usize_type: LangItem,
    copy: LangItem,
    drop: LangItem,
    add: LangItem,
    sub: LangItem,
    mul: LangItem,
    div: LangItem,
    rem: LangItem,
    add_assign: LangItem,
    sub_assign: LangItem,
    mul_assign: LangItem,
    div_assign: LangItem,
    rem_assign: LangItem,
    bit_and_assign: LangItem,
    bit_or_assign: LangItem,
    bit_xor_assign: LangItem,
    shl_assign: LangItem,
    shr_assign: LangItem,
    eq: LangItem,
    partial_ordering: LangItem,
    partial_ord: LangItem,
    neg: LangItem,
    not: LangItem,
    bit_and: LangItem,
    bit_or: LangItem,
    bit_xor: LangItem,
    shl: LangItem,
    shr: LangItem,
    chain: LangItem,
    coalesce: LangItem,
    unwrap: LangItem,
    raise: LangItem,
    unsafe_effect: LangItem,
    throws_effect: LangItem,
    type_domain: LangItem,
    region_domain: LangItem,
    access_domain: LangItem,
    passing_domain: LangItem,
    effect_domain: LangItem,
    parameters_domain: LangItem,
    borrow_type_form: LangItem,
    borrow_value_form: LangItem,
    continuation: LangItem,
    effect_callable: LangItem,
    handle: LangItem,
    do_function: LangItem,
    do_while_function: LangItem,
    try_function: LangItem,
    throw_function: LangItem,
    unsafe_function: LangItem,
    loop_function: LangItem,
    while_function: LangItem,
    if_function: LangItem,
    match_function: LangItem,
    for_function: LangItem,
    iterator: LangItem,
    into_iterator: LangItem,
}

impl LangItems {
    pub const fn option(&self) -> &LangItem {
        &self.option
    }

    pub const fn result(&self) -> &LangItem {
        &self.result
    }

    pub const fn never(&self) -> &LangItem {
        &self.never
    }
    pub const fn bool_type(&self) -> &LangItem {
        &self.bool_type
    }
    pub const fn i32_type(&self) -> &LangItem {
        &self.i32_type
    }
    pub const fn i64_type(&self) -> &LangItem {
        &self.i64_type
    }
    pub const fn u32_type(&self) -> &LangItem {
        &self.u32_type
    }
    pub const fn u64_type(&self) -> &LangItem {
        &self.u64_type
    }

    pub const fn copy(&self) -> &LangItem {
        &self.copy
    }

    pub const fn drop(&self) -> &LangItem {
        &self.drop
    }

    pub const fn add(&self) -> &LangItem {
        &self.add
    }

    pub const fn sub(&self) -> &LangItem {
        &self.sub
    }

    pub const fn mul(&self) -> &LangItem {
        &self.mul
    }

    pub const fn div(&self) -> &LangItem {
        &self.div
    }

    pub const fn rem(&self) -> &LangItem {
        &self.rem
    }
    pub const fn add_assign(&self) -> &LangItem {
        &self.add_assign
    }
    pub const fn sub_assign(&self) -> &LangItem {
        &self.sub_assign
    }
    pub const fn mul_assign(&self) -> &LangItem {
        &self.mul_assign
    }
    pub const fn div_assign(&self) -> &LangItem {
        &self.div_assign
    }
    pub const fn rem_assign(&self) -> &LangItem {
        &self.rem_assign
    }
    pub const fn bit_and_assign(&self) -> &LangItem {
        &self.bit_and_assign
    }
    pub const fn bit_or_assign(&self) -> &LangItem {
        &self.bit_or_assign
    }
    pub const fn bit_xor_assign(&self) -> &LangItem {
        &self.bit_xor_assign
    }
    pub const fn shl_assign(&self) -> &LangItem {
        &self.shl_assign
    }
    pub const fn shr_assign(&self) -> &LangItem {
        &self.shr_assign
    }

    pub const fn eq(&self) -> &LangItem {
        &self.eq
    }

    pub const fn partial_ordering(&self) -> &LangItem {
        &self.partial_ordering
    }

    pub const fn partial_ord(&self) -> &LangItem {
        &self.partial_ord
    }

    pub const fn neg(&self) -> &LangItem {
        &self.neg
    }

    pub const fn not(&self) -> &LangItem {
        &self.not
    }

    pub const fn bit_and(&self) -> &LangItem {
        &self.bit_and
    }

    pub const fn bit_or(&self) -> &LangItem {
        &self.bit_or
    }

    pub const fn bit_xor(&self) -> &LangItem {
        &self.bit_xor
    }

    pub const fn shl(&self) -> &LangItem {
        &self.shl
    }

    pub const fn shr(&self) -> &LangItem {
        &self.shr
    }

    pub const fn chain(&self) -> &LangItem {
        &self.chain
    }

    pub const fn coalesce(&self) -> &LangItem {
        &self.coalesce
    }

    pub const fn unsafe_effect(&self) -> &LangItem {
        &self.unsafe_effect
    }
    pub const fn throws_effect(&self) -> &LangItem {
        &self.throws_effect
    }
    pub const fn type_domain(&self) -> &LangItem {
        &self.type_domain
    }
    pub const fn region_domain(&self) -> &LangItem {
        &self.region_domain
    }
    pub const fn access_domain(&self) -> &LangItem {
        &self.access_domain
    }
    pub const fn passing_domain(&self) -> &LangItem {
        &self.passing_domain
    }
    pub const fn effect_domain(&self) -> &LangItem {
        &self.effect_domain
    }
    pub const fn parameters_domain(&self) -> &LangItem {
        &self.parameters_domain
    }
    pub const fn borrow_type_form(&self) -> &LangItem {
        &self.borrow_type_form
    }
    pub const fn borrow_value_form(&self) -> &LangItem {
        &self.borrow_value_form
    }
    pub const fn continuation(&self) -> &LangItem {
        &self.continuation
    }
    pub const fn effect_callable(&self) -> &LangItem {
        &self.effect_callable
    }
    pub const fn handle(&self) -> &LangItem {
        &self.handle
    }
    pub const fn do_function(&self) -> &LangItem {
        &self.do_function
    }
    pub const fn do_while_function(&self) -> &LangItem {
        &self.do_while_function
    }
    pub const fn try_function(&self) -> &LangItem {
        &self.try_function
    }
    pub const fn throw_function(&self) -> &LangItem {
        &self.throw_function
    }
    pub const fn unsafe_function(&self) -> &LangItem {
        &self.unsafe_function
    }
    pub const fn loop_function(&self) -> &LangItem {
        &self.loop_function
    }
    pub const fn while_function(&self) -> &LangItem {
        &self.while_function
    }
    pub const fn if_function(&self) -> &LangItem {
        &self.if_function
    }
    pub const fn match_function(&self) -> &LangItem {
        &self.match_function
    }
    pub const fn for_function(&self) -> &LangItem {
        &self.for_function
    }
    pub const fn iterator(&self) -> &LangItem {
        &self.iterator
    }
    pub const fn into_iterator(&self) -> &LangItem {
        &self.into_iterator
    }

    pub const fn get(&self, kind: LangItemKind) -> &LangItem {
        match kind {
            LangItemKind::Option => &self.option,
            LangItemKind::Result => &self.result,
            LangItemKind::Never => &self.never,
            LangItemKind::Bool => &self.bool_type,
            LangItemKind::I8 => &self.i8_type,
            LangItemKind::I16 => &self.i16_type,
            LangItemKind::I32 => &self.i32_type,
            LangItemKind::I64 => &self.i64_type,
            LangItemKind::I128 => &self.i128_type,
            LangItemKind::ISize => &self.isize_type,
            LangItemKind::U8 => &self.u8_type,
            LangItemKind::U16 => &self.u16_type,
            LangItemKind::U32 => &self.u32_type,
            LangItemKind::U64 => &self.u64_type,
            LangItemKind::U128 => &self.u128_type,
            LangItemKind::USize => &self.usize_type,
            LangItemKind::Copy => &self.copy,
            LangItemKind::Drop => &self.drop,
            LangItemKind::Add => &self.add,
            LangItemKind::Sub => &self.sub,
            LangItemKind::Mul => &self.mul,
            LangItemKind::Div => &self.div,
            LangItemKind::Rem => &self.rem,
            LangItemKind::AddAssign => &self.add_assign,
            LangItemKind::SubAssign => &self.sub_assign,
            LangItemKind::MulAssign => &self.mul_assign,
            LangItemKind::DivAssign => &self.div_assign,
            LangItemKind::RemAssign => &self.rem_assign,
            LangItemKind::BitAndAssign => &self.bit_and_assign,
            LangItemKind::BitOrAssign => &self.bit_or_assign,
            LangItemKind::BitXorAssign => &self.bit_xor_assign,
            LangItemKind::ShlAssign => &self.shl_assign,
            LangItemKind::ShrAssign => &self.shr_assign,
            LangItemKind::Eq => &self.eq,
            LangItemKind::PartialOrdering => &self.partial_ordering,
            LangItemKind::PartialOrd => &self.partial_ord,
            LangItemKind::Neg => &self.neg,
            LangItemKind::Not => &self.not,
            LangItemKind::BitAnd => &self.bit_and,
            LangItemKind::BitOr => &self.bit_or,
            LangItemKind::BitXor => &self.bit_xor,
            LangItemKind::Shl => &self.shl,
            LangItemKind::Shr => &self.shr,
            LangItemKind::Chain => &self.chain,
            LangItemKind::Coalesce => &self.coalesce,
            LangItemKind::Unwrap => &self.unwrap,
            LangItemKind::Raise => &self.raise,
            LangItemKind::UnsafeEffect => &self.unsafe_effect,
            LangItemKind::ThrowsEffect => &self.throws_effect,
            LangItemKind::TypeDomain => &self.type_domain,
            LangItemKind::RegionDomain => &self.region_domain,
            LangItemKind::AccessDomain => &self.access_domain,
            LangItemKind::PassingDomain => &self.passing_domain,
            LangItemKind::EffectDomain => &self.effect_domain,
            LangItemKind::ParametersDomain => &self.parameters_domain,
            LangItemKind::BorrowTypeForm => &self.borrow_type_form,
            LangItemKind::BorrowValueForm => &self.borrow_value_form,
            LangItemKind::Continuation => &self.continuation,
            LangItemKind::EffectCallable => &self.effect_callable,
            LangItemKind::Handle => &self.handle,
            LangItemKind::Do => &self.do_function,
            LangItemKind::DoWhile => &self.do_while_function,
            LangItemKind::Try => &self.try_function,
            LangItemKind::Throw => &self.throw_function,
            LangItemKind::Unsafe => &self.unsafe_function,
            LangItemKind::Loop => &self.loop_function,
            LangItemKind::While => &self.while_function,
            LangItemKind::If => &self.if_function,
            LangItemKind::Match => &self.match_function,
            LangItemKind::For => &self.for_function,
            LangItemKind::Iterator => &self.iterator,
            LangItemKind::IntoIterator => &self.into_iterator,
        }
    }
}

/// Parsed and validated compiler-owned declarations for one language edition.
#[derive(Clone, Debug, PartialEq)]
pub struct CoreBundle {
    edition: Edition,
    program: Program,
    lang_items: LangItems,
}

impl CoreBundle {
    /// Load the compiler-embedded `core` declarations for `edition`.
    pub fn for_edition(edition: Edition) -> Result<Self, CoreBundleError> {
        match edition {
            Edition::Edition2026 => Self::from_modules(
                edition,
                &[
                    ("lib", EDITION_2026_LIB),
                    ("prelude", EDITION_2026_PRELUDE),
                    ("never", EDITION_2026_NEVER),
                    ("marker", EDITION_2026_MARKER),
                    ("primitives", EDITION_2026_PRIMITIVES),
                    ("option", EDITION_2026_OPTION),
                    ("result", EDITION_2026_RESULT),
                    ("cmp", EDITION_2026_CMP),
                    ("flow", EDITION_2026_FLOW),
                    ("ops", EDITION_2026_OPS),
                    ("ops/arith", EDITION_2026_OPS_ARITH),
                    ("ops/bit", EDITION_2026_OPS_BIT),
                    ("ops/assign", EDITION_2026_OPS_ASSIGN),
                    ("effect", EDITION_2026_EFFECT),
                    ("effect/handler", EDITION_2026_EFFECT_HANDLER),
                    ("domains", EDITION_2026_DOMAINS),
                    ("control", EDITION_2026_CONTROL),
                    ("iter", EDITION_2026_ITER),
                    ("algebra", EDITION_2026_ALGEBRA),
                    ("functional", EDITION_2026_FUNCTIONAL),
                ],
            ),
        }
    }

    pub const fn edition(&self) -> Edition {
        self.edition
    }

    pub const fn program(&self) -> &Program {
        &self.program
    }

    pub const fn lang_items(&self) -> &LangItems {
        &self.lang_items
    }

    #[cfg(test)]
    fn from_source(edition: Edition, source: &str) -> Result<Self, CoreBundleError> {
        // Most contract tests isolate one prelude/operator declaration. Keep
        // the independently tested control module present in those fixtures.
        let source = format!(
            "{source}\n{TEST_ASSIGNMENT_OPS}\n{TEST_CHAIN_OPS}\n{TEST_EFFECT}\n{TEST_EFFECT_HANDLER}\n{EDITION_2026_PRIMITIVES}\n{EDITION_2026_DOMAINS}\n{EDITION_2026_CONTROL}\n{EDITION_2026_ITER}"
        );
        let mut program = parser::parse(&source).map_err(|error| {
            CoreBundleError::new(
                edition,
                vec![format!("embedded prelude does not parse: {error}")],
            )
        })?;
        program.item_origins = vec![
            ItemOrigin {
                package: PackageId::CORE.0,
                module_path: vec!["@core".to_owned()],
            };
            program.items.len()
        ];
        let lang_items = validate_program(edition, &program)?;
        Ok(Self {
            edition,
            program,
            lang_items,
        })
    }

    fn from_modules(edition: Edition, modules: &[(&str, &str)]) -> Result<Self, CoreBundleError> {
        let mut combined = Program::new(Vec::new());
        for (module, source) in modules {
            let mut program = parser::parse(source).map_err(|error| {
                CoreBundleError::new(
                    edition,
                    vec![format!(
                        "embedded core module `{module}` does not parse: {error}"
                    )],
                )
            })?;
            program.item_origins = vec![
                ItemOrigin {
                    package: PackageId::CORE.0,
                    module_path: core_origin_module_path(module),
                };
                program.items.len()
            ];
            combined.items.append(&mut program.items);
            combined
                .item_visibilities
                .append(&mut program.item_visibilities);
            combined.item_origins.append(&mut program.item_origins);
            combined.uses.append(&mut program.uses);
        }
        let mut lang_items = validate_program(edition, &combined)?;
        let sources = modules
            .iter()
            .map(|(module, source)| SourceUnit {
                path: format!("<core/{module}>"),
                module_path: core_source_module_path(module),
                source: (*source).to_owned(),
                is_root: *module == "prelude",
            })
            .collect::<Vec<_>>();
        let mut program = modules::resolve_embedded_sources(&sources)
            .map_err(|diagnostics| CoreBundleError::new(edition, diagnostics))?;
        for origin in &mut program.item_origins {
            origin.package = PackageId::CORE.0;
            origin.module_path = if origin.module_path.is_empty() {
                vec!["@core".to_owned(), "prelude".to_owned()]
            } else {
                let mut mapped = vec!["@core".to_owned()];
                if origin
                    .module_path
                    .first()
                    .is_some_and(|name| name == "core")
                {
                    mapped.extend(origin.module_path.iter().skip(1).cloned());
                } else {
                    mapped.extend(origin.module_path.iter().cloned());
                }
                mapped
            };
        }
        for lang_item in [
            &mut lang_items.option,
            &mut lang_items.result,
            &mut lang_items.never,
            &mut lang_items.bool_type,
            &mut lang_items.i8_type,
            &mut lang_items.i16_type,
            &mut lang_items.i32_type,
            &mut lang_items.i64_type,
            &mut lang_items.i128_type,
            &mut lang_items.isize_type,
            &mut lang_items.u8_type,
            &mut lang_items.u16_type,
            &mut lang_items.u32_type,
            &mut lang_items.u64_type,
            &mut lang_items.u128_type,
            &mut lang_items.usize_type,
            &mut lang_items.copy,
            &mut lang_items.drop,
            &mut lang_items.add,
            &mut lang_items.sub,
            &mut lang_items.mul,
            &mut lang_items.div,
            &mut lang_items.rem,
            &mut lang_items.add_assign,
            &mut lang_items.sub_assign,
            &mut lang_items.mul_assign,
            &mut lang_items.div_assign,
            &mut lang_items.rem_assign,
            &mut lang_items.bit_and_assign,
            &mut lang_items.bit_or_assign,
            &mut lang_items.bit_xor_assign,
            &mut lang_items.shl_assign,
            &mut lang_items.shr_assign,
            &mut lang_items.eq,
            &mut lang_items.partial_ordering,
            &mut lang_items.partial_ord,
            &mut lang_items.neg,
            &mut lang_items.not,
            &mut lang_items.bit_and,
            &mut lang_items.bit_or,
            &mut lang_items.bit_xor,
            &mut lang_items.shl,
            &mut lang_items.shr,
            &mut lang_items.chain,
            &mut lang_items.coalesce,
            &mut lang_items.unwrap,
            &mut lang_items.raise,
            &mut lang_items.unsafe_effect,
            &mut lang_items.throws_effect,
            &mut lang_items.type_domain,
            &mut lang_items.region_domain,
            &mut lang_items.access_domain,
            &mut lang_items.passing_domain,
            &mut lang_items.effect_domain,
            &mut lang_items.parameters_domain,
            &mut lang_items.borrow_type_form,
            &mut lang_items.borrow_value_form,
            &mut lang_items.continuation,
            &mut lang_items.effect_callable,
            &mut lang_items.handle,
            &mut lang_items.do_function,
            &mut lang_items.do_while_function,
            &mut lang_items.try_function,
            &mut lang_items.throw_function,
            &mut lang_items.unsafe_function,
            &mut lang_items.loop_function,
            &mut lang_items.while_function,
            &mut lang_items.if_function,
            &mut lang_items.match_function,
            &mut lang_items.for_function,
            &mut lang_items.iterator,
            &mut lang_items.into_iterator,
        ] {
            lang_item.canonical_name = item_name(&program.items[lang_item.item_index])
                .expect("resolved core lang item remains named")
                .to_owned();
        }
        Ok(Self {
            edition,
            program,
            lang_items,
        })
    }
}

fn core_source_module_path(module: &str) -> Vec<String> {
    match module {
        "prelude" => Vec::new(),
        "lib" => vec!["core".to_owned()],
        module => {
            let mut path = vec!["core".to_owned()];
            path.extend(module.split('/').map(str::to_owned));
            path
        }
    }
}

fn core_origin_module_path(module: &str) -> Vec<String> {
    match module {
        "prelude" => vec!["@core".to_owned(), "prelude".to_owned()],
        "lib" => vec!["@core".to_owned()],
        module => {
            let mut path = vec!["@core".to_owned()];
            path.extend(module.split('/').map(str::to_owned));
            path
        }
    }
}

/// Deterministic diagnostics for a malformed compiler-owned `core` bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreBundleError {
    edition: Edition,
    diagnostics: Vec<String>,
}

impl CoreBundleError {
    fn new(edition: Edition, diagnostics: Vec<String>) -> Self {
        debug_assert!(!diagnostics.is_empty());
        Self {
            edition,
            diagnostics,
        }
    }

    pub const fn edition(&self) -> Edition {
        self.edition
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

impl fmt::Display for CoreBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid embedded core bundle for edition {}",
            self.edition
        )?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n- {diagnostic}")?;
        }
        Ok(())
    }
}

impl Error for CoreBundleError {}

/// Return the source text compiled into this compiler for an edition.
pub const fn embedded_prelude_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_PRELUDE,
    }
}

/// Return the operator protocol source compiled into this compiler.
pub const fn embedded_ops_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_OPS,
    }
}

/// Return the flow protocol source compiled into this compiler.
pub const fn embedded_flow_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_FLOW,
    }
}

/// Return the effect protocol source compiled into this compiler.
pub const fn embedded_effects_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_EFFECT,
    }
}

/// Return the compile-time domain source compiled into this compiler.
pub const fn embedded_domains_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_DOMAINS,
    }
}

/// Return the error-control protocol source compiled into this compiler.
pub const fn embedded_control_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_CONTROL,
    }
}

/// Return the iteration protocol source compiled into this compiler.
pub const fn embedded_iter_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_ITER,
    }
}

/// Return the algebra protocol source compiled into this compiler.
pub const fn embedded_algebra_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_ALGEBRA,
    }
}

fn validate_program(edition: Edition, program: &Program) -> Result<LangItems, CoreBundleError> {
    let mut diagnostics = Vec::new();

    if program.items.len() != program.item_visibilities.len()
        || program.items.len() != program.item_origins.len()
    {
        diagnostics.push("embedded prelude item metadata is inconsistent".to_owned());
        return Err(CoreBundleError::new(edition, diagnostics));
    }

    let mut indices: BTreeMap<LangItemKind, Vec<usize>> = BTreeMap::new();
    for (index, ((item, visibility), origin)) in program
        .items
        .iter()
        .zip(&program.item_visibilities)
        .zip(&program.item_origins)
        .enumerate()
    {
        if matches!(item, Item::Extend(_)) {
            continue;
        }
        let Some(name) = item_name(item) else {
            diagnostics.push(format!(
                "unexpected anonymous {} declaration at item {}",
                item_kind(item),
                index + 1
            ));
            continue;
        };
        let candidates = LangItemKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.source_name() == name)
            .collect::<Vec<_>>();
        let matching = candidates
            .iter()
            .copied()
            .filter(|kind| item_has_expected_kind(*kind, item))
            .collect::<Vec<_>>();
        let kind = match (candidates.as_slice(), matching.as_slice()) {
            ([], []) => {
                if !is_allowed_non_lang_item(origin) && !is_control_support_item(name) {
                    diagnostics.push(format!(
                        "unexpected declaration `{name}` at item {}",
                        index + 1
                    ));
                }
                continue;
            }
            ([kind], []) => *kind,
            (_, [kind]) => *kind,
            (_, []) => {
                diagnostics.push(format!(
                    "lang item name `{name}` at item {} must be one of the expected compiler-owned shapes",
                    index + 1
                ));
                continue;
            }
            (_, matches) => {
                diagnostics.push(format!(
                    "lang item name `{name}` at item {} ambiguously matches {} compiler-owned shapes",
                    index + 1,
                    matches.len()
                ));
                continue;
            }
        };
        if candidates.is_empty() {
            if !is_allowed_non_lang_item(origin) && !is_control_support_item(name) {
                diagnostics.push(format!(
                    "unexpected declaration `{name}` at item {}",
                    index + 1
                ));
            }
            continue;
        }
        indices.entry(kind).or_default().push(index);
        if *visibility != Visibility::Public {
            diagnostics.push(format!(
                "lang item `{kind}` must be `pub`, found {} visibility",
                visibility_name(*visibility)
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    for kind in LangItemKind::ALL {
        match indices.get(&kind).map(Vec::as_slice) {
            None | Some([]) => diagnostics.push(format!("missing lang item `{kind}`")),
            Some([index]) => {
                validate_item_shape(kind, &program.items[*index], &mut diagnostics);
                resolved.insert(kind, *index);
            }
            Some(duplicates) => diagnostics.push(format!(
                "duplicate lang item `{kind}` appears {} times",
                duplicates.len()
            )),
        }
    }

    if !diagnostics.is_empty() {
        return Err(CoreBundleError::new(edition, diagnostics));
    }

    let item = |kind| {
        let item_index = resolved[&kind];
        LangItem {
            kind,
            item_index,
            canonical_name: item_name(&program.items[item_index])
                .expect("validated lang items are named")
                .to_owned(),
        }
    };
    Ok(LangItems {
        option: item(LangItemKind::Option),
        result: item(LangItemKind::Result),
        never: item(LangItemKind::Never),
        bool_type: item(LangItemKind::Bool),
        i8_type: item(LangItemKind::I8),
        i16_type: item(LangItemKind::I16),
        i32_type: item(LangItemKind::I32),
        i64_type: item(LangItemKind::I64),
        i128_type: item(LangItemKind::I128),
        isize_type: item(LangItemKind::ISize),
        u8_type: item(LangItemKind::U8),
        u16_type: item(LangItemKind::U16),
        u32_type: item(LangItemKind::U32),
        u64_type: item(LangItemKind::U64),
        u128_type: item(LangItemKind::U128),
        usize_type: item(LangItemKind::USize),
        copy: item(LangItemKind::Copy),
        drop: item(LangItemKind::Drop),
        add: item(LangItemKind::Add),
        sub: item(LangItemKind::Sub),
        mul: item(LangItemKind::Mul),
        div: item(LangItemKind::Div),
        rem: item(LangItemKind::Rem),
        add_assign: item(LangItemKind::AddAssign),
        sub_assign: item(LangItemKind::SubAssign),
        mul_assign: item(LangItemKind::MulAssign),
        div_assign: item(LangItemKind::DivAssign),
        rem_assign: item(LangItemKind::RemAssign),
        bit_and_assign: item(LangItemKind::BitAndAssign),
        bit_or_assign: item(LangItemKind::BitOrAssign),
        bit_xor_assign: item(LangItemKind::BitXorAssign),
        shl_assign: item(LangItemKind::ShlAssign),
        shr_assign: item(LangItemKind::ShrAssign),
        eq: item(LangItemKind::Eq),
        partial_ordering: item(LangItemKind::PartialOrdering),
        partial_ord: item(LangItemKind::PartialOrd),
        neg: item(LangItemKind::Neg),
        not: item(LangItemKind::Not),
        bit_and: item(LangItemKind::BitAnd),
        bit_or: item(LangItemKind::BitOr),
        bit_xor: item(LangItemKind::BitXor),
        shl: item(LangItemKind::Shl),
        shr: item(LangItemKind::Shr),
        chain: item(LangItemKind::Chain),
        coalesce: item(LangItemKind::Coalesce),
        unwrap: item(LangItemKind::Unwrap),
        raise: item(LangItemKind::Raise),
        unsafe_effect: item(LangItemKind::UnsafeEffect),
        throws_effect: item(LangItemKind::ThrowsEffect),
        type_domain: item(LangItemKind::TypeDomain),
        region_domain: item(LangItemKind::RegionDomain),
        access_domain: item(LangItemKind::AccessDomain),
        passing_domain: item(LangItemKind::PassingDomain),
        effect_domain: item(LangItemKind::EffectDomain),
        parameters_domain: item(LangItemKind::ParametersDomain),
        borrow_type_form: item(LangItemKind::BorrowTypeForm),
        borrow_value_form: item(LangItemKind::BorrowValueForm),
        continuation: item(LangItemKind::Continuation),
        effect_callable: item(LangItemKind::EffectCallable),
        handle: item(LangItemKind::Handle),
        do_function: item(LangItemKind::Do),
        do_while_function: item(LangItemKind::DoWhile),
        try_function: item(LangItemKind::Try),
        throw_function: item(LangItemKind::Throw),
        unsafe_function: item(LangItemKind::Unsafe),
        loop_function: item(LangItemKind::Loop),
        while_function: item(LangItemKind::While),
        if_function: item(LangItemKind::If),
        match_function: item(LangItemKind::Match),
        for_function: item(LangItemKind::For),
        iterator: item(LangItemKind::Iterator),
        into_iterator: item(LangItemKind::IntoIterator),
    })
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(function) => Some(&function.name),
        Item::Global(binding) => Some(&binding.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
        Item::Effect(definition) => Some(&definition.name),
        Item::Domain(definition) => Some(&definition.name),
        Item::TypeAlias(definition) => Some(&definition.name),
        Item::TypeForm(definition) => Some(&definition.name),
        Item::Trait(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn is_allowed_non_lang_item(origin: &ItemOrigin) -> bool {
    origin
        .module_path
        .last()
        .is_some_and(|module| NON_LANG_ITEM_CORE_MODULES.contains(&module.as_str()))
}

fn is_control_support_item(name: &str) -> bool {
    matches!(
        name,
        "Break" | "Continue" | "Return" | "break" | "continue" | "return"
    )
}

fn item_kind(item: &Item) -> &'static str {
    match item {
        Item::Function(_) => "function",
        Item::Global(_) => "global",
        Item::Struct(_) => "struct",
        Item::Enum(_) => "enum",
        Item::Effect(_) => "effect",
        Item::Domain(_) => "domain",
        Item::TypeAlias(_) => "type alias",
        Item::TypeForm(_) => "type form",
        Item::Trait(_) => "trait",
        Item::Extend(_) => "extension",
    }
}

fn item_has_expected_kind(kind: LangItemKind, item: &Item) -> bool {
    if matches!(kind, LangItemKind::Do | LangItemKind::DoWhile) {
        let Item::Function(function) = item else {
            return false;
        };
        return match kind {
            LangItemKind::Do => function.groups.len() == 1,
            LangItemKind::DoWhile => {
                matches!(
                    function.groups.as_slice(),
                    [_, while_group]
                        if matches!(while_group.as_slice(), [parameter] if parameter.name == "while")
                )
            }
            _ => unreachable!(),
        };
    }
    match kind.expected_kind() {
        "enum" => matches!(item, Item::Enum(_)),
        "struct" => matches!(item, Item::Struct(_)),
        "effect" => matches!(item, Item::Effect(_)),
        "domain" => matches!(item, Item::Domain(_)),
        "type form" => matches!(item, Item::TypeForm(_)),
        "function" => matches!(item, Item::Function(_)),
        "trait" => matches!(item, Item::Trait(_)),
        _ => false,
    }
}

fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Package => "package",
        Visibility::Public => "public",
    }
}

fn validate_item_shape(kind: LangItemKind, item: &Item, diagnostics: &mut Vec<String>) {
    match (kind, item) {
        (LangItemKind::Option, Item::Enum(definition)) => validate_option(definition, diagnostics),
        (LangItemKind::Result, Item::Enum(definition)) => validate_result(definition, diagnostics),
        (LangItemKind::Never, Item::Enum(definition)) => validate_never(definition, diagnostics),
        (LangItemKind::PartialOrdering, Item::Enum(definition)) => {
            validate_partial_ordering(definition, diagnostics)
        }
        (LangItemKind::Copy, Item::Trait(definition)) => validate_copy(definition, diagnostics),
        (LangItemKind::Drop, Item::Trait(definition)) => validate_drop(definition, diagnostics),
        (LangItemKind::UnsafeEffect | LangItemKind::ThrowsEffect, Item::Effect(definition)) => {
            validate_effect(kind, definition, diagnostics)
        }
        (
            LangItemKind::TypeDomain
            | LangItemKind::RegionDomain
            | LangItemKind::AccessDomain
            | LangItemKind::PassingDomain
            | LangItemKind::EffectDomain
            | LangItemKind::ParametersDomain,
            Item::Domain(definition),
        ) => validate_domain(kind, definition, diagnostics),
        (LangItemKind::BorrowTypeForm, Item::TypeForm(definition)) => {
            validate_borrow_type_form(definition, diagnostics)
        }
        (LangItemKind::Bool, Item::TypeForm(definition)) => {
            if !definition.compile_groups.is_empty()
                || definition.values.as_slice() != ["false", "true"]
            {
                diagnostics.push(
                    "primitive lang item `bool` must have shape `pub let bool = type { false, true }`"
                        .to_owned(),
                );
            }
        }
        (
            LangItemKind::I8
            | LangItemKind::I16
            | LangItemKind::I32
            | LangItemKind::I64
            | LangItemKind::I128
            | LangItemKind::ISize
            | LangItemKind::U8
            | LangItemKind::U16
            | LangItemKind::U32
            | LangItemKind::U64
            | LangItemKind::U128
            | LangItemKind::USize,
            Item::TypeForm(definition),
        ) => {
            if !definition.compile_groups.is_empty() || !definition.values.is_empty() {
                diagnostics.push(format!(
                    "primitive lang item `{}` must have shape `pub let {} = type`",
                    definition.name, definition.name
                ));
            }
        }
        (LangItemKind::BorrowValueForm, Item::Function(function)) => {
            validate_borrow_value_form(function, diagnostics)
        }
        (LangItemKind::Continuation, Item::Struct(definition)) => {
            let valid = definition.compile_groups
                == vec![vec![type_parameter("Input"), type_parameter("Output")]]
                && definition.fields.is_empty();
            if !valid {
                diagnostics.push(
                    "lang item `Continuation` must have shape `pub let Continuation(Input: type, Output: type) = struct {}`"
                        .to_owned(),
                );
            }
        }
        (LangItemKind::EffectCallable, Item::Struct(definition)) => {
            let valid = definition.compile_groups
                == vec![vec![
                    type_parameter("Input"),
                    type_parameter("Output"),
                    type_parameter("Answer"),
                ]]
                && definition.fields.is_empty();
            if !valid {
                diagnostics.push(
                    "lang item `EffectCallable` must have shape `pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}`"
                        .to_owned(),
                );
            }
        }
        (LangItemKind::Handle, Item::Trait(definition)) => validate_handle(definition, diagnostics),
        (
            LangItemKind::Do
            | LangItemKind::DoWhile
            | LangItemKind::Try
            | LangItemKind::Throw
            | LangItemKind::Unsafe
            | LangItemKind::Loop
            | LangItemKind::While
            | LangItemKind::If
            | LangItemKind::Match
            | LangItemKind::For,
            Item::Function(function),
        ) => validate_control_function(kind, function, diagnostics),
        (LangItemKind::Iterator, Item::Trait(definition)) => {
            validate_iterator(definition, diagnostics)
        }
        (LangItemKind::IntoIterator, Item::Trait(definition)) => {
            validate_into_iterator(definition, diagnostics)
        }
        (LangItemKind::Chain, Item::Trait(definition)) => validate_chain(definition, diagnostics),
        (LangItemKind::Coalesce, Item::Trait(definition)) => {
            validate_coalesce(definition, diagnostics)
        }
        (LangItemKind::Unwrap, Item::Trait(definition)) => validate_unwrap(definition, diagnostics),
        (LangItemKind::Raise, Item::Trait(definition)) => validate_raise(definition, diagnostics),
        (kind @ (LangItemKind::Neg | LangItemKind::Not), Item::Trait(definition)) => {
            validate_unary_operator(kind, definition, diagnostics)
        }
        (kind, Item::Trait(definition)) if kind.assignment_operator_method().is_some() => {
            validate_assignment_operator(kind, definition, diagnostics)
        }
        (kind, Item::Trait(definition)) if kind.operator_method().is_some() => {
            validate_operator(kind, definition, diagnostics)
        }
        (kind, item) => diagnostics.push(format!(
            "lang item `{kind}` must be {}, found {}",
            kind.expected_kind(),
            item_kind(item)
        )),
    }
}

fn validate_domain(
    kind: LangItemKind,
    definition: &crate::ast::DomainDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::TypeDomain
        | LangItemKind::RegionDomain
        | LangItemKind::EffectDomain
        | LangItemKind::ParametersDomain => definition.members.is_none(),
        LangItemKind::AccessDomain => definition.members.as_ref().is_some_and(|members| {
            members.len() == 2 && members[0] == "shared" && members[1] == "mut"
        }),
        LangItemKind::PassingDomain => definition.members.as_ref().is_some_and(|members| {
            members.len() == 3
                && members[0] == "auto"
                && members[1] == "copy"
                && members[2] == "move"
        }),
        _ => unreachable!("validate_domain called for non-domain lang item"),
    };
    if !valid {
        let shape = match kind {
            LangItemKind::TypeDomain => "pub let type = domain",
            LangItemKind::RegionDomain => "pub let region = domain",
            LangItemKind::EffectDomain => "pub let effect = domain",
            LangItemKind::ParametersDomain => "pub let parameters = domain",
            LangItemKind::AccessDomain => "pub let access = domain { shared, mut }",
            LangItemKind::PassingDomain => "pub let passing = domain { auto, copy, move }",
            _ => unreachable!("validate_domain called for non-domain lang item"),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn validate_borrow_type_form(definition: &TypeFormDef, diagnostics: &mut Vec<String>) {
    let valid =
        definition.compile_groups == borrow_compile_groups() && definition.values.is_empty();
    if !valid {
        diagnostics.push(
            "lang item `borrow` type form must have shape `pub let borrow(A: access = shared)(R: region)(T: type): type`"
                .to_owned(),
        );
    }
}

fn validate_borrow_value_form(function: &Function, diagnostics: &mut Vec<String>) {
    let valid = function.compile_groups == borrow_compile_groups()
        && function.return_type == Some(borrow_type("A", "R", named_type("T")))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && matches!(
            function.groups.as_slice(),
            [group] if matches!(
                group.as_slice(),
                [parameter] if parameter.name == "value"
                    && parameter.mode == PassMode::Inferred
                    && parameter.ty == named_type("T")
            )
        );
    if !valid {
        diagnostics.push(
            "lang item `borrow` value form must have shape `pub let borrow(A: access = shared)(R: region)(T: type)(value: T): borrow(A)(R)(T)`"
                .to_owned(),
        );
    }
}

fn validate_assignment_operator(
    kind: LangItemKind,
    definition: &TraitDef,
    diagnostics: &mut Vec<String>,
) {
    let method = kind
        .assignment_operator_method()
        .expect("assignment operator lang item has a method");
    let valid = trait_has_default_self(definition)
        && definition.compile_groups == vec![vec![type_parameter("Rhs")]]
        && matches!(
            definition.members.as_slice(),
            [TraitMember::Function(function)]
                if valid_assignment_operator_method(function, method)
        );
    if !valid {
        diagnostics.push(format!(
            "lang item `{kind}` must have shape `pub let {kind}(Rhs: type) = trait {{ let {method}(self: borrow(mut)(Self))(rhs: Rhs): () }}`"
        ));
    }
}

fn valid_assignment_operator_method(function: &Function, method: &str) -> bool {
    let [receiver_group, rhs_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    let [rhs] = rhs_group.as_slice() else {
        return false;
    };
    function.name == method
        && function.compile_groups.is_empty()
        && function.return_type == Some(Type::Unit)
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == simple_borrow_type(true, named_type("Self"))
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == named_type("Rhs")
}

fn validate_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name, compile_groups, default: None, .. },
                TraitMember::Function(function),
            ] if name == "Item"
                && compile_groups.is_empty()
                && valid_iteration_method(
                    function,
                    "next",
                    PassMode::MutBorrow,
                    Type::Named("core.Option".to_owned(), vec![named_type("Item")]),
                )
        );
    if !valid {
        diagnostics.push(
            "lang item `Iterator` must declare `Item` and `next(self: borrow(mut)(Self))(): Option(Item)`"
                .to_owned(),
        );
    }
}

fn validate_into_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name: into_iter, compile_groups: iter_groups, default: None, .. },
                TraitMember::Function(function),
            ] if into_iter == "IntoIter"
                && iter_groups.is_empty()
                && valid_iteration_method(
                    function,
                    "into_iter",
                    PassMode::Move,
                    named_type("IntoIter"),
                )
        );
    if !valid {
        diagnostics.push(
            "lang item `IntoIterator` must declare `IntoIter` and `into_iter(move self)(): IntoIter`"
                .to_owned(),
        );
    }
}

fn valid_iteration_method(function: &Function, name: &str, mode: PassMode, result: Type) -> bool {
    let [receiver_group, empty_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    function.name == name
        && function.compile_groups.is_empty()
        && function.return_type == Some(result)
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && match mode {
            PassMode::Move => receiver.mode == PassMode::Move && receiver.ty == named_type("Self"),
            PassMode::Borrow => {
                receiver.mode == PassMode::Inferred
                    && receiver.ty == simple_borrow_type(false, named_type("Self"))
            }
            PassMode::MutBorrow => {
                receiver.mode == PassMode::Inferred
                    && receiver.ty == simple_borrow_type(true, named_type("Self"))
            }
            PassMode::Inferred | PassMode::Copy => false,
        }
        && empty_group.is_empty()
}

fn validate_chain(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType {
                    name: item_name,
                    compile_groups: item_groups,
                    default: None,
                    ..
                },
                TraitMember::AssociatedType {
                    name: rebind_name,
                    compile_groups: rebind_groups,
                    default: None,
                    ..
                },
                TraitMember::Function(function),
            ] if item_name == "Item"
                && item_groups.is_empty()
                && rebind_name == "Rebind"
                && *rebind_groups == vec![vec![type_parameter("Value")]]
                && valid_chain_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `Chain` must declare `Item`, `Rebind(Value: type): type`, and `chain(E: effect, U: type) (self) (transform: (Item): U with(E)): Rebind(U) with(E)`"
                .to_owned(),
        );
    }
}

fn valid_chain_method(function: &Function) -> bool {
    let [receiver_group, transform_group] = function.groups.as_slice() else {
        return false;
    };
    let ([receiver], [transform]) = (receiver_group.as_slice(), transform_group.as_slice()) else {
        return false;
    };
    let effects = effect_parameter("E");
    function.name == "chain"
        && function.compile_groups == vec![vec![compile_effect_parameter("E"), type_parameter("U")]]
        && function.return_type == Some(Type::Named("Rebind".to_owned(), vec![named_type("U")]))
        && function.effects == effects
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("Self")
        && transform.name == "transform"
        && transform.mode == PassMode::Inferred
        && transform.ty == function_type(vec![vec![named_type("Item")]], named_type("U"), effects)
}

fn validate_coalesce(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    default: None,
                    ..
                },
                TraitMember::Function(function),
            ] if name == "Item"
                && compile_groups.is_empty()
                && valid_coalesce_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `Coalesce` must declare `Item` and `coalesce(E: effect) (self) (fallback: (): Item with(E)): Item with(E)`"
                .to_owned(),
        );
    }
}

fn valid_coalesce_method(function: &Function) -> bool {
    let [receiver_group, fallback_group] = function.groups.as_slice() else {
        return false;
    };
    let ([receiver], [fallback]) = (receiver_group.as_slice(), fallback_group.as_slice()) else {
        return false;
    };
    let effects = effect_parameter("E");
    function.name == "coalesce"
        && function.compile_groups == vec![vec![compile_effect_parameter("E")]]
        && function.return_type == Some(named_type("Item"))
        && function.effects == effects
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("Self")
        && fallback.name == "fallback"
        && fallback.mode == PassMode::Inferred
        && fallback.ty == function_type(vec![Vec::new()], named_type("Item"), effects)
}

fn validate_unwrap(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    default: None,
                    ..
                },
                TraitMember::Function(function),
            ] if name == "Output"
                && compile_groups.is_empty()
                && valid_unwrap_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `Unwrap` must declare `Output` and `unwrap(move self): Output`".to_owned(),
        );
    }
}

fn valid_unwrap_method(function: &Function) -> bool {
    let [receiver_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    function.name == "unwrap"
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("Output"))
        && function.effects == Default::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("Self")
}

fn validate_raise(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType {
                    name: output,
                    compile_groups: output_groups,
                    default: None,
                    ..
                },
                TraitMember::AssociatedType {
                    name: error,
                    compile_groups: error_groups,
                    default: None,
                    ..
                },
                TraitMember::Function(function),
            ] if output == "Output"
                && output_groups.is_empty()
                && error == "Error"
                && error_groups.is_empty()
                && valid_raise_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `Raise` must declare `Output`, `Error`, and `raise(move self): Output with(Throws(Error))`"
                .to_owned(),
        );
    }
}

fn valid_raise_method(function: &Function) -> bool {
    let [receiver_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    let throws_error = matches!(
        function.effects.custom.as_slice(),
        [Type::Named(name, arguments)]
            if name.split('.').next_back() == Some("Throws")
                && arguments == &vec![named_type("Error")]
    );
    function.name == "raise"
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("Output"))
        && throws_error
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.parameters.is_empty()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("Self")
}

fn validate_effect(
    kind: LangItemKind,
    definition: &crate::ast::EffectDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::UnsafeEffect => {
            definition.compile_groups.is_empty() && definition.operations.is_empty()
        }
        LangItemKind::ThrowsEffect => {
            definition.compile_groups == vec![vec![type_parameter("Error")]]
                && matches!(
                    definition.operations.as_slice(),
                    [operation] if valid_throws_raise_operation(operation)
                )
        }
        _ => false,
    };
    if !valid {
        let shape = match kind {
            LangItemKind::UnsafeEffect => "pub let Unsafe = effect {}",
            LangItemKind::ThrowsEffect => {
                "pub let Throws(Error: type) = effect { let raise(move error: Error): Never }"
            }
            _ => unreachable!(),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn valid_throws_raise_operation(function: &Function) -> bool {
    let [group] = function.groups.as_slice() else {
        return false;
    };
    let [error] = group.as_slice() else {
        return false;
    };
    function.name == "raise"
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("Never"))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && error.name == "error"
        && error.mode == PassMode::Move
        && error.ty == named_type("Error")
}

fn validate_control_function(
    kind: LangItemKind,
    function: &Function,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::For => valid_for(function),
        _ => {
            function.where_predicates.is_empty()
                && match kind {
                    LangItemKind::Do => valid_do(function),
                    LangItemKind::DoWhile => valid_do_while(function),
                    LangItemKind::Try => valid_try(function),
                    LangItemKind::Throw => valid_throw(function),
                    LangItemKind::Unsafe => valid_unsafe(function),
                    LangItemKind::Loop => valid_loop(function),
                    LangItemKind::While => valid_while(function),
                    LangItemKind::If => valid_if(function),
                    LangItemKind::Match => valid_match(function),
                    _ => false,
                }
        }
    };
    if !valid {
        diagnostics.push(format!(
            "lang item `{kind}` has an invalid validated control signature"
        ));
    }
}

fn valid_do(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(function, "action", named_type("T"), effect_parameter("E"))
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_do_while(function: &Function) -> bool {
    let [action_group, while_group] = function.groups.as_slice() else {
        return false;
    };
    let [action] = action_group.as_slice() else {
        return false;
    };
    let [condition] = while_group.as_slice() else {
        return false;
    };
    function.compile_groups
        == vec![vec![CompileParam {
            name: "E".to_owned(),
            kind: CompileParamKind::Effect,
            default: None,
        }]]
        && moved_callable_parameter(
            action,
            "action",
            Type::Unit,
            loop_body_effects(Type::Unit, "E"),
        )
        && moved_callable_parameter(
            condition,
            "while",
            Type::Bool,
            loop_body_effects(Type::Unit, "E"),
        )
        && function.return_type == Some(Type::Unit)
        && function.effects == effect_parameter("E")
        && function.body.is_none()
}

fn valid_try(function: &Function) -> bool {
    let result = Type::Named(
        "core.Result".to_owned(),
        vec![named_type("E"), named_type("T")],
    );
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named(
            "core.effect.Throws".to_owned(),
            vec![named_type("E")],
        )],
        parameters: vec!["F".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "F".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("T"),
            type_parameter("E"),
        ]]
        && single_moved_callable(function, "action", named_type("T"), effects)
        && function.return_type == Some(result)
        && function.effects.parameters == vec!["F"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_throw(function: &Function) -> bool {
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named(
            "core.effect.Throws".to_owned(),
            vec![named_type("Error")],
        )],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups == vec![vec![type_parameter("Error")]]
        && single_moved_parameter(function, "error", named_type("Error"))
        && function.return_type == Some(named_type("Never"))
        && function.effects == effects
        && function.body.is_some()
}

fn valid_unsafe(function: &Function) -> bool {
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named("core.effect.Unsafe".to_owned(), Vec::new())],
        parameters: vec!["E".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(function, "action", named_type("T"), effects)
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_loop(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(
            function,
            "body",
            Type::Unit,
            loop_body_effects(named_type("T"), "E"),
        )
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_none()
}

fn valid_while(function: &Function) -> bool {
    let [condition_group, do_group] = function.groups.as_slice() else {
        return false;
    };
    let [condition] = condition_group.as_slice() else {
        return false;
    };
    let [body] = do_group.as_slice() else {
        return false;
    };
    function.compile_groups
        == vec![vec![CompileParam {
            name: "E".to_owned(),
            kind: CompileParamKind::Effect,
            default: None,
        }]]
        && moved_callable_parameter(condition, "condition", Type::Bool, effect_parameter("E"))
        && moved_callable_parameter(body, "do", Type::Unit, effect_parameter("E"))
        && function.return_type == Some(Type::Unit)
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_if(function: &Function) -> bool {
    let [condition_group, then_group, else_group] = function.groups.as_slice() else {
        return false;
    };
    let [condition] = condition_group.as_slice() else {
        return false;
    };
    let [then] = then_group.as_slice() else {
        return false;
    };
    let [else_branch] = else_group.as_slice() else {
        return false;
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("T"),
        ]]
        && condition.name == "condition"
        && condition.mode == PassMode::Inferred
        && condition.ty == Type::Bool
        && moved_callable_parameter(then, "then", named_type("T"), effect_parameter("E"))
        && moved_callable_parameter(else_branch, "else", named_type("T"), effect_parameter("E"))
        && function.return_type == Some(named_type("T"))
        && function.effects == effect_parameter("E")
        && function.body.is_none()
}

fn valid_match(function: &Function) -> bool {
    let [input_group, cases_group] = function.groups.as_slice() else {
        return false;
    };
    let [input] = input_group.as_slice() else {
        return false;
    };
    let [cases] = cases_group.as_slice() else {
        return false;
    };
    function.compile_groups
        == vec![vec![
            type_parameter("Input"),
            type_parameter("Output"),
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            CompileParam {
                name: "Cases".to_owned(),
                kind: CompileParamKind::ParameterPack,
                default: None,
            },
        ]]
        && input.name == "input"
        && input.mode == PassMode::Move
        && input.ty == named_type("Input")
        && cases.name == "Cases"
        && cases.mode == PassMode::Inferred
        && cases.ty
            == Type::Named(
                "$parameter$groups$expand".to_owned(),
                vec![named_type("Cases")],
            )
        && function.return_type == Some(named_type("Output"))
        && function.effects == effect_parameter("E")
        && function.body.is_none()
}

fn valid_for(function: &Function) -> bool {
    let [iterable_group, body_group] = function.groups.as_slice() else {
        return false;
    };
    let [iterable] = iterable_group.as_slice() else {
        return false;
    };
    let [body] = body_group.as_slice() else {
        return false;
    };
    let expected_predicates = vec![
        crate::ast::WherePredicate {
            subject: named_type("Iterable"),
            trait_ref: Type::Named("core.iter.IntoIterator".to_owned(), Vec::new()),
            associated_types: vec![crate::ast::AssociatedTypeBinding {
                name: "IntoIter".to_owned(),
                ty: named_type("Iter"),
            }],
        },
        crate::ast::WherePredicate {
            subject: named_type("Iter"),
            trait_ref: Type::Named("core.iter.Iterator".to_owned(), Vec::new()),
            associated_types: vec![crate::ast::AssociatedTypeBinding {
                name: "Item".to_owned(),
                ty: named_type("Item"),
            }],
        },
    ];
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
                default: None,
            },
            type_parameter("Iterable"),
            type_parameter("Iter"),
            type_parameter("Item"),
        ]]
        && iterable.name == "iterable"
        && iterable.mode == PassMode::Move
        && iterable.ty == named_type("Iterable")
        && body.name == "body"
        && body.mode == PassMode::Move
        && body.ty
            == Type::Function {
                groups: vec![vec![named_type("Item")]],
                effects: loop_body_effects(Type::Unit, "E"),
                result: Box::new(Type::Unit),
            }
        && function.return_type == Some(Type::Unit)
        && function.effects == effect_parameter("E")
        && function.where_predicates == expected_predicates
        && function.body.is_none()
}

fn effect_parameter(name: &str) -> crate::ast::FunctionEffects {
    crate::ast::FunctionEffects {
        parameters: vec![name.to_owned()],
        ..crate::ast::FunctionEffects::default()
    }
}

fn loop_body_effects(result: Type, rest: &str) -> crate::ast::FunctionEffects {
    crate::ast::FunctionEffects {
        custom: vec![
            Type::Named("core.control.Break".to_owned(), vec![result]),
            Type::Named("core.control.Continue".to_owned(), Vec::new()),
        ],
        parameters: vec![rest.to_owned()],
        ..crate::ast::FunctionEffects::default()
    }
}

fn single_moved_parameter(function: &Function, name: &str, ty: Type) -> bool {
    let [group] = function.groups.as_slice() else {
        return false;
    };
    let [parameter] = group.as_slice() else {
        return false;
    };
    parameter.name == name && parameter.mode == PassMode::Move && parameter.ty == ty
}

fn single_moved_callable(
    function: &Function,
    name: &str,
    result: Type,
    effects: crate::ast::FunctionEffects,
) -> bool {
    let [group] = function.groups.as_slice() else {
        return false;
    };
    let [parameter] = group.as_slice() else {
        return false;
    };
    moved_callable_parameter(parameter, name, result, effects)
}

fn moved_callable_parameter(
    parameter: &crate::ast::Param,
    name: &str,
    result: Type,
    effects: crate::ast::FunctionEffects,
) -> bool {
    parameter.name == name
        && parameter.mode == PassMode::Move
        && parameter.ty
            == Type::Function {
                groups: vec![Vec::new()],
                effects,
                result: Box::new(result),
            }
}

fn type_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: CompileParamKind::Type,
        default: None,
    }
}

fn access_parameter(name: &str, default: Option<&str>) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: CompileParamKind::Access,
        default: default.map(|value| CompileParamDefault::Name(value.to_owned())),
    }
}

fn region_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: CompileParamKind::Region,
        default: None,
    }
}

fn borrow_compile_groups() -> Vec<Vec<CompileParam>> {
    vec![
        vec![access_parameter("A", Some("shared"))],
        vec![region_parameter("R")],
        vec![type_parameter("T")],
    ]
}

fn borrow_type(access: &str, region: &str, pointee: Type) -> Type {
    Type::Borrow {
        mutable: false,
        access: Some(access.to_owned()),
        region: Some(region.to_owned()),
        pointee: Box::new(pointee),
    }
}

fn simple_borrow_type(mutable: bool, pointee: Type) -> Type {
    Type::Borrow {
        mutable,
        access: None,
        region: None,
        pointee: Box::new(pointee),
    }
}

fn trait_has_default_self(definition: &TraitDef) -> bool {
    definition.self_parameter.name == "Self"
        && definition.self_parameter.kind == CompileParamKind::Type
}

fn validate_handle(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = definition.self_parameter.name == "Self"
        && definition.self_parameter.kind == CompileParamKind::Effect
        && definition.compile_groups.is_empty()
        && definition.where_predicates.is_empty()
        && matches!(
            definition.members.as_slice(),
            [TraitMember::AssociatedType {
                name,
                compile_groups,
                kind,
                default,
            }, TraitMember::Function(function)] if name == "Clauses"
                && compile_groups == &vec![vec![type_parameter("Value"), type_parameter("Answer")]]
                && *kind == AssociatedKind::Parameters
                && default.is_none()
                && valid_handle_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `Handle` must have shape `pub let Handle = trait(Self: effect) { let Clauses(Value: type, Answer: type): parameters; let handle(Value: type, Answer: type, Rest: effect)(...move clauses: Clauses(Value, Answer))(move action: (): Value with(Self, Rest)): Answer with(Rest) }`"
                .to_owned(),
        );
    }
}

fn valid_handle_method(function: &Function) -> bool {
    let [clauses_group, action_group] = function.groups.as_slice() else {
        return false;
    };
    let ([clauses], [action]) = (clauses_group.as_slice(), action_group.as_slice()) else {
        return false;
    };
    let action_effects = crate::ast::FunctionEffects {
        parameters: vec!["Rest".to_owned(), "Self".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.name == "handle"
        && function.compile_groups
            == vec![vec![
                type_parameter("Value"),
                type_parameter("Answer"),
                compile_effect_parameter("Rest"),
            ]]
        && function.return_type == Some(named_type("Answer"))
        && function.effects == effect_parameter("Rest")
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && clauses.name == "clauses"
        && clauses.mode == PassMode::Move
        && clauses.ty
            == Type::Named(
                "$parameters$expand".to_owned(),
                vec![Type::Named(
                    "Clauses".to_owned(),
                    vec![named_type("Value"), named_type("Answer")],
                )],
            )
        && action.name == "action"
        && action.mode == PassMode::Move
        && action.ty == function_type(vec![Vec::new()], named_type("Value"), action_effects)
}

fn compile_effect_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: CompileParamKind::Effect,
        default: None,
    }
}

fn named_type(name: &str) -> Type {
    Type::Named(name.to_owned(), Vec::new())
}

fn function_type(
    groups: Vec<Vec<Type>>,
    result: Type,
    effects: crate::ast::FunctionEffects,
) -> Type {
    Type::Function {
        groups,
        effects,
        result: Box::new(result),
    }
}

fn positional_variant(name: &str, field: Type) -> VariantDef {
    VariantDef {
        name: name.to_owned(),
        fields: VariantFields::Positional(vec![field]),
    }
}

fn unit_variant(name: &str) -> VariantDef {
    VariantDef {
        name: name.to_owned(),
        fields: VariantFields::Unit,
    }
}

fn validate_option(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_groups = vec![vec![type_parameter("T")]];
    let expected_variants = vec![
        positional_variant("Some", named_type("T")),
        unit_variant("None"),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `Option` must have shape `pub let Option(T: type) = enum { Some(T), None }`"
                .to_owned(),
        );
    }
}

fn validate_result(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_groups = vec![vec![type_parameter("E")], vec![type_parameter("T")]];
    let expected_variants = vec![
        positional_variant("Ok", named_type("T")),
        positional_variant("Err", named_type("E")),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `Result` must have shape `pub let Result(E: type)(T: type) = enum { Ok(T), Err(E) }`"
                .to_owned(),
        );
    }
}

fn validate_never(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    if !definition.compile_groups.is_empty() || !definition.variants.is_empty() {
        diagnostics.push("lang item `Never` must have shape `pub let Never = enum {}`".to_owned());
    }
}

fn validate_partial_ordering(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_variants = vec![
        unit_variant("Less"),
        unit_variant("Equal"),
        unit_variant("Greater"),
        unit_variant("Unordered"),
    ];
    if !definition.compile_groups.is_empty() || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `PartialOrdering` must have shape `pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }`"
                .to_owned(),
        );
    }
}

fn validate_copy(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !copy_trait_has_required_shape(definition) {
        diagnostics.push("lang item `Copy` must have shape `pub let Copy = trait {}`".to_owned());
    }
}

/// Check the marker contract shared by core bootstrapping and ownership lowering.
pub(crate) fn copy_trait_has_required_shape(definition: &TraitDef) -> bool {
    trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && definition.members.is_empty()
}

fn validate_drop(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !drop_trait_has_required_shape(definition) {
        diagnostics.push(
            "lang item `Drop` must have shape `pub let Drop = trait { let drop(self: borrow(mut)(Self))(): () }`"
                .to_owned(),
        );
    }
}

/// Check the destruction contract shared by core bootstrapping and lowering.
pub(crate) fn drop_trait_has_required_shape(definition: &TraitDef) -> bool {
    let [TraitMember::Function(function)] = definition.members.as_slice() else {
        return false;
    };
    let [receiver_group, empty_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && function.name == "drop"
        && function.compile_groups.is_empty()
        && function.return_type == Some(Type::Unit)
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == simple_borrow_type(true, named_type("Self"))
        && empty_group.is_empty()
}

fn validate_operator(kind: LangItemKind, definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let method = kind
        .operator_method()
        .expect("operator lang items have a method name");
    if !operator_trait_has_required_shape(kind, definition) {
        let shape = match kind {
            LangItemKind::Eq => format!(
                "pub let Eq(Rhs: type) = trait {{ let {method}(self: borrow(Self))(rhs: borrow(Rhs)): bool }}"
            ),
            LangItemKind::PartialOrd => format!(
                "pub let PartialOrd(Rhs: type) = trait {{ let {method}(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering }}"
            ),
            _ => format!(
                "pub let {kind}(Rhs: type) = trait {{ let Output: type; let {method}(self)(rhs: Rhs): Output }}"
            ),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn validate_unary_operator(
    kind: LangItemKind,
    definition: &TraitDef,
    diagnostics: &mut Vec<String>,
) {
    let method = kind
        .operator_method()
        .expect("unary operator lang items have a method");
    if !unary_operator_trait_has_required_shape(kind, definition) {
        diagnostics.push(format!(
            "lang item `{kind}` must have shape `pub let {kind} = trait {{ let Output: type; let {method}(self)(): Output }}`"
        ));
    }
}

pub(crate) fn unary_operator_trait_has_required_shape(
    kind: LangItemKind,
    definition: &TraitDef,
) -> bool {
    if !matches!(kind, LangItemKind::Neg | LangItemKind::Not)
        || !trait_has_default_self(definition)
        || !definition.compile_groups.is_empty()
    {
        return false;
    }
    let Some(method) = kind.operator_method() else {
        return false;
    };
    matches!(
        definition.members.as_slice(),
        [
            TraitMember::AssociatedType { name, compile_groups, default: None, .. },
            TraitMember::Function(function),
        ] if name == "Output"
            && compile_groups.is_empty()
            && valid_unary_operator_method(function, method)
    )
}

fn valid_unary_operator_method(function: &Function, method: &str) -> bool {
    let [receiver_group, empty_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    function.name == method
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("Output"))
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("Self")
        && empty_group.is_empty()
}

/// Check the operator contract shared by core bootstrapping and HIR lowering.
pub(crate) fn operator_trait_has_required_shape(kind: LangItemKind, definition: &TraitDef) -> bool {
    let Some(method) = kind.operator_method() else {
        return false;
    };
    let valid_groups = trait_has_default_self(definition)
        && definition.compile_groups == vec![vec![type_parameter("Rhs")]];
    let valid_members = if matches!(kind, LangItemKind::Eq | LangItemKind::PartialOrd) {
        match definition.members.as_slice() {
            [TraitMember::Function(function)] => valid_borrowing_comparison_method(function, kind),
            _ => false,
        }
    } else {
        match definition.members.as_slice() {
            [TraitMember::AssociatedType {
                name,
                compile_groups,
                default,
                ..
            }, TraitMember::Function(function)] => {
                name == "Output"
                    && compile_groups.is_empty()
                    && default.is_none()
                    && valid_operator_method(function, method)
            }
            _ => false,
        }
    };
    valid_groups && valid_members
}

fn valid_borrowing_comparison_method(function: &Function, kind: LangItemKind) -> bool {
    let [receiver_group, rhs_group] = function.groups.as_slice() else {
        return false;
    };
    let ([receiver], [rhs]) = (receiver_group.as_slice(), rhs_group.as_slice()) else {
        return false;
    };
    let (method, result_is_valid) = match kind {
        LangItemKind::Eq => ("eq", function.return_type == Some(Type::Bool)),
        LangItemKind::PartialOrd => (
            "partial_cmp",
            matches!(
                function.return_type.as_ref(),
                Some(Type::Named(name, arguments))
                    if arguments.is_empty()
                        && matches!(name.as_str(), "PartialOrdering" | "core::cmp::PartialOrdering")
            ),
        ),
        _ => return false,
    };
    function.name == method
        && function.compile_groups.is_empty()
        && result_is_valid
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == simple_borrow_type(false, named_type("Self"))
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == simple_borrow_type(false, named_type("Rhs"))
}

fn valid_operator_method(function: &Function, method: &str) -> bool {
    let [receiver_group, rhs_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    let [rhs] = rhs_group.as_slice() else {
        return false;
    };
    function.name == method
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("Output"))
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("Self")
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == named_type("Rhs")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_source_with_copy(copy_declaration: &str) -> String {
        [
            r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Result(E: type)(T: type) = enum { Ok(T), Err(E) }
pub let Never = enum {}
"#,
            copy_declaration,
            r#"
pub let Drop = trait {
  let drop(self: borrow(mut)(Self))(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(self)(rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(self)(rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(self)(rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(self)(rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(self)(rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(self: borrow(Self))(rhs: borrow(Rhs)): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(self)(rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(self)(rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(self)(rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(self)(rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(self)(rhs: Rhs): Output
}
"#,
        ]
        .concat()
    }

    fn edition_2026_test_modules<'a>(
        overrides: &[(&str, &'a str)],
    ) -> Vec<(&'static str, &'a str)> {
        let mut modules = vec![
            ("lib", EDITION_2026_LIB),
            ("prelude", EDITION_2026_PRELUDE),
            ("primitives", EDITION_2026_PRIMITIVES),
            ("never", EDITION_2026_NEVER),
            ("marker", EDITION_2026_MARKER),
            ("option", EDITION_2026_OPTION),
            ("result", EDITION_2026_RESULT),
            ("cmp", EDITION_2026_CMP),
            ("flow", EDITION_2026_FLOW),
            ("ops", EDITION_2026_OPS),
            ("ops/arith", EDITION_2026_OPS_ARITH),
            ("ops/bit", EDITION_2026_OPS_BIT),
            ("ops/assign", EDITION_2026_OPS_ASSIGN),
            ("effect", EDITION_2026_EFFECT),
            ("effect/handler", EDITION_2026_EFFECT_HANDLER),
            ("domains", EDITION_2026_DOMAINS),
            ("control", EDITION_2026_CONTROL),
            ("iter", EDITION_2026_ITER),
            ("algebra", EDITION_2026_ALGEBRA),
            ("functional", EDITION_2026_FUNCTIONAL),
        ];
        for (module, source) in overrides {
            let Some((_, target)) = modules
                .iter_mut()
                .find(|(candidate, _)| candidate == module)
            else {
                panic!("unknown edition 2026 core test module `{module}`");
            };
            *target = *source;
        }
        modules
    }

    #[test]
    fn edition_2026_bundle_parses_and_validates() {
        let bundle = CoreBundle::for_edition(Edition::Edition2026).unwrap();

        assert_eq!(bundle.edition(), Edition::Edition2026);
        assert_eq!(bundle.program().items.len(), LangItemKind::ALL.len() + 124);
        for kind in LangItemKind::ALL {
            let lang_item = bundle.lang_items().get(kind);
            assert_eq!(lang_item.kind(), kind);
            let canonical = match kind {
                LangItemKind::Option => "core::option::Option".to_owned(),
                LangItemKind::Result => "core::result::Result".to_owned(),
                LangItemKind::Never => "core::never::Never".to_owned(),
                LangItemKind::Bool
                | LangItemKind::I8
                | LangItemKind::I16
                | LangItemKind::I32
                | LangItemKind::I64
                | LangItemKind::I128
                | LangItemKind::ISize
                | LangItemKind::U8
                | LangItemKind::U16
                | LangItemKind::U32
                | LangItemKind::U64
                | LangItemKind::U128
                | LangItemKind::USize => {
                    format!("core::primitives::{}", kind.source_name())
                }
                LangItemKind::Copy | LangItemKind::Drop => {
                    format!("core::marker::{}", kind.source_name())
                }
                LangItemKind::Add
                | LangItemKind::Sub
                | LangItemKind::Mul
                | LangItemKind::Div
                | LangItemKind::Rem
                | LangItemKind::Neg => format!("core::ops::arith::{}", kind.source_name()),
                LangItemKind::BitAnd
                | LangItemKind::BitOr
                | LangItemKind::BitXor
                | LangItemKind::Shl
                | LangItemKind::Shr
                | LangItemKind::Not => format!("core::ops::bit::{}", kind.source_name()),
                LangItemKind::AddAssign
                | LangItemKind::SubAssign
                | LangItemKind::MulAssign
                | LangItemKind::DivAssign
                | LangItemKind::RemAssign
                | LangItemKind::BitAndAssign
                | LangItemKind::BitOrAssign
                | LangItemKind::BitXorAssign
                | LangItemKind::ShlAssign
                | LangItemKind::ShrAssign => {
                    format!("core::ops::assign::{}", kind.source_name())
                }
                LangItemKind::Eq | LangItemKind::PartialOrdering | LangItemKind::PartialOrd => {
                    format!("core::cmp::{}", kind.source_name())
                }
                LangItemKind::Chain
                | LangItemKind::Coalesce
                | LangItemKind::Unwrap
                | LangItemKind::Raise => {
                    format!("core::flow::{}", kind.source_name())
                }
                LangItemKind::UnsafeEffect | LangItemKind::ThrowsEffect => {
                    format!("core::effect::{}", kind.source_name())
                }
                LangItemKind::TypeDomain
                | LangItemKind::RegionDomain
                | LangItemKind::AccessDomain
                | LangItemKind::PassingDomain
                | LangItemKind::EffectDomain
                | LangItemKind::ParametersDomain
                | LangItemKind::BorrowTypeForm
                | LangItemKind::BorrowValueForm => {
                    format!("core::domains::{}", kind.source_name())
                }
                LangItemKind::Continuation
                | LangItemKind::EffectCallable
                | LangItemKind::Handle => {
                    format!("core::effect::handler::{}", kind.source_name())
                }
                LangItemKind::Do
                | LangItemKind::DoWhile
                | LangItemKind::Try
                | LangItemKind::Throw
                | LangItemKind::Unsafe
                | LangItemKind::Loop
                | LangItemKind::While
                | LangItemKind::If
                | LangItemKind::Match
                | LangItemKind::For => format!("core::control::{}", kind.source_name()),
                LangItemKind::Iterator | LangItemKind::IntoIterator => {
                    format!("core::iter::{}", kind.source_name())
                }
            };
            assert_eq!(
                item_name(&bundle.program().items[lang_item.item_index()]),
                Some(canonical.as_str())
            );
            assert_eq!(lang_item.canonical_name(), canonical.as_str());
            let module_path: Vec<&str> = match kind {
                LangItemKind::Option => vec!["option"],
                LangItemKind::Result => vec!["result"],
                LangItemKind::Never => vec!["never"],
                LangItemKind::Bool
                | LangItemKind::I8
                | LangItemKind::I16
                | LangItemKind::I32
                | LangItemKind::I64
                | LangItemKind::I128
                | LangItemKind::ISize
                | LangItemKind::U8
                | LangItemKind::U16
                | LangItemKind::U32
                | LangItemKind::U64
                | LangItemKind::U128
                | LangItemKind::USize => vec!["primitives"],
                LangItemKind::Copy | LangItemKind::Drop => vec!["marker"],
                LangItemKind::Add
                | LangItemKind::Sub
                | LangItemKind::Mul
                | LangItemKind::Div
                | LangItemKind::Rem
                | LangItemKind::Neg => vec!["ops", "arith"],
                LangItemKind::BitAnd
                | LangItemKind::BitOr
                | LangItemKind::BitXor
                | LangItemKind::Shl
                | LangItemKind::Shr
                | LangItemKind::Not => vec!["ops", "bit"],
                LangItemKind::AddAssign
                | LangItemKind::SubAssign
                | LangItemKind::MulAssign
                | LangItemKind::DivAssign
                | LangItemKind::RemAssign
                | LangItemKind::BitAndAssign
                | LangItemKind::BitOrAssign
                | LangItemKind::BitXorAssign
                | LangItemKind::ShlAssign
                | LangItemKind::ShrAssign => vec!["ops", "assign"],
                LangItemKind::Eq | LangItemKind::PartialOrdering | LangItemKind::PartialOrd => {
                    vec!["cmp"]
                }
                LangItemKind::Chain
                | LangItemKind::Coalesce
                | LangItemKind::Unwrap
                | LangItemKind::Raise => vec!["flow"],
                LangItemKind::UnsafeEffect | LangItemKind::ThrowsEffect => vec!["effect"],
                LangItemKind::TypeDomain
                | LangItemKind::RegionDomain
                | LangItemKind::AccessDomain
                | LangItemKind::PassingDomain
                | LangItemKind::EffectDomain
                | LangItemKind::ParametersDomain
                | LangItemKind::BorrowTypeForm
                | LangItemKind::BorrowValueForm => vec!["domains"],
                LangItemKind::Continuation
                | LangItemKind::EffectCallable
                | LangItemKind::Handle => vec!["effect", "handler"],
                LangItemKind::Do
                | LangItemKind::DoWhile
                | LangItemKind::Try
                | LangItemKind::Throw
                | LangItemKind::Unsafe
                | LangItemKind::Loop
                | LangItemKind::While
                | LangItemKind::If
                | LangItemKind::Match
                | LangItemKind::For => vec!["control"],
                LangItemKind::Iterator | LangItemKind::IntoIterator => vec!["iter"],
            };
            let mut expected_origin_path = vec!["@core".to_owned()];
            expected_origin_path.extend(module_path.into_iter().map(str::to_owned));
            assert_eq!(
                bundle.program().item_origins[lang_item.item_index()],
                ItemOrigin {
                    package: PackageId::CORE.0,
                    module_path: expected_origin_path,
                }
            );
        }

        let throws = &bundle.program().items[bundle.lang_items().throws_effect().item_index()];
        let never_name = bundle.lang_items().never().canonical_name().to_owned();
        assert!(matches!(
            throws,
            Item::Effect(definition)
                if matches!(
                    definition.operations.as_slice(),
                    [operation]
                        if operation.name == "raise"
                            && operation.return_type == Some(Type::Named(never_name.clone(), Vec::new()))
                )
        ));
        let async_effect = bundle
            .program()
            .items
            .iter()
            .find(|item| item_name(item) == Some("core::effect::Async"))
            .expect("core.effect.Async must be mounted");
        assert!(matches!(
            async_effect,
            Item::Effect(definition)
                if matches!(
                    definition.operations.as_slice(),
                    [operation] if operation.name == "suspend" && operation.return_type == Some(Type::Unit)
                )
        ));
    }

    #[test]
    fn bool_lang_item_requires_its_closed_value_set() {
        let malformed = EDITION_2026_PRIMITIVES
            .replace("pub let bool = type { false, true }", "pub let bool = type");
        let modules = edition_2026_test_modules(&[("primitives", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error.diagnostics().iter().any(|diagnostic| {
            diagnostic
                == "primitive lang item `bool` must have shape `pub let bool = type { false, true }`"
        }));
    }

    #[test]
    fn rejects_malformed_control_contracts() {
        for (name, malformed) in [
            (
                "do",
                EDITION_2026_CONTROL.replace(
                    "  (move while: (): bool with(core.control.Break(()), core.control.Continue, E)): () with(E)",
                    "  (move until: (): bool with(core.control.Break(()), core.control.Continue, E)): () with(E)",
                ),
            ),
            (
                "if",
                EDITION_2026_CONTROL.replace(
                    "  (condition: bool)\n  (move then: (): T with(E))",
                    "  (condition: i32)\n  (move then: (): T with(E))",
                ),
            ),
            (
                "match",
                EDITION_2026_CONTROL.replace(
                    "  ...Cases: Output with(E)",
                    "  (case: Input): Output with(E)",
                ),
            ),
            (
                "for",
                EDITION_2026_CONTROL.replace(
                    "  Iter: core.iter.Iterator(Item = Item)",
                    "  Iter: core.iter.Iterator",
                ),
            ),
        ] {
            let modules = edition_2026_test_modules(&[("control", &malformed)]);
            let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.contains(&format!("lang item `{name}`"))),
                "{:?}",
                error.diagnostics()
            );
        }

        let malformed = EDITION_2026_CONTROL.replace(
            "pub let unsafe(E: effect, T: type)\n  (move action: (): T with(core.effect.Unsafe, E)): T with(E)",
            "pub let unsafe(E: effect, T: type)\n  (move action: (): T with(E)): T with(E)",
        );
        let modules = edition_2026_test_modules(&[("control", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `unsafe`")));

        let bodyless = EDITION_2026_CONTROL.replace(
            " = {\n  core.effect.Unsafe.handle() {\n    action()\n  }\n}",
            "",
        );
        let modules = edition_2026_test_modules(&[("control", &bodyless)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `unsafe`")));

        let malformed = EDITION_2026_EFFECT_HANDLER.replace(
            "pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}",
            "pub let EffectCallable(Input: type, Output: type) = struct {}",
        );
        let modules = edition_2026_test_modules(&[("effect/handler", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `EffectCallable`")));

        let malformed = EDITION_2026_EFFECT_HANDLER.replace(
            "pub let Handle = trait(Self: effect)",
            "pub let Handle = trait",
        );
        let modules = edition_2026_test_modules(&[("effect/handler", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Handle`")));

        let malformed = EDITION_2026_EFFECT_HANDLER
            .replace(
                "let Clauses(Value: type, Answer: type): parameters",
                "let Clauses(Value: type, Answer: type): type",
            )
            .replace(
                "(...move clauses: Clauses(Value, Answer))",
                "(move clauses: Clauses(Value, Answer))",
            );
        let modules = edition_2026_test_modules(&[("effect/handler", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Handle`")));

        let malformed = EDITION_2026_CONTROL.replace(
            "pub let throw(Error: type)\n  (move error: Error): Never with(core.effect.Throws(Error))",
            "pub let throw(Error: type)\n  (move error: Error): Never",
        );
        let modules = edition_2026_test_modules(&[("control", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `throw`")));
    }

    #[test]
    fn rejects_malformed_iteration_contracts() {
        let malformed = EDITION_2026_ITER.replace(
            "let next(self: borrow(mut)(Self))\n    (): core.Option(Item)",
            "let next(self: borrow(Self))\n    (): core.Option(Item)",
        );
        let modules = edition_2026_test_modules(&[("iter", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Iterator`")));
    }

    #[test]
    fn rejects_malformed_assignment_operator_contracts() {
        let malformed = EDITION_2026_OPS_ASSIGN.replace(
            "let add_assign(self: borrow(mut)(Self))\n    (rhs: Rhs): ()",
            "let add_assign(self: borrow(Self))\n    (rhs: Rhs): ()",
        );
        let modules = edition_2026_test_modules(&[("ops/assign", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `AddAssign`")));
    }

    #[test]
    fn rejects_malformed_flow_operator_contracts() {
        let malformed =
            EDITION_2026_FLOW.replace("let Rebind(Value: type): type", "let Rebind: type");
        let modules = edition_2026_test_modules(&[("flow", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Chain`")));

        let malformed = EDITION_2026_FLOW.replace(
            "let coalesce(E: effect)\n    (self)\n    (fallback: (): Item with(E)): Item with(E)",
            "let coalesce(move self)\n    (move fallback: (): Item): Item",
        );
        let modules = edition_2026_test_modules(&[("flow", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Coalesce`")));

        let malformed =
            EDITION_2026_FLOW.replace("let unwrap(move self): Output", "let unwrap(self): Output");
        let modules = edition_2026_test_modules(&[("flow", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Unwrap`")));

        let malformed = EDITION_2026_FLOW.replace(
            "let raise(move self): Output with(core.effect.Throws(Error))",
            "let raise(move self): Output",
        );
        let modules = edition_2026_test_modules(&[("flow", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Raise`")));
    }

    #[test]
    fn lang_item_identities_follow_validated_declarations_not_source_order() {
        let source = r#"
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(self)(rhs: Rhs): Output
}
pub let Copy = trait {}
pub let Drop = trait {
  let drop(self: borrow(mut)(Self))(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(self)(rhs: Rhs): Output
}
pub let Never = enum {}
pub let Option(T: type) = enum { Some(T), None }
pub let Result(E: type)(T: type) = enum { Ok(T), Err(E) }
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(self)(rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(self)(rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(self)(rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(self: borrow(Self))(rhs: borrow(Rhs)): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(self)(rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(self)(rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(self)(rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(self)(rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(self)(rhs: Rhs): Output
}
"#;
        let bundle = CoreBundle::from_source(Edition::Edition2026, source).unwrap();

        assert_eq!(bundle.lang_items().rem().item_index(), 0);
        assert_eq!(bundle.lang_items().copy().item_index(), 1);
        assert_eq!(bundle.lang_items().drop().item_index(), 2);
        assert_eq!(bundle.lang_items().add().item_index(), 3);
        assert_eq!(bundle.lang_items().never().item_index(), 4);
        assert_eq!(bundle.lang_items().option().item_index(), 5);
        assert_eq!(bundle.lang_items().result().item_index(), 6);
        assert_eq!(bundle.lang_items().div().item_index(), 7);
        assert_eq!(bundle.lang_items().sub().item_index(), 8);
        assert_eq!(bundle.lang_items().mul().item_index(), 9);
        assert_eq!(bundle.lang_items().eq().item_index(), 10);
        assert_eq!(bundle.lang_items().partial_ordering().item_index(), 11);
        assert_eq!(bundle.lang_items().partial_ord().item_index(), 12);
        assert_eq!(bundle.lang_items().neg().item_index(), 13);
        assert_eq!(bundle.lang_items().not().item_index(), 14);
        assert_eq!(bundle.lang_items().bit_and().item_index(), 15);
        assert_eq!(bundle.lang_items().bit_or().item_index(), 16);
        assert_eq!(bundle.lang_items().bit_xor().item_index(), 17);
        assert_eq!(bundle.lang_items().shl().item_index(), 18);
        assert_eq!(bundle.lang_items().shr().item_index(), 19);
        for kind in LangItemKind::ALL {
            let item = bundle.lang_items().get(kind);
            assert_eq!(
                item.canonical_name(),
                item_name(&bundle.program().items[item.item_index()]).unwrap()
            );
        }
    }

    #[test]
    fn rejects_wrong_visibility_kind_shape_and_extra_items_deterministically() {
        let source = r#"
let Option(T: type) = enum { Some(T), None }
pub let Result = struct { value: i32 }
pub let Never = enum { Reachable }
pub let Copy(T: type) = trait {}
pub let Add(Rhs: type) = trait {
  let add(self)(rhs: Rhs): Rhs
}
pub let Extra = enum {}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(self)(rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(self)(rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(self)(rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(self)(rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(self: borrow(Self))(rhs: borrow(Rhs)): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(self)(rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(self)(rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(self)(rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(self)(rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(self)(rhs: Rhs): Output
}
pub let Drop = trait {
  let drop(self: borrow(mut)(Self))(): ()
}
"#;
        let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

        assert_eq!(
            error.diagnostics(),
            [
                "lang item `Option` must be `pub`, found private visibility",
                "unexpected declaration `Extra` at item 6",
                "lang item `Result` must be enum, found struct",
                "lang item `Never` must have shape `pub let Never = enum {}`",
                "lang item `Copy` must have shape `pub let Copy = trait {}`",
                "lang item `Add` must have shape `pub let Add(Rhs: type) = trait { let Output: type; let add(self)(rhs: Rhs): Output }`",
            ]
        );
        assert_eq!(
            error.to_string(),
            "invalid embedded core bundle for edition 2026\n- lang item `Option` must be `pub`, found private visibility\n- unexpected declaration `Extra` at item 6\n- lang item `Result` must be enum, found struct\n- lang item `Never` must have shape `pub let Never = enum {}`\n- lang item `Copy` must have shape `pub let Copy = trait {}`\n- lang item `Add` must have shape `pub let Add(Rhs: type) = trait { let Output: type; let add(self)(rhs: Rhs): Output }`"
        );
    }

    #[test]
    fn rejects_missing_and_duplicate_lang_items_in_fixed_role_order() {
        let source = r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Option(T: type) = enum { Some(T), None }
pub let Never = enum {}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(self)(rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(self)(rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(self)(rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(self)(rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(self)(rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(self: borrow(Self))(rhs: borrow(Rhs)): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(self)(rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(self)(rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(self)(rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(self)(rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(self)(rhs: Rhs): Output
}
"#;
        let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

        assert_eq!(
            error.diagnostics(),
            [
                "duplicate lang item `Option` appears 2 times",
                "missing lang item `Result`",
                "missing lang item `Copy`",
                "missing lang item `Drop`",
            ]
        );
    }

    #[test]
    fn rejects_copy_compile_parameters_associated_types_and_methods() {
        let malformed_declarations = [
            "pub let Copy(T: type) = trait {}",
            "pub let Copy = trait { let Item: type }",
            "pub let Copy = trait { let clone(self: borrow(Self))(): Self }",
        ];

        for declaration in malformed_declarations {
            let source = core_source_with_copy(declaration);
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();

            assert_eq!(
                error.diagnostics(),
                ["lang item `Copy` must have shape `pub let Copy = trait {}`"],
                "unexpected diagnostic for `{declaration}`"
            );
        }
    }

    #[test]
    fn rejects_malformed_drop_traits() {
        let malformed_declarations = [
            "pub let Drop(T: type) = trait { let drop(self: borrow(mut)(Self))(): () }",
            "pub let Drop = trait {}",
            "pub let Drop = trait { let drop(self: borrow(Self))(): () }",
            "pub let Drop = trait { let drop(self: borrow(mut)(Self))(): i32 }",
        ];

        for declaration in malformed_declarations {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                "pub let Drop = trait {\n  let drop(self: borrow(mut)(Self))(): ()\n}",
                declaration,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(
                error.diagnostics(),
                ["lang item `Drop` must have shape `pub let Drop = trait { let drop(self: borrow(mut)(Self))(): () }`"],
                "unexpected diagnostic for `{declaration}`"
            );
        }
    }

    #[test]
    fn rejects_malformed_operator_traits_in_fixed_role_order() {
        let source = r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Result(E: type)(T: type) = enum { Ok(T), Err(E) }
pub let Never = enum {}
pub let Copy = trait {}
pub let Drop = trait {
  let drop(self: borrow(mut)(Self))(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(self)(rhs: Rhs): Output
}
pub let Sub = trait {
  let Output: type
  let sub(self)(rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let mul(self)(rhs: Rhs): Rhs
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let divide(self)(rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(self)(rhs: Rhs): Output = { rhs }
}
pub let Eq(Rhs: type) = trait {
  let eq(move self)(rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(move self)(rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(self)(rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(self)(rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(self)(rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(self)(rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(self)(rhs: Rhs): Output
}
"#;
        let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

        assert_eq!(
            error.diagnostics(),
            [
                "lang item `Sub` must have shape `pub let Sub(Rhs: type) = trait { let Output: type; let sub(self)(rhs: Rhs): Output }`",
                "lang item `Mul` must have shape `pub let Mul(Rhs: type) = trait { let Output: type; let mul(self)(rhs: Rhs): Output }`",
                "lang item `Div` must have shape `pub let Div(Rhs: type) = trait { let Output: type; let div(self)(rhs: Rhs): Output }`",
                "lang item `Rem` must have shape `pub let Rem(Rhs: type) = trait { let Output: type; let rem(self)(rhs: Rhs): Output }`",
                "lang item `Eq` must have shape `pub let Eq(Rhs: type) = trait { let eq(self: borrow(Self))(rhs: borrow(Rhs)): bool }`",
                "lang item `PartialOrd` must have shape `pub let PartialOrd(Rhs: type) = trait { let partial_cmp(self: borrow(Self))(rhs: borrow(Rhs)): PartialOrdering }`",
            ]
        );
    }

    #[test]
    fn rejects_malformed_partial_ordering() {
        for declaration in [
            "pub let PartialOrdering(T: type) = enum { Less, Equal, Greater, Unordered }",
            "pub let PartialOrdering = enum { Less, Equal, Greater }",
            "pub let PartialOrdering = enum { Less, Equal, Greater, Unknown }",
        ] {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                "pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }",
                declaration,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(
                error.diagnostics(),
                ["lang item `PartialOrdering` must have shape `pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }`"],
                "unexpected diagnostic for `{declaration}`"
            );
        }
    }

    #[test]
    fn rejects_malformed_unary_operator_traits() {
        for (original, malformed, expected) in [
            (
                "pub let Neg = trait {\n  let Output: type\n  let neg(self)(): Output\n}",
                "pub let Neg(Rhs: type) = trait { let neg(self)(): i32 }",
                "lang item `Neg` must have shape `pub let Neg = trait { let Output: type; let neg(self)(): Output }`",
            ),
            (
                "pub let Not = trait {\n  let Output: type\n  let not(self)(): Output\n}",
                "pub let Not = trait { let Output: type; let not(self: borrow(Self))(): Output }",
                "lang item `Not` must have shape `pub let Not = trait { let Output: type; let not(self)(): Output }`",
            ),
        ] {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                original,
                malformed,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(error.diagnostics(), [expected]);
        }
    }

    #[test]
    fn rejects_malformed_bitwise_operator_traits() {
        for (original, malformed, expected) in [
            (
                "pub let BitAnd(Rhs: type) = trait {\n  let Output: type\n  let bit_and(self)(rhs: Rhs): Output\n}",
                "pub let BitAnd = trait { let bit_and(self: borrow(Self))(move rhs: i32): i32 }",
                "lang item `BitAnd` must have shape `pub let BitAnd(Rhs: type) = trait { let Output: type; let bit_and(self)(rhs: Rhs): Output }`",
            ),
            (
                "pub let Shr(Rhs: type) = trait {\n  let Output: type\n  let shr(self)(rhs: Rhs): Output\n}",
                "pub let Shr(Rhs: type) = trait { let Output: type; let shift(move self)(rhs: Rhs): Output }",
                "lang item `Shr` must have shape `pub let Shr(Rhs: type) = trait { let Output: type; let shr(self)(rhs: Rhs): Output }`",
            ),
        ] {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                original,
                malformed,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(error.diagnostics(), [expected]);
        }
    }

    #[test]
    fn reports_embedded_source_parse_errors() {
        let error =
            CoreBundle::from_source(Edition::Edition2026, "pub let Option = enum {").unwrap_err();

        assert_eq!(error.diagnostics().len(), 1);
        assert!(error.diagnostics()[0].starts_with("embedded prelude does not parse: "));
    }
}
