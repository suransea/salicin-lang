//! Edition-pinned Salicin `core` sources and their language-item contract.
//!
//! The declarations live in ordinary Salicin source. This module only owns
//! bootstrapping: selecting the source for an edition, parsing it, and
//! rejecting a toolchain bundle whose public surface does not have the exact
//! shape required by the compiler.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use crate::ast::{
    AssociatedKind, CompileParam, CompileParamDefault, EnumDef, Function, FunctionEffects, Item,
    ItemOrigin, PassMode, Program, Sort, TraitDef, TraitMember, Type, TypeFormDef, VariantDef,
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
const EDITION_2026_NUMERIC: &str = include_str!("../../library/core/src/numeric.sc");
const EDITION_2026_OPTION: &str = include_str!("../../library/core/src/option.sc");
const EDITION_2026_RESULT: &str = include_str!("../../library/core/src/result.sc");
const EDITION_2026_ERROR: &str = include_str!("../../library/core/src/error.sc");
const EDITION_2026_CMP: &str = include_str!("../../library/core/src/cmp.sc");
const EDITION_2026_FLOW: &str = include_str!("../../library/core/src/flow.sc");
const EDITION_2026_OPS: &str = include_str!("../../library/core/src/ops.sc");
const EDITION_2026_OPS_ARITH: &str = include_str!("../../library/core/src/ops/arith.sc");
const EDITION_2026_OPS_BIT: &str = include_str!("../../library/core/src/ops/bit.sc");
const EDITION_2026_OPS_ASSIGN: &str = include_str!("../../library/core/src/ops/assign.sc");
const EDITION_2026_OPS_INDEX: &str = include_str!("../../library/core/src/ops/index.sc");
const EDITION_2026_EFFECT: &str = include_str!("../../library/core/src/effect.sc");
const EDITION_2026_UNSAFE: &str = include_str!("../../library/core/src/unsafe.sc");
const EDITION_2026_ASYNC: &str = include_str!("../../library/core/src/async.sc");
const EDITION_2026_SORTS: &str = include_str!("../../library/core/src/sorts.sc");
const EDITION_2026_FOREIGN: &str = include_str!("../../library/core/src/foreign.sc");
const EDITION_2026_PASSING: &str = include_str!("../../library/core/src/passing.sc");
const EDITION_2026_BORROW: &str = include_str!("../../library/core/src/borrow.sc");
const EDITION_2026_CONTROL: &str = include_str!("../../library/core/src/control.sc");
const EDITION_2026_ITER: &str = include_str!("../../library/core/src/iter.sc");
const EDITION_2026_MEMORY: &str = include_str!("../../library/core/src/memory.sc");
const EDITION_2026_LITERAL: &str = include_str!("../../library/core/src/literal.sc");
const EDITION_2026_STRING: &str = include_str!("../../library/core/src/string.sc");
const EDITION_2026_MODULES: &[(&str, &str)] = &[
    ("lib", EDITION_2026_LIB),
    ("prelude", EDITION_2026_PRELUDE),
    ("never", EDITION_2026_NEVER),
    ("marker", EDITION_2026_MARKER),
    ("primitives", EDITION_2026_PRIMITIVES),
    ("numeric", EDITION_2026_NUMERIC),
    ("option", EDITION_2026_OPTION),
    ("result", EDITION_2026_RESULT),
    ("error", EDITION_2026_ERROR),
    ("cmp", EDITION_2026_CMP),
    ("flow", EDITION_2026_FLOW),
    ("ops", EDITION_2026_OPS),
    ("ops/arith", EDITION_2026_OPS_ARITH),
    ("ops/bit", EDITION_2026_OPS_BIT),
    ("ops/assign", EDITION_2026_OPS_ASSIGN),
    ("ops/index", EDITION_2026_OPS_INDEX),
    ("effect", EDITION_2026_EFFECT),
    ("unsafe", EDITION_2026_UNSAFE),
    ("async", EDITION_2026_ASYNC),
    ("sorts", EDITION_2026_SORTS),
    ("foreign", EDITION_2026_FOREIGN),
    ("passing", EDITION_2026_PASSING),
    ("borrow", EDITION_2026_BORROW),
    ("control", EDITION_2026_CONTROL),
    ("iter", EDITION_2026_ITER),
    ("memory", EDITION_2026_MEMORY),
    ("literal", EDITION_2026_LITERAL),
    ("string", EDITION_2026_STRING),
];

const NON_LANG_ITEM_CORE_MODULES: &[&str] = &[
    "primitives",
    "numeric",
    "effect",
    "control",
    "iter",
    "passing",
    "literal",
    "string",
];

static EDITION_2026_BUNDLE: OnceLock<Result<CoreBundle, CoreBundleError>> = OnceLock::new();

pub(crate) fn incremental_sources(
    edition: Edition,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    match edition {
        Edition::Edition2026 => EDITION_2026_MODULES.iter().copied(),
    }
}

