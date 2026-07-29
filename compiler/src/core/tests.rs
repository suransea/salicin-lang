use super::*;

fn core_source_with_copy(copy_declaration: &str) -> String {
    [
        r#"
pub let option(comptime t: type) = enum { some(t), none }
pub let result(comptime e: type)(comptime t: type) = enum { ok(t), err(e) }
pub let never = enum {}
pub let movable = trait {}
"#,
        copy_declaration,
        r#"
pub let droppable = trait {
  let drop(self: borrow(mut)(self))(): ()
}
pub let add(comptime rhs: type) = trait {
  let output: type
  let add(self)(rhs: rhs): output
}
pub let sub(comptime rhs: type) = trait {
  let output: type
  let sub(self)(rhs: rhs): output
}
pub let mul(comptime rhs: type) = trait {
  let output: type
  let mul(self)(rhs: rhs): output
}
pub let div(comptime rhs: type) = trait {
  let output: type
  let div(self)(rhs: rhs): output
}
pub let rem(comptime rhs: type) = trait {
  let output: type
  let rem(self)(rhs: rhs): output
}
pub let eq(comptime rhs: type) = trait {
  let eq(self: borrow(self))(rhs: borrow(rhs)): bool
}
pub let partial_ordering = enum { less, equal, greater, unordered }
pub let partial_ord(comptime rhs: type) = trait {
  let partial_cmp(self: borrow(self))(rhs: borrow(rhs)): partial_ordering
}
pub let neg = trait {
  let output: type
  let neg(self)(): output
}
pub let not = trait {
  let output: type
  let not(self)(): output
}
pub let bit_and(comptime rhs: type) = trait {
  let output: type
  let bit_and(self)(rhs: rhs): output
}
pub let bit_or(comptime rhs: type) = trait {
  let output: type
  let bit_or(self)(rhs: rhs): output
}
pub let bit_xor(comptime rhs: type) = trait {
  let output: type
  let bit_xor(self)(rhs: rhs): output
}
pub let shl(comptime rhs: type) = trait {
  let output: type
  let shl(self)(rhs: rhs): output
}
pub let shr(comptime rhs: type) = trait {
  let output: type
  let shr(self)(rhs: rhs): output
}
pub let index(comptime key: type) = trait {
  let output: type
  let index(comptime a: access)(self: borrow(a)(self))(key: key): borrow(a)(output)
}
pub let str: type = builtin()
"#,
    ]
    .concat()
}

