//! Versioned, path-independent inputs and artifact identity for incremental compilation.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::manifest::Edition;
use crate::modules::SourcePackage;

/// Version of the byte stream hashed by [`fingerprint_package_graph`].
pub const INPUT_SCHEMA_VERSION: u32 = 2;

/// Version of the first persistent LLVM-IR cache entry format.
pub const CACHE_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const CACHE_DIRECTORY_ENV: &str = "SALICIN_CACHE_DIR";
pub const CACHE_PAYLOAD_FILE: &str = "module.ll";
pub const CACHE_METADATA_FILE: &str = "metadata.toml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IncrementalTarget<'a> {
    Binary,
    Library,
    Test { filter: Option<&'a str> },
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct IncrementalFingerprint([u8; 32]);

impl IncrementalFingerprint {
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use fmt::Write;
            write!(output, "{byte:02x}").expect("writing to a string cannot fail");
        }
        output
    }
}

impl fmt::Debug for IncrementalFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("IncrementalFingerprint")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for IncrementalFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// Map a fingerprint to its compiler-owned, content-addressed entry directory.
///
/// The returned path is relative to the selected Salicin cache root. Output
/// paths and checkout paths cannot influence it.
pub fn cache_entry_relative_path(fingerprint: IncrementalFingerprint) -> PathBuf {
    let hex = fingerprint.to_hex();
    PathBuf::from("llvm-ir")
        .join(format!("v{CACHE_ARTIFACT_SCHEMA_VERSION}"))
        .join("sha256")
        .join(&hex[..2])
        .join(&hex[2..])
}

/// Fingerprint every semantic and native-code input to one package-graph
/// compilation. Input collection is independent of graph-local package IDs,
/// source diagnostic paths, and caller iteration order.
pub fn fingerprint_package_graph(
    packages: &[SourcePackage],
    edition: Edition,
    target: IncrementalTarget<'_>,
) -> Result<IncrementalFingerprint, String> {
    validate_graph(packages)?;
    let identities = packages
        .iter()
        .map(|package| (package.id, package.identity.as_str()))
        .collect::<HashMap<_, _>>();
    let mut ordered = packages.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.identity.cmp(&right.identity));

    let mut encoder = FingerprintEncoder::new();
    encoder.field(b"salicin.incremental-input");
    encoder.u32(INPUT_SCHEMA_VERSION);
    encoder.field(env!("CARGO_PKG_VERSION").as_bytes());
    encoder.field(std::env::consts::OS.as_bytes());
    encoder.field(std::env::consts::ARCH.as_bytes());
    match target {
        IncrementalTarget::Binary => encoder.field(b"binary"),
        IncrementalTarget::Library => encoder.field(b"library"),
        IncrementalTarget::Test { filter } => {
            encoder.field(b"test");
            encoder.boolean(filter.is_some());
            if let Some(filter) = filter {
                encoder.field(filter.as_bytes());
            }
        }
    }
    encoder.field(edition.as_str().as_bytes());
    encode_source_bundle(
        &mut encoder,
        b"core",
        crate::core::incremental_sources(edition),
    );
    encode_source_bundle(
        &mut encoder,
        b"alloc",
        crate::alloc::incremental_sources(edition),
    );
    encode_source_bundle(
        &mut encoder,
        b"std",
        crate::standard::incremental_sources(edition),
    );

    encoder.usize(ordered.len());
    for package in ordered {
        encoder.field(package.identity.as_bytes());
        encoder.field(package.name.as_bytes());
        encoder.field(package.version.as_bytes());
        encoder.boolean(package.is_primary);

        let dependencies = package
            .dependencies
            .iter()
            .map(|(alias, id)| {
                let identity = identities
                    .get(id)
                    .expect("validated dependency target must have an identity");
                (alias.as_str(), *identity)
            })
            .collect::<BTreeMap<_, _>>();
        encoder.usize(dependencies.len());
        for (alias, identity) in dependencies {
            encoder.field(alias.as_bytes());
            encoder.field(identity.as_bytes());
        }

        let mut sources = package.sources.iter().collect::<Vec<_>>();
        sources.sort_by(|left, right| {
            left.module_path
                .cmp(&right.module_path)
                .then_with(|| left.is_root.cmp(&right.is_root))
                .then_with(|| left.source.as_bytes().cmp(right.source.as_bytes()))
        });
        encoder.usize(sources.len());
        for source in sources {
            encoder.boolean(source.is_root);
            encoder.usize(source.module_path.len());
            for segment in &source.module_path {
                encoder.field(segment.as_bytes());
            }
            encoder.field(source.source.as_bytes());
        }
    }
    Ok(IncrementalFingerprint(encoder.finish()))
}