#[cfg(test)]
const TEST_ASSIGNMENT_OPS: &str = r#"
pub let add_assign(comptime rhs: type) = trait { let add_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let sub_assign(comptime rhs: type) = trait { let sub_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let mul_assign(comptime rhs: type) = trait { let mul_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let div_assign(comptime rhs: type) = trait { let div_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let rem_assign(comptime rhs: type) = trait { let rem_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let bit_and_assign(comptime rhs: type) = trait { let bit_and_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let bit_or_assign(comptime rhs: type) = trait { let bit_or_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let bit_xor_assign(comptime rhs: type) = trait { let bit_xor_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let shl_assign(comptime rhs: type) = trait { let shl_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
pub let shr_assign(comptime rhs: type) = trait { let shr_assign(self: borrow(mut)(self))
  (rhs: rhs): () }
"#;

#[cfg(test)]
const TEST_CHAIN_OPS: &str = r#"
pub let chain = trait {
  let item: type
  let rebind(comptime value: type): type

  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (item): u with(e)): rebind(u) with(e)
}
pub let coalesce = trait {
  let item: type

  let coalesce(comptime e: effects)
    (self)
    (fallback: (): item with(e)): item with(e)
}
pub let unwrap = trait {
  let output: type
  let unwrap(move self): output
}
pub let raise = trait {
  let output: type
  let error: type
  let raise(move self): output with(throwing(error))
}
"#;

/// A stable logical role fulfilled by one declaration in the edition's
/// `core` bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LangItemKind {
    Builtin,
    Foreign,
    Test,
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
    Move,
    Copy,
    Drop,
    Poll,
    Future,
    Executor,
    AsyncFunction,
    AwaitFunction,
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
    Index,
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
    AsyncEffect,
    TypeSort,
    RegionSort,
    AccessSort,
    EffectSort,
    EffectsSort,
    ParametersSort,
    AbiSort,
    CopyParameters,
    MoveParameters,
    ComptimeParameters,
    BorrowTypeForm,
    BorrowValueForm,
    ArrayTypeForm,
    SliceTypeForm,
    StrTypeForm,
    PtrTypeForm,
    PtrValueForm,
    SizeOf,
    AlignOf,
    Continuation,
    EffectCallable,
    Handle,
    BreakEffect,
    ContinueEffect,
    ReturnEffect,
    Attempt,
    Break,
    BreakUnit,
    Continue,
    Return,
    ReturnUnit,
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
    Defer,
    Iterator,
    IntoIterator,
}

impl LangItemKind {
    const ALL: [Self; 104] = [
        Self::Builtin,
        Self::Foreign,
        Self::Test,
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
        Self::Move,
        Self::Copy,
        Self::Drop,
        Self::Poll,
        Self::Future,
        Self::Executor,
        Self::AsyncFunction,
        Self::AwaitFunction,
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
        Self::Index,
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
        Self::AsyncEffect,
        Self::TypeSort,
        Self::RegionSort,
        Self::AccessSort,
        Self::EffectSort,
        Self::EffectsSort,
        Self::ParametersSort,
        Self::AbiSort,
        Self::CopyParameters,
        Self::MoveParameters,
        Self::ComptimeParameters,
        Self::BorrowTypeForm,
        Self::BorrowValueForm,
        Self::ArrayTypeForm,
        Self::SliceTypeForm,
        Self::StrTypeForm,
        Self::PtrTypeForm,
        Self::PtrValueForm,
        Self::SizeOf,
        Self::AlignOf,
        Self::Continuation,
        Self::EffectCallable,
        Self::Handle,
        Self::BreakEffect,
        Self::ContinueEffect,
        Self::ReturnEffect,
        Self::Attempt,
        Self::Break,
        Self::BreakUnit,
        Self::Continue,
        Self::Return,
        Self::ReturnUnit,
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
        Self::Defer,
        Self::Iterator,
        Self::IntoIterator,
    ];

    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Foreign => "foreign",
            Self::Test => "test",
            Self::Option => "option",
            Self::Result => "result",
            Self::Never => "never",
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
            Self::Move => "movable",
            Self::Copy => "copyable",
            Self::Drop => "droppable",
            Self::Poll => "poll",
            Self::Future => "future",
            Self::Executor => "executor",
            Self::AsyncFunction => "async",
            Self::AwaitFunction => "await",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::AddAssign => "add_assign",
            Self::SubAssign => "sub_assign",
            Self::MulAssign => "mul_assign",
            Self::DivAssign => "div_assign",
            Self::RemAssign => "rem_assign",
            Self::BitAndAssign => "bit_and_assign",
            Self::BitOrAssign => "bit_or_assign",
            Self::BitXorAssign => "bit_xor_assign",
            Self::ShlAssign => "shl_assign",
            Self::ShrAssign => "shr_assign",
            Self::Eq => "eq",
            Self::PartialOrdering => "partial_ordering",
            Self::PartialOrd => "partial_ord",
            Self::Index => "index",
            Self::Neg => "neg",
            Self::Not => "not",
            Self::BitAnd => "bit_and",
            Self::BitOr => "bit_or",
            Self::BitXor => "bit_xor",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Chain => "chain",
            Self::Coalesce => "coalesce",
            Self::Unwrap => "unwrap",
            Self::Raise => "raise",
            Self::UnsafeEffect => "unsafety",
            Self::ThrowsEffect => "throwing",
            Self::AsyncEffect => "suspension",
            Self::TypeSort => "type",
            Self::RegionSort => "region",
            Self::AccessSort => "access",
            Self::EffectSort => "effect",
            Self::EffectsSort => "effects",
            Self::ParametersSort => "parameters",
            Self::AbiSort => "abi",
            Self::CopyParameters => "copy",
            Self::MoveParameters => "move",
            Self::ComptimeParameters => "comptime",
            Self::BorrowTypeForm => "borrow",
            Self::BorrowValueForm => "borrow",
            Self::ArrayTypeForm => "array",
            Self::SliceTypeForm => "slice",
            Self::StrTypeForm => "str",
            Self::PtrTypeForm | Self::PtrValueForm => "ptr",
            Self::SizeOf => "size_of",
            Self::AlignOf => "align_of",
            Self::Continuation => "continuation",
            Self::EffectCallable => "effect_callable",
            Self::Handle => "handle",
            Self::BreakEffect => "loop_exit",
            Self::ContinueEffect => "iteration_skip",
            Self::ReturnEffect => "function_exit",
            Self::Attempt => "attempt",
            Self::Break | Self::BreakUnit => "break",
            Self::Continue => "continue",
            Self::Return | Self::ReturnUnit => "return",
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
            Self::Defer => "defer",
            Self::Iterator => "iterator",
            Self::IntoIterator => "into_iterator",
        }
    }

    const fn expected_kind(self) -> &'static str {
        match self {
            Self::Option
            | Self::Result
            | Self::Never
            | Self::Poll
            | Self::PartialOrdering
            | Self::Attempt => "enum",
            Self::UnsafeEffect
            | Self::ThrowsEffect
            | Self::AsyncEffect
            | Self::BreakEffect
            | Self::ContinueEffect
            | Self::ReturnEffect => "effect",
            Self::TypeSort
            | Self::RegionSort
            | Self::AccessSort
            | Self::EffectSort
            | Self::EffectsSort
            | Self::ParametersSort
            | Self::AbiSort => "sort",
            Self::Bool => "enum",
            Self::BorrowTypeForm
            | Self::ArrayTypeForm
            | Self::SliceTypeForm
            | Self::StrTypeForm
            | Self::PtrTypeForm
            | Self::Continuation
            | Self::EffectCallable
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
            Self::Builtin
            | Self::Foreign
            | Self::Test
            | Self::CopyParameters
            | Self::MoveParameters
            | Self::ComptimeParameters
            | Self::BorrowValueForm
            | Self::PtrValueForm
            | Self::SizeOf
            | Self::AlignOf => "function",
            Self::AsyncFunction | Self::AwaitFunction => "function",
            Self::Do
            | Self::DoWhile
            | Self::Break
            | Self::BreakUnit
            | Self::Continue
            | Self::Return
            | Self::ReturnUnit
            | Self::Try
            | Self::Throw
            | Self::Unsafe
            | Self::Loop
            | Self::While
            | Self::If
            | Self::Match
            | Self::For
            | Self::Defer => "function",
            Self::Handle
            | Self::Move
            | Self::Copy
            | Self::Drop
            | Self::Future
            | Self::Executor
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
            | Self::Index
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
            Self::Builtin
            | Self::Foreign
            | Self::Test
            | Self::Option
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
            | Self::Move
            | Self::Copy
            | Self::Drop
            | Self::Poll
            | Self::Future
            | Self::Executor
            | Self::AsyncFunction
            | Self::AwaitFunction
            | Self::PartialOrdering
            | Self::Index
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
            | Self::AsyncEffect
            | Self::TypeSort
            | Self::RegionSort
            | Self::AccessSort
            | Self::EffectSort
            | Self::EffectsSort
            | Self::ParametersSort
            | Self::AbiSort
            | Self::CopyParameters
            | Self::MoveParameters
            | Self::ComptimeParameters
            | Self::BorrowTypeForm
            | Self::BorrowValueForm
            | Self::ArrayTypeForm
            | Self::StrTypeForm
            | Self::SliceTypeForm
            | Self::PtrTypeForm
            | Self::PtrValueForm
            | Self::SizeOf
            | Self::AlignOf
            | Self::Continuation
            | Self::EffectCallable
            | Self::Handle
            | Self::BreakEffect
            | Self::ContinueEffect
            | Self::ReturnEffect
            | Self::Attempt
            | Self::Break
            | Self::BreakUnit
            | Self::Continue
            | Self::Return
            | Self::ReturnUnit
            | Self::Do
            | Self::DoWhile
            | Self::Try
            | Self::Throw
            | Self::Unsafe
            | Self::Loop
            | Self::While
            | Self::If
            | Self::Match
            | Self::For
            | Self::Defer => None,
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
    additional: BTreeMap<LangItemKind, LangItem>,
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
    move_trait: LangItem,
    copy: LangItem,
    drop: LangItem,
    poll: LangItem,
    future: LangItem,
    executor: LangItem,
    async_function: LangItem,
    await_function: LangItem,
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
    index: LangItem,
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
    unsafety: LangItem,
    failure_effect: LangItem,
    suspension: LangItem,
    type_sort: LangItem,
    region_sort: LangItem,
    access_sort: LangItem,
    effect_sort: LangItem,
    effects_sort: LangItem,
    parameters_sort: LangItem,
    abi_sort: LangItem,
    borrow_type_form: LangItem,
    borrow_value_form: LangItem,
    array_type_form: LangItem,
    slice_type_form: LangItem,
    str_type_form: LangItem,
    ptr_type_form: LangItem,
    ptr_value_form: LangItem,
    size_of: LangItem,
    align_of: LangItem,
    continuation: LangItem,
    effect_callable: LangItem,
    handle: LangItem,
    attempt: LangItem,
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

    pub const fn move_trait(&self) -> &LangItem {
        &self.move_trait
    }

    pub const fn drop(&self) -> &LangItem {
        &self.drop
    }

    pub const fn poll(&self) -> &LangItem {
        &self.poll
    }

    pub const fn future(&self) -> &LangItem {
        &self.future
    }

    pub const fn executor(&self) -> &LangItem {
        &self.executor
    }

    pub const fn async_function(&self) -> &LangItem {
        &self.async_function
    }

    pub const fn await_function(&self) -> &LangItem {
        &self.await_function
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

    pub const fn index(&self) -> &LangItem {
        &self.index
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

    pub const fn unsafety(&self) -> &LangItem {
        &self.unsafety
    }
    pub const fn failure_effect(&self) -> &LangItem {
        &self.failure_effect
    }
    pub const fn suspension(&self) -> &LangItem {
        &self.suspension
    }
    pub const fn type_sort(&self) -> &LangItem {
        &self.type_sort
    }
    pub const fn region_sort(&self) -> &LangItem {
        &self.region_sort
    }
    pub const fn access_sort(&self) -> &LangItem {
        &self.access_sort
    }
    pub const fn effect_sort(&self) -> &LangItem {
        &self.effect_sort
    }
    pub const fn effects_sort(&self) -> &LangItem {
        &self.effects_sort
    }
    pub const fn parameters_sort(&self) -> &LangItem {
        &self.parameters_sort
    }
    pub const fn borrow_type_form(&self) -> &LangItem {
        &self.borrow_type_form
    }
    pub const fn borrow_value_form(&self) -> &LangItem {
        &self.borrow_value_form
    }
    pub const fn array_type_form(&self) -> &LangItem {
        &self.array_type_form
    }
    pub const fn slice_type_form(&self) -> &LangItem {
        &self.slice_type_form
    }
    pub const fn str_type_form(&self) -> &LangItem {
        &self.str_type_form
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
    pub const fn attempt(&self) -> &LangItem {
        &self.attempt
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

    pub fn get(&self, kind: LangItemKind) -> &LangItem {
        match kind {
            LangItemKind::Builtin
            | LangItemKind::Foreign
            | LangItemKind::Test
            | LangItemKind::CopyParameters
            | LangItemKind::MoveParameters
            | LangItemKind::ComptimeParameters
            | LangItemKind::BreakEffect
            | LangItemKind::ContinueEffect
            | LangItemKind::ReturnEffect
            | LangItemKind::Break
            | LangItemKind::BreakUnit
            | LangItemKind::Continue
            | LangItemKind::Return
            | LangItemKind::ReturnUnit
            | LangItemKind::Defer => self
                .additional
                .get(&kind)
                .expect("every additional lang item is registered"),
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
            LangItemKind::Move => &self.move_trait,
            LangItemKind::Copy => &self.copy,
            LangItemKind::Drop => &self.drop,
            LangItemKind::Poll => &self.poll,
            LangItemKind::Future => &self.future,
            LangItemKind::Executor => &self.executor,
            LangItemKind::AsyncFunction => &self.async_function,
            LangItemKind::AwaitFunction => &self.await_function,
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
            LangItemKind::Index => &self.index,
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
            LangItemKind::UnsafeEffect => &self.unsafety,
            LangItemKind::ThrowsEffect => &self.failure_effect,
            LangItemKind::AsyncEffect => &self.suspension,
            LangItemKind::TypeSort => &self.type_sort,
            LangItemKind::RegionSort => &self.region_sort,
            LangItemKind::AccessSort => &self.access_sort,
            LangItemKind::EffectSort => &self.effect_sort,
            LangItemKind::EffectsSort => &self.effects_sort,
            LangItemKind::ParametersSort => &self.parameters_sort,
            LangItemKind::AbiSort => &self.abi_sort,
            LangItemKind::BorrowTypeForm => &self.borrow_type_form,
            LangItemKind::BorrowValueForm => &self.borrow_value_form,
            LangItemKind::ArrayTypeForm => &self.array_type_form,
            LangItemKind::SliceTypeForm => &self.slice_type_form,
            LangItemKind::StrTypeForm => &self.str_type_form,
            LangItemKind::PtrTypeForm => &self.ptr_type_form,
            LangItemKind::PtrValueForm => &self.ptr_value_form,
            LangItemKind::SizeOf => &self.size_of,
            LangItemKind::AlignOf => &self.align_of,
            LangItemKind::Continuation => &self.continuation,
            LangItemKind::EffectCallable => &self.effect_callable,
            LangItemKind::Handle => &self.handle,
            LangItemKind::Attempt => &self.attempt,
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
        Self::cached_for_edition(edition).cloned()
    }

    pub(crate) fn cached_for_edition(edition: Edition) -> Result<&'static Self, CoreBundleError> {
        match edition {
            Edition::Edition2026 => match EDITION_2026_BUNDLE
                .get_or_init(|| Self::from_modules(edition, EDITION_2026_MODULES))
            {
                Ok(bundle) => Ok(bundle),
                Err(error) => Err(error.clone()),
            },
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
        // independently tested capability modules present in those fixtures.
        let source = format!(
            "{source}\n{TEST_ASSIGNMENT_OPS}\n{TEST_CHAIN_OPS}\n{EDITION_2026_EFFECT}\n{EDITION_2026_ERROR}\n{EDITION_2026_UNSAFE}\n{EDITION_2026_ASYNC}\n{EDITION_2026_PRIMITIVES}\n{EDITION_2026_SORTS}\n{EDITION_2026_FOREIGN}\n{EDITION_2026_PASSING}\n{EDITION_2026_BORROW}\n{EDITION_2026_CONTROL}\n{EDITION_2026_ITER}\n{EDITION_2026_MEMORY}\nlet builtin() = builtin()\npub let test(move body: (): bool): () = builtin()"
        );
        let mut program = parser::parse(&source).map_err(|error| {
            CoreBundleError::new(
                edition,
                vec![format!("embedded prelude does not parse: {error}")],
            )
        })?;
        for origin in &mut program.item_origins {
            origin.package = PackageId::CORE.0;
            origin.module_path = vec!["@core".to_owned()];
            if let Some(location) = &mut origin.source {
                location.path = Some("<core:test>".to_owned());
            }
        }
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
            for origin in &mut program.item_origins {
                origin.package = PackageId::CORE.0;
                origin.module_path = core_origin_module_path(module);
                if let Some(location) = &mut origin.source {
                    location.path = Some(format!("<core:{module}>"));
                }
            }
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
            &mut lang_items.move_trait,
            &mut lang_items.copy,
            &mut lang_items.drop,
            &mut lang_items.poll,
            &mut lang_items.future,
            &mut lang_items.executor,
            &mut lang_items.async_function,
            &mut lang_items.await_function,
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
            &mut lang_items.index,
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
            &mut lang_items.unsafety,
            &mut lang_items.failure_effect,
            &mut lang_items.suspension,
            &mut lang_items.type_sort,
            &mut lang_items.region_sort,
            &mut lang_items.access_sort,
            &mut lang_items.effect_sort,
            &mut lang_items.effects_sort,
            &mut lang_items.parameters_sort,
            &mut lang_items.abi_sort,
            &mut lang_items.borrow_type_form,
            &mut lang_items.borrow_value_form,
            &mut lang_items.array_type_form,
            &mut lang_items.slice_type_form,
            &mut lang_items.str_type_form,
            &mut lang_items.ptr_type_form,
            &mut lang_items.ptr_value_form,
            &mut lang_items.size_of,
            &mut lang_items.align_of,
            &mut lang_items.continuation,
            &mut lang_items.effect_callable,
            &mut lang_items.handle,
            &mut lang_items.attempt,
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
        for lang_item in lang_items.additional.values_mut() {
            lang_item.canonical_name = item_name(&program.items[lang_item.item_index])
                .expect("resolved additional core lang item remains named")
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

/// Return the compile-time sort source compiled into this compiler.
pub const fn embedded_sorts_source(edition: Edition) -> &'static str {
    match edition {
        Edition::Edition2026 => EDITION_2026_SORTS,
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

fn validate_program(edition: Edition, program: &Program) -> Result<LangItems, CoreBundleError> {
    let mut diagnostics = crate::standard::naming_diagnostics(program, "core");

    if program.items.len() != program.item_visibilities.len()
        || program.items.len() != program.item_origins.len()
    {
        diagnostics.push("embedded prelude item metadata is inconsistent".to_owned());
        return Err(CoreBundleError::new(edition, diagnostics));
    }

    let mut indices: BTreeMap<LangItemKind, Vec<usize>> = BTreeMap::new();
    let mut builtin_bootstraps = Vec::new();
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
                if !is_allowed_non_lang_item(origin) && !is_core_support_item(name) {
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
        if kind == LangItemKind::Builtin {
            builtin_bootstraps.push(index);
        }
        if candidates.is_empty() {
            if !is_allowed_non_lang_item(origin) && !is_core_support_item(name) {
                diagnostics.push(format!(
                    "unexpected declaration `{name}` at item {}",
                    index + 1
                ));
            }
            continue;
        }
        indices.entry(kind).or_default().push(index);
        let expected_visibility = if kind == LangItemKind::Builtin {
            Visibility::Private
        } else {
            Visibility::Public
        };
        if *visibility != expected_visibility {
            if expected_visibility == Visibility::Public {
                diagnostics.push(format!(
                    "lang item `{kind}` must be `pub`, found {} visibility",
                    visibility_name(*visibility),
                ));
            } else {
                diagnostics.push(format!(
                    "lang item `{kind}` must be private, found {} visibility",
                    visibility_name(*visibility),
                ));
            }
        }
    }

    let mut resolved = BTreeMap::new();
    for kind in LangItemKind::ALL {
        match indices.get(&kind).map(Vec::as_slice) {
            None | Some([]) => diagnostics.push(format!("missing lang item `{kind}`")),
            Some([index]) => {
                validate_item_shape(kind, &program.items[*index], &mut diagnostics);
                validate_lang_item_builtin(kind, &program.items[*index], &mut diagnostics);
                resolved.insert(kind, *index);
            }
            Some(duplicates) => diagnostics.push(format!(
                "duplicate lang item `{kind}` appears {} times",
                duplicates.len()
            )),
        }
    }

    validate_builtin_boundaries(program, &resolved, &builtin_bootstraps, &mut diagnostics);

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
    let additional = [
        LangItemKind::Builtin,
        LangItemKind::Foreign,
        LangItemKind::Test,
        LangItemKind::CopyParameters,
        LangItemKind::MoveParameters,
        LangItemKind::ComptimeParameters,
        LangItemKind::BreakEffect,
        LangItemKind::ContinueEffect,
        LangItemKind::ReturnEffect,
        LangItemKind::Break,
        LangItemKind::BreakUnit,
        LangItemKind::Continue,
        LangItemKind::Return,
        LangItemKind::ReturnUnit,
        LangItemKind::Defer,
    ]
    .into_iter()
    .map(|kind| (kind, item(kind)))
    .collect();
    Ok(LangItems {
        additional,
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
        move_trait: item(LangItemKind::Move),
        copy: item(LangItemKind::Copy),
        drop: item(LangItemKind::Drop),
        poll: item(LangItemKind::Poll),
        future: item(LangItemKind::Future),
        executor: item(LangItemKind::Executor),
        async_function: item(LangItemKind::AsyncFunction),
        await_function: item(LangItemKind::AwaitFunction),
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
        index: item(LangItemKind::Index),
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
        unsafety: item(LangItemKind::UnsafeEffect),
        failure_effect: item(LangItemKind::ThrowsEffect),
        suspension: item(LangItemKind::AsyncEffect),
        type_sort: item(LangItemKind::TypeSort),
        region_sort: item(LangItemKind::RegionSort),
        access_sort: item(LangItemKind::AccessSort),
        effect_sort: item(LangItemKind::EffectSort),
        effects_sort: item(LangItemKind::EffectsSort),
        parameters_sort: item(LangItemKind::ParametersSort),
        abi_sort: item(LangItemKind::AbiSort),
        borrow_type_form: item(LangItemKind::BorrowTypeForm),
        borrow_value_form: item(LangItemKind::BorrowValueForm),
        array_type_form: item(LangItemKind::ArrayTypeForm),
        slice_type_form: item(LangItemKind::SliceTypeForm),
        str_type_form: item(LangItemKind::StrTypeForm),
        ptr_type_form: item(LangItemKind::PtrTypeForm),
        ptr_value_form: item(LangItemKind::PtrValueForm),
        size_of: item(LangItemKind::SizeOf),
        align_of: item(LangItemKind::AlignOf),
        continuation: item(LangItemKind::Continuation),
        effect_callable: item(LangItemKind::EffectCallable),
        handle: item(LangItemKind::Handle),
        attempt: item(LangItemKind::Attempt),
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

fn validate_builtin_bootstrap(item: &Item, diagnostics: &mut Vec<String>) {
    let valid = matches!(
        item,
        Item::Function(function)
            if function.name == "builtin"
                && function.compile_groups.is_empty()
                && function.groups == vec![Vec::new()]
                && matches!(
                    function.return_type.as_ref(),
                    Some(Type::Named(name, arguments))
                        if name.split(['.', ':']).rfind(|part| !part.is_empty())
                            == Some("never")
                            && arguments.is_empty()
                )
                && function.effects == FunctionEffects::default()
                && function.where_predicates.is_empty()
                && function.foreign.is_none()
                && function.builtin
                && function.body.is_none()
    );
    if !valid {
        diagnostics.push(
            "compiler-definition bootstrap must have exact private shape `let builtin() = builtin()`"
                .to_owned(),
        );
    }
}

fn validate_lang_item_builtin(kind: LangItemKind, item: &Item, diagnostics: &mut Vec<String>) {
    let required = matches!(
        kind,
        LangItemKind::Builtin
            | LangItemKind::Foreign
            | LangItemKind::Test
            | LangItemKind::CopyParameters
            | LangItemKind::MoveParameters
            | LangItemKind::ComptimeParameters
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
            | LangItemKind::USize
            | LangItemKind::BorrowTypeForm
            | LangItemKind::BorrowValueForm
            | LangItemKind::ArrayTypeForm
            | LangItemKind::SliceTypeForm
            | LangItemKind::StrTypeForm
            | LangItemKind::PtrTypeForm
            | LangItemKind::PtrValueForm
            | LangItemKind::SizeOf
            | LangItemKind::AlignOf
            | LangItemKind::Continuation
            | LangItemKind::EffectCallable
            | LangItemKind::AsyncFunction
            | LangItemKind::Loop
            | LangItemKind::Match
            | LangItemKind::Defer
    );
    let marked = match item {
        Item::Function(function) => function.builtin,
        Item::TypeForm(definition) => definition.builtin,
        _ => false,
    };
    if required && !marked {
        diagnostics.push(format!(
            "compiler-owned lang item `{kind}` must use the complete `= builtin()` initializer"
        ));
    } else if !required && marked {
        diagnostics.push(format!(
            "lang item `{kind}` is source-owned or abstract and must not use `builtin()`"
        ));
    }
}

fn validate_builtin_boundaries(
    program: &Program,
    resolved: &BTreeMap<LangItemKind, usize>,
    bootstraps: &[usize],
    diagnostics: &mut Vec<String>,
) {
    let known = resolved
        .values()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for (index, item) in program.items.iter().enumerate() {
        if (known.contains(&index) && !matches!(item, Item::Trait(_) | Item::Effect(_)))
            || bootstraps.contains(&index)
        {
            continue;
        }
        match item {
            Item::Function(function)
                if function.builtin
                    && matches!(function.name.as_str(), "sort" | "sort_of" | "type_of") =>
            {
                validate_introspection_builtin(function, diagnostics);
                continue;
            }
            Item::Function(function) if function.builtin => diagnostics.push(format!(
                "unknown compiler-owned core function `{}` uses `builtin()`",
                function.name
            )),
            Item::TypeForm(definition) if definition.builtin => diagnostics.push(format!(
                "unknown compiler-owned core type `{}` uses `builtin()`",
                definition.name
            )),
            Item::Trait(definition) => {
                for member in &definition.members {
                    if matches!(member, TraitMember::Function(function) if function.builtin) {
                        diagnostics.push(format!(
                            "trait requirement in `{}` must remain abstract and cannot use `builtin()`",
                            definition.name
                        ));
                    }
                }
            }
            Item::Effect(definition) => {
                for operation in &definition.operations {
                    if operation.builtin {
                        diagnostics.push(format!(
                            "effect operation in `{}` must remain abstract and cannot use `builtin()`",
                            definition.name
                        ));
                    }
                }
            }
            Item::Extend(extension) => {
                for member in &extension.members {
                    if let crate::ast::ExtendMember::Function(function) = member {
                        if function.body.is_none() && !function.builtin {
                            diagnostics.push(format!(
                                "compiler-owned extension method `{}` must use `= builtin()`",
                                function.name
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn validate_introspection_builtin(function: &Function, diagnostics: &mut Vec<String>) {
    let valid = match function.name.as_str() {
        "sort" => {
            matches!(
                function.compile_groups.as_slice(),
                [group] if matches!(group.as_slice(), [level]
                    if level.name == "level" && level.kind == Sort::USize)
            ) && function.groups.is_empty()
                && function.return_type
                    == Some(Type::Named(
                        "sort".to_owned(),
                        vec![Type::Named("level+1".to_owned(), Vec::new())],
                    ))
        }
        "sort_of" => {
            matches!(
                function.compile_groups.as_slice(),
                [group] if matches!(group.as_slice(), [level, classifier, value]
                    if level.name == "level"
                        && level.kind == Sort::USize
                        && classifier.name == "classifier"
                        && classifier.kind
                            == Sort::Universe(crate::ast::SortLevel::Parameter(
                                "level".to_owned(),
                            ))
                        && value.name == "value"
                        && value.kind == Sort::Named("classifier".to_owned()))
            ) && function.groups.is_empty()
                && function.return_type
                    == Some(Type::Named(
                        "sort".to_owned(),
                        vec![Type::Named("level".to_owned(), Vec::new())],
                    ))
        }
        "type_of" => {
            matches!(
                function.compile_groups.as_slice(),
                [group] if matches!(group.as_slice(), [parameter]
                    if parameter.name == "t" && parameter.kind == Sort::Type)
            ) && matches!(
                function.groups.as_slice(),
                [group] if matches!(group.as_slice(), [parameter]
                    if parameter.name == "expression"
                        && parameter.mode == PassMode::Move
                        && matches!(&parameter.ty, Type::Function { groups, result, .. }
                            if groups == &[Vec::new()]
                                && result.as_ref()
                                    == &Type::Named("t".to_owned(), Vec::new())))
            ) && function.return_type == Some(Type::Named("type".to_owned(), Vec::new()))
        }
        _ => false,
    };
    if !valid
        || function.effects != FunctionEffects::default()
        || !function.where_predicates.is_empty()
        || function.foreign.is_some()
        || function.body.is_some()
    {
        diagnostics.push(format!(
            "core introspection builtin `{}` has an invalid source contract",
            function.name
        ));
    }
}

fn validate_defer_support(function: &Function, diagnostics: &mut Vec<String>) {
    let effects = effect_parameter("e");
    let valid = function.compile_groups == vec![vec![compile_effects_parameter("e")]]
        && single_moved_callable(function, "action", Type::Unit, effects.clone())
        && function.return_type == Some(Type::Unit)
        && function.effects == effects
        && function.where_predicates.is_empty()
        && function.foreign.is_none()
        && function.builtin
        && function.body.is_none();
    if !valid {
        diagnostics.push(
            "compiler-owned support function `defer` must have shape `pub let defer(comptime e: effects)(move action: (): () with(e)): () with(e) = builtin()`"
                .to_owned(),
        );
    }
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(function) => Some(&function.name),
        Item::Global(binding) => Some(&binding.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
        Item::Effect(definition) => Some(&definition.name),
        Item::Sort(definition) => Some(&definition.name),
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

fn is_core_support_item(name: &str) -> bool {
    matches!(
        name,
        "array_into_iter"
            | "slice_iter"
            | "owned_item"
            | "borrowed_item"
            | "sort"
            | "sort_of"
            | "type_of"
    )
}

fn item_kind(item: &Item) -> &'static str {
    match item {
        Item::Function(_) => "function",
        Item::Global(_) => "global",
        Item::Struct(_) => "struct",
        Item::Enum(_) => "enum",
        Item::Effect(_) => "effect",
        Item::Sort(_) => "sort",
        Item::TypeAlias(_) => "type alias",
        Item::TypeForm(_) => "type form",
        Item::Trait(_) => "trait",
        Item::Extend(_) => "extension",
    }
}

fn item_has_expected_kind(kind: LangItemKind, item: &Item) -> bool {
    if matches!(
        kind,
        LangItemKind::Break
            | LangItemKind::BreakUnit
            | LangItemKind::Return
            | LangItemKind::ReturnUnit
    ) {
        let Item::Function(function) = item else {
            return false;
        };
        return match kind {
            LangItemKind::Break | LangItemKind::Return => !function.compile_groups.is_empty(),
            LangItemKind::BreakUnit | LangItemKind::ReturnUnit => {
                function.compile_groups.is_empty()
            }
            _ => unreachable!(),
        };
    }
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
        "sort" => matches!(item, Item::Sort(_)),
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
        (LangItemKind::Builtin, _) => validate_builtin_bootstrap(item, diagnostics),
        (LangItemKind::Foreign | LangItemKind::Test, Item::Function(function)) => {
            validate_syntax_contract(kind, function, diagnostics)
        }
        (LangItemKind::Option, Item::Enum(definition)) => validate_option(definition, diagnostics),
        (LangItemKind::Result, Item::Enum(definition)) => validate_result(definition, diagnostics),
        (LangItemKind::Never, Item::Enum(definition)) => validate_never(definition, diagnostics),
        (LangItemKind::Attempt, Item::Enum(definition)) => {
            validate_attempt(definition, diagnostics)
        }
        (LangItemKind::PartialOrdering, Item::Enum(definition)) => {
            validate_partial_ordering(definition, diagnostics)
        }
        (LangItemKind::Poll, Item::Enum(definition)) => validate_poll(definition, diagnostics),
        (LangItemKind::Move, Item::Trait(definition)) => validate_move(definition, diagnostics),
        (LangItemKind::Copy, Item::Trait(definition)) => validate_copy(definition, diagnostics),
        (LangItemKind::Drop, Item::Trait(definition)) => validate_drop(definition, diagnostics),
        (LangItemKind::Future, Item::Trait(definition)) => validate_future(definition, diagnostics),
        (LangItemKind::Executor, Item::Trait(definition)) => {
            validate_executor(definition, diagnostics)
        }
        (LangItemKind::AsyncFunction | LangItemKind::AwaitFunction, Item::Function(definition)) => {
            validate_async_function(kind, definition, diagnostics)
        }
        (
            LangItemKind::UnsafeEffect | LangItemKind::ThrowsEffect | LangItemKind::AsyncEffect,
            Item::Effect(definition),
        ) => validate_effect(kind, definition, diagnostics),
        (
            kind @ (LangItemKind::BreakEffect
            | LangItemKind::ContinueEffect
            | LangItemKind::ReturnEffect),
            Item::Effect(definition),
        ) => validate_control_effect(kind, definition, diagnostics),
        (
            LangItemKind::TypeSort
            | LangItemKind::RegionSort
            | LangItemKind::AccessSort
            | LangItemKind::EffectSort
            | LangItemKind::EffectsSort
            | LangItemKind::ParametersSort
            | LangItemKind::AbiSort,
            Item::Sort(definition),
        ) => validate_sort(kind, definition, diagnostics),
        (
            kind @ (LangItemKind::CopyParameters
            | LangItemKind::MoveParameters
            | LangItemKind::ComptimeParameters),
            Item::Function(function),
        ) => validate_parameter_modifier(kind.source_name(), function, diagnostics),
        (LangItemKind::BorrowTypeForm, Item::TypeForm(definition)) => {
            validate_borrow_type_form(definition, diagnostics)
        }
        (LangItemKind::ArrayTypeForm, Item::TypeForm(definition)) => {
            validate_array_type_form(definition, diagnostics)
        }
        (LangItemKind::SliceTypeForm, Item::TypeForm(definition)) => {
            validate_slice_type_form(definition, diagnostics)
        }
        (LangItemKind::StrTypeForm, Item::TypeForm(definition)) => {
            if !definition.compile_groups.is_empty() || !definition.values.is_empty() {
                diagnostics.push(
                    "lang item `str` type form must have shape `pub let str: type`".to_owned(),
                );
            }
        }
        (LangItemKind::Bool, Item::Enum(definition)) => validate_closed_enum(
            "bool",
            &["false", "true"],
            "pub let bool = enum { false, true }",
            definition,
            diagnostics,
        ),
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
                    "primitive lang item `{}` must have shape `pub let {}: type`",
                    definition.name, definition.name
                ));
            }
        }
        (LangItemKind::BorrowValueForm, Item::Function(function)) => {
            validate_borrow_value_form(function, diagnostics)
        }
        (LangItemKind::PtrTypeForm, Item::TypeForm(definition)) => {
            validate_pointer_type_form(definition, diagnostics)
        }
        (LangItemKind::PtrValueForm, Item::Function(function)) => {
            validate_pointer_value_form(function, diagnostics)
        }
        (kind @ (LangItemKind::SizeOf | LangItemKind::AlignOf), Item::Function(function)) => {
            validate_layout_query(kind, function, diagnostics)
        }
        (LangItemKind::Continuation, Item::TypeForm(definition)) => {
            let valid = definition.compile_groups
                == vec![vec![type_parameter("input"), type_parameter("output")]]
                && definition.values.is_empty();
            if !valid {
                diagnostics.push(
                    "lang item `continuation` must have shape `pub let continuation(comptime input: type, comptime output: type): type`"
                        .to_owned(),
                );
            }
        }
        (LangItemKind::EffectCallable, Item::TypeForm(definition)) => {
            let valid = definition.compile_groups
                == vec![vec![
                    type_parameter("input"),
                    type_parameter("output"),
                    type_parameter("answer"),
                ]]
                && definition.values.is_empty();
            if !valid {
                diagnostics.push(
                    "lang item `effect_callable` must have shape `pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type): type`"
                        .to_owned(),
                );
            }
        }
        (LangItemKind::Handle, Item::Trait(definition)) => validate_handle(definition, diagnostics),
        (
            LangItemKind::Do
            | LangItemKind::DoWhile
            | LangItemKind::Break
            | LangItemKind::BreakUnit
            | LangItemKind::Continue
            | LangItemKind::Return
            | LangItemKind::ReturnUnit
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
        (LangItemKind::Defer, Item::Function(function)) => {
            validate_defer_support(function, diagnostics)
        }
        (LangItemKind::Iterator, Item::Trait(definition)) => {
            validate_iterator(definition, diagnostics)
        }
        (LangItemKind::IntoIterator, Item::Trait(definition)) => {
            validate_into_iterator(definition, diagnostics)
        }
        (LangItemKind::Index, Item::Trait(definition)) => validate_index(definition, diagnostics),
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

fn validate_sort(
    kind: LangItemKind,
    definition: &crate::ast::SortDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::TypeSort
        | LangItemKind::RegionSort
        | LangItemKind::EffectSort
        | LangItemKind::EffectsSort
        | LangItemKind::ParametersSort => definition.level == 2 && definition.members.is_none(),
        LangItemKind::AbiSort => {
            matches!(
                definition.members.as_deref(),
                Some([c]) if c == "c"
            ) && definition.level == 1
        }
        LangItemKind::AccessSort => {
            matches!(
                definition.members.as_deref(),
                Some([shared, mutable]) if shared == "shared" && mutable == "mut"
            ) && definition.level == 1
        }
        _ => unreachable!("validate_sort called for non-sort lang item"),
    };
    if !valid {
        let shape = match kind {
            LangItemKind::TypeSort => "pub let type: sort(2)",
            LangItemKind::RegionSort => "pub let region: sort(2)",
            LangItemKind::EffectSort => "pub let effect: sort(2)",
            LangItemKind::EffectsSort => "pub let effects: sort(2)",
            LangItemKind::ParametersSort => "pub let parameters: sort(2)",
            LangItemKind::AbiSort => "pub let abi = sort(1) { c }",
            LangItemKind::AccessSort => "pub let access = sort(1) { shared, mut }",
            _ => unreachable!("validate_sort called for non-sort lang item"),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn validate_parameter_modifier(name: &str, function: &Function, diagnostics: &mut Vec<String>) {
    let valid = function.compile_groups.len() == 1
        && function.compile_groups[0].len() == 1
        && function.compile_groups[0][0].kind == Sort::Parameters
        && function.groups.is_empty()
        && matches!(
            function.return_type.as_ref(),
            Some(Type::Named(result, arguments))
                if result == "parameters" && arguments.is_empty()
        )
        && function.effects == FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.builtin
        && function.body.is_none();
    if !valid {
        diagnostics.push(format!(
            "parameter modifier `{name}` must have shape `pub let {name}(comptime p: parameters): parameters`"
        ));
    }
}

fn validate_syntax_contract(
    kind: LangItemKind,
    function: &Function,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::Foreign => {
            function.compile_groups
                == vec![vec![CompileParam {
                    name: "abi".to_owned(),
                    kind: Sort::Named("abi".to_owned()),
                    default: None,
                }]]
                && function.groups.is_empty()
                && function.return_type == Some(named_type("never"))
        }
        LangItemKind::Test => {
            function.compile_groups.is_empty()
                && single_moved_callable(function, "body", Type::Bool, FunctionEffects::default())
                && function.return_type == Some(Type::Unit)
        }
        _ => false,
    } && function.effects == FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.foreign.is_none()
        && function.builtin
        && function.body.is_none();
    if !valid {
        let shape = match kind {
            LangItemKind::Foreign => "pub let foreign(comptime abi: abi): never = builtin()",
            LangItemKind::Test => "pub let test(move body: (): bool): () = builtin()",
            _ => unreachable!(),
        };
        diagnostics.push(format!(
            "syntax lang item `{kind}` must have shape `{shape}`"
        ));
    }
}

fn validate_closed_enum(
    name: &str,
    variants: &[&str],
    shape: &str,
    definition: &EnumDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = definition.compile_groups.is_empty()
        && definition
            .variants
            .iter()
            .filter_map(|variant| match variant.fields {
                crate::ast::VariantFields::Unit => Some(variant.name.as_str()),
                _ => None,
            })
            .eq(variants.iter().copied())
        && definition.variants.len() == variants.len();
    if !valid {
        diagnostics.push(format!("lang item `{name}` must have shape `{shape}`"));
    }
}

fn validate_borrow_type_form(definition: &TypeFormDef, diagnostics: &mut Vec<String>) {
    let valid =
        definition.compile_groups == borrow_compile_groups() && definition.values.is_empty();
    if !valid {
        diagnostics.push(
            "lang item `borrow` type form must have shape `pub let borrow(comptime a: access = shared)(comptime r: region)(comptime t: type): type`"
                .to_owned(),
        );
    }
}

fn validate_borrow_value_form(function: &Function, diagnostics: &mut Vec<String>) {
    let valid = function.compile_groups == borrow_compile_groups()
        && function.return_type == Some(borrow_type("a", "r", named_type("t")))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && matches!(
            function.groups.as_slice(),
            [group] if matches!(
                group.as_slice(),
                [parameter] if parameter.name == "value"
                    && parameter.mode == PassMode::Inferred
                    && parameter.ty == named_type("t")
            )
        );
    if !valid {
        diagnostics.push(
            "lang item `borrow` value form must have shape `pub let borrow(comptime a: access = shared)(comptime r: region)(comptime t: type)(value: t): borrow(a)(r)(t)`"
                .to_owned(),
        );
    }
}

fn validate_pointer_type_form(definition: &TypeFormDef, diagnostics: &mut Vec<String>) {
    let valid =
        definition.compile_groups == pointer_compile_groups() && definition.values.is_empty();
    if !valid {
        diagnostics.push(
            "lang item `ptr` type form must have shape `pub let ptr(comptime a: access = shared)(comptime t: type): type`"
                .to_owned(),
        );
    }
}

fn validate_array_type_form(definition: &TypeFormDef, diagnostics: &mut Vec<String>) {
    let valid = definition.compile_groups
        == vec![vec![type_parameter("t")], vec![usize_parameter("l")]]
        && definition.values.is_empty();
    if !valid {
        diagnostics.push(
            "lang item `array` type form must have shape `pub let array(comptime t: type)(comptime l: usize): type`"
                .to_owned(),
        );
    }
}

fn validate_slice_type_form(definition: &TypeFormDef, diagnostics: &mut Vec<String>) {
    let valid = definition.compile_groups == vec![vec![type_parameter("t")]]
        && definition.values.is_empty();
    if !valid {
        diagnostics.push(
            "lang item `slice` type form must have shape `pub let slice(comptime t: type): type`"
                .to_owned(),
        );
    }
}

fn validate_pointer_value_form(function: &Function, diagnostics: &mut Vec<String>) {
    let valid = function.compile_groups == pointer_compile_groups()
        && function.return_type
            == Some(Type::Named(
                "ptr".to_owned(),
                vec![named_type("a"), named_type("t")],
            ))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && matches!(
            function.groups.as_slice(),
            [group] if matches!(
                group.as_slice(),
                [parameter] if parameter.name == "value"
                    && parameter.mode == PassMode::Inferred
                    && parameter.ty == access_borrow_type("a", named_type("t"))
            )
        );
    if !valid {
        diagnostics.push(
            "lang item `ptr` value form must have shape `pub let ptr(comptime a: access = shared)(comptime t: type)(value: borrow(a)(t)): ptr(a)(t)`"
                .to_owned(),
        );
    }
}

fn validate_layout_query(kind: LangItemKind, function: &Function, diagnostics: &mut Vec<String>) {
    let name = kind.source_name();
    let valid = function.compile_groups == vec![vec![type_parameter("t")]]
        && function.groups.is_empty()
        && function.return_type == Some(Type::U64)
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none();
    if !valid {
        diagnostics.push(format!(
            "lang item `{name}` must have shape `pub let {name}(comptime t: type): u64`"
        ));
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
        && definition.compile_groups == vec![vec![type_parameter("rhs")]]
        && matches!(
            definition.members.as_slice(),
            [TraitMember::Function(function)]
                if valid_assignment_operator_method(function, method)
        );
    if !valid {
        diagnostics.push(format!(
            "lang item `{kind}` must have shape `pub let {kind}(comptime rhs: type) = trait {{ let {method}(self: borrow(mut)(self))(rhs: rhs): () }}`"
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
        && receiver.ty == simple_borrow_type(true, named_type("self"))
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == named_type("rhs")
}

fn validate_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name, compile_groups, default: None, .. },
                TraitMember::Function(function),
            ] if name == "item"
                && compile_groups == &vec![vec![region_parameter("r")]]
                && valid_iterator_next_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `iterator` must declare `item(r: region): type` and `next(r: region)(self: borrow(mut)(r)(self))(): option(item(r))`"
                .to_owned(),
        );
    }
}

fn valid_iterator_next_method(function: &Function) -> bool {
    let [receiver_group, empty_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    function.name == "next"
        && function.compile_groups == vec![vec![region_parameter("r")]]
        && function.return_type
            == Some(Type::Named(
                "core.option".to_owned(),
                vec![Type::Named("item".to_owned(), vec![named_type("r")])],
            ))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty
            == Type::Borrow {
                mutable: true,
                access: None,
                region: Some("r".to_owned()),
                pointee: Box::new(named_type("self")),
            }
        && empty_group.is_empty()
}

fn validate_into_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name: iter, compile_groups: iter_groups, default: None, .. },
                TraitMember::Function(function),
            ] if iter == "iter"
                && iter_groups.is_empty()
                && valid_iteration_method(
                    function,
                    "into_iter",
                    PassMode::Move,
                    named_type("iter"),
                )
        );
    if !valid {
        diagnostics.push(
            "lang item `into_iterator` must declare `iter` and `into_iter(move self)(): iter`"
                .to_owned(),
        );
    }
}

fn validate_index(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = trait_has_default_self(definition)
        && definition.compile_groups == vec![vec![type_parameter("key")]]
        && definition.where_predicates.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    kind: AssociatedKind::Type,
                    default: None,
                },
                TraitMember::Function(function),
            ] if name == "output"
                && compile_groups.is_empty()
                && valid_index_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `index` must have shape `pub let index(comptime key: type) = trait { let output: type; let index(comptime a: access)(self: borrow(a)(self))(key: key): borrow(a)(output) }`"
                .to_owned(),
        );
    }
}

fn valid_index_method(function: &Function) -> bool {
    let [receiver_group, key_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    let [key] = key_group.as_slice() else {
        return false;
    };
    function.name == "index"
        && function.compile_groups == vec![vec![access_parameter("a", None)]]
        && function.return_type == Some(access_borrow_type("a", named_type("output")))
        && function.effects == FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == access_borrow_type("a", named_type("self"))
        && key.name == "key"
        && key.mode == PassMode::Inferred
        && key.ty == named_type("key")
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
            PassMode::Move => receiver.mode == PassMode::Move && receiver.ty == named_type("self"),
            PassMode::Borrow => {
                receiver.mode == PassMode::Inferred
                    && receiver.ty == simple_borrow_type(false, named_type("self"))
            }
            PassMode::MutBorrow => {
                receiver.mode == PassMode::Inferred
                    && receiver.ty == simple_borrow_type(true, named_type("self"))
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
            ] if item_name == "item"
                && item_groups.is_empty()
                && rebind_name == "rebind"
                && *rebind_groups == vec![vec![type_parameter("value")]]
                && valid_chain_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `chain` must declare `item`, `rebind(value: type): type`, and `chain(e: effects, u: type) (self) (transform: (item): u with(e)): rebind(u) with(e)`"
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
    let effects = effect_parameter("e");
    function.name == "chain"
        && function.compile_groups
            == vec![vec![compile_effects_parameter("e"), type_parameter("u")]]
        && function.return_type == Some(Type::Named("rebind".to_owned(), vec![named_type("u")]))
        && function.effects == effects
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("self")
        && transform.name == "transform"
        && transform.mode == PassMode::Inferred
        && transform.ty == function_type(vec![vec![named_type("item")]], named_type("u"), effects)
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
            ] if name == "item"
                && compile_groups.is_empty()
                && valid_coalesce_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `coalesce` must declare `item` and `coalesce(e: effects) (self) (fallback: (): item with(e)): item with(e)`"
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
    let effects = effect_parameter("e");
    function.name == "coalesce"
        && function.compile_groups == vec![vec![compile_effects_parameter("e")]]
        && function.return_type == Some(named_type("item"))
        && function.effects == effects
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("self")
        && fallback.name == "fallback"
        && fallback.mode == PassMode::Inferred
        && fallback.ty == function_type(vec![Vec::new()], named_type("item"), effects)
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
            ] if name == "output"
                && compile_groups.is_empty()
                && valid_unwrap_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `unwrap` must declare `output` and `unwrap(move self): output`".to_owned(),
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
        && function.return_type == Some(named_type("output"))
        && function.effects == Default::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("self")
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
            ] if output == "output"
                && output_groups.is_empty()
                && error == "error"
                && error_groups.is_empty()
                && valid_raise_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `raise` must declare `output`, `error`, and `raise(move self): output with(throwing(error))`"
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
    let failure_error = matches!(
        function.effects.custom.as_slice(),
        [Type::Named(name, arguments)]
            if name.split('.').next_back() == Some("throwing")
                && arguments == &vec![named_type("error")]
    );
    function.name == "raise"
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("output"))
        && failure_error
        && !function.effects.unsafety
        && function.effects.failure.is_none()
        && function.effects.parameters.is_empty()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("self")
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
            definition.compile_groups == vec![vec![type_parameter("error")]]
                && matches!(
                    definition.operations.as_slice(),
                    [operation] if valid_failure_raise_operation(operation)
                )
        }
        LangItemKind::AsyncEffect => {
            definition.compile_groups.is_empty()
                && matches!(
                    definition.operations.as_slice(),
                    [operation] if valid_async_suspend_operation(operation)
                )
        }
        _ => false,
    };
    if !valid {
        let shape = match kind {
            LangItemKind::UnsafeEffect => "pub let unsafety = effect {}",
            LangItemKind::ThrowsEffect => {
                "pub let throwing(comptime error: type) = effect { let raise(move error: error): never }"
            }
            LangItemKind::AsyncEffect => "pub let async = effect { let suspend(): () }",
            _ => unreachable!(),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn valid_async_suspend_operation(function: &Function) -> bool {
    function.name == "suspend"
        && function.compile_groups.is_empty()
        && function.groups == vec![Vec::new()]
        && function.return_type == Some(Type::Unit)
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
}

fn valid_failure_raise_operation(function: &Function) -> bool {
    let [group] = function.groups.as_slice() else {
        return false;
    };
    let [error] = group.as_slice() else {
        return false;
    };
    function.name == "raise"
        && function.compile_groups.is_empty()
        && function.return_type == Some(named_type("never"))
        && function.effects == crate::ast::FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && error.name == "error"
        && error.mode == PassMode::Move
        && error.ty == named_type("error")
}

fn validate_control_effect(
    kind: LangItemKind,
    definition: &crate::ast::EffectDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::BreakEffect | LangItemKind::ReturnEffect => {
            definition.compile_groups == vec![vec![type_parameter("t")]]
                && matches!(
                    definition.operations.as_slice(),
                    [operation] if valid_control_exit_operation(operation)
                )
        }
        LangItemKind::ContinueEffect => {
            definition.compile_groups.is_empty()
                && matches!(
                    definition.operations.as_slice(),
                    [operation] if operation.name == "next"
                        && operation.compile_groups.is_empty()
                        && operation.groups == vec![Vec::new()]
                        && operation.return_type == Some(named_type("never"))
                        && operation.effects == FunctionEffects::default()
                        && operation.where_predicates.is_empty()
                        && operation.body.is_none()
                )
        }
        _ => false,
    };
    if !valid {
        let shape = match kind {
            LangItemKind::BreakEffect => {
                "pub let break(comptime t: type) = effect { let exit(move value: t): never }"
            }
            LangItemKind::ContinueEffect => "pub let continue = effect { let next(): never }",
            LangItemKind::ReturnEffect => {
                "pub let return(comptime t: type) = effect { let exit(move value: t): never }"
            }
            _ => unreachable!(),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn valid_control_exit_operation(function: &Function) -> bool {
    function.name == "exit"
        && function.compile_groups.is_empty()
        && single_moved_parameter(function, "value", named_type("t"))
        && function.return_type == Some(named_type("never"))
        && function.effects == FunctionEffects::default()
        && function.where_predicates.is_empty()
        && function.body.is_none()
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
                    LangItemKind::Break | LangItemKind::Return => {
                        valid_control_exit_function(kind, function, false)
                    }
                    LangItemKind::BreakUnit | LangItemKind::ReturnUnit => {
                        valid_control_exit_function(kind, function, true)
                    }
                    LangItemKind::Continue => valid_continue_function(function),
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

fn valid_control_exit_function(kind: LangItemKind, function: &Function, unit: bool) -> bool {
    let effect_name = match kind {
        LangItemKind::Break | LangItemKind::BreakUnit => "loop_exit",
        LangItemKind::Return | LangItemKind::ReturnUnit => "function_exit",
        _ => return false,
    };
    let argument = if unit { Type::Unit } else { named_type("t") };
    let valid_groups = if unit {
        function.compile_groups.is_empty() && function.groups == vec![Vec::new()]
    } else {
        function.compile_groups == vec![vec![type_parameter("t")]]
            && single_moved_parameter(function, "value", named_type("t"))
    };
    valid_groups
        && function.return_type == Some(named_type("never"))
        && has_only_control_effect(&function.effects, effect_name, &[argument])
        && function.body.is_some()
}

fn valid_continue_function(function: &Function) -> bool {
    function.compile_groups.is_empty()
        && function.groups == vec![Vec::new()]
        && function.return_type == Some(named_type("never"))
        && has_only_control_effect(&function.effects, "iteration_skip", &[])
        && function.body.is_some()
}

fn has_only_control_effect(effects: &FunctionEffects, name: &str, arguments: &[Type]) -> bool {
    !effects.unsafety
        && effects.failure.is_none()
        && effects.parameters.is_empty()
        && matches!(
            effects.custom.as_slice(),
            [Type::Named(candidate, candidate_arguments)]
                if candidate.split(['.', ':']).rfind(|part| !part.is_empty()) == Some(name)
                    && candidate_arguments == arguments
        )
}

fn valid_do(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("t"),
        ]]
        && single_moved_callable(function, "action", named_type("t"), effect_parameter("e"))
        && function.return_type == Some(named_type("t"))
        && function.effects.parameters == vec!["e"]
        && !function.effects.unsafety
        && function.effects.failure.is_none()
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
            name: "e".to_owned(),
            kind: Sort::Effects,
            default: None,
        }]]
        && moved_callable_parameter(
            action,
            "action",
            Type::Unit,
            loop_body_effects(Type::Unit, "e"),
        )
        && moved_callable_parameter(
            condition,
            "while",
            Type::Bool,
            loop_body_effects(Type::Unit, "e"),
        )
        && function.return_type == Some(Type::Unit)
        && function.effects == effect_parameter("e")
        && function.body.is_some()
}

fn valid_try(function: &Function) -> bool {
    let result = Type::Named(
        "core.result".to_owned(),
        vec![named_type("e"), named_type("t")],
    );
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named(
            "core.error.throwing".to_owned(),
            vec![named_type("e")],
        )],
        parameters: vec!["f".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "f".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("t"),
            type_parameter("e"),
        ]]
        && single_moved_callable(function, "action", named_type("t"), effects)
        && function.return_type == Some(result)
        && function.effects.parameters == vec!["f"]
        && !function.effects.unsafety
        && function.effects.failure.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_throw(function: &Function) -> bool {
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named(
            "core.error.throwing".to_owned(),
            vec![named_type("error")],
        )],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups == vec![vec![type_parameter("error")]]
        && single_moved_parameter(function, "error", named_type("error"))
        && function.return_type == Some(named_type("never"))
        && function.effects == effects
        && function.body.is_some()
}

fn valid_unsafe(function: &Function) -> bool {
    let effects = crate::ast::FunctionEffects {
        custom: vec![Type::Named("core.unsafe.unsafety".to_owned(), Vec::new())],
        parameters: vec!["e".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("t"),
        ]]
        && single_moved_callable(function, "action", named_type("t"), effects)
        && function.return_type == Some(named_type("t"))
        && function.effects.parameters == vec!["e"]
        && !function.effects.unsafety
        && function.effects.failure.is_none()
        && function.effects.custom.is_empty()
        && function.body.is_some()
}

fn valid_loop(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("t"),
        ]]
        && single_moved_callable(
            function,
            "body",
            Type::Unit,
            loop_body_effects(named_type("t"), "e"),
        )
        && function.return_type == Some(named_type("t"))
        && function.effects.parameters == vec!["e"]
        && !function.effects.unsafety
        && function.effects.failure.is_none()
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
            name: "e".to_owned(),
            kind: Sort::Effects,
            default: None,
        }]]
        && moved_callable_parameter(condition, "condition", Type::Bool, effect_parameter("e"))
        && moved_callable_parameter(body, "do", Type::Unit, effect_parameter("e"))
        && function.return_type == Some(Type::Unit)
        && function.effects.parameters == vec!["e"]
        && !function.effects.unsafety
        && function.effects.failure.is_none()
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
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("t"),
        ]]
        && condition.name == "condition"
        && condition.mode == PassMode::Inferred
        && condition.ty == Type::Bool
        && moved_callable_parameter(then, "then", named_type("t"), effect_parameter("e"))
        && moved_callable_parameter(else_branch, "else", named_type("t"), effect_parameter("e"))
        && function.return_type == Some(named_type("t"))
        && function.effects == effect_parameter("e")
        && function.body.is_some()
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
            type_parameter("input"),
            type_parameter("output"),
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            CompileParam {
                name: "cases".to_owned(),
                kind: Sort::ParameterPack,
                default: None,
            },
        ]]
        && input.name == "input"
        && input.mode == PassMode::Move
        && input.ty == named_type("input")
        && cases.name == "cases"
        && cases.mode == PassMode::Inferred
        && cases.ty
            == Type::Named(
                "$parameter$groups$expand".to_owned(),
                vec![named_type("cases")],
            )
        && function.return_type == Some(named_type("output"))
        && function.effects == effect_parameter("e")
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
            subject: named_type("iterable"),
            trait_ref: Type::Named("core.iter.into_iterator".to_owned(), Vec::new()),
            associated_types: vec![crate::ast::AssociatedTypeBinding {
                name: "iter".to_owned(),
                compile_groups: Vec::new(),
                ty: named_type("iter"),
            }],
        },
        crate::ast::WherePredicate {
            subject: named_type("iter"),
            trait_ref: Type::Named("core.iter.iterator".to_owned(), Vec::new()),
            associated_types: vec![crate::ast::AssociatedTypeBinding {
                name: "item".to_owned(),
                compile_groups: Vec::new(),
                ty: named_type("item"),
            }],
        },
    ];
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            type_parameter("iterable"),
            type_parameter("iter"),
            type_parameter("item"),
        ]]
        && iterable.name == "iterable"
        && iterable.mode == PassMode::Move
        && iterable.ty == named_type("iterable")
        && body.name == "body"
        && body.mode == PassMode::Move
        && body.ty
            == Type::Function {
                groups: vec![vec![named_type("item")]],
                effects: loop_body_effects(Type::Unit, "e"),
                result: Box::new(Type::Unit),
            }
        && function.return_type == Some(Type::Unit)
        && function.effects == effect_parameter("e")
        && function.where_predicates == expected_predicates
        && function.body.is_some()
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
            Type::Named("core.control.loop_exit".to_owned(), vec![result]),
            Type::Named("core.control.iteration_skip".to_owned(), Vec::new()),
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
        kind: Sort::Type,
        default: None,
    }
}