fn edition_2026_test_modules<'a>(overrides: &[(&str, &'a str)]) -> Vec<(&'static str, &'a str)> {
    let mut modules = vec![
        ("lib", EDITION_2026_LIB),
        ("prelude", EDITION_2026_PRELUDE),
        ("primitives", EDITION_2026_PRIMITIVES),
        ("never", EDITION_2026_NEVER),
        ("marker", EDITION_2026_MARKER),
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
        ("numeric", EDITION_2026_NUMERIC),
        ("string", EDITION_2026_STRING),
        ("fmt", EDITION_2026_FMT),
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
    assert_eq!(bundle.program().items.len(), LangItemKind::ALL.len() + 419);
    for kind in LangItemKind::ALL {
        let lang_item = bundle.lang_items().get(kind);
        assert_eq!(lang_item.kind(), kind);
        let canonical = match kind {
            LangItemKind::Builtin | LangItemKind::Test => {
                format!("core::{}", kind.source_name())
            }
            LangItemKind::Foreign => "core::foreign::foreign".to_owned(),
            LangItemKind::Option => "core::option::option".to_owned(),
            LangItemKind::Result => "core::result::result".to_owned(),
            LangItemKind::Never => "core::never::never".to_owned(),
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
            LangItemKind::Move | LangItemKind::Copy | LangItemKind::Drop => {
                format!("core::marker::{}", kind.source_name())
            }
            LangItemKind::Poll
            | LangItemKind::Future
            | LangItemKind::Executor
            | LangItemKind::AsyncFunction
            | LangItemKind::AwaitFunction => {
                format!("core::async::{}", kind.source_name())
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
            LangItemKind::Index => "core::ops::index::index".to_owned(),
            LangItemKind::Chain
            | LangItemKind::Coalesce
            | LangItemKind::Unwrap
            | LangItemKind::Raise => {
                format!("core::flow::{}", kind.source_name())
            }
            LangItemKind::UnsafeEffect => "core::unsafe::unsafety".to_owned(),
            LangItemKind::ThrowsEffect => "core::error::throwing".to_owned(),
            LangItemKind::AsyncEffect => "core::async::suspension".to_owned(),
            LangItemKind::TypeSort
            | LangItemKind::RegionSort
            | LangItemKind::EffectSort
            | LangItemKind::EffectsSort
            | LangItemKind::ParametersSort => {
                format!("core::sorts::{}", kind.source_name())
            }
            LangItemKind::AbiSort => "core::foreign::abi".to_owned(),
            LangItemKind::CopyParameters
            | LangItemKind::MoveParameters
            | LangItemKind::ComptimeParameters => {
                format!("core::passing::{}", kind.source_name())
            }
            LangItemKind::AccessSort => {
                format!("core::borrow::{}", kind.source_name())
            }
            LangItemKind::BorrowTypeForm | LangItemKind::BorrowValueForm => {
                format!("core::borrow::{}", kind.source_name())
            }
            LangItemKind::ArrayTypeForm
            | LangItemKind::SliceTypeForm
            | LangItemKind::PtrTypeForm
            | LangItemKind::PtrValueForm
            | LangItemKind::SizeOf
            | LangItemKind::AlignOf => {
                format!("core::memory::{}", kind.source_name())
            }
            LangItemKind::StrTypeForm => "core::string::str".to_owned(),
            LangItemKind::Continuation | LangItemKind::EffectCallable | LangItemKind::Handle => {
                format!("core::effect::{}", kind.source_name())
            }
            LangItemKind::Attempt
            | LangItemKind::BreakEffect
            | LangItemKind::ContinueEffect
            | LangItemKind::ReturnEffect
            | LangItemKind::Break
            | LangItemKind::BreakUnit
            | LangItemKind::Continue
            | LangItemKind::Return
            | LangItemKind::ReturnUnit
            | LangItemKind::Do
            | LangItemKind::DoWhile
            | LangItemKind::Loop
            | LangItemKind::While
            | LangItemKind::If
            | LangItemKind::Match
            | LangItemKind::For
            | LangItemKind::Defer => format!("core::control::{}", kind.source_name()),
            LangItemKind::Try | LangItemKind::Throw => {
                format!("core::error::{}", kind.source_name())
            }
            LangItemKind::Unsafe => "core::unsafe::unsafe".to_owned(),
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
            LangItemKind::Builtin | LangItemKind::Test => vec![],
            LangItemKind::Foreign | LangItemKind::AbiSort => vec!["foreign"],
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
            LangItemKind::Move | LangItemKind::Copy | LangItemKind::Drop => vec!["marker"],
            LangItemKind::Poll
            | LangItemKind::Future
            | LangItemKind::Executor
            | LangItemKind::AsyncFunction
            | LangItemKind::AwaitFunction => vec!["async"],
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
            LangItemKind::Index => vec!["ops", "index"],
            LangItemKind::Chain
            | LangItemKind::Coalesce
            | LangItemKind::Unwrap
            | LangItemKind::Raise => vec!["flow"],
            LangItemKind::UnsafeEffect => vec!["unsafe"],
            LangItemKind::ThrowsEffect => vec!["error"],
            LangItemKind::AsyncEffect => vec!["async"],
            LangItemKind::TypeSort
            | LangItemKind::RegionSort
            | LangItemKind::EffectSort
            | LangItemKind::EffectsSort
            | LangItemKind::ParametersSort => vec!["sorts"],
            LangItemKind::CopyParameters
            | LangItemKind::MoveParameters
            | LangItemKind::ComptimeParameters => vec!["passing"],
            LangItemKind::AccessSort => vec!["borrow"],
            LangItemKind::BorrowTypeForm | LangItemKind::BorrowValueForm => vec!["borrow"],
            LangItemKind::ArrayTypeForm
            | LangItemKind::SliceTypeForm
            | LangItemKind::PtrTypeForm
            | LangItemKind::PtrValueForm
            | LangItemKind::SizeOf
            | LangItemKind::AlignOf => vec!["memory"],
            LangItemKind::StrTypeForm => vec!["string"],
            LangItemKind::Continuation | LangItemKind::EffectCallable | LangItemKind::Handle => {
                vec!["effect"]
            }
            LangItemKind::Attempt
            | LangItemKind::BreakEffect
            | LangItemKind::ContinueEffect
            | LangItemKind::ReturnEffect
            | LangItemKind::Break
            | LangItemKind::BreakUnit
            | LangItemKind::Continue
            | LangItemKind::Return
            | LangItemKind::ReturnUnit
            | LangItemKind::Do
            | LangItemKind::DoWhile
            | LangItemKind::Loop
            | LangItemKind::While
            | LangItemKind::If
            | LangItemKind::Match
            | LangItemKind::For
            | LangItemKind::Defer => vec!["control"],
            LangItemKind::Try | LangItemKind::Throw => vec!["error"],
            LangItemKind::Unsafe => vec!["unsafe"],
            LangItemKind::Iterator | LangItemKind::IntoIterator => vec!["iter"],
        };
        let mut expected_origin_path = vec!["@core".to_owned()];
        expected_origin_path.extend(module_path.into_iter().map(str::to_owned));
        let origin = &bundle.program().item_origins[lang_item.item_index()];
        assert_eq!(origin.package, PackageId::CORE.0);
        assert_eq!(origin.module_path, expected_origin_path);
        let location = origin.source.as_deref().expect("core item source location");
        assert!(location.line > 0);
        assert!(location.column > 0);
    }

    let failure = &bundle.program().items[bundle.lang_items().failure_effect().item_index()];
    let never_name = bundle.lang_items().never().canonical_name().to_owned();
    assert!(matches!(
        failure,
        Item::Effect(definition)
            if matches!(
                definition.operations.as_slice(),
                [operation]
                    if operation.name == "raise"
                        && operation.return_type == Some(Type::Named(never_name.clone(), Vec::new()))
            )
    ));
    let suspension = bundle
        .program()
        .items
        .iter()
        .find(|item| item_name(item) == Some("core::async::suspension"))
        .expect("core.async.suspension must be mounted");
    assert!(matches!(
        suspension,
        Item::Effect(definition)
            if matches!(
                definition.operations.as_slice(),
                [operation] if operation.name == "suspend" && operation.return_type == Some(Type::Unit)
            )
    ));
}

#[test]
fn builtin_markers_are_explicit_and_bounded_core_contracts() {
    let missing_bootstrap = EDITION_2026_LIB.replace("let builtin() = builtin()\n", "");
    let modules = edition_2026_test_modules(&[("lib", &missing_bootstrap)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic == "missing lang item `builtin`"));

    for (module, name, malformed) in [
        (
            "foreign",
            "foreign",
            EDITION_2026_FOREIGN.replace(
                "pub let foreign(comptime abi: abi): never = builtin()",
                "pub let foreign(): never = builtin()",
            ),
        ),
        (
            "foreign",
            "foreign",
            EDITION_2026_FOREIGN.replace(
                "pub let foreign(comptime abi: abi): never = builtin()",
                "pub let foreign(comptime abi: abi): () = builtin()",
            ),
        ),
        (
            "foreign",
            "foreign",
            EDITION_2026_FOREIGN.replace(
                "pub let foreign(comptime abi: abi, comptime symbol: string): never = builtin()",
                "pub let foreign(comptime abi: abi, comptime symbol: usize): never = builtin()",
            ),
        ),
        (
            "lib",
            "test",
            EDITION_2026_LIB.replace(
                "pub let test(move body: (): bool): () = builtin()",
                "pub let test(): () = builtin()",
            ),
        ),
        (
            "lib",
            "test",
            EDITION_2026_LIB.replace(
                "pub let test(move body: (): bool): () = builtin()",
                "pub let test(move body: (): i32): () = builtin()",
            ),
        ),
    ] {
        let modules = edition_2026_test_modules(&[(module, &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.contains(&format!("syntax lang item `{name}`"))),
            "{:?}",
            error.diagnostics()
        );
    }

    let missing_primitive_marker =
        EDITION_2026_PRIMITIVES.replace("pub let i32: type = builtin()", "pub let i32: type");
    let modules = edition_2026_test_modules(&[("primitives", &missing_primitive_marker)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.contains("compiler-owned lang item `i32`") && diagnostic.contains("= builtin()")
    }));

    let unknown = format!("{EDITION_2026_PRIMITIVES}\npub let mystery(): i32 = builtin()\n");
    let modules = edition_2026_test_modules(&[("primitives", &unknown)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.contains("unknown compiler-owned core function `mystery`")
    }));

    let malformed_defer = EDITION_2026_CONTROL.replace(
        ": with(e)(move action: with(e)((): ())): () = builtin()",
        ": with(e)(move action: with(e)((): bool)): () = builtin()",
    );
    assert_ne!(malformed_defer, EDITION_2026_CONTROL);
    let modules = edition_2026_test_modules(&[("control", &malformed_defer)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.contains("compiler-owned support function `defer`")
            && diagnostic.contains("= builtin()")
    }));

    let abstract_builtin = EDITION_2026_MARKER.replace(
        "let drop(self: borrow(mut)(self))\n    (): ()",
        "let drop(self: borrow(mut)(self))\n    (): () = builtin()",
    );
    assert_ne!(abstract_builtin, EDITION_2026_MARKER);
    let modules = edition_2026_test_modules(&[("marker", &abstract_builtin)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(
        error.diagnostics().iter().any(|diagnostic| {
            diagnostic.contains("trait requirements are abstract")
                && diagnostic.contains("cannot use `builtin()`")
        }),
        "{:?}",
        error.diagnostics()
    );
}

#[test]
fn constraint_query_contracts_are_explicit_and_bounded() {
    for malformed in [
        EDITION_2026_SORTS.replace("pub let constraint: sort(2)", "pub let constraint: sort(1)"),
        EDITION_2026_SORTS.replace("comptime right: constraint", "comptime right: type"),
        EDITION_2026_SORTS.replace("): bool = builtin()", "): usize = builtin()"),
    ] {
        let modules = edition_2026_test_modules(&[("sorts", &malformed)]);
        let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.contains("constraint")),
            "{:?}",
            error.diagnostics()
        );
    }
}

