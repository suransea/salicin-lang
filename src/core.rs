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
use crate::modules::PackageId;
use crate::parser;

const EDITION_2026_PRELUDE: &str = include_str!("../library/core/src/prelude.sali");

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
}

impl LangItemKind {
    const ALL: [Self; 10] = [
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
        }
    }

    const fn expected_kind(self) -> &'static str {
        match self {
            Self::Option | Self::Result | Self::Never => "enum",
            Self::Copy | Self::Drop | Self::Add | Self::Sub | Self::Mul | Self::Div | Self::Rem => {
                "trait"
            }
        }
    }

    pub(crate) const fn operator_method(self) -> Option<&'static str> {
        match self {
            Self::Add => Some("add"),
            Self::Sub => Some("sub"),
            Self::Mul => Some("mul"),
            Self::Div => Some("div"),
            Self::Rem => Some("rem"),
            Self::Option | Self::Result | Self::Never | Self::Copy | Self::Drop => None,
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
        Self::from_source(edition, embedded_prelude_source(edition))
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

    fn from_source(edition: Edition, source: &str) -> Result<Self, CoreBundleError> {
        let mut program = parser::parse(source).map_err(|error| {
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
    })
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(function) => Some(&function.name),
        Item::Global(binding) => Some(&binding.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
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
        (LangItemKind::Copy, Item::Trait(definition)) => validate_copy(definition, diagnostics),
        (LangItemKind::Drop, Item::Trait(definition)) => validate_drop(definition, diagnostics),
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
            "lang item `Drop` must have shape `pub let Drop = trait { let drop(mut borrow self)(): () }`"
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
        diagnostics.push(
            format!(
                "lang item `{kind}` must have shape `pub let {kind}(Rhs: type) = trait {{ let Output: type; let {method}(move self)(move rhs: Rhs): Output }}`"
            ),
        );
    }
}

/// Check the operator contract shared by core bootstrapping and HIR lowering.
pub(crate) fn operator_trait_has_required_shape(kind: LangItemKind, definition: &TraitDef) -> bool {
    let Some(method) = kind.operator_method() else {
        return false;
    };
    let valid_groups = definition.compile_groups == vec![vec![type_parameter("Rhs")]];
    let valid_members = match definition.members.as_slice() {
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
    };
    valid_groups && valid_members
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
  let drop(mut borrow self)(): ()
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
"#,
        ]
        .concat()
    }

    #[test]
    fn edition_2026_bundle_parses_and_validates() {
        let bundle = CoreBundle::for_edition(Edition::Edition2026).unwrap();

        assert_eq!(bundle.edition(), Edition::Edition2026);
        assert_eq!(bundle.program().items.len(), 10);
        for kind in LangItemKind::ALL {
            let lang_item = bundle.lang_items().get(kind);
            assert_eq!(lang_item.kind(), kind);
            assert_eq!(
                item_name(&bundle.program().items[lang_item.item_index()]),
                Some(kind.source_name())
            );
            assert_eq!(lang_item.canonical_name(), kind.source_name());
            assert_eq!(
                bundle.program().item_origins[lang_item.item_index()],
                ItemOrigin {
                    package: PackageId::CORE.0,
                    module_path: vec!["@core".to_owned()],
                }
            );
        }
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
  let drop(mut borrow self)(): ()
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
pub let Drop = trait {
  let drop(mut borrow self)(): ()
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
            "pub let Drop(T: type) = trait { let drop(mut borrow self)(): () }",
            "pub let Drop = trait {}",
            "pub let Drop = trait { let drop(borrow self)(): () }",
            "pub let Drop = trait { let drop(mut borrow self)(): i32 }",
        ];

        for declaration in malformed_declarations {
            let source = core_source_with_copy("pub let Copy = trait {}").replacen(
                "pub let Drop = trait {\n  let drop(mut borrow self)(): ()\n}",
                declaration,
                1,
            );
            let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
            assert_eq!(
                error.diagnostics(),
                ["lang item `Drop` must have shape `pub let Drop = trait { let drop(mut borrow self)(): () }`"],
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
  let drop(mut borrow self)(): ()
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
  let rem(move self)(move rhs: Rhs): Output = rhs
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
            ]
        );
    }

    #[test]
    fn reports_embedded_source_parse_errors() {
        let error =
            CoreBundle::from_source(Edition::Edition2026, "pub let Option = enum {").unwrap_err();

        assert_eq!(error.diagnostics().len(), 1);
        assert!(error.diagnostics()[0].starts_with("embedded prelude does not parse: "));
    }
}
