//! Edition-pinned Salicin `alloc` bootstrap sources.

use std::error::Error;
use std::fmt;

use crate::ast::{
    CompileParamKind, Function, Item, ItemOrigin, PassMode, Program, StructDef, Type, Visibility,
};
use crate::manifest::Edition;
use crate::modules::PackageId;
use crate::parser;

const EDITION_2026_PRELUDE: &str = include_str!("../library/alloc/src/prelude.sali");

#[derive(Clone, Debug, PartialEq)]
pub struct AllocBundle {
    program: Program,
}

impl AllocBundle {
    pub fn for_edition(edition: Edition) -> Result<Self, AllocBundleError> {
        let source = match edition {
            Edition::Edition2026 => EDITION_2026_PRELUDE,
        };
        let mut program = parser::parse(source).map_err(|error| {
            AllocBundleError::new(
                edition,
                vec![format!("embedded alloc does not parse: {error}")],
            )
        })?;
        program.item_origins = vec![
            ItemOrigin {
                package: PackageId::ALLOC.0,
                module_path: vec!["@alloc".to_owned()],
            };
            program.items.len()
        ];
        validate_program(edition, &program)?;
        Ok(Self { program })
    }

    pub const fn program(&self) -> &Program {
        &self.program
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocBundleError {
    edition: Edition,
    diagnostics: Vec<String>,
}

impl AllocBundleError {
    fn new(edition: Edition, diagnostics: Vec<String>) -> Self {
        Self {
            edition,
            diagnostics,
        }
    }
}

impl fmt::Display for AllocBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid embedded alloc bundle for edition {}",
            self.edition
        )?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n- {diagnostic}")?;
        }
        Ok(())
    }
}

impl Error for AllocBundleError {}

fn validate_program(edition: Edition, program: &Program) -> Result<(), AllocBundleError> {
    let mut diagnostics = Vec::new();
    if !program.uses.is_empty() {
        diagnostics.push("embedded alloc must not contain `use` declarations".to_owned());
    }
    if program.items.len() != 6
        || program.item_visibilities.len() != 6
        || program.item_origins.len() != 6
    {
        diagnostics.push(
            "embedded alloc must contain exactly Box, box_new, box_ptr, box_into_inner, box_replace, and the Box extension"
                .to_owned(),
        );
    } else {
        if program.item_visibilities[..5]
            .iter()
            .any(|visibility| *visibility != Visibility::Public)
        {
            diagnostics.push("all embedded alloc bootstrap items must be public".to_owned());
        }
        if program.item_visibilities[5] != Visibility::Private {
            diagnostics.push("the embedded Box extension must not declare visibility".to_owned());
        }
        match &program.items[0] {
            Item::Struct(definition) if valid_box(definition) => {}
            _ => diagnostics.push(
                "alloc Box must have shape `pub let Box(T: type) = struct(pointer: MutPtr(T))`"
                    .to_owned(),
            ),
        }
        match &program.items[1] {
            Item::Function(function) if valid_box_new(function) => {}
            _ => diagnostics.push(
                "alloc box_new must be a generic owning constructor `(move value: T): Box(T)`"
                    .to_owned(),
            ),
        }
        match &program.items[2] {
            Item::Function(function) if valid_box_ptr(function) => {}
            _ => diagnostics
                .push("alloc box_ptr must borrow `Box(T)` and return `MutPtr(T)`".to_owned()),
        }
        match &program.items[3] {
            Item::Function(function) if valid_box_into_inner(function) => {}
            _ => diagnostics.push(
                "alloc box_into_inner must consume `Box(T)` and return its owned `T`".to_owned(),
            ),
        }
        match &program.items[4] {
            Item::Function(function) if valid_box_replace(function) => {}
            _ => diagnostics.push(
                "alloc box_replace must mutably borrow `Box(T)`, consume a replacement `T`, and return the old `T`"
                    .to_owned(),
            ),
        }
        match &program.items[5] {
            Item::Extend(extension) if valid_box_extension(extension) => {}
            _ => diagnostics.push(
                "alloc Box extension must provide new, as_mut_ptr, into_inner, and replace"
                    .to_owned(),
            ),
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(AllocBundleError::new(edition, diagnostics))
    }
}

fn generic_t(function: &Function) -> bool {
    matches!(
        function.compile_groups.as_slice(),
        [group]
            if matches!(group.as_slice(), [parameter]
                if parameter.name == "T" && parameter.kind == CompileParamKind::Type)
    )
}

fn named(name: &str) -> Type {
    Type::Named(name.to_owned(), Vec::new())
}

fn applied(name: &str, argument: Type) -> Type {
    Type::Named(name.to_owned(), vec![argument])
}

fn valid_box(definition: &StructDef) -> bool {
    definition.name == "Box"
        && matches!(
            definition.compile_groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "T" && parameter.kind == CompileParamKind::Type)
        )
        && matches!(
            definition.fields.as_slice(),
            [field]
                if field.visibility == Visibility::Private
                    && field.name == "pointer"
                    && field.ty == applied("MutPtr", named("T"))
        )
}