#[test]
fn derived_primitive_operations_are_source_defined() {
    let bundle = CoreBundle::for_edition(Edition::Edition2026).unwrap();
    let expected = BTreeMap::from([
        ("not", 1),
        ("eq", 4),
        ("neg", 6),
        ("add_assign", 12),
        ("sub_assign", 12),
        ("mul_assign", 12),
        ("div_assign", 12),
        ("rem_assign", 12),
        ("bit_and_assign", 12),
        ("bit_or_assign", 12),
        ("bit_xor_assign", 12),
        ("shl_assign", 12),
        ("shr_assign", 12),
    ]);
    let mut actual = BTreeMap::<&str, usize>::new();

    for item in &bundle.program().items {
        let Item::Extend(extension) = item else {
            continue;
        };
        for member in &extension.members {
            let crate::ast::ExtendMember::Function(function) = member else {
                continue;
            };
            let Some((name, _)) = expected.get_key_value(function.name.as_str()) else {
                continue;
            };
            if function.body.is_some() && !function.builtin {
                *actual.entry(name).or_default() += 1;
            }
        }
    }

    assert_eq!(actual, expected);
}

#[test]
fn await_is_source_defined_while_async_remains_intrinsic() {
    let bundle = CoreBundle::for_edition(Edition::Edition2026).unwrap();
    let async_function = &bundle.program().items[bundle.lang_items().async_function().item_index()];
    let await_function = &bundle.program().items[bundle.lang_items().await_function().item_index()];

    assert!(matches!(
        async_function,
        Item::Function(function) if function.builtin && function.body.is_none()
    ));
    assert!(matches!(
        await_function,
        Item::Function(function) if !function.builtin && function.body.is_some()
    ));
}

#[test]
fn bool_lang_item_requires_its_enum_variants() {
    let malformed = EDITION_2026_PRIMITIVES.replace(
        "pub let bool = enum { false, true }",
        "pub let bool = enum { true }",
    );
    let modules = edition_2026_test_modules(&[("primitives", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic == "lang item `bool` must have shape `pub let bool = enum { false, true }`"
    }));
}

