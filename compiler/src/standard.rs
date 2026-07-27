//! Edition-pinned Salicin `std` facade sources.
//!
//! Unlike `core`, this bundle owns no language items. Every declaration in
//! the initial bundle is a public alias to an already validated `core` or
//! `alloc` identity. Host-authority declarations will require explicit
//! validation before they can be added here.

use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use crate::ast::{Program, Visibility};
use crate::manifest::Edition;
use crate::modules::{self, PackageId, SourceUnit};
use crate::parser;

const EDITION_2026_LIB: &str = include_str!("../../library/std/src/lib.sc");
const EDITION_2026_PRELUDE: &str = include_str!("../../library/std/src/prelude.sc");
const EDITION_2026_NEVER: &str = include_str!("../../library/std/src/never.sc");
const EDITION_2026_MARKER: &str = include_str!("../../library/std/src/marker.sc");
const EDITION_2026_OPTION: &str = include_str!("../../library/std/src/option.sc");
const EDITION_2026_RESULT: &str = include_str!("../../library/std/src/result.sc");
const EDITION_2026_ERROR: &str = include_str!("../../library/std/src/error.sc");
const EDITION_2026_OPS: &str = include_str!("../../library/std/src/ops.sc");
const EDITION_2026_OPS_ARITH: &str = include_str!("../../library/std/src/ops/arith.sc");
const EDITION_2026_OPS_BIT: &str = include_str!("../../library/std/src/ops/bit.sc");
const EDITION_2026_OPS_ASSIGN: &str = include_str!("../../library/std/src/ops/assign.sc");
const EDITION_2026_OPS_INDEX: &str = include_str!("../../library/std/src/ops/index.sc");
const EDITION_2026_CMP: &str = include_str!("../../library/std/src/cmp.sc");
const EDITION_2026_FLOW: &str = include_str!("../../library/std/src/flow.sc");
const EDITION_2026_EFFECT: &str = include_str!("../../library/std/src/effect.sc");
const EDITION_2026_ASYNC: &str = include_str!("../../library/std/src/async.sc");
const EDITION_2026_UNSAFE: &str = include_str!("../../library/std/src/unsafe.sc");
const EDITION_2026_SORTS: &str = include_str!("../../library/std/src/sorts.sc");
const EDITION_2026_FOREIGN: &str = include_str!("../../library/std/src/foreign.sc");
const EDITION_2026_PASSING: &str = include_str!("../../library/std/src/passing.sc");
const EDITION_2026_BORROW: &str = include_str!("../../library/std/src/borrow.sc");
const EDITION_2026_CONTROL: &str = include_str!("../../library/std/src/control.sc");
const EDITION_2026_ITER: &str = include_str!("../../library/std/src/iter.sc");
const EDITION_2026_ALGEBRA: &str = include_str!("../../library/std/src/algebra.sc");
const EDITION_2026_FUNCTIONAL: &str = include_str!("../../library/std/src/functional.sc");
const EDITION_2026_BOXED: &str = include_str!("../../library/std/src/boxed.sc");
const EDITION_2026_VEC: &str = include_str!("../../library/std/src/vec.sc");
const EDITION_2026_STRING: &str = include_str!("../../library/std/src/string.sc");
const EDITION_2026_ARRAY: &str = include_str!("../../library/std/src/array.sc");
const EDITION_2026_SLICE: &str = include_str!("../../library/std/src/slice.sc");

const EDITION_2026_MODULES: &[(&str, &str)] = &[
    ("lib", EDITION_2026_LIB),
    ("prelude", EDITION_2026_PRELUDE),
    ("never", EDITION_2026_NEVER),
    ("marker", EDITION_2026_MARKER),
    ("option", EDITION_2026_OPTION),
    ("result", EDITION_2026_RESULT),
    ("error", EDITION_2026_ERROR),
    ("ops", EDITION_2026_OPS),
    ("ops/arith", EDITION_2026_OPS_ARITH),
    ("ops/bit", EDITION_2026_OPS_BIT),
    ("ops/assign", EDITION_2026_OPS_ASSIGN),
    ("ops/index", EDITION_2026_OPS_INDEX),
    ("cmp", EDITION_2026_CMP),
    ("flow", EDITION_2026_FLOW),
    ("effect", EDITION_2026_EFFECT),
    ("async", EDITION_2026_ASYNC),
    ("unsafe", EDITION_2026_UNSAFE),
    ("sorts", EDITION_2026_SORTS),
    ("foreign", EDITION_2026_FOREIGN),
    ("passing", EDITION_2026_PASSING),
    ("borrow", EDITION_2026_BORROW),
    ("control", EDITION_2026_CONTROL),
    ("iter", EDITION_2026_ITER),
    ("algebra", EDITION_2026_ALGEBRA),
    ("functional", EDITION_2026_FUNCTIONAL),
    ("boxed", EDITION_2026_BOXED),
    ("vec", EDITION_2026_VEC),
    ("string", EDITION_2026_STRING),
    ("array", EDITION_2026_ARRAY),
    ("slice", EDITION_2026_SLICE),
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
            if !parsed.items.is_empty() {
                return Err(StdBundleError::new(
                    edition,
                    vec![format!(
                        "embedded std module `{module}` may contain only public aliases"
                    )],
                ));
            }
            for import in parsed.uses {
                if import.visibility != Visibility::Public {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std module `{module}` alias must be public"
                        )],
                    ));
                }
                let Some(name) = import.alias else {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std module `{module}` alias must have an explicit name"
                        )],
                    ));
                };
                if !matches!(
                    import.path.first().map(String::as_str),
                    Some("core" | "alloc")
                ) {
                    return Err(StdBundleError::new(
                        edition,
                        vec![format!(
                            "embedded std alias `{}` may target only `core` or `alloc`",
                            display_export(module, &name)
                        )],
                    ));
                }
                exports.push(StdExport {
                    module: if module == "lib" {
                        String::new()
                    } else {
                        module.replace('/', ".")
                    },
                    name,
                    target: import.path,
                });
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
    fn edition_2026_std_bundle_is_alias_only_and_resolves() {
        let bundle = StdBundle::for_edition(Edition::Edition2026).unwrap();
        assert!(bundle.program().items.is_empty());
        assert!(bundle.exports().len() > 100);
        assert!(bundle.exports().iter().any(|export| {
            export.module == "prelude"
                && export.name == "movable"
                && export.target == ["core", "marker", "movable"]
        }));
        assert!(bundle.exports().iter().any(|export| {
            export.module == "array"
                && export.name == "array"
                && export.target == ["core", "memory", "array"]
        }));
    }

    #[test]
    fn std_bundle_rejects_definitions_private_aliases_and_foreign_targets() {
        for (source, expected) in [
            ("pub let forged = effect {}", "only public aliases"),
            ("let option = core.option.option", "alias must be public"),
            (
                "pub let forged = dependency.value",
                "may target only `core` or `alloc`",
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
}
