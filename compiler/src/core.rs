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
    CompileParam, CompileParamKind, EnumDef, Function, Item, ItemOrigin, PassMode, Program,
    TraitDef, TraitMember, Type, VariantDef, VariantFields, Visibility,
};
use crate::manifest::Edition;
use crate::modules::{self, PackageId, SourceUnit};
use crate::parser;

const EDITION_2026_PRELUDE: &str = include_str!("../../library/core/src/prelude.sc");
const EDITION_2026_OPS: &str = include_str!("../../library/core/src/ops.sc");
const EDITION_2026_CONTROL: &str = include_str!("../../library/core/src/control.sc");
const EDITION_2026_ITER: &str = include_str!("../../library/core/src/iter.sc");

/// A stable logical role fulfilled by one declaration in the edition's
/// `core` bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LangItemKind {
    Option,
    Result,
    Never,
    Copy,
    Drop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
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
    UnsafeEffect,
    ThrowsEffect,
    SharedAccess,
    MutableAccess,
    Do,
    Try,
    Unsafe,
    Loop,
    Iterator,
    IntoIterator,
}

impl LangItemKind {
    const ALL: [Self; 30] = [
        Self::Option,
        Self::Result,
        Self::Never,
        Self::Copy,
        Self::Drop,
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Rem,
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
        Self::UnsafeEffect,
        Self::ThrowsEffect,
        Self::SharedAccess,
        Self::MutableAccess,
        Self::Do,
        Self::Try,
        Self::Unsafe,
        Self::Loop,
        Self::Iterator,
        Self::IntoIterator,
    ];

    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Option => "Option",
            Self::Result => "Result",
            Self::Never => "never",
            Self::Copy => "Copy",
            Self::Drop => "Drop",
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Rem => "Rem",
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
            Self::UnsafeEffect => "Unsafe",
            Self::ThrowsEffect => "Throws",
            Self::SharedAccess => "Shared",
            Self::MutableAccess => "Mutable",
            Self::Do => "do",
            Self::Try => "try",
            Self::Unsafe => "unsafe",
            Self::Loop => "loop",
            Self::Iterator => "Iterator",
            Self::IntoIterator => "IntoIterator",
        }
    }

    const fn expected_kind(self) -> &'static str {
        match self {
            Self::Option | Self::Result | Self::Never | Self::PartialOrdering => "enum",
            Self::UnsafeEffect | Self::ThrowsEffect => "effect",
            Self::SharedAccess | Self::MutableAccess => "access",
            Self::Do | Self::Try | Self::Unsafe | Self::Loop => "function",
            Self::Copy
            | Self::Drop
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Rem
            | Self::Eq
            | Self::PartialOrd
            | Self::Neg
            | Self::Not
            | Self::BitAnd
            | Self::BitOr
            | Self::BitXor
            | Self::Shl
            | Self::Shr
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
            | Self::Copy
            | Self::Drop
            | Self::PartialOrdering
            | Self::UnsafeEffect
            | Self::ThrowsEffect
            | Self::SharedAccess
            | Self::MutableAccess
            | Self::Do
            | Self::Try
            | Self::Unsafe
            | Self::Loop => None,
            Self::Iterator | Self::IntoIterator => None,
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
    copy: LangItem,
    drop: LangItem,
    add: LangItem,
    sub: LangItem,
    mul: LangItem,
    div: LangItem,
    rem: LangItem,
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
    unsafe_effect: LangItem,
    throws_effect: LangItem,
    shared_access: LangItem,
    mutable_access: LangItem,
    do_function: LangItem,
    try_function: LangItem,
    unsafe_function: LangItem,
    loop_function: LangItem,
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

    pub const fn unsafe_effect(&self) -> &LangItem {
        &self.unsafe_effect
    }
    pub const fn throws_effect(&self) -> &LangItem {
        &self.throws_effect
    }
    pub const fn shared_access(&self) -> &LangItem {
        &self.shared_access
    }
    pub const fn mutable_access(&self) -> &LangItem {
        &self.mutable_access
    }
    pub const fn do_function(&self) -> &LangItem {
        &self.do_function
    }
    pub const fn try_function(&self) -> &LangItem {
        &self.try_function
    }
    pub const fn unsafe_function(&self) -> &LangItem {
        &self.unsafe_function
    }
    pub const fn loop_function(&self) -> &LangItem {
        &self.loop_function
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
            LangItemKind::Copy => &self.copy,
            LangItemKind::Drop => &self.drop,
            LangItemKind::Add => &self.add,
            LangItemKind::Sub => &self.sub,
            LangItemKind::Mul => &self.mul,
            LangItemKind::Div => &self.div,
            LangItemKind::Rem => &self.rem,
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
            LangItemKind::UnsafeEffect => &self.unsafe_effect,
            LangItemKind::ThrowsEffect => &self.throws_effect,
            LangItemKind::SharedAccess => &self.shared_access,
            LangItemKind::MutableAccess => &self.mutable_access,
            LangItemKind::Do => &self.do_function,
            LangItemKind::Try => &self.try_function,
            LangItemKind::Unsafe => &self.unsafe_function,
            LangItemKind::Loop => &self.loop_function,
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
                    ("prelude", EDITION_2026_PRELUDE),
                    ("ops", EDITION_2026_OPS),
                    ("control", EDITION_2026_CONTROL),
                    ("iter", EDITION_2026_ITER),
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
        let source = format!("{source}\n{EDITION_2026_CONTROL}\n{EDITION_2026_ITER}");
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
                    module_path: vec!["@core".to_owned(), (*module).to_owned()],
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
                module_path: if *module == "prelude" {
                    Vec::new()
                } else {
                    vec!["core".to_owned(), (*module).to_owned()]
                },
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
                vec![
                    "@core".to_owned(),
                    origin
                        .module_path
                        .last()
                        .expect("non-root embedded core module has a name")
                        .clone(),
                ]
            };
        }
        for lang_item in [
            &mut lang_items.option,
            &mut lang_items.result,
            &mut lang_items.never,
            &mut lang_items.copy,
            &mut lang_items.drop,
            &mut lang_items.add,
            &mut lang_items.sub,
            &mut lang_items.mul,
            &mut lang_items.div,
            &mut lang_items.rem,
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
            &mut lang_items.unsafe_effect,
            &mut lang_items.throws_effect,
            &mut lang_items.shared_access,
            &mut lang_items.mutable_access,
            &mut lang_items.do_function,
            &mut lang_items.try_function,
            &mut lang_items.unsafe_function,
            &mut lang_items.loop_function,
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
    let mut diagnostics = Vec::new();

    if !program.uses.is_empty() {
        diagnostics.push("embedded prelude must not contain `use` declarations".to_owned());
    }
    if program.items.len() != program.item_visibilities.len()
        || program.items.len() != program.item_origins.len()
    {
        diagnostics.push("embedded prelude item metadata is inconsistent".to_owned());
        return Err(CoreBundleError::new(edition, diagnostics));
    }

    let mut indices: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, (item, visibility)) in program
        .items
        .iter()
        .zip(&program.item_visibilities)
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
        let Some(kind) = LangItemKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.source_name() == name)
        else {
            diagnostics.push(format!(
                "unexpected declaration `{name}` at item {}",
                index + 1
            ));
            continue;
        };
        indices.entry(kind.source_name()).or_default().push(index);
        if *visibility != Visibility::Public {
            diagnostics.push(format!(
                "lang item `{kind}` must be `pub`, found {} visibility",
                visibility_name(*visibility)
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    for kind in LangItemKind::ALL {
        match indices.get(kind.source_name()).map(Vec::as_slice) {
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
        copy: item(LangItemKind::Copy),
        drop: item(LangItemKind::Drop),
        add: item(LangItemKind::Add),
        sub: item(LangItemKind::Sub),
        mul: item(LangItemKind::Mul),
        div: item(LangItemKind::Div),
        rem: item(LangItemKind::Rem),
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
        unsafe_effect: item(LangItemKind::UnsafeEffect),
        throws_effect: item(LangItemKind::ThrowsEffect),
        shared_access: item(LangItemKind::SharedAccess),
        mutable_access: item(LangItemKind::MutableAccess),
        do_function: item(LangItemKind::Do),
        try_function: item(LangItemKind::Try),
        unsafe_function: item(LangItemKind::Unsafe),
        loop_function: item(LangItemKind::Loop),
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
        Item::Access(definition) => Some(&definition.name),
        Item::Trait(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn item_kind(item: &Item) -> &'static str {
    match item {
        Item::Function(_) => "function",
        Item::Global(_) => "global",
        Item::Struct(_) => "struct",
        Item::Enum(_) => "enum",
        Item::Effect(_) => "effect",
        Item::Access(_) => "access",
        Item::Trait(_) => "trait",
        Item::Extend(_) => "extension",
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
        (LangItemKind::SharedAccess | LangItemKind::MutableAccess, Item::Access(_)) => {}
        (
            LangItemKind::Do | LangItemKind::Try | LangItemKind::Unsafe | LangItemKind::Loop,
            Item::Function(function),
        ) => validate_control_function(kind, function, diagnostics),
        (LangItemKind::Iterator, Item::Trait(definition)) => {
            validate_iterator(definition, diagnostics)
        }
        (LangItemKind::IntoIterator, Item::Trait(definition)) => {
            validate_into_iterator(definition, diagnostics)
        }
        (kind @ (LangItemKind::Neg | LangItemKind::Not), Item::Trait(definition)) => {
            validate_unary_operator(kind, definition, diagnostics)
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

fn validate_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name, compile_groups, default: None },
                TraitMember::Function(function),
            ] if name == "Item"
                && compile_groups.is_empty()
                && valid_iteration_method(
                    function,
                    "next",
                    PassMode::MutBorrow,
                    Type::Named("Option".to_owned(), vec![named_type("Item")]),
                )
        );
    if !valid {
        diagnostics.push(
            "lang item `Iterator` must declare `Item` and `next(borrow(mut) self)(): Option(Item)`"
                .to_owned(),
        );
    }
}

fn validate_into_iterator(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let valid = definition.compile_groups.is_empty()
        && matches!(
            definition.members.as_slice(),
            [
                TraitMember::AssociatedType { name: into_iter, compile_groups: iter_groups, default: None },
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
        && receiver.mode == mode
        && receiver.ty == named_type("Self")
        && empty_group.is_empty()
}

fn validate_effect(
    kind: LangItemKind,
    definition: &crate::ast::EffectDef,
    diagnostics: &mut Vec<String>,
) {
    let valid = match kind {
        LangItemKind::UnsafeEffect => definition.compile_groups.is_empty(),
        LangItemKind::ThrowsEffect => definition.compile_groups == vec![vec![type_parameter("E")]],
        _ => false,
    };
    if !valid {
        let shape = match kind {
            LangItemKind::UnsafeEffect => "pub let Unsafe = effect",
            LangItemKind::ThrowsEffect => "pub let Throws(E: type) = effect",
            _ => unreachable!(),
        };
        diagnostics.push(format!("lang item `{kind}` must have shape `{shape}`"));
    }
}

fn validate_control_function(
    kind: LangItemKind,
    function: &Function,
    diagnostics: &mut Vec<String>,
) {
    let valid = function.body.is_none()
        && function.where_predicates.is_empty()
        && match kind {
            LangItemKind::Do => valid_do(function),
            LangItemKind::Try => valid_try(function),
            LangItemKind::Unsafe => valid_unsafe(function),
            LangItemKind::Loop => valid_loop(function),
            _ => false,
        };
    if !valid {
        diagnostics.push(format!(
            "lang item `{kind}` has an invalid compiler-provided control signature"
        ));
    }
}

fn valid_do(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(function, "action", named_type("T"), effect_parameter("E"))
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
}

fn valid_try(function: &Function) -> bool {
    let result = Type::Named("Result".to_owned(), vec![named_type("T"), named_type("E")]);
    let effects = crate::ast::FunctionEffects {
        throws: Some(Box::new(named_type("E"))),
        parameters: vec!["F".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "F".to_owned(),
                kind: CompileParamKind::Effect,
            },
            type_parameter("T"),
            type_parameter("E"),
        ]]
        && single_moved_callable(function, "action", result.clone(), effects)
        && function.return_type == Some(result)
        && function.effects.parameters == vec!["F"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
}

fn valid_unsafe(function: &Function) -> bool {
    let effects = crate::ast::FunctionEffects {
        unsafe_effect: true,
        parameters: vec!["E".to_owned()],
        ..crate::ast::FunctionEffects::default()
    };
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(function, "action", named_type("T"), effects)
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
}

fn valid_loop(function: &Function) -> bool {
    function.compile_groups
        == vec![vec![
            CompileParam {
                name: "E".to_owned(),
                kind: CompileParamKind::Effect,
            },
            type_parameter("T"),
        ]]
        && single_moved_callable(function, "body", Type::Unit, effect_parameter("E"))
        && function.return_type == Some(named_type("T"))
        && function.effects.parameters == vec!["E"]
        && !function.effects.unsafe_effect
        && function.effects.throws.is_none()
        && function.effects.custom.is_empty()
}

fn effect_parameter(name: &str) -> crate::ast::FunctionEffects {
    crate::ast::FunctionEffects {
        parameters: vec![name.to_owned()],
        ..crate::ast::FunctionEffects::default()
    }
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
    }
}

fn named_type(name: &str) -> Type {
    Type::Named(name.to_owned(), Vec::new())
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
    let expected_groups = vec![vec![type_parameter("T"), type_parameter("E")]];
    let expected_variants = vec![
        positional_variant("Ok", named_type("T")),
        positional_variant("Err", named_type("E")),
    ];
    if definition.compile_groups != expected_groups || definition.variants != expected_variants {
        diagnostics.push(
            "lang item `Result` must have shape `pub let Result(T: type, E: type) = enum { Ok(T), Err(E) }`"
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
    definition.compile_groups.is_empty() && definition.members.is_empty()
}

fn validate_drop(definition: &TraitDef, diagnostics: &mut Vec<String>) {
    if !drop_trait_has_required_shape(definition) {
        diagnostics.push(
            "lang item `Drop` must have shape `pub let Drop = trait { let drop(borrow(mut) self)(): () }`"
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
    definition.compile_groups.is_empty()
        && function.name == "drop"
        && function.compile_groups.is_empty()
        && function.return_type == Some(Type::Unit)
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::MutBorrow
        && receiver.ty == named_type("Self")
        && empty_group.is_empty()
}

fn validate_operator(kind: LangItemKind, definition: &TraitDef, diagnostics: &mut Vec<String>) {
    let method = kind
        .operator_method()
        .expect("operator lang items have a method name");
    if !operator_trait_has_required_shape(kind, definition) {
        let shape = match kind {
            LangItemKind::Eq => format!(
                "pub let Eq(Rhs: type) = trait {{ let {method}(borrow self)(borrow rhs: Rhs): bool }}"
            ),
            LangItemKind::PartialOrd => format!(
                "pub let PartialOrd(Rhs: type) = trait {{ let {method}(borrow self)(borrow rhs: Rhs): PartialOrdering }}"
            ),
            _ => format!(
                "pub let {kind}(Rhs: type) = trait {{ let Output: type; let {method}(move self)(move rhs: Rhs): Output }}"
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
            "lang item `{kind}` must have shape `pub let {kind} = trait {{ let Output: type; let {method}(move self)(): Output }}`"
        ));
    }
}

pub(crate) fn unary_operator_trait_has_required_shape(
    kind: LangItemKind,
    definition: &TraitDef,
) -> bool {
    if !matches!(kind, LangItemKind::Neg | LangItemKind::Not)
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
            TraitMember::AssociatedType { name, compile_groups, default: None },
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
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("Self")
        && empty_group.is_empty()
}

/// Check the operator contract shared by core bootstrapping and HIR lowering.
pub(crate) fn operator_trait_has_required_shape(kind: LangItemKind, definition: &TraitDef) -> bool {
    let Some(method) = kind.operator_method() else {
        return false;
    };
    let valid_groups = definition.compile_groups == vec![vec![type_parameter("Rhs")]];
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
                        && matches!(name.as_str(), "PartialOrdering" | "core::ops::PartialOrdering")
            ),
        ),
        _ => return false,
    };
    function.name == method
        && function.compile_groups.is_empty()
        && result_is_valid
        && function.body.is_none()
        && receiver.name == "self"
        && receiver.mode == PassMode::Borrow
        && receiver.ty == named_type("Self")
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Borrow
        && rhs.ty == named_type("Rhs")
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
        && receiver.mode == PassMode::Move
        && receiver.ty == named_type("Self")
        && rhs.name == "rhs"
        && rhs.mode == PassMode::Move
        && rhs.ty == named_type("Rhs")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_source_with_copy(copy_declaration: &str) -> String {
        [
            r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Result(T: type, E: type) = enum { Ok(T), Err(E) }
pub let never = enum {}
"#,
            copy_declaration,
            r#"
pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(move self)(move rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(move self)(move rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(borrow self)(borrow rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
}
"#,
        ]
        .concat()
    }

    #[test]
    fn edition_2026_bundle_parses_and_validates() {
        let bundle = CoreBundle::for_edition(Edition::Edition2026).unwrap();

        assert_eq!(bundle.edition(), Edition::Edition2026);
        assert_eq!(bundle.program().items.len(), 30);
        for kind in LangItemKind::ALL {
            let lang_item = bundle.lang_items().get(kind);
            assert_eq!(lang_item.kind(), kind);
            let canonical = match kind {
                LangItemKind::Option
                | LangItemKind::Result
                | LangItemKind::Never
                | LangItemKind::Copy
                | LangItemKind::Drop => kind.source_name().to_owned(),
                LangItemKind::Add
                | LangItemKind::Sub
                | LangItemKind::Mul
                | LangItemKind::Div
                | LangItemKind::Rem
                | LangItemKind::Eq
                | LangItemKind::PartialOrdering
                | LangItemKind::PartialOrd
                | LangItemKind::Neg
                | LangItemKind::Not
                | LangItemKind::BitAnd
                | LangItemKind::BitOr
                | LangItemKind::BitXor
                | LangItemKind::Shl
                | LangItemKind::Shr => format!("core::ops::{}", kind.source_name()),
                LangItemKind::UnsafeEffect
                | LangItemKind::ThrowsEffect
                | LangItemKind::SharedAccess
                | LangItemKind::MutableAccess
                | LangItemKind::Do
                | LangItemKind::Try
                | LangItemKind::Unsafe
                | LangItemKind::Loop => format!("core::control::{}", kind.source_name()),
                LangItemKind::Iterator | LangItemKind::IntoIterator => {
                    format!("core::iter::{}", kind.source_name())
                }
            };
            assert_eq!(
                item_name(&bundle.program().items[lang_item.item_index()]),
                Some(canonical.as_str())
            );
            assert_eq!(lang_item.canonical_name(), canonical);
            let module = match kind {
                LangItemKind::Option
                | LangItemKind::Result
                | LangItemKind::Never
                | LangItemKind::Copy
                | LangItemKind::Drop => "prelude",
                LangItemKind::Add
                | LangItemKind::Sub
                | LangItemKind::Mul
                | LangItemKind::Div
                | LangItemKind::Rem
                | LangItemKind::Eq
                | LangItemKind::PartialOrdering
                | LangItemKind::PartialOrd
                | LangItemKind::Neg
                | LangItemKind::Not
                | LangItemKind::BitAnd
                | LangItemKind::BitOr
                | LangItemKind::BitXor
                | LangItemKind::Shl
                | LangItemKind::Shr => "ops",
                LangItemKind::UnsafeEffect
                | LangItemKind::ThrowsEffect
                | LangItemKind::SharedAccess
                | LangItemKind::MutableAccess
                | LangItemKind::Do
                | LangItemKind::Try
                | LangItemKind::Unsafe
                | LangItemKind::Loop => "control",
                LangItemKind::Iterator | LangItemKind::IntoIterator => "iter",
            };
            assert_eq!(
                bundle.program().item_origins[lang_item.item_index()],
                ItemOrigin {
                    package: PackageId::CORE.0,
                    module_path: vec!["@core".to_owned(), module.to_owned()],
                }
            );
        }
    }

    #[test]
    fn rejects_malformed_control_contracts() {
        let malformed = EDITION_2026_CONTROL.replace(
            "pub let unsafe(E: effect, T: type)(move action: (): T with(unsafe, E)): T with(E)",
            "pub let unsafe(E: effect, T: type)(move action: (): T with(E)): T with(E)",
        );
        let error = CoreBundle::from_modules(
            Edition::Edition2026,
            &[
                ("prelude", EDITION_2026_PRELUDE),
                ("ops", EDITION_2026_OPS),
                ("control", &malformed),
                ("iter", EDITION_2026_ITER),
            ],
        )
        .unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `unsafe`")));
    }

    #[test]
    fn rejects_malformed_iteration_contracts() {
        let malformed = EDITION_2026_ITER.replace(
            "let next(borrow(mut) self)(): Option(Item)",
            "let next(borrow self)(): Option(Item)",
        );
        let error = CoreBundle::from_modules(
            Edition::Edition2026,
            &[
                ("prelude", EDITION_2026_PRELUDE),
                ("ops", EDITION_2026_OPS),
                ("control", EDITION_2026_CONTROL),
                ("iter", &malformed),
            ],
        )
        .unwrap_err();
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("lang item `Iterator`")));
    }

    #[test]
    fn lang_item_identities_follow_validated_declarations_not_source_order() {
        let source = r#"
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output
}
pub let Copy = trait {}
pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
pub let never = enum {}
pub let Option(T: type) = enum { Some(T), None }
pub let Result(T: type, E: type) = enum { Ok(T), Err(E) }
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(move self)(move rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(move self)(move rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(borrow self)(borrow rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
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
pub let Result = struct(value: i32)
pub let never = enum { Reachable }
pub let Copy(T: type) = trait {}
pub let Add(Rhs: type) = trait {
  let add(move self)(move rhs: Rhs): Rhs
}
pub let Extra = enum {}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(move self)(move rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(move self)(move rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(borrow self)(borrow rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
}
pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
"#;
        let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

        assert_eq!(
            error.diagnostics(),
            [
                "lang item `Option` must be `pub`, found private visibility",
                "unexpected declaration `Extra` at item 6",
                "lang item `Result` must be enum, found struct",
                "lang item `never` must have shape `pub let never = enum {}`",
                "lang item `Copy` must have shape `pub let Copy = trait {}`",
                "lang item `Add` must have shape `pub let Add(Rhs: type) = trait { let Output: type; let add(move self)(move rhs: Rhs): Output }`",
            ]
        );
        assert_eq!(
            error.to_string(),
            "invalid embedded core bundle for edition 2026\n- lang item `Option` must be `pub`, found private visibility\n- unexpected declaration `Extra` at item 6\n- lang item `Result` must be enum, found struct\n- lang item `never` must have shape `pub let never = enum {}`\n- lang item `Copy` must have shape `pub let Copy = trait {}`\n- lang item `Add` must have shape `pub let Add(Rhs: type) = trait { let Output: type; let add(move self)(move rhs: Rhs): Output }`"
        );
    }

    #[test]
    fn rejects_missing_and_duplicate_lang_items_in_fixed_role_order() {
        let source = r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Option(T: type) = enum { Some(T), None }
pub let never = enum {}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(move self)(move rhs: Rhs): Output
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let div(move self)(move rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output
}
pub let Eq(Rhs: type) = trait {
  let eq(borrow self)(borrow rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
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
            "pub let Copy = trait { let clone(borrow self)(): Self }",
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
            "pub let Drop(T: type) = trait { let drop(borrow(mut) self)(): () }",
            "pub let Drop = trait {}",
            "pub let Drop = trait { let drop(borrow self)(): () }",
            "pub let Drop = trait { let drop(borrow(mut) self)(): i32 }",
        ];

        for declaration in malformed_declarations {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                "pub let Drop = trait {\n  let drop(borrow(mut) self)(): ()\n}",
                declaration,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(
                error.diagnostics(),
                ["lang item `Drop` must have shape `pub let Drop = trait { let drop(borrow(mut) self)(): () }`"],
                "unexpected diagnostic for `{declaration}`"
            );
        }
    }

    #[test]
    fn rejects_malformed_operator_traits_in_fixed_role_order() {
        let source = r#"
pub let Option(T: type) = enum { Some(T), None }
pub let Result(T: type, E: type) = enum { Ok(T), Err(E) }
pub let never = enum {}
pub let Copy = trait {}
pub let Drop = trait {
  let drop(borrow(mut) self)(): ()
}
pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
pub let Sub = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Mul(Rhs: type) = trait {
  let mul(move self)(move rhs: Rhs): Rhs
}
pub let Div(Rhs: type) = trait {
  let Output: type
  let divide(move self)(move rhs: Rhs): Output
}
pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output = { rhs }
}
pub let Eq(Rhs: type) = trait {
  let eq(move self)(move rhs: Rhs): bool
}
pub let PartialOrdering = enum { Less, Equal, Greater, Unordered }
pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(move self)(move rhs: Rhs): PartialOrdering
}
pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}
pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}
pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}
pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}
pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}
pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}
pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
}
"#;
        let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

        assert_eq!(
            error.diagnostics(),
            [
                "lang item `Sub` must have shape `pub let Sub(Rhs: type) = trait { let Output: type; let sub(move self)(move rhs: Rhs): Output }`",
                "lang item `Mul` must have shape `pub let Mul(Rhs: type) = trait { let Output: type; let mul(move self)(move rhs: Rhs): Output }`",
                "lang item `Div` must have shape `pub let Div(Rhs: type) = trait { let Output: type; let div(move self)(move rhs: Rhs): Output }`",
                "lang item `Rem` must have shape `pub let Rem(Rhs: type) = trait { let Output: type; let rem(move self)(move rhs: Rhs): Output }`",
                "lang item `Eq` must have shape `pub let Eq(Rhs: type) = trait { let eq(borrow self)(borrow rhs: Rhs): bool }`",
                "lang item `PartialOrd` must have shape `pub let PartialOrd(Rhs: type) = trait { let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering }`",
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
                "pub let Neg = trait {\n  let Output: type\n  let neg(move self)(): Output\n}",
                "pub let Neg(Rhs: type) = trait { let neg(move self)(): i32 }",
                "lang item `Neg` must have shape `pub let Neg = trait { let Output: type; let neg(move self)(): Output }`",
            ),
            (
                "pub let Not = trait {\n  let Output: type\n  let not(move self)(): Output\n}",
                "pub let Not = trait { let Output: type; let not(borrow self)(): Output }",
                "lang item `Not` must have shape `pub let Not = trait { let Output: type; let not(move self)(): Output }`",
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
                "pub let BitAnd(Rhs: type) = trait {\n  let Output: type\n  let bit_and(move self)(move rhs: Rhs): Output\n}",
                "pub let BitAnd = trait { let bit_and(borrow self)(move rhs: i32): i32 }",
                "lang item `BitAnd` must have shape `pub let BitAnd(Rhs: type) = trait { let Output: type; let bit_and(move self)(move rhs: Rhs): Output }`",
            ),
            (
                "pub let Shr(Rhs: type) = trait {\n  let Output: type\n  let shr(move self)(move rhs: Rhs): Output\n}",
                "pub let Shr(Rhs: type) = trait { let Output: type; let shift(move self)(move rhs: Rhs): Output }",
                "lang item `Shr` must have shape `pub let Shr(Rhs: type) = trait { let Output: type; let shr(move self)(move rhs: Rhs): Output }`",
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