#[test]
fn pointer_and_layout_lang_items_require_memory_contracts() {
    let modules = edition_2026_test_modules(&[("memory", "")]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    for name in ["array", "slice", "ptr", "size_of", "align_of"] {
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic == &format!("missing lang item `{name}`")),
            "{:?}",
            error.diagnostics()
        );
    }

    for (name, malformed) in [
        (
            "array",
            EDITION_2026_MEMORY.replace(
                "pub let array(comptime t: type)\n  (comptime l: usize): type",
                "pub let array(comptime t: type, comptime l: usize): type",
            ),
        ),
        (
            "slice",
            EDITION_2026_MEMORY.replace(
                "pub let slice(comptime t: type): type",
                "pub let slice: type",
            ),
        ),
        (
            "ptr",
            EDITION_2026_MEMORY.replace(
                "(value: borrow(a)(t)): ptr(a)(t)",
                "(value: borrow(t)): ptr(a)(t)",
            ),
        ),
        (
            "size_of",
            EDITION_2026_MEMORY.replace(
                "pub let size_of(comptime t: type): u64",
                "pub let size_of(comptime t: type): i32",
            ),
        ),
        (
            "align_of",
            EDITION_2026_MEMORY.replace(
                "pub let align_of(comptime t: type): u64",
                "pub let align_of(comptime t: type)(value: t): u64",
            ),
        ),
    ] {
        let modules = edition_2026_test_modules(&[("memory", &malformed)]);
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
}

#[test]
fn borrow_lang_items_require_the_borrow_module() {
    let modules = edition_2026_test_modules(&[("borrow", "")]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert_eq!(
        error
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.as_str() == "missing lang item `borrow`")
            .count(),
        2
    );
}