fn valid_box_new(function: &Function) -> bool {
    function.name == "box_new"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "value"
                        && parameter.mode == PassMode::Move
                        && parameter.ty == named("T"))
        )
        && function.return_type == Some(applied("Box", named("T")))
        && function.body.is_some()
}

fn valid_box_ptr(function: &Function) -> bool {
    function.name == "box_ptr"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "boxed"
                        && parameter.mode == PassMode::Borrow
                        && parameter.ty == applied("Box", named("T")))
        )
        && function.return_type == Some(applied("MutPtr", named("T")))
        && function.body.is_some()
}

fn valid_box_into_inner(function: &Function) -> bool {
    function.name == "box_into_inner"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "boxed"
                        && parameter.mode == PassMode::Move
                        && parameter.ty == applied("Box", named("T")))
        )
        && function.return_type == Some(named("T"))
        && function.body.is_some()
}

fn valid_box_replace(function: &Function) -> bool {
    function.name == "box_replace"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [receiver, replacement]
                if matches!(receiver.as_slice(), [parameter]
                    if parameter.name == "boxed"
                        && parameter.mode == PassMode::MutBorrow
                        && parameter.ty == applied("Box", named("T")))
                    && matches!(replacement.as_slice(), [parameter]
                        if parameter.name == "value"
                            && parameter.mode == PassMode::Move
                            && parameter.ty == named("T"))
        )
        && function.return_type == Some(named("T"))
        && function.body.is_some()
}

fn valid_box_extension(extension: &crate::ast::ExtendDef) -> bool {
    matches!(
        extension.compile_groups.as_slice(),
        [group]
            if matches!(group.as_slice(), [parameter]
                if parameter.name == "T" && parameter.kind == CompileParamKind::Type)
    ) && extension.target == applied("Box", named("T"))
        && extension.trait_ref.is_none()
        && extension.members.len() == 4
        && matches!(&extension.members[0], crate::ast::ExtendMember::Function(function)
            if function.name == "new"
                && function.compile_groups.is_empty()
                && matches!(function.groups.as_slice(), [group]
                    if matches!(group.as_slice(), [parameter]
                        if parameter.name == "value"
                            && parameter.mode == PassMode::Move
                            && parameter.ty == named("T")))
                && function.return_type == Some(applied("Box", named("T")))
                && function.body.is_some())
        && matches!(&extension.members[1], crate::ast::ExtendMember::Function(function)
            if valid_box_method(function, "as_mut_ptr", PassMode::Borrow, &[], applied("MutPtr", named("T"))))
        && matches!(&extension.members[2], crate::ast::ExtendMember::Function(function)
            if valid_box_method(function, "into_inner", PassMode::Move, &[], named("T")))
        && matches!(&extension.members[3], crate::ast::ExtendMember::Function(function)
        if valid_box_method(
            function,
            "replace",
            PassMode::MutBorrow,
            &[("value", PassMode::Move, named("T"))],
            named("T"),
        ))
}

fn valid_box_method(
    function: &Function,
    name: &str,
    receiver_mode: PassMode,
    parameters: &[(&str, PassMode, Type)],
    result: Type,
) -> bool {
    function.name == name
        && function.compile_groups.is_empty()
        && function.groups.len() == 2
        && matches!(function.groups[0].as_slice(), [receiver]
            if receiver.name == "self"
                && receiver.mode == receiver_mode
                && receiver.ty == named("Self"))
        && function.groups[1].len() == parameters.len()
        && function.groups[1]
            .iter()
            .zip(parameters)
            .all(|(actual, (name, mode, ty))| {
                actual.name == *name && actual.mode == *mode && actual.ty == *ty
            })
        && function.return_type == Some(result)
        && function.body.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edition_2026_alloc_bundle_parses_and_validates() {
        let bundle = AllocBundle::for_edition(Edition::Edition2026).unwrap();
        assert_eq!(bundle.program.items.len(), 6);
        assert!(bundle
            .program
            .item_origins
            .iter()
            .all(|origin| origin.package == PackageId::ALLOC.0));
    }
}