fn validate_graph(packages: &[SourcePackage]) -> Result<(), String> {
    if packages.iter().filter(|package| package.is_primary).count() != 1 {
        return Err("incremental input requires exactly one primary package".into());
    }
    let ids = packages
        .iter()
        .map(|package| package.id)
        .collect::<HashSet<_>>();
    if ids.len() != packages.len() {
        return Err("incremental input contains duplicate package IDs".into());
    }
    let identities = packages
        .iter()
        .map(|package| package.identity.as_str())
        .collect::<HashSet<_>>();
    if identities.len() != packages.len() {
        return Err("incremental input contains duplicate provider identities".into());
    }
    for package in packages {
        for dependency in package.dependencies.values() {
            if !ids.contains(dependency) {
                return Err(format!(
                    "incremental input package `{}` refers to missing package ID {}",
                    package.identity, dependency.0
                ));
            }
        }
    }
    Ok(())
}

fn encode_source_bundle(
    encoder: &mut FingerprintEncoder,
    name: &[u8],
    sources: impl Iterator<Item = (&'static str, &'static str)>,
) {
    let sources = sources.collect::<Vec<_>>();
    encoder.field(name);
    encoder.usize(sources.len());
    for (module, source) in sources {
        encoder.field(module.as_bytes());
        encoder.field(source.as_bytes());
    }
}

struct FingerprintEncoder(Sha256);

