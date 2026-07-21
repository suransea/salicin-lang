//! Edition-pinned Salicin `alloc` bootstrap sources.

use std::error::Error;
use std::fmt;

use crate::ast::{
    CompileParamKind, Function, Item, ItemOrigin, PassMode, Program, StructDef, Type, Visibility,
};
use crate::manifest::Edition;
use crate::modules::PackageId;
use crate::parser;

const EDITION_2026_BOXED: &str = include_str!("../../library/alloc/src/boxed.sali");
const EDITION_2026_VEC: &str = include_str!("../../library/alloc/src/vec.sali");

#[derive(Clone, Debug, PartialEq)]
pub struct AllocBundle {
    program: Program,
}

impl AllocBundle {
    pub fn for_edition(edition: Edition) -> Result<Self, AllocBundleError> {
        let modules = match edition {
            Edition::Edition2026 => [("boxed", EDITION_2026_BOXED), ("vec", EDITION_2026_VEC)],
        };
        let mut combined = Program::new(Vec::new());
        for (module, source) in modules {
            let mut program = parser::parse(source).map_err(|error| {
                AllocBundleError::new(
                    edition,
                    vec![format!(
                        "embedded alloc module `{module}` does not parse: {error}"
                    )],
                )
            })?;
            program.item_origins = vec![
                ItemOrigin {
                    package: PackageId::ALLOC.0,
                    module_path: vec!["@alloc".to_owned(), module.to_owned()],
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
        validate_program(edition, &combined)?;
        Ok(Self { program: combined })
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
    if program.items.len() != 38
        || program.item_visibilities.len() != 38
        || program.item_origins.len() != 38
    {
        diagnostics
            .push("embedded alloc must contain the fixed Box and Vec bootstrap schema".to_owned());
    } else {
        let visibilities_are_valid =
            program
                .item_visibilities
                .iter()
                .enumerate()
                .all(|(index, visibility)| {
                    let expected = if matches!(index, 0..=7 | 10 | 14..=34) {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    *visibility == expected
                });
        if !visibilities_are_valid {
            diagnostics.push("embedded alloc bootstrap item visibility is invalid".to_owned());
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
            Item::Function(function) if valid_box_read(function) => {}
            _ => diagnostics.push(
                "alloc box_read must borrow `Box(T)`, require `T: Copy`, and return `T`".to_owned(),
            ),
        }
        match &program.items[4] {
            Item::Function(function) if valid_box_write(function) => {}
            _ => diagnostics.push(
                "alloc box_write must mutably borrow `Box(T)`, copy a `T`, require `T: Copy`, and return unit"
                    .to_owned(),
            ),
        }
        match &program.items[5] {
            Item::Function(function) if valid_box_into_inner(function) => {}
            _ => diagnostics.push(
                "alloc box_into_inner must consume `Box(T)` and return its owned `T`".to_owned(),
            ),
        }
        match &program.items[6] {
            Item::Function(function) if valid_box_replace(function) => {}
            _ => diagnostics.push(
                "alloc box_replace must mutably borrow `Box(T)`, consume a replacement `T`, and return the old `T`"
                    .to_owned(),
            ),
        }
        match &program.items[7] {
            Item::Function(function) if valid_box_borrow(function) => {}
            _ => diagnostics.push("alloc box_as_ref has an invalid signature".to_owned()),
        }
        match &program.items[8] {
            Item::Extend(extension) if valid_box_extension(extension) => {}
            _ => diagnostics.push(
                "alloc Box extension must provide new, pointer/reference access, into_inner, and replace"
                    .to_owned(),
            ),
        }
        match &program.items[9] {
            Item::Extend(extension) if valid_copy_box_extension(extension) => {}
            _ => diagnostics.push(
                "alloc Copy Box extension must provide `read` and `write` under a `T: Copy` constraint"
                    .to_owned(),
            ),
        }
        match &program.items[10] {
            Item::Struct(definition) if valid_vec(definition) => {}
            _ => diagnostics.push(
                "alloc Vec must have private pointer, length, and capacity fields".to_owned(),
            ),
        }
        match &program.items[11] {
            Item::Function(function) if valid_vec_layout_size(function) => {}
            _ => diagnostics.push("alloc vec_layout_size has an invalid signature".to_owned()),
        }
        match &program.items[12] {
            Item::Function(function) if valid_vec_allocate(function) => {}
            _ => diagnostics.push("alloc vec_allocate has an invalid signature".to_owned()),
        }
        match &program.items[13] {
            Item::Function(function) if valid_vec_deallocate(function) => {}
            _ => diagnostics.push("alloc vec_deallocate has an invalid signature".to_owned()),
        }
        match &program.items[14] {
            Item::Function(function) if valid_vec_new(function) => {}
            _ => diagnostics.push("alloc vec_new has an invalid signature".to_owned()),
        }
        match &program.items[15] {
            Item::Function(function) if valid_vec_with_capacity(function) => {}
            _ => diagnostics.push("alloc vec_with_capacity has an invalid signature".to_owned()),
        }
        match &program.items[16] {
            Item::Function(function) if valid_vec_len_or_capacity(function, "vec_len") => {}
            _ => diagnostics.push("alloc vec_len has an invalid signature".to_owned()),
        }
        match &program.items[17] {
            Item::Function(function) if valid_vec_len_or_capacity(function, "vec_capacity") => {}
            _ => diagnostics.push("alloc vec_capacity has an invalid signature".to_owned()),
        }
        match &program.items[18] {
            Item::Function(function) if valid_vec_at(function) => {}
            _ => diagnostics.push("alloc vec_at has an invalid signature".to_owned()),
        }
        match &program.items[19] {
            Item::Function(function) if valid_vec_reserve(function) => {}
            _ => diagnostics.push("alloc vec_reserve has an invalid signature".to_owned()),
        }
        match &program.items[20] {
            Item::Function(function) if valid_vec_push(function) => {}
            _ => diagnostics.push("alloc vec_push has an invalid signature".to_owned()),
        }
        match &program.items[21] {
            Item::Function(function) if valid_vec_replace(function) => {}
            _ => diagnostics.push("alloc vec_replace has an invalid signature".to_owned()),
        }
        match &program.items[22] {
            Item::Function(function) if valid_vec_pop(function) => {}
            _ => diagnostics.push("alloc vec_pop has an invalid signature".to_owned()),
        }
        match &program.items[23] {
            Item::Function(function) if valid_vec_truncate(function) => {}
            _ => diagnostics.push("alloc vec_truncate has an invalid signature".to_owned()),
        }
        match &program.items[24] {
            Item::Function(function) if valid_vec_clear(function) => {}
            _ => diagnostics.push("alloc vec_clear has an invalid signature".to_owned()),
        }
        match &program.items[25] {
            Item::Function(function) if valid_vec_is_empty(function) => {}
            _ => diagnostics.push("alloc vec_is_empty has an invalid signature".to_owned()),
        }
        match &program.items[26] {
            Item::Function(function) if valid_vec_swap_remove(function) => {}
            _ => diagnostics.push("alloc vec_swap_remove has an invalid signature".to_owned()),
        }
        match &program.items[27] {
            Item::Function(function) if valid_vec_swap(function) => {}
            _ => diagnostics.push("alloc vec_swap has an invalid signature".to_owned()),
        }
        match &program.items[28] {
            Item::Function(function) if valid_vec_reverse(function) => {}
            _ => diagnostics.push("alloc vec_reverse has an invalid signature".to_owned()),
        }
        match &program.items[29] {
            Item::Function(function) if valid_vec_insert(function) => {}
            _ => diagnostics.push("alloc vec_insert has an invalid signature".to_owned()),
        }
        match &program.items[30] {
            Item::Function(function) if valid_vec_remove(function) => {}
            _ => diagnostics.push("alloc vec_remove has an invalid signature".to_owned()),
        }
        match &program.items[31] {
            Item::Function(function) if valid_vec_append(function) => {}
            _ => diagnostics.push("alloc vec_append has an invalid signature".to_owned()),
        }
        match &program.items[32] {
            Item::Function(function) if valid_vec_shrink_to_fit(function) => {}
            _ => diagnostics.push("alloc vec_shrink_to_fit has an invalid signature".to_owned()),
        }
        match &program.items[33] {
            Item::Function(function) if valid_vec_read(function) => {}
            _ => diagnostics.push("alloc vec_read has an invalid signature".to_owned()),
        }
        match &program.items[34] {
            Item::Function(function) if valid_vec_write(function) => {}
            _ => diagnostics.push("alloc vec_write has an invalid signature".to_owned()),
        }
        match &program.items[35] {
            Item::Extend(extension) if valid_vec_extension(extension) => {}
            _ => diagnostics.push("alloc Vec extension has an invalid shape".to_owned()),
        }
        match &program.items[36] {
            Item::Extend(extension) if valid_copy_vec_extension(extension) => {}
            _ => diagnostics.push("alloc Copy Vec extension has an invalid shape".to_owned()),
        }
        match &program.items[37] {
            Item::Extend(extension) if valid_vec_drop_extension(extension) => {}
            _ => diagnostics.push("alloc Vec Drop extension has an invalid shape".to_owned()),
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

fn is_copy_bound(predicate: &crate::ast::WherePredicate) -> bool {
    predicate.subject == named("T")
        && predicate.trait_ref == named("Copy")
        && predicate.associated_types.is_empty()
}

fn valid_box_read(function: &Function) -> bool {
    function.name == "box_read"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "boxed"
                        && parameter.mode == PassMode::Borrow
                        && parameter.ty == applied("Box", named("T")))
        )
        && matches!(function.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && function.return_type == Some(named("T"))
        && function.body.is_some()
}

fn valid_box_write(function: &Function) -> bool {
    function.name == "box_write"
        && generic_t(function)
        && matches!(
            function.groups.as_slice(),
            [receiver, value]
                if matches!(receiver.as_slice(), [parameter]
                    if parameter.name == "boxed"
                        && parameter.mode == PassMode::MutBorrow
                        && parameter.ty == applied("Box", named("T")))
                    && matches!(value.as_slice(), [parameter]
                        if parameter.name == "value"
                            && parameter.mode == PassMode::Copy
                            && parameter.ty == named("T"))
        )
        && matches!(function.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && function.return_type == Some(Type::Unit)
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

fn valid_box_borrow(function: &Function) -> bool {
    function.name == "box_as_ref"
        && matches!(function.compile_groups.as_slice(), [group]
            if matches!(group.as_slice(), [access, region, element]
                if access.name == "A"
                    && access.kind == CompileParamKind::Access
                    && region.name == "a"
                    && region.kind == CompileParamKind::Region
                    && element.name == "T"
                    && element.kind == CompileParamKind::Type))
        && matches!(function.groups.as_slice(), [receiver]
            if matches!(receiver.as_slice(), [parameter]
                if parameter.name == "boxed"
                    && parameter.mode == PassMode::Borrow
                    && parameter.access.as_deref() == Some("A")
                    && parameter.region.as_deref() == Some("a")
                    && parameter.ty == applied("Box", named("T"))))
        && function.return_type
            == Some(Type::Borrow {
                mutable: false,
                access: Some("A".to_owned()),
                region: Some("a".to_owned()),
                pointee: Box::new(named("T")),
            })
        && function.where_predicates.is_empty()
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
        && extension.where_predicates.is_empty()
        && extension.members.len() == 5
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
            if valid_box_access_method(function))
        && matches!(&extension.members[3], crate::ast::ExtendMember::Function(function)
            if valid_box_method(function, "into_inner", PassMode::Move, &[], named("T")))
        && matches!(&extension.members[4], crate::ast::ExtendMember::Function(function)
        if valid_box_method(
            function,
            "replace",
            PassMode::MutBorrow,
            &[("value", PassMode::Move, named("T"))],
            named("T"),
        ))
}

fn valid_box_access_method(function: &Function) -> bool {
    function.name == "as_ref"
        && matches!(function.compile_groups.as_slice(), [group]
            if matches!(group.as_slice(), [access]
                if access.name == "A" && access.kind == CompileParamKind::Access))
        && matches!(function.groups.as_slice(), [receiver, arguments]
            if arguments.is_empty()
                && matches!(receiver.as_slice(), [parameter]
                    if parameter.name == "self"
                        && parameter.mode == PassMode::Borrow
                        && parameter.access.as_deref() == Some("A")
                        && parameter.ty == named("Self")))
        && function.return_type
            == Some(Type::Borrow {
                mutable: false,
                access: Some("A".to_owned()),
                region: None,
                pointee: Box::new(named("T")),
            })
        && function.body.is_some()
}

fn valid_copy_box_extension(extension: &crate::ast::ExtendDef) -> bool {
    matches!(
        extension.compile_groups.as_slice(),
        [group]
            if matches!(group.as_slice(), [parameter]
                if parameter.name == "T" && parameter.kind == CompileParamKind::Type)
    ) && extension.target == applied("Box", named("T"))
        && extension.trait_ref.is_none()
        && matches!(extension.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && matches!(extension.members.as_slice(), [
            crate::ast::ExtendMember::Function(read),
            crate::ast::ExtendMember::Function(write),
        ] if valid_box_method(read, "read", PassMode::Borrow, &[], named("T"))
            && valid_box_method(
                write,
                "write",
                PassMode::MutBorrow,
                &[("value", PassMode::Copy, named("T"))],
                Type::Unit,
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

fn valid_vec(definition: &StructDef) -> bool {
    definition.name == "Vec"
        && matches!(
            definition.compile_groups.as_slice(),
            [group]
                if matches!(group.as_slice(), [parameter]
                    if parameter.name == "T" && parameter.kind == CompileParamKind::Type)
        )
        && matches!(
            definition.fields.as_slice(),
            [pointer, length, capacity]
                if pointer.visibility == Visibility::Private
                    && pointer.name == "pointer"
                    && pointer.ty == applied("MutPtr", named("T"))
                    && length.visibility == Visibility::Private
                    && length.name == "length"
                    && length.ty == Type::U64
                    && capacity.visibility == Visibility::Private
                    && capacity.name == "storage_capacity"
                    && capacity.ty == Type::U64
        )
}

fn has_parameter(group: &[crate::ast::Param], name: &str, mode: PassMode, ty: Type) -> bool {
    matches!(group, [parameter]
        if parameter.name == name && parameter.mode == mode && parameter.ty == ty)
}

fn valid_vec_layout_size(function: &Function) -> bool {
    function.name == "vec_layout_size"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group]
            if has_parameter(group, "capacity", PassMode::Inferred, Type::U64))
        && function.return_type == Some(Type::U64)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_allocate(function: &Function) -> bool {
    function.name == "vec_allocate"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group]
            if has_parameter(group, "capacity", PassMode::Inferred, Type::U64))
        && function.return_type == Some(applied("MutPtr", named("T")))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_deallocate(function: &Function) -> bool {
    function.name == "vec_deallocate"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group]
            if matches!(group.as_slice(), [pointer, capacity]
                if pointer.name == "pointer"
                    && pointer.mode == PassMode::Inferred
                    && pointer.ty == applied("MutPtr", named("T"))
                    && capacity.name == "capacity"
                    && capacity.mode == PassMode::Inferred
                    && capacity.ty == Type::U64))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_new(function: &Function) -> bool {
    function.name == "vec_new"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group] if group.is_empty())
        && function.return_type == Some(applied("Vec", named("T")))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_with_capacity(function: &Function) -> bool {
    function.name == "vec_with_capacity"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group]
            if has_parameter(group, "capacity", PassMode::Inferred, Type::U64))
        && function.return_type == Some(applied("Vec", named("T")))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_len_or_capacity(function: &Function, name: &str) -> bool {
    function.name == name
        && generic_t(function)
        && matches!(function.groups.as_slice(), [group]
            if has_parameter(group, "values", PassMode::Borrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::U64)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_at(function: &Function) -> bool {
    function.name == "vec_at"
        && matches!(function.compile_groups.as_slice(), [group]
            if matches!(group.as_slice(), [access, region, element]
                if access.name == "A"
                    && access.kind == CompileParamKind::Access
                    && region.name == "a"
                    && region.kind == CompileParamKind::Region
                    && element.name == "T"
                    && element.kind == CompileParamKind::Type))
        && matches!(function.groups.as_slice(), [receiver, index]
            if matches!(receiver.as_slice(), [parameter]
                if parameter.name == "values"
                    && parameter.mode == PassMode::Borrow
                    && parameter.access.as_deref() == Some("A")
                    && parameter.region.as_deref() == Some("a")
                    && parameter.ty == applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64))
        && function.return_type
            == Some(Type::Borrow {
                mutable: false,
                access: Some("A".to_owned()),
                region: Some("a".to_owned()),
                pointee: Box::new(named("T")),
            })
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_reserve(function: &Function) -> bool {
    function.name == "vec_reserve"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, additional]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(additional, "additional", PassMode::Inferred, Type::U64))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_push(function: &Function) -> bool {
    function.name == "vec_push"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, value]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(value, "value", PassMode::Inferred, named("T")))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_replace(function: &Function) -> bool {
    function.name == "vec_replace"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index, value]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64)
                && has_parameter(value, "value", PassMode::Inferred, named("T")))
        && function.return_type == Some(named("T"))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_pop(function: &Function) -> bool {
    function.name == "vec_pop"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T"))))
        && function.return_type == Some(applied("Option", named("T")))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_truncate(function: &Function) -> bool {
    function.name == "vec_truncate"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, new_length]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(new_length, "new_length", PassMode::Inferred, Type::U64))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_clear(function: &Function) -> bool {
    function.name == "vec_clear"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_is_empty(function: &Function) -> bool {
    function.name == "vec_is_empty"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver]
            if has_parameter(receiver, "values", PassMode::Borrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::Bool)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_swap_remove(function: &Function) -> bool {
    function.name == "vec_swap_remove"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64))
        && function.return_type == Some(named("T"))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_swap(function: &Function) -> bool {
    function.name == "vec_swap"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, indices]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && matches!(indices.as_slice(), [left, right]
                    if left.name == "left"
                        && left.mode == PassMode::Inferred
                        && left.ty == Type::U64
                        && right.name == "right"
                        && right.mode == PassMode::Inferred
                        && right.ty == Type::U64))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_reverse(function: &Function) -> bool {
    function.name == "vec_reverse"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_insert(function: &Function) -> bool {
    function.name == "vec_insert"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index, value]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64)
                && has_parameter(value, "value", PassMode::Inferred, named("T")))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_remove(function: &Function) -> bool {
    function.name == "vec_remove"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64))
        && function.return_type == Some(named("T"))
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_append(function: &Function) -> bool {
    function.name == "vec_append"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, other]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(other, "other", PassMode::MutBorrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_shrink_to_fit(function: &Function) -> bool {
    function.name == "vec_shrink_to_fit"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T"))))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_read(function: &Function) -> bool {
    function.name == "vec_read"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index]
            if has_parameter(receiver, "values", PassMode::Borrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64))
        && function.return_type == Some(named("T"))
        && matches!(function.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && function.body.is_some()
}

fn valid_vec_write(function: &Function) -> bool {
    function.name == "vec_write"
        && generic_t(function)
        && matches!(function.groups.as_slice(), [receiver, index, value]
            if has_parameter(receiver, "values", PassMode::MutBorrow, applied("Vec", named("T")))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64)
                && has_parameter(value, "value", PassMode::Copy, named("T")))
        && function.return_type == Some(Type::Unit)
        && matches!(function.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && function.body.is_some()
}

fn valid_vec_receiver_method(
    function: &Function,
    name: &str,
    receiver_mode: PassMode,
    remaining_groups: &[(&str, PassMode, Type)],
    result: Type,
) -> bool {
    let arguments_match = if remaining_groups.is_empty() {
        function.groups.len() == 2 && function.groups[1].is_empty()
    } else {
        function.groups.len() == remaining_groups.len() + 1
            && function.groups[1..]
                .iter()
                .zip(remaining_groups)
                .all(|(group, (name, mode, ty))| has_parameter(group, name, *mode, ty.clone()))
    };
    function.name == name
        && function.compile_groups.is_empty()
        && arguments_match
        && has_parameter(&function.groups[0], "self", receiver_mode, named("Self"))
        && function.return_type == Some(result)
        && function.body.is_some()
}

fn valid_vec_access_method(function: &Function) -> bool {
    function.name == "at"
        && matches!(function.compile_groups.as_slice(), [group]
            if matches!(group.as_slice(), [access]
                if access.name == "A" && access.kind == CompileParamKind::Access))
        && matches!(function.groups.as_slice(), [receiver, index]
            if matches!(receiver.as_slice(), [parameter]
                if parameter.name == "self"
                    && parameter.mode == PassMode::Borrow
                    && parameter.access.as_deref() == Some("A")
                    && parameter.ty == named("Self"))
                && has_parameter(index, "index", PassMode::Inferred, Type::U64))
        && function.return_type
            == Some(Type::Borrow {
                mutable: false,
                access: Some("A".to_owned()),
                region: None,
                pointee: Box::new(named("T")),
            })
        && function.body.is_some()
}

fn valid_vec_swap_method(function: &Function) -> bool {
    function.name == "swap"
        && function.compile_groups.is_empty()
        && matches!(function.groups.as_slice(), [receiver, indices]
            if has_parameter(receiver, "self", PassMode::MutBorrow, named("Self"))
                && matches!(indices.as_slice(), [left, right]
                    if left.name == "left"
                        && left.mode == PassMode::Inferred
                        && left.ty == Type::U64
                        && right.name == "right"
                        && right.mode == PassMode::Inferred
                        && right.ty == Type::U64))
        && function.return_type == Some(Type::Unit)
        && function.where_predicates.is_empty()
        && function.body.is_some()
}

fn valid_vec_extension(extension: &crate::ast::ExtendDef) -> bool {
    matches!(extension.compile_groups.as_slice(), [group]
        if matches!(group.as_slice(), [parameter]
            if parameter.name == "T" && parameter.kind == CompileParamKind::Type))
        && extension.target == applied("Vec", named("T"))
        && extension.trait_ref.is_none()
        && extension.where_predicates.is_empty()
        && matches!(extension.members.as_slice(), [
            crate::ast::ExtendMember::Function(new),
            crate::ast::ExtendMember::Function(with_capacity),
            crate::ast::ExtendMember::Function(len),
            crate::ast::ExtendMember::Function(capacity),
            crate::ast::ExtendMember::Function(at),
            crate::ast::ExtendMember::Function(reserve),
            crate::ast::ExtendMember::Function(push),
            crate::ast::ExtendMember::Function(replace),
            crate::ast::ExtendMember::Function(pop),
            crate::ast::ExtendMember::Function(truncate),
            crate::ast::ExtendMember::Function(clear),
            crate::ast::ExtendMember::Function(is_empty),
            crate::ast::ExtendMember::Function(swap_remove),
            crate::ast::ExtendMember::Function(swap),
            crate::ast::ExtendMember::Function(reverse),
            crate::ast::ExtendMember::Function(insert),
            crate::ast::ExtendMember::Function(remove),
            crate::ast::ExtendMember::Function(append),
            crate::ast::ExtendMember::Function(shrink_to_fit),
        ] if new.name == "new"
            && new.compile_groups.is_empty()
            && matches!(new.groups.as_slice(), [group] if group.is_empty())
            && new.return_type == Some(applied("Vec", named("T")))
            && new.body.is_some()
            && with_capacity.name == "with_capacity"
            && with_capacity.compile_groups.is_empty()
            && matches!(with_capacity.groups.as_slice(), [group]
                if has_parameter(group, "capacity", PassMode::Inferred, Type::U64))
            && with_capacity.return_type == Some(applied("Vec", named("T")))
            && with_capacity.body.is_some()
            && valid_vec_receiver_method(len, "len", PassMode::Borrow, &[], Type::U64)
            && valid_vec_receiver_method(capacity, "capacity", PassMode::Borrow, &[], Type::U64)
            && valid_vec_access_method(at)
            && valid_vec_receiver_method(reserve, "reserve", PassMode::MutBorrow, &[("additional", PassMode::Inferred, Type::U64)], Type::Unit)
            && valid_vec_receiver_method(push, "push", PassMode::MutBorrow, &[("value", PassMode::Inferred, named("T"))], Type::Unit)
            && valid_vec_receiver_method(replace, "replace", PassMode::MutBorrow, &[("index", PassMode::Inferred, Type::U64), ("value", PassMode::Inferred, named("T"))], named("T"))
            && valid_vec_receiver_method(pop, "pop", PassMode::MutBorrow, &[], applied("Option", named("T")))
            && valid_vec_receiver_method(truncate, "truncate", PassMode::MutBorrow, &[("new_length", PassMode::Inferred, Type::U64)], Type::Unit)
            && valid_vec_receiver_method(clear, "clear", PassMode::MutBorrow, &[], Type::Unit)
            && valid_vec_receiver_method(is_empty, "is_empty", PassMode::Borrow, &[], Type::Bool)
            && valid_vec_receiver_method(swap_remove, "swap_remove", PassMode::MutBorrow, &[("index", PassMode::Inferred, Type::U64)], named("T"))
            && valid_vec_swap_method(swap)
            && valid_vec_receiver_method(reverse, "reverse", PassMode::MutBorrow, &[], Type::Unit)
            && valid_vec_receiver_method(insert, "insert", PassMode::MutBorrow, &[("index", PassMode::Inferred, Type::U64), ("value", PassMode::Inferred, named("T"))], Type::Unit)
            && valid_vec_receiver_method(remove, "remove", PassMode::MutBorrow, &[("index", PassMode::Inferred, Type::U64)], named("T"))
            && valid_vec_receiver_method(append, "append", PassMode::MutBorrow, &[("other", PassMode::MutBorrow, applied("Vec", named("T")))], Type::Unit)
            && valid_vec_receiver_method(shrink_to_fit, "shrink_to_fit", PassMode::MutBorrow, &[], Type::Unit))
}

fn valid_copy_vec_extension(extension: &crate::ast::ExtendDef) -> bool {
    matches!(extension.compile_groups.as_slice(), [group]
        if matches!(group.as_slice(), [parameter]
            if parameter.name == "T" && parameter.kind == CompileParamKind::Type))
        && extension.target == applied("Vec", named("T"))
        && extension.trait_ref.is_none()
        && matches!(extension.where_predicates.as_slice(), [predicate] if is_copy_bound(predicate))
        && matches!(extension.members.as_slice(), [
            crate::ast::ExtendMember::Function(read),
            crate::ast::ExtendMember::Function(write),
        ] if valid_vec_receiver_method(read, "read", PassMode::Borrow, &[("index", PassMode::Inferred, Type::U64)], named("T"))
            && valid_vec_receiver_method(write, "write", PassMode::MutBorrow, &[("index", PassMode::Inferred, Type::U64), ("value", PassMode::Copy, named("T"))], Type::Unit))
}

fn valid_vec_drop_extension(extension: &crate::ast::ExtendDef) -> bool {
    matches!(extension.compile_groups.as_slice(), [group]
        if matches!(group.as_slice(), [parameter]
            if parameter.name == "T" && parameter.kind == CompileParamKind::Type))
        && extension.target == applied("Vec", named("T"))
        && extension.trait_ref == Some(named("Drop"))
        && extension.where_predicates.is_empty()
        && matches!(extension.members.as_slice(), [crate::ast::ExtendMember::Function(drop)]
            if valid_vec_receiver_method(drop, "drop", PassMode::MutBorrow, &[], Type::Unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_alloc(source: &str) -> Program {
        let mut program = parser::parse(source).expect("test alloc source must parse");
        program.item_origins = vec![
            ItemOrigin {
                package: PackageId::ALLOC.0,
                module_path: vec!["@alloc".to_owned()],
            };
            program.items.len()
        ];
        program
    }

    fn alloc_source() -> String {
        [EDITION_2026_BOXED, EDITION_2026_VEC].join("\n")
    }

    #[test]
    fn edition_2026_alloc_bundle_parses_and_validates() {
        let bundle = AllocBundle::for_edition(Edition::Edition2026).unwrap();
        assert_eq!(bundle.program.items.len(), 38);
        assert!(bundle
            .program
            .item_origins
            .iter()
            .all(|origin| origin.package == PackageId::ALLOC.0));
        assert!(bundle.program.item_origins[..10]
            .iter()
            .all(|origin| origin.module_path == ["@alloc", "boxed"]));
        assert!(bundle.program.item_origins[10..]
            .iter()
            .all(|origin| origin.module_path == ["@alloc", "vec"]));
    }

    #[test]
    fn rejects_box_read_without_its_copy_proof() {
        let source = alloc_source().replacen("where T: Copy = unsafe do {", "= unsafe do {", 1);
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("box_read without Copy must fail bootstrap validation");
        assert!(error.to_string().contains("box_read"));
    }

    #[test]
    fn rejects_box_write_without_its_copy_proof() {
        let source = alloc_source().replacen(
            "pub let box_write(T: type)(borrow(mut) boxed: Box(T))(copy value: T): ()\nwhere T: Copy = unsafe do {",
            "pub let box_write(T: type)(borrow(mut) boxed: Box(T))(copy value: T): ()\n= unsafe do {",
            1,
        );
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("box_write without Copy must fail bootstrap validation");
        assert!(error.to_string().contains("box_write"));
    }

    #[test]
    fn rejects_a_malformed_copy_box_extension() {
        let source = alloc_source().replacen(
            "let read(borrow self)(): T = box_read(self)",
            "let peek(borrow self)(): T = box_read(self)",
            1,
        );
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("malformed Copy Box extension must fail bootstrap validation");
        assert!(error.to_string().contains("Copy Box extension"));
    }

    #[test]
    fn rejects_a_malformed_vec_representation() {
        let source =
            alloc_source().replacen("  storage_capacity: u64,", "  exposed_capacity: u64,", 1);
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("malformed Vec representation must fail bootstrap validation");
        assert!(error.to_string().contains("alloc Vec"));
    }

    #[test]
    fn rejects_a_malformed_vec_drop_extension() {
        let source = alloc_source().replacen(
            "let drop(borrow(mut) self)(): () = {",
            "let release(borrow(mut) self)(): () = {",
            1,
        );
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("malformed Vec Drop must fail bootstrap validation");
        assert!(error.to_string().contains("Vec Drop extension"));
    }

    #[test]
    fn rejects_a_malformed_vec_owning_extension() {
        let source = alloc_source().replacen(
            "let pop(borrow(mut) self)(): Option(T) = vec_pop(self)",
            "let take(borrow(mut) self)(): Option(T) = vec_pop(self)",
            1,
        );
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("malformed Vec owning extension must fail bootstrap validation");
        assert!(error.to_string().contains("alloc Vec extension"));
    }

    #[test]
    fn rejects_a_malformed_copy_vec_extension() {
        let source = alloc_source().replacen(
            "let read(borrow self)(index: u64): T = vec_read(self)(index)",
            "let peek(borrow self)(index: u64): T = vec_read(self)(index)",
            1,
        );
        let error = validate_program(Edition::Edition2026, &parse_alloc(&source))
            .expect_err("malformed Copy Vec extension must fail bootstrap validation");
        assert!(error.to_string().contains("Copy Vec extension"));
    }
}
