//! Edition-pinned Salicin `std` sources.
//!
//! Unlike `core`, this bundle owns no language items. It contains only
//! higher-level, host-facing, or policy-bearing standard abstractions; lower
//! layers remain available through their canonical `core` and `alloc` paths.

use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use crate::ast::{Item, Program, Visibility};
use crate::manifest::Edition;
use crate::modules::{self, PackageId, SourceUnit};
use crate::parser;

const EDITION_2026_LIB: &str = include_str!("../../library/std/src/lib.sc");
const EDITION_2026_ASYNC: &str = include_str!("../../library/std/src/async.sc");
const EDITION_2026_ALGEBRA: &str = include_str!("../../library/std/src/algebra.sc");
const EDITION_2026_FUNCTIONAL: &str = include_str!("../../library/std/src/functional.sc");
const EDITION_2026_IO: &str = include_str!("../../library/std/src/io.sc");
const EDITION_2026_TEST: &str = include_str!("../../library/std/src/test.sc");

const EDITION_2026_MODULES: &[(&str, &str)] = &[
    ("lib", EDITION_2026_LIB),
    ("async", EDITION_2026_ASYNC),
    ("algebra", EDITION_2026_ALGEBRA),
    ("functional", EDITION_2026_FUNCTIONAL),
    ("io", EDITION_2026_IO),
    ("test", EDITION_2026_TEST),
];

static EDITION_2026_BUNDLE: OnceLock<Result<StdBundle, StdBundleError>> = OnceLock::new();