impl FingerprintEncoder {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn field(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    fn u32(&mut self, value: u32) {
        self.0.update(value.to_le_bytes());
    }

    fn usize(&mut self, value: usize) {
        self.0.update((value as u64).to_le_bytes());
    }

    fn boolean(&mut self, value: bool) {
        self.0.update([u8::from(value)]);
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{PackageId, SourceUnit};

    fn package(
        id: usize,
        identity: &str,
        primary: bool,
        dependencies: &[(&str, usize)],
        path: &str,
        module_path: &[&str],
        source: &str,
    ) -> SourcePackage {
        SourcePackage {
            id: PackageId(id),
            name: identity
                .rsplit_once('|')
                .map_or(identity, |(_, declaration)| declaration)
                .split('@')
                .next()
                .unwrap_or(identity)
                .to_owned(),
            version: "1.0.0".to_owned(),
            identity: identity.to_owned(),
            is_primary: primary,
            dependencies: dependencies
                .iter()
                .map(|(alias, id)| ((*alias).to_owned(), PackageId(*id)))
                .collect(),
            sources: vec![SourceUnit {
                path: path.to_owned(),
                module_path: module_path
                    .iter()
                    .map(|segment| (*segment).to_owned())
                    .collect(),
                source: source.to_owned(),
                is_root: module_path.is_empty(),
            }],
        }
    }

    fn graph() -> Vec<SourcePackage> {
        vec![
            package(
                4,
                "workspace:app|app@1.0.0",
                true,
                &[("math", 9)],
                "/checkout/app/src/main.sc",
                &[],
                "let main(): i32 = { math.answer() }\n",
            ),
            package(
                9,
                "workspace:math|math@1.0.0",
                false,
                &[],
                "/checkout/math/src/lib.sc",
                &[],
                "pub let answer(): i32 = { 42 }\n",
            ),
        ]
    }

    #[test]
    fn ignores_graph_ids_order_and_absolute_source_paths() {
        let first =
            fingerprint_package_graph(&graph(), Edition::Edition2026, IncrementalTarget::Binary)
                .unwrap();
        let mut reordered = graph();
        reordered.reverse();
        reordered[0].id = PackageId(2);
        reordered[1].id = PackageId(7);
        reordered[1]
            .dependencies
            .insert("math".into(), PackageId(2));
        reordered[0].sources[0].path = "/different/root/math/src/lib.sc".into();
        reordered[1].sources[0].path = "/different/root/app/src/main.sc".into();
        let second =
            fingerprint_package_graph(&reordered, Edition::Edition2026, IncrementalTarget::Binary)
                .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.to_hex().len(), 64);
    }

    #[test]
    fn invalidates_on_semantic_graph_source_and_target_changes() {
        let baseline =
            fingerprint_package_graph(&graph(), Edition::Edition2026, IncrementalTarget::Binary)
                .unwrap();
        let mut cases = Vec::new();
        let mut source = graph();
        source[1].sources[0].source = "pub let answer(): i32 = { 43 }\n".into();
        cases.push((source, IncrementalTarget::Binary));
        let mut alias = graph();
        alias[0].dependencies = [("arithmetic".into(), PackageId(9))].into();
        cases.push((alias, IncrementalTarget::Binary));
        let mut provider = graph();
        provider[1].identity = "registry:public|math@1.0.0".into();
        cases.push((provider, IncrementalTarget::Binary));
        cases.push((graph(), IncrementalTarget::Library));

        for (packages, target) in cases {
            assert_ne!(
                baseline,
                fingerprint_package_graph(&packages, Edition::Edition2026, target).unwrap()
            );
        }
    }

    #[test]
    fn test_filter_is_part_of_test_compilation_identity() {
        let all = fingerprint_package_graph(
            &graph(),
            Edition::Edition2026,
            IncrementalTarget::Test { filter: None },
        )
        .unwrap();
        let selected = fingerprint_package_graph(
            &graph(),
            Edition::Edition2026,
            IncrementalTarget::Test {
                filter: Some("arithmetic"),
            },
        )
        .unwrap();
        let same_selection = fingerprint_package_graph(
            &graph(),
            Edition::Edition2026,
            IncrementalTarget::Test {
                filter: Some("arithmetic"),
            },
        )
        .unwrap();
        let empty_filter = fingerprint_package_graph(
            &graph(),
            Edition::Edition2026,
            IncrementalTarget::Test { filter: Some("") },
        )
        .unwrap();

        assert_ne!(all, selected);
        assert_eq!(selected, same_selection);
        assert_ne!(all, empty_filter);
    }

    #[test]
    fn cache_entry_mapping_is_sharded_and_output_path_independent() {
        let fingerprint =
            fingerprint_package_graph(&graph(), Edition::Edition2026, IncrementalTarget::Binary)
                .unwrap();
        let hex = fingerprint.to_hex();
        assert_eq!(
            cache_entry_relative_path(fingerprint),
            PathBuf::from("llvm-ir")
                .join("v1")
                .join("sha256")
                .join(&hex[..2])
                .join(&hex[2..])
        );
        assert_eq!(hex.len(), 64);
        assert!(!cache_entry_relative_path(fingerprint).is_absolute());
    }

    #[test]
    fn rejects_ambiguous_or_incomplete_graphs() {
        let mut no_primary = graph();
        no_primary[0].is_primary = false;
        assert!(fingerprint_package_graph(
            &no_primary,
            Edition::Edition2026,
            IncrementalTarget::Binary
        )
        .unwrap_err()
        .contains("exactly one primary"));

        let mut missing = graph();
        missing[0]
            .dependencies
            .insert("missing".into(), PackageId(99));
        assert!(fingerprint_package_graph(
            &missing,
            Edition::Edition2026,
            IncrementalTarget::Binary
        )
        .unwrap_err()
        .contains("missing package ID"));
    }
}