fn usize_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: Sort::USize,
        default: None,
    }
}

fn access_parameter(name: &str, default: Option<&str>) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: Sort::Named("access".to_owned()),
        default: default.map(|value| CompileParamDefault::Name(value.to_owned())),
    }
}

fn region_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: Sort::Region,
        default: None,
    }
}

fn borrow_compile_groups() -> Vec<Vec<CompileParam>> {
    vec![
        vec![access_parameter("a", Some("shared"))],
        vec![region_parameter("r")],
        vec![type_parameter("t")],
    ]
}

fn pointer_compile_groups() -> Vec<Vec<CompileParam>> {
    vec![
        vec![access_parameter("a", Some("shared"))],
        vec![type_parameter("t")],
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

fn access_borrow_type(access: &str, pointee: Type) -> Type {
    Type::Borrow {
        mutable: false,
        access: Some(access.to_owned()),
        region: None,
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

fn region_borrow_type(mutable: bool, region: &str, pointee: Type) -> Type {
    Type::Borrow {
        mutable,
        access: None,
        region: Some(region.to_owned()),
        pointee: Box::new(pointee),
    }
}

fn trait_has_default_self(definition: &TraitDef) -> bool {
    definition.self_parameter.name == "self" && definition.self_parameter.kind == Sort::Type
}

fn validate_handle(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = definition.self_parameter.name == "self"
        && definition.self_parameter.kind == Sort::Effect
        && definition.compile_groups.is_empty()
        && definition.where_predicates.is_empty()
        && matches!(
            definition.members.as_slice(),
            [TraitMember::AssociatedType {
                name,
                compile_groups,
                kind,
                default,
            }, TraitMember::Function(function)] if name == "clauses"
                && compile_groups == &vec![vec![type_parameter("value"), type_parameter("answer")]]
                && *kind == AssociatedKind::Parameters
                && default.is_none()
                && valid_handle_method(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `handle` must have shape `pub let handle = trait(comptime self: effect) { let clauses(comptime value: type, comptime answer: type): parameters; let handle(comptime value: type, comptime answer: type, comptime rest: effects) ...clauses(value, answer) (move action: (): value with(self, rest)): answer with(rest) }`"
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
        parameters: vec!["rest".to_owned(), "self".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.name == "handle"
        && function.compile_groups
            == vec![vec![
                type_parameter("value"),
                type_parameter("answer"),
                compile_effects_parameter("rest"),
            ]]
        && function.return_type == Some(named_type("answer"))
        && function.effects == effect_parameter("rest")
        && function.where_predicates.is_empty()
        && function.body.is_none()
        && clauses.name == "clauses"
        && clauses.mode == PassMode::Inferred
        && clauses.ty
            == Type::Named(
                "$parameter$groups$expand".to_owned(),
                vec![Type::Named(
                    "clauses".to_owned(),
                    vec![named_type("value"), named_type("answer")],
                )],
            )
        && action.name == "action"
        && action.mode == PassMode::Move
        && action.ty == function_type(vec![Vec::new()], named_type("value"), action_effects)
}

fn compile_effects_parameter(name: &str) -> CompileParam {
    CompileParam {
        name: name.to_owned(),
        kind: Sort::Effects,
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
    let expected_groups = vec![vec![type_parameter("t")]];
    let expected_variants = vec![
        positional_variant("some", named_type("t")),
        unit_variant("none"),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `option` must have shape `pub let option(comptime t: type) = enum { some(t), none }`"
                .to_owned(),
        );
    }
}

fn validate_result(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_groups = vec![vec![type_parameter("e")], vec![type_parameter("t")]];
    let expected_variants = vec![
        positional_variant("ok", named_type("t")),
        positional_variant("err", named_type("e")),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `result` must have shape `pub let result(comptime e: type)(comptime t: type) = enum { ok(t), err(e) }`"
                .to_owned(),
        );
    }
}

fn validate_attempt(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_groups = vec![
        vec![type_parameter("input")],
        vec![type_parameter("output")],
    ];
    let expected_variants = vec![
        positional_variant("hit", named_type("output")),
        positional_variant("miss", named_type("input")),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `attempt` must have shape `pub let attempt(comptime input: type)(comptime output: type) = enum { hit(output), miss(input) }`"
                .to_owned(),
        );
    }
}

fn validate_never(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    if !definition.compile_groups.is_empty() || !definition.variants.is_empty() {
        diagnostics.push("lang item `never` must have shape `pub let never = enum {}`".to_owned());
    }
}

fn validate_partial_ordering(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    let expected_variants = vec![
        unit_variant("less"),
        unit_variant("equal"),
        unit_variant("greater"),
        unit_variant("unordered"),
    ];
    if !definition.compile_groups.is_empty() || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `partial_ordering` must have shape `pub let partial_ordering = enum { less, equal, greater, unordered }`"
                .to_owned(),
        );
    }
}

fn validate_poll(definition: &EnumDef, diagnostics: &mut Vec<String>) {
    if definition.compile_groups != vec![vec![type_parameter("t")]]
        || definition.variants
            != vec![
                unit_variant("pending"),
                positional_variant("ready", named_type("t")),
            ]
    {
        diagnostics.push(
            "lang item `poll` must have shape `pub let poll(comptime t: type) = enum { pending, ready(t) }`"
                .to_owned(),
        );
    }
}

fn validate_move(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !move_trait_has_required_shape(definition) {
        diagnostics
            .push("lang item `movable` must have shape `pub let movable = trait {}`".to_owned());
    }
}

/// Check the relocation marker contract shared by core and ownership lowering.
pub(crate) fn move_trait_has_required_shape(definition: &TraitDef) -> bool {
    trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && definition.where_predicates.is_empty()
        && definition.members.is_empty()
}

fn validate_copy(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !copy_trait_has_required_shape(definition) {
        diagnostics.push(
            "lang item `copyable` must have shape `pub let copyable = trait where self: movable {}`"
                .to_owned(),
        );
    }
}

/// Check the marker contract shared by core bootstrapping and ownership lowering.
pub(crate) fn copy_trait_has_required_shape(definition: &TraitDef) -> bool {
    let [predicate] = definition.where_predicates.as_slice() else {
        return false;
    };
    trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && predicate.subject == named_type("self")
        && matches!(
            &predicate.trait_ref,
            Type::Named(name, arguments)
                if arguments.is_empty()
                    && matches!(name.as_str(), "movable" | "core.marker.movable" | "core::marker::movable")
        )
        && predicate.associated_types.is_empty()
        && definition.members.is_empty()
}

fn validate_drop(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !drop_trait_has_required_shape(definition) {
        diagnostics.push(
            "lang item `droppable` must have shape `pub let droppable = trait { let drop(self: borrow(mut)(self))(): () }`"
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
        && receiver.ty == simple_borrow_type(true, named_type("self"))
        && empty_group.is_empty()
}

fn validate_future(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid_supertrait = matches!(
        definition.where_predicates.as_slice(),
        [crate::ast::WherePredicate {
            subject: Type::Named(subject, subject_arguments),
            trait_ref: Type::Named(trait_name, trait_arguments),
            associated_types,
        }] if subject == "self"
            && subject_arguments.is_empty()
            && matches!(trait_name.as_str(), "movable" | "core.marker.movable" | "core::marker::movable")
            && trait_arguments.is_empty()
            && associated_types.is_empty()
    );
    let valid = trait_has_default_self(definition)
        && definition.compile_groups == vec![vec![compile_effects_parameter("e")]]
        && valid_supertrait
        && matches!(
            definition.members.as_slice(),
            [TraitMember::AssociatedType {
                name,
                compile_groups,
                kind: AssociatedKind::Type,
                default: None,
            }, TraitMember::Function(function)]
                if name == "output"
                    && compile_groups.is_empty()
                    && valid_future_poll(function)
        );
    if !valid {
        diagnostics.push(
            "lang item `future` must declare `output` and `poll(r: region)(self: borrow(mut)(r)(self))(): poll(output) with(e)`, with `self: movable`"
                .to_owned(),
        );
    }
}

fn valid_future_poll(function: &Function) -> bool {
    let [receiver_group, empty_group] = function.groups.as_slice() else {
        return false;
    };
    let [receiver] = receiver_group.as_slice() else {
        return false;
    };
    function.name == "poll"
        && function.compile_groups
            == vec![vec![CompileParam {
                name: "r".to_owned(),
                kind: Sort::Region,
                default: None,
            }]]
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == region_borrow_type(true, "r", named_type("self"))
        && empty_group.is_empty()
        && function.return_type == Some(Type::Named("poll".to_owned(), vec![named_type("output")]))
        && function.effects == effect_parameter("e")
        && function.where_predicates.is_empty()
        && function.body.is_none()
}

fn validate_executor(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let expected_bound = future_output_bound("f", "e", "t");
    let valid = trait_has_default_self(definition)
        && definition.compile_groups.is_empty()
        && definition.where_predicates.is_empty()
        && matches!(
            definition.members.as_slice(),
            [TraitMember::Function(function)]
                if function.name == "run"
                    && function.body.is_none()
                    && function.compile_groups
                        == vec![vec![
                            compile_effects_parameter("e"),
                            type_parameter("f"),
                            type_parameter("t"),
                        ]]
                    && function.effects == effect_parameter("e")
                    && function.return_type == Some(named_type("t"))
                    && function.where_predicates == vec![expected_bound]
                    && matches!(
                        function.groups.as_slice(),
                        [receiver_group, future_group]
                            if matches!(
                                receiver_group.as_slice(),
                                [receiver]
                                    if receiver.name == "self"
                                        && receiver.mode == PassMode::Inferred
                                        && receiver.ty
                                            == simple_borrow_type(true, named_type("self"))
                            )
                                && matches!(
                                    future_group.as_slice(),
                                    [future]
                                        if future.name == "future"
                                            && future.mode == PassMode::Move
                                            && future.ty == named_type("f")
                                )
                    )
        );
    if !valid {
        diagnostics.push(
            "lang item `executor` must declare `run(e: effects, f: type, t: type)` with `f: future(e, output = t)`"
                .to_owned(),
        );
    }
}

fn validate_async_function(
    kind: LangItemKind,
    definition: &Function,
    diagnostics: &mut Vec<String>,
) {
    let effects = suspension_row("e");
    let expected_bound = future_output_bound("f", "e", "t");
    let valid = definition.name == kind.source_name()
        && definition.where_predicates == vec![expected_bound]
        && match kind {
            LangItemKind::AsyncFunction => {
                definition.compile_groups
                    == vec![vec![
                        compile_effects_parameter("e"),
                        type_parameter("f"),
                        type_parameter("t"),
                    ]]
                    && single_moved_callable(
                        definition,
                        "action",
                        named_type("t"),
                        suspension_row("e"),
                    )
                    && definition.return_type == Some(named_type("f"))
                    && definition.effects == crate::ast::FunctionEffects::default()
                    && definition.body.is_none()
                    && definition.builtin
            }
            LangItemKind::AwaitFunction => {
                definition.compile_groups
                    == vec![vec![
                        compile_effects_parameter("e"),
                        type_parameter("f"),
                        type_parameter("t"),
                    ]]
                    && single_moved_parameter(definition, "future", named_type("f"))
                    && definition.return_type == Some(named_type("t"))
                    && definition.effects == effects
                    && definition.body.is_some()
                    && !definition.builtin
            }
            _ => false,
        };
    if !valid {
        diagnostics.push(format!(
            "lang item `{}` must match the source-backed core async contract",
            kind.source_name()
        ));
    }
}

fn suspension_row(rest: &str) -> crate::ast::FunctionEffects {
    crate::ast::FunctionEffects {
        custom: vec![Type::Named("core.async.suspension".to_owned(), Vec::new())],
        parameters: vec![rest.to_owned()],
        ..crate::ast::FunctionEffects::default()
    }
}

fn future_output_bound(future: &str, effects: &str, output: &str) -> crate::ast::WherePredicate {
    crate::ast::WherePredicate {
        subject: named_type(future),
        trait_ref: Type::Named(
            "future".to_owned(),
            vec![Type::Named(effects.to_owned(), Vec::new())],
        ),
        associated_types: vec![crate::ast::AssociatedTypeBinding {
            name: "output".to_owned(),
            compile_groups: Vec::new(),
            ty: named_type(output),
        }],
    }
}

fn validate_operator(kind: LangItemKind, definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let method = kind
        .operator_method()
        .expect("operator lang items have a method name");
    if !operator_trait_has_required_shape(kind, definition) {
        let shape = match kind {
            LangItemKind::Eq => format!(
                "pub let eq(comptime rhs: type) = trait {{ let {method}(self: borrow(self))(rhs: borrow(rhs)): bool }}"
            ),
            LangItemKind::PartialOrd => format!(
                "pub let partial_ord(comptime rhs: type) = trait {{ let {method}(self: borrow(self))(rhs: borrow(rhs)): partial_ordering }}"
            ),
            _ => format!(
                "pub let {kind}(comptime rhs: type) = trait {{ let output: type; let {method}(self)(rhs: rhs): output }}"
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
            "lang item `{kind}` must have shape `pub let {kind} = trait {{ let output: type; let {method}(self)(): output }}`"
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
        ] if name == "output"
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
        && function.return_type == Some(named_type("output"))
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("self")
        && empty_group.is_empty()
}

/// Check the operator contract shared by core bootstrapping and HIR lowering.
pub(crate) fn operator_trait_has_required_shape(kind: LangItemKind, definition: &TraitDef) -> bool {
    let Some(method) = kind.operator_method() else {
        return false;
    };
    let valid_groups = trait_has_default_self(definition)
        && definition.compile_groups == vec![vec![type_parameter("rhs")]];
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
                name == "output"
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
                        && matches!(name.as_str(), "partial_ordering" | "core::cmp::partial_ordering")
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
        && receiver.ty == simple_borrow_type(false, named_type("self"))
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == simple_borrow_type(false, named_type("rhs"))
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
        && function.return_type == Some(named_type("output"))
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Inferred
        && receiver.ty == named_type("self")
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Inferred
        && rhs.ty == named_type("rhs")
}

#[cfg(test)]
mod tests;