pub(crate) fn incremental_sources(
    edition: Edition,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    match edition {
        Edition::Edition2026 => EDITION_2026_MODULES.iter().copied(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StdExport {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) target: Vec<String>,
}

const CATEGORY_SUFFIXES: &[&str] = &[
    "_effect",
    "_trait",
    "_type",
    "_struct",
    "_enum",
    "_function",
    "_sort",
];

pub(crate) fn naming_diagnostics(program: &Program, layer: &str) -> Vec<String> {
    program
        .items
        .iter()
        .zip(&program.item_visibilities)
        .filter(|(_, visibility)| **visibility == Visibility::Public)
        .filter_map(|(item, _)| {
            let (name, category) = match item {
                Item::Function(definition) => (&definition.name, "function"),
                Item::Global(definition) => (&definition.name, "value"),
                Item::Struct(definition) => (&definition.name, "struct"),
                Item::Enum(definition) => (&definition.name, "enum"),
                Item::Effect(definition) => (&definition.name, "effect"),
                Item::Sort(definition) => (&definition.name, "sort"),
                Item::TypeForm(definition) => (&definition.name, "type"),
                Item::TypeAlias(definition) => (&definition.name, "type alias"),
                Item::Trait(definition) => (&definition.name, "trait"),
                Item::Extend(_) => return None,
            };
            validate_standard_name(name, category).map(|reason| {
                format!("public {layer} {category} `{name}` violates standard naming: {reason}")
            })
        })
        .collect()
}

fn validate_standard_name(name: &str, category: &str) -> Option<String> {
    let ascii_snake_case = !name.is_empty()
        && !name.starts_with('_')
        && !name.ends_with('_')
        && !name.contains("__")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if !ascii_snake_case {
        return Some(
            "use ASCII `snake_case` without leading, trailing, or repeated underscores".into(),
        );
    }
    if let Some(suffix) = CATEGORY_SUFFIXES
        .iter()
        .find(|suffix| name.ends_with(**suffix))
    {
        return Some(format!(
            "name the {category} for its semantics instead of using the `{suffix}` category suffix"
        ));
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct StdBundle {
    program: Program,
    exports: Vec<StdExport>,
}

impl StdBundle {
    pub fn for_edition(edition: Edition) -> Result<Self, StdBundleError> {
        Self::cached_for_edition(edition).cloned()
    }

    pub(crate) fn cached_for_edition(edition: Edition) -> Result<&'static Self, StdBundleError> {
        match EDITION_2026_BUNDLE.get_or_init(|| Self::load(edition)) {
            Ok(bundle) => Ok(bundle),
            Err(error) => Err(error.clone()),
        }
    }

    fn load(edition: Edition) -> Result<Self, StdBundleError> {
        validate_host_target(std::env::consts::OS, std::env::consts::ARCH, edition)?;
        Self::from_modules(edition, EDITION_2026_MODULES)
    }

    fn from_modules(edition: Edition, modules: &[(&str, &str)]) -> Result<Self, StdBundleError> {
        let mut exports = Vec::new();
        for &(module, source) in modules {
            let parsed = parser::parse(source).map_err(|error| {
                StdBundleError::new(
                    edition,
                    vec![format!(
                        "embedded std module `{module}` does not parse: {error}"
                    )],
                )
            })?;
            let naming = naming_diagnostics(&parsed, "std");
            if !naming.is_empty() {
                return Err(StdBundleError::new(edition, naming));
            }
            for (item, visibility) in parsed.items.iter().zip(&parsed.item_visibilities) {
                if *visibility != Visibility::Public {
                    continue;
                }
                let Some(name) = standard_item_name(item) else {
                    continue;
                };
                let mut target = vec!["std".to_owned()];
                if module != "lib" {
                    target.extend(module.split('/').map(str::to_owned));
                }
                target.push(name.to_owned());
                exports.push(StdExport {
                    module: if module == "lib" {
                        String::new()
                    } else {
                        module.replace('/', ".")
                    },
                    name: name.to_owned(),
                    target,
                });
            }
            if let Some(import) = parsed.uses.into_iter().next() {
                let Some(name) = import.alias else {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std module `{module}` alias must have an explicit name"
                        )],
                    ));
                };
                if let Some(reason) = validate_standard_name(&name, "alias") {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std alias `{}` violates standard naming: {reason}",
                            display_export(module, &name)
                        )],
                    ));
                }
                if !matches!(
                    import.path.first().map(String::as_str),
                    Some("core" | "alloc" | "std")
                ) {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std alias `{}` may target only `core`, `alloc`, or `std`",
                            display_export(module, &name)
                        )],
                    ));
                }
                return Err(StdBundleError::new(
                    edition,
                    vec![format!(
                        "embedded std alias `{}` mirrors another canonical path; reference the target directly",
                        display_export(module, &name)
                    )],
                ));
            }
        }
        exports
            .sort_by(|left, right| (&left.module, &left.name).cmp(&(&right.module, &right.name)));
        for pair in exports.windows(2) {
            if pair[0].module == pair[1].module && pair[0].name == pair[1].name {
                return Err(StdBundleError::new(
                    edition,
                    vec![format!(
                        "embedded std export `{}` is duplicated",
                        display_export(&pair[0].module, &pair[0].name)
                    )],
                ));
            }
        }

        let mut sources = vec![SourceUnit {
            path: "<std>".to_owned(),
            module_path: Vec::new(),
            source: String::new(),
            is_root: true,
        }];
        sources.extend(modules.iter().map(|(module, source)| SourceUnit {
            path: format!("<std/{module}>"),
            module_path: std_source_module_path(module),
            source: (*source).to_owned(),
            is_root: false,
        }));
        let mut program = modules::resolve_embedded_std_sources(&sources)
            .map_err(|diagnostics| StdBundleError::new(edition, diagnostics))?;
        for origin in &mut program.item_origins {
            origin.package = PackageId::STD.0;
            origin.module_path = if origin.module_path.is_empty() {
                vec!["@std".to_owned()]
            } else {
                let mut mapped = vec!["@std".to_owned()];
                mapped.extend(origin.module_path.iter().skip(1).cloned());
                mapped
            };
        }
        Ok(Self { program, exports })
    }

    pub const fn program(&self) -> &Program {
        &self.program
    }

    pub(crate) fn exports(&self) -> &[StdExport] {
        &self.exports
    }
}