#[test]
fn rejects_malformed_control_contracts() {
    for (name, malformed) in [
            (
                "loop_exit",
                EDITION_2026_CONTROL.replace(
                    "let exit(move value: t): never",
                    "let exit(value: t): never",
                ),
            ),
            (
                "continue",
                EDITION_2026_CONTROL.replace(
                    "pub let continue: with(iteration_skip)(): never",
                    "pub let continue: with(iteration_skip)(): ()",
                ),
            ),
            (
                "return",
                EDITION_2026_CONTROL.replace(
                    ": with(function_exit(t))(move value: t): never",
                    ": with(function_exit(t))(value: t): never",
                ),
            ),
            (
                "do",
                EDITION_2026_CONTROL.replace(
                    "(move while: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): bool)): ()",
                    "(move until: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): bool)): ()",
                ),
            ),
            (
                "if",
                EDITION_2026_CONTROL.replace(
                    ": with(e)(condition: bool)(move then: with(e)((): t))",
                    ": with(e)(condition: i32)(move then: with(e)((): t))",
                ),
            ),
            (
                "match",
                EDITION_2026_CONTROL.replace(
                    "  ...cases: output",
                    "  (case: input): output",
                ),
            ),
            (
                "for",
                EDITION_2026_CONTROL.replace(
                    "  iter.item == item",
                    "  iter.item == bool",
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

    let malformed = EDITION_2026_UNSAFE.replace(
            "pub let unsafe(comptime e: effects, comptime t: type): with(e)(move action: with(core.unsafe.unsafety, e)((): t)): t",
            "pub let unsafe(comptime e: effects, comptime t: type): with(e)(move action: with(e)((): t)): t",
        );
    let modules = edition_2026_test_modules(&[("unsafe", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `unsafe`")));

    let bodyless = EDITION_2026_UNSAFE.replace(
        " = {\n  core.unsafe.unsafety.handle\n    action {\n      action()\n    }\n}",
        "",
    );
    let modules = edition_2026_test_modules(&[("unsafe", &bodyless)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `unsafe`")));

    let malformed = EDITION_2026_EFFECT.replace(
            "pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type): type = builtin()",
            "pub let effect_callable(comptime input: type, comptime output: type): type = builtin()",
        );
    let modules = edition_2026_test_modules(&[("effect", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `effect_callable`")));

    for (source_declaration, malformed_declaration, name) in [
            (
                "pub let continuation(comptime input: type, comptime output: type): type = builtin()",
                "pub let continuation(comptime input: type, comptime output: type) = struct {}",
                "continuation",
            ),
            (
                "pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type): type = builtin()",
                "pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type) = struct {}",
                "effect_callable",
            ),
        ] {
            let malformed = EDITION_2026_EFFECT.replace(source_declaration, malformed_declaration);
            let modules = edition_2026_test_modules(&[("effect", &malformed)]);
            let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
            assert!(error.diagnostics().iter().any(|diagnostic| {
                diagnostic.contains(&format!(
                    "lang item `{name}` must be type form, found struct"
                ))
            }));
        }

    let malformed = EDITION_2026_EFFECT.replace(
        "pub let handle = trait(comptime self: effect)",
        "pub let handle = trait",
    );
    let modules = edition_2026_test_modules(&[("effect", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `handle`")));

    let malformed = EDITION_2026_EFFECT
        .replace(
            "let clauses(comptime value: type, comptime answer: type): parameters",
            "let clauses(comptime value: type, comptime answer: type): type",
        )
        .replace(
            "(...move clauses: clauses(value, answer))",
            "(move clauses: clauses(value, answer))",
        );
    let modules = edition_2026_test_modules(&[("effect", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `handle`")));

    let malformed = EDITION_2026_ERROR.replace(
            "pub let throw(comptime error: type): with(core.error.throwing(error))(move error: error): never",
            "pub let throw(comptime error: type)(move error: error): never",
        );
    let modules = edition_2026_test_modules(&[("error", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `throw`")));
}

#[test]
fn rejects_malformed_async_contracts() {
    for (name, malformed) in [
        (
            "suspension",
            EDITION_2026_ASYNC.replace("let suspend(): ()", "let suspend(): i32"),
        ),
        ("poll", EDITION_2026_ASYNC.replace("  pending,\n", "")),
        (
            "future",
            EDITION_2026_ASYNC.replace(
                "trait(requires: self is movable)",
                "trait(requires: self is copyable)",
            ),
        ),
        (
            "executor",
            EDITION_2026_ASYNC.replace(
                "let run(comptime e: effects, comptime f: type, comptime t: type)",
                "let run(comptime f: type, comptime t: type)",
            ),
        ),
        (
            "async",
            EDITION_2026_ASYNC.replace(
                "(move action: with(core.async.suspension, e)((): t)): f",
                "(move action: with(e)((): t)): f",
            ),
        ),
        (
            "await",
            EDITION_2026_ASYNC.replace(
                ": with(core.async.suspension, e)(move future: f): t",
                ": with(e)(move future: f): t",
            ),
        ),
    ] {
        let modules = edition_2026_test_modules(&[("async", &malformed)]);
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
}

#[test]
fn rejects_malformed_iteration_contracts() {
    let malformed = EDITION_2026_ITER.replace(
        "let next(comptime r: region)(self: borrow(mut)(r)(self))\n    (): core.option(item(r))",
        "let next(comptime r: region)(self: borrow(r)(self))\n    (): core.option(item(r))",
    );
    let modules = edition_2026_test_modules(&[("iter", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `iterator`")));
}

#[test]
fn rejects_malformed_assignment_operator_contracts() {
    let malformed = EDITION_2026_OPS_ASSIGN.replace(
        "let add_assign(self: borrow(mut)(self))\n    (rhs: rhs): ()",
        "let add_assign(self: borrow(self))\n    (rhs: rhs): ()",
    );
    let modules = edition_2026_test_modules(&[("ops/assign", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `add_assign`")));
}

#[test]
fn rejects_malformed_index_contracts() {
    for malformed in [
            "pub let index = trait {}",
            "pub let index(comptime key: type) = trait { let output: type; let index(self)(key: key): output }",
            "pub let index(comptime key: type) = trait { let output: type; let index(comptime a: access)(self: borrow(self))(key: key): borrow(a)(output) }",
        ] {
            let modules = edition_2026_test_modules(&[("ops/index", malformed)]);
            let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.contains("lang item `index` must have shape")),
                "{malformed}: {:?}",
                error.diagnostics()
            );
        }
}

#[test]
fn rejects_malformed_flow_operator_contracts() {
    let malformed =
        EDITION_2026_FLOW.replace("let rebind(comptime value: type): type", "let rebind: type");
    let modules = edition_2026_test_modules(&[("flow", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `chain`")));

    let malformed = EDITION_2026_FLOW.replace(
        "let coalesce(comptime e: effects): with(e)(self)(fallback: with(e)((): item)): item",
        "let coalesce(move self)\n    (move fallback: (): item): item",
    );
    let modules = edition_2026_test_modules(&[("flow", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `coalesce`")));

    let malformed =
        EDITION_2026_FLOW.replace("let unwrap(move self): output", "let unwrap(self): output");
    let modules = edition_2026_test_modules(&[("flow", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `unwrap`")));

    let malformed = EDITION_2026_FLOW.replace(
        "let raise: with(core.error.throwing(error))(move self): output",
        "let raise(move self): output",
    );
    let modules = edition_2026_test_modules(&[("flow", &malformed)]);
    let error = CoreBundle::from_modules(Edition::Edition2026, &modules).unwrap_err();
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.contains("lang item `raise`")));
}

#[test]
fn lang_item_identities_follow_validated_declarations_not_source_order() {
    let source = r#"
pub let rem(comptime rhs: type) = trait {
  let output: type
  let rem(self)(rhs: rhs): output
}
pub let movable = trait {}
pub let copyable = trait(requires: self is movable) {}
pub let droppable = trait {
  let drop(self: borrow(mut)(self))(): ()
}
pub let add(comptime rhs: type) = trait {
  let output: type
  let add(self)(rhs: rhs): output
}
pub let never = enum {}
pub let option(comptime t: type) = enum { some(t), none }
pub let result(comptime e: type)(comptime t: type) = enum { ok(t), err(e) }
pub let div(comptime rhs: type) = trait {
  let output: type
  let div(self)(rhs: rhs): output
}
pub let sub(comptime rhs: type) = trait {
  let output: type
  let sub(self)(rhs: rhs): output
}
pub let mul(comptime rhs: type) = trait {
  let output: type
  let mul(self)(rhs: rhs): output
}
pub let eq(comptime rhs: type) = trait {
  let eq(self: borrow(self))(rhs: borrow(rhs)): bool
}
pub let partial_ordering = enum { less, equal, greater, unordered }
pub let partial_ord(comptime rhs: type) = trait {
  let partial_cmp(self: borrow(self))(rhs: borrow(rhs)): partial_ordering
}
pub let neg = trait {
  let output: type
  let neg(self)(): output
}
pub let not = trait {
  let output: type
  let not(self)(): output
}
pub let bit_and(comptime rhs: type) = trait {
  let output: type
  let bit_and(self)(rhs: rhs): output
}
pub let bit_or(comptime rhs: type) = trait {
  let output: type
  let bit_or(self)(rhs: rhs): output
}
pub let bit_xor(comptime rhs: type) = trait {
  let output: type
  let bit_xor(self)(rhs: rhs): output
}
pub let shl(comptime rhs: type) = trait {
  let output: type
  let shl(self)(rhs: rhs): output
}
pub let shr(comptime rhs: type) = trait {
  let output: type
  let shr(self)(rhs: rhs): output
}
pub let index(comptime key: type) = trait {
  let output: type
  let index(comptime a: access)(self: borrow(a)(self))(key: key): borrow(a)(output)
}
pub let str: type = builtin()
"#;
    let bundle = CoreBundle::from_source(Edition::Edition2026, source).unwrap();

    assert_eq!(bundle.lang_items().rem().item_index(), 0);
    assert_eq!(bundle.lang_items().move_trait().item_index(), 1);
    assert_eq!(bundle.lang_items().copy().item_index(), 2);
    assert_eq!(bundle.lang_items().drop().item_index(), 3);
    assert_eq!(bundle.lang_items().add().item_index(), 4);
    assert_eq!(bundle.lang_items().never().item_index(), 5);
    assert_eq!(bundle.lang_items().option().item_index(), 6);
    assert_eq!(bundle.lang_items().result().item_index(), 7);
    assert_eq!(bundle.lang_items().div().item_index(), 8);
    assert_eq!(bundle.lang_items().sub().item_index(), 9);
    assert_eq!(bundle.lang_items().mul().item_index(), 10);
    assert_eq!(bundle.lang_items().eq().item_index(), 11);
    assert_eq!(bundle.lang_items().partial_ordering().item_index(), 12);
    assert_eq!(bundle.lang_items().partial_ord().item_index(), 13);
    assert_eq!(bundle.lang_items().neg().item_index(), 14);
    assert_eq!(bundle.lang_items().not().item_index(), 15);
    assert_eq!(bundle.lang_items().bit_and().item_index(), 16);
    assert_eq!(bundle.lang_items().bit_or().item_index(), 17);
    assert_eq!(bundle.lang_items().bit_xor().item_index(), 18);
    assert_eq!(bundle.lang_items().shl().item_index(), 19);
    assert_eq!(bundle.lang_items().shr().item_index(), 20);
    assert_eq!(bundle.lang_items().index().item_index(), 21);
    assert_eq!(bundle.lang_items().str_type_form().item_index(), 22);
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
let option(comptime t: type) = enum { some(t), none }
pub let result = struct { value: i32 }
pub let never = enum { reachable }
pub let movable = trait {}
pub let copyable(comptime t: type) = trait {}
pub let add(comptime rhs: type) = trait {
  let add(self)(rhs: rhs): rhs
}
pub let extra = enum {}
pub let sub(comptime rhs: type) = trait {
  let output: type
  let sub(self)(rhs: rhs): output
}
pub let mul(comptime rhs: type) = trait {
  let output: type
  let mul(self)(rhs: rhs): output
}
pub let div(comptime rhs: type) = trait {
  let output: type
  let div(self)(rhs: rhs): output
}
pub let rem(comptime rhs: type) = trait {
  let output: type
  let rem(self)(rhs: rhs): output
}
pub let eq(comptime rhs: type) = trait {
  let eq(self: borrow(self))(rhs: borrow(rhs)): bool
}
pub let partial_ordering = enum { less, equal, greater, unordered }
pub let partial_ord(comptime rhs: type) = trait {
  let partial_cmp(self: borrow(self))(rhs: borrow(rhs)): partial_ordering
}
pub let neg = trait {
  let output: type
  let neg(self)(): output
}
pub let not = trait {
  let output: type
  let not(self)(): output
}
pub let bit_and(comptime rhs: type) = trait {
  let output: type
  let bit_and(self)(rhs: rhs): output
}
pub let bit_or(comptime rhs: type) = trait {
  let output: type
  let bit_or(self)(rhs: rhs): output
}
pub let bit_xor(comptime rhs: type) = trait {
  let output: type
  let bit_xor(self)(rhs: rhs): output
}
pub let shl(comptime rhs: type) = trait {
  let output: type
  let shl(self)(rhs: rhs): output
}
pub let shr(comptime rhs: type) = trait {
  let output: type
  let shr(self)(rhs: rhs): output
}
pub let droppable = trait {
  let drop(self: borrow(mut)(self))(): ()
}
pub let str: type = builtin()
"#;
    let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

    assert_eq!(
            error.diagnostics(),
            [
                "lang item `option` must be `pub`, found private visibility",
                "unexpected declaration `extra` at item 7",
                "lang item `result` must be enum, found struct",
                "lang item `never` must have shape `pub let never = enum {}`",
                "lang item `copyable` must have shape `pub let copyable = trait(requires: self is movable) {}`",
                "lang item `add` must have shape `pub let add(comptime rhs: type) = trait { let output: type; let add(self)(rhs: rhs): output }`",
                "missing lang item `index`",
            ]
        );
    assert_eq!(
            error.to_string(),
            "invalid embedded core bundle for edition 2026\n- lang item `option` must be `pub`, found private visibility\n- unexpected declaration `extra` at item 7\n- lang item `result` must be enum, found struct\n- lang item `never` must have shape `pub let never = enum {}`\n- lang item `copyable` must have shape `pub let copyable = trait(requires: self is movable) {}`\n- lang item `add` must have shape `pub let add(comptime rhs: type) = trait { let output: type; let add(self)(rhs: rhs): output }`\n- missing lang item `index`"
        );
}

#[test]
fn rejects_missing_and_duplicate_lang_items_in_fixed_role_order() {
    let source = r#"
pub let option(comptime t: type) = enum { some(t), none }
pub let option(comptime t: type) = enum { some(t), none }
pub let never = enum {}
pub let add(comptime rhs: type) = trait {
  let output: type
  let add(self)(rhs: rhs): output
}
pub let sub(comptime rhs: type) = trait {
  let output: type
  let sub(self)(rhs: rhs): output
}
pub let mul(comptime rhs: type) = trait {
  let output: type
  let mul(self)(rhs: rhs): output
}
pub let div(comptime rhs: type) = trait {
  let output: type
  let div(self)(rhs: rhs): output
}
pub let rem(comptime rhs: type) = trait {
  let output: type
  let rem(self)(rhs: rhs): output
}
pub let eq(comptime rhs: type) = trait {
  let eq(self: borrow(self))(rhs: borrow(rhs)): bool
}
pub let partial_ordering = enum { less, equal, greater, unordered }
pub let partial_ord(comptime rhs: type) = trait {
  let partial_cmp(self: borrow(self))(rhs: borrow(rhs)): partial_ordering
}
pub let neg = trait {
  let output: type
  let neg(self)(): output
}
pub let not = trait {
  let output: type
  let not(self)(): output
}
pub let bit_and(comptime rhs: type) = trait {
  let output: type
  let bit_and(self)(rhs: rhs): output
}
pub let bit_or(comptime rhs: type) = trait {
  let output: type
  let bit_or(self)(rhs: rhs): output
}
pub let bit_xor(comptime rhs: type) = trait {
  let output: type
  let bit_xor(self)(rhs: rhs): output
}
pub let shl(comptime rhs: type) = trait {
  let output: type
  let shl(self)(rhs: rhs): output
}
pub let shr(comptime rhs: type) = trait {
  let output: type
  let shr(self)(rhs: rhs): output
}
pub let str: type = builtin()
"#;
    let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

    assert_eq!(
        error.diagnostics(),
        [
            "duplicate lang item `option` appears 2 times",
            "missing lang item `result`",
            "missing lang item `movable`",
            "missing lang item `copyable`",
            "missing lang item `droppable`",
            "missing lang item `index`",
        ]
    );
}

#[test]
fn rejects_copy_compile_parameters_associated_types_and_methods() {
    let malformed_declarations = [
        "pub let copyable(comptime t: type) = trait {}",
        "pub let copyable = trait { let item: type }",
        "pub let copyable = trait { let clone(self: borrow(self))(): self }",
    ];

    for declaration in malformed_declarations {
        let source = core_source_with_copy(declaration);
        let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();

        assert_eq!(
                error.diagnostics(),
                ["lang item `copyable` must have shape `pub let copyable = trait(requires: self is movable) {}`"],
                "unexpected diagnostic for `{declaration}`"
            );
    }
}

#[test]
fn rejects_malformed_move_traits_and_copy_without_move_supertrait() {
    for malformed in [
        "pub let movable(comptime t: type) = trait {}",
        "pub let movable = trait { let item: type }",
        "pub let movable = trait(requires: self is copyable) {}",
    ] {
        let source =
            core_source_with_copy("pub let copyable = trait(requires: self is movable) {}")
                .replacen("pub let movable = trait {}", malformed, 1);
        let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
        assert_eq!(
            error.diagnostics(),
            ["lang item `movable` must have shape `pub let movable = trait {}`"],
            "unexpected diagnostic for `{malformed}`"
        );
    }

    let source = core_source_with_copy("pub let copyable = trait {}");
    let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
    assert_eq!(
        error.diagnostics(),
        ["lang item `copyable` must have shape `pub let copyable = trait(requires: self is movable) {}`"]
    );
}

#[test]
fn rejects_malformed_drop_traits() {
    let malformed_declarations = [
        "pub let droppable(comptime t: type) = trait { let drop(self: borrow(mut)(self))(): () }",
        "pub let droppable = trait {}",
        "pub let droppable = trait { let drop(self: borrow(self))(): () }",
        "pub let droppable = trait { let drop(self: borrow(mut)(self))(): i32 }",
    ];

    for declaration in malformed_declarations {
        let source =
            core_source_with_copy("pub let copyable = trait(requires: self is movable) {}")
                .replacen(
                    "pub let droppable = trait {\n  let drop(self: borrow(mut)(self))(): ()\n}",
                    declaration,
                    1,
                );
        let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
        assert_eq!(
                error.diagnostics(),
                ["lang item `droppable` must have shape `pub let droppable = trait { let drop(self: borrow(mut)(self))(): () }`"],
                "unexpected diagnostic for `{declaration}`"
            );
    }
}

#[test]
fn rejects_malformed_operator_traits_in_fixed_role_order() {
    let source = r#"
pub let option(comptime t: type) = enum { some(t), none }
pub let result(comptime e: type)(comptime t: type) = enum { ok(t), err(e) }
pub let never = enum {}
pub let movable = trait {}
pub let copyable = trait(requires: self is movable) {}
pub let droppable = trait {
  let drop(self: borrow(mut)(self))(): ()
}
pub let add(comptime rhs: type) = trait {
  let output: type
  let add(self)(rhs: rhs): output
}
pub let sub = trait {
  let output: type
  let sub(self)(rhs: rhs): output
}
pub let mul(comptime rhs: type) = trait {
  let mul(self)(rhs: rhs): rhs
}
pub let div(comptime rhs: type) = trait {
  let output: type
  let divide(self)(rhs: rhs): output
}
pub let rem(comptime rhs: type) = trait {
  let output: type
  let rem(self)(rhs: rhs): output = { rhs }
}
pub let eq(comptime rhs: type) = trait {
  let eq(move self)(rhs: rhs): bool
}
pub let partial_ordering = enum { less, equal, greater, unordered }
pub let partial_ord(comptime rhs: type) = trait {
  let partial_cmp(move self)(rhs: rhs): partial_ordering
}
pub let neg = trait {
  let output: type
  let neg(self)(): output
}
pub let not = trait {
  let output: type
  let not(self)(): output
}
pub let bit_and(comptime rhs: type) = trait {
  let output: type
  let bit_and(self)(rhs: rhs): output
}
pub let bit_or(comptime rhs: type) = trait {
  let output: type
  let bit_or(self)(rhs: rhs): output
}
pub let bit_xor(comptime rhs: type) = trait {
  let output: type
  let bit_xor(self)(rhs: rhs): output
}
pub let shl(comptime rhs: type) = trait {
  let output: type
  let shl(self)(rhs: rhs): output
}
pub let shr(comptime rhs: type) = trait {
  let output: type
  let shr(self)(rhs: rhs): output
}
pub let index(comptime key: type) = trait {
  let output: type
  let index(comptime a: access)(self: borrow(a)(self))(key: key): borrow(a)(output)
}
pub let str: type = builtin()
"#;
    let error = CoreBundle::from_source(Edition::Edition2026, source).unwrap_err();

    assert_eq!(
            error.diagnostics(),
            [
                "lang item `sub` must have shape `pub let sub(comptime rhs: type) = trait { let output: type; let sub(self)(rhs: rhs): output }`",
                "lang item `mul` must have shape `pub let mul(comptime rhs: type) = trait { let output: type; let mul(self)(rhs: rhs): output }`",
                "lang item `div` must have shape `pub let div(comptime rhs: type) = trait { let output: type; let div(self)(rhs: rhs): output }`",
                "lang item `rem` must have shape `pub let rem(comptime rhs: type) = trait { let output: type; let rem(self)(rhs: rhs): output }`",
                "lang item `eq` must have shape `pub let eq(comptime rhs: type) = trait { let eq(self: borrow(self))(rhs: borrow(rhs)): bool }`",
                "lang item `partial_ord` must have shape `pub let partial_ord(comptime rhs: type) = trait { let partial_cmp(self: borrow(self))(rhs: borrow(rhs)): partial_ordering }`",
            ]
        );
}

#[test]
fn rejects_malformed_partial_ordering() {
    for declaration in [
        "pub let partial_ordering(comptime t: type) = enum { less, equal, greater, unordered }",
        "pub let partial_ordering = enum { less, equal, greater }",
        "pub let partial_ordering = enum { less, equal, greater, unknown }",
    ] {
        let source =
            core_source_with_copy("pub let copyable = trait(requires: self is movable) {}")
                .replacen(
                    "pub let partial_ordering = enum { less, equal, greater, unordered }",
                    declaration,
                    1,
                );
        let error = CoreBundle::from_source(Edition::Edition2026, &source).unwrap_err();
        assert_eq!(
                error.diagnostics(),
                ["lang item `partial_ordering` must have shape `pub let partial_ordering = enum { less, equal, greater, unordered }`"],
                "unexpected diagnostic for `{declaration}`"
            );
    }
}

#[test]
fn rejects_malformed_unary_operator_traits() {
    for (original, malformed, expected) in [
            (
                "pub let neg = trait {\n  let output: type\n  let neg(self)(): output\n}",
                "pub let neg(comptime rhs: type) = trait { let neg(self)(): i32 }",
                "lang item `neg` must have shape `pub let neg = trait { let output: type; let neg(self)(): output }`",
            ),
            (
                "pub let not = trait {\n  let output: type\n  let not(self)(): output\n}",
                "pub let not = trait { let output: type; let not(self: borrow(self))(): output }",
                "lang item `not` must have shape `pub let not = trait { let output: type; let not(self)(): output }`",
            ),
        ] {
            let source =
                core_source_with_copy("pub let copyable = trait(requires: self is movable) {}").replacen(
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
                "pub let bit_and(comptime rhs: type) = trait {\n  let output: type\n  let bit_and(self)(rhs: rhs): output\n}",
                "pub let bit_and = trait { let bit_and(self: borrow(self))(move rhs: i32): i32 }",
                "lang item `bit_and` must have shape `pub let bit_and(comptime rhs: type) = trait { let output: type; let bit_and(self)(rhs: rhs): output }`",
            ),
            (
                "pub let shr(comptime rhs: type) = trait {\n  let output: type\n  let shr(self)(rhs: rhs): output\n}",
                "pub let shr(comptime rhs: type) = trait { let output: type; let shift(move self)(rhs: rhs): output }",
                "lang item `shr` must have shape `pub let shr(comptime rhs: type) = trait { let output: type; let shr(self)(rhs: rhs): output }`",
            ),
        ] {
            let source =
                core_source_with_copy("pub let copyable = trait(requires: self is movable) {}").replacen(
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
        CoreBundle::from_source(Edition::Edition2026, "pub let option = enum {").unwrap_err();

    assert_eq!(error.diagnostics().len(), 1);
    assert!(error.diagnostics()[0].starts_with("embedded prelude does not parse: "));
}