fn standard_item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(definition) => Some(&definition.name),
        Item::Global(definition) => Some(&definition.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
        Item::Effect(definition) => Some(&definition.name),
        Item::Sort(definition) => Some(&definition.name),
        Item::TypeForm(definition) => Some(&definition.name),
        Item::TypeAlias(definition) => Some(&definition.name),
        Item::Trait(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn std_source_module_path(module: &str) -> Vec<String> {
    match module {
        "lib" => vec!["std".to_owned()],
        module => std::iter::once("std".to_owned())
            .chain(module.split('/').map(str::to_owned))
            .collect(),
    }
}

fn display_export(module: &str, name: &str) -> String {
    if module.is_empty() || module == "lib" {
        format!("std.{name}")
    } else {
        format!("std.{}.{name}", module.replace('/', "."))
    }
}

fn host_target_is_supported(os: &str, arch: &str) -> bool {
    matches!((os, arch), ("linux", "x86_64") | ("macos", "aarch64"))
}

fn validate_host_target(os: &str, arch: &str, edition: Edition) -> Result<(), StdBundleError> {
    if host_target_is_supported(os, arch) {
        Ok(())
    } else {
        Err(StdBundleError::new(
            edition,
            vec![format!(
                "host standard library is unavailable for target `{arch}-{os}`; supported targets are `x86_64-linux` and `aarch64-macos`"
            )],
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StdBundleError {
    edition: Edition,
    diagnostics: Vec<String>,
}

impl StdBundleError {
    fn new(edition: Edition, diagnostics: Vec<String>) -> Self {
        Self {
            edition,
            diagnostics,
        }
    }
}

impl fmt::Display for StdBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid embedded std bundle for edition {}",
            self.edition
        )?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n- {diagnostic}")?;
        }
        Ok(())
    }
}

impl Error for StdBundleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edition_2026_std_bundle_owns_policy_without_mirrors() {
        let bundle = StdBundle::for_edition(Edition::Edition2026).unwrap();
        assert!(!bundle.program().items.is_empty());
        assert_eq!(bundle.exports().len(), 42);
        assert!(bundle.exports().iter().any(|export| {
            export.module == "algebra"
                && export.name == "semigroup"
                && export.target == ["std", "algebra", "semigroup"]
        }));
        assert!(bundle.exports().iter().any(|export| {
            export.module == "async"
                && export.name == "spin"
                && export.target == ["std", "async", "spin"]
        }));
        assert!(bundle.exports().iter().any(|export| {
            export.module == "io" && export.name == "io" && export.target == ["std", "io", "io"]
        }));
        assert!(bundle.exports().iter().any(|export| {
            export.module == "io"
                && export.name == "io_error"
                && export.target == ["std", "io", "io_error"]
        }));
        assert!(bundle.exports().iter().any(|export| {
            export.module == "test"
                && export.name == "assert_eq"
                && export.target == ["std", "test", "assert_eq"]
        }));
        assert!(bundle
            .exports()
            .iter()
            .all(|export| export.target.first().is_some_and(|root| root == "std")));

        let io = bundle
            .program()
            .items
            .iter()
            .find_map(|item| match item {
                Item::Effect(effect) if effect.name == "std::io::io" => Some(effect),
                _ => None,
            })
            .expect("std.io.io must be the canonical embedded authority identity");
        assert!(io.compile_groups.is_empty());
        assert!(io.operations.is_empty());

        let kinds = bundle
            .program()
            .items
            .iter()
            .find_map(|item| match item {
                Item::Enum(definition) if definition.name == "std::io::io_error_kind" => {
                    Some(definition)
                }
                _ => None,
            })
            .expect("std.io.io_error_kind must be embedded")
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "not_found",
                "permission_denied",
                "already_exists",
                "invalid_input",
                "invalid_data",
                "interrupted",
                "would_block",
                "write_zero",
                "unexpected_eof",
                "broken_pipe",
                "unsupported",
                "out_of_memory",
                "other",
            ]
        );
    }

    #[test]
    fn std_bundle_accepts_definitions_and_rejects_mirror_aliases() {
        StdBundle::from_modules(
            Edition::Edition2026,
            &[("owned", "pub let service = trait {}\n")],
        )
        .unwrap();
        for (source, expected) in [
            (
                "let option = core.option.option",
                "mirrors another canonical path",
            ),
            (
                "pub let option = core.option.option",
                "mirrors another canonical path",
            ),
            (
                "pub let forged = dependency.value",
                "may target only `core`, `alloc`, or `std`",
            ),
        ] {
            let error =
                StdBundle::from_modules(Edition::Edition2026, &[("lib", source)]).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn host_std_target_matrix_is_explicit() {
        assert!(host_target_is_supported("linux", "x86_64"));
        assert!(host_target_is_supported("macos", "aarch64"));
        for (os, arch) in [
            ("linux", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("wasi", "wasm32"),
        ] {
            assert!(!host_target_is_supported(os, arch));
            let error = validate_host_target(os, arch, Edition::Edition2026).unwrap_err();
            assert!(error.to_string().contains(&format!("`{arch}-{os}`")));
        }
    }

    #[test]
    fn standard_names_encode_semantics_instead_of_declaration_categories() {
        let valid = parser::parse(
            "pub let option(comptime t: type) = enum { some(t), none }\n\
             pub let copyable = trait {}\n\
             pub let suspension = effect { let suspend(): () }\n",
        )
        .unwrap();
        assert!(naming_diagnostics(&valid, "test").is_empty());

        for (source, expected) in [
            ("pub let network_effect = effect {}\n", "`_effect`"),
            ("pub let iterator_trait = trait {}\n", "`_trait`"),
            ("pub let message_type = struct {}\n", "`_type`"),
        ] {
            let program = parser::parse(source).unwrap();
            let diagnostics = naming_diagnostics(&program, "test");
            assert_eq!(diagnostics.len(), 1);
            assert!(diagnostics[0].contains(expected), "{diagnostics:?}");
        }
    }
}
