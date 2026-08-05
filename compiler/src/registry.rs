//! Validated input protocol for immutable source-package registries.
//!
//! This module owns registry input validation and PKG-2 provider selection.
//! It performs no network access, archive extraction, or cache publication.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manifest::DependencyGraph;
use crate::manifest::RegistryDependency;

pub const REGISTRY_CONFIG_FILE_NAME: &str = "salicin-registries.toml";
pub const REGISTRY_CONFIG_FORMAT: u32 = 1;
pub const REGISTRY_SNAPSHOT_FORMAT: u32 = 1;
pub const REGISTRY_RESOLUTION_ATTEMPT_LIMIT: usize = 100_000;

/// Validated registry configuration. Registry names are stable provider
/// identities; changing an endpoint never changes a locked identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryConfig {
    pub registries: BTreeMap<String, RegistryEndpoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryEndpoint {
    /// An HTTPS registry root. PKG-1 validates but never accesses it.
    Https(String),
    /// A fixture root relative to the configuration file. Absolute paths and
    /// escaping `..` components are rejected so fixtures remain relocatable.
    LocalFixture(PathBuf),
}

/// Exact SHA-256 digest of input bytes, rendered as 64 lowercase hex digits.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: &str) -> Result<Self, RegistryError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RegistryError::Invalid(format!(
                "SHA-256 digest `{value}` must contain exactly 64 lowercase hexadecimal digits"
            )));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One validated immutable index snapshot. `digest` owns the exact UTF-8 JSON
/// bytes, while `registry` prevents a snapshot from being replayed under a
/// different provider identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrySnapshot {
    pub digest: Sha256Digest,
    pub registry: String,
    pub packages: Vec<RegistryPackage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryPackage {
    pub name: String,
    pub releases: Vec<RegistryRelease>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryRelease {
    pub version: Version,
    pub yanked: bool,
    /// SHA-256 of the compressed `.tar.gz` bytes, owned by the index entry.
    pub archive_sha256: Sha256Digest,
    /// Portable path relative to the registry root.
    pub archive: String,
    pub dependencies: Vec<RegistryRequirement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryRequirement {
    pub alias: String,
    pub package: String,
    pub registry: String,
    pub requirement: VersionReq,
}

/// One registry request owned by a local package manifest. Keeping the owner
/// path structural lets lockfile generation attach the selected provider to
/// the correct local package without using traversal order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRegistryRequirement {
    pub owner_manifest_path: PathBuf,
    pub dependency: RegistryDependency,
}

pub fn registry_requirements_from_graph(graph: &DependencyGraph) -> Vec<RootRegistryRequirement> {
    let mut requirements = graph
        .packages
        .iter()
        .flat_map(|manifest| {
            manifest
                .registry_dependencies
                .iter()
                .cloned()
                .map(|dependency| RootRegistryRequirement {
                    owner_manifest_path: manifest.manifest_path.clone(),
                    dependency,
                })
        })
        .collect::<Vec<_>>();
    requirements.sort_by(|left, right| {
        left.owner_manifest_path
            .cmp(&right.owner_manifest_path)
            .then_with(|| left.dependency.alias.cmp(&right.dependency.alias))
    });
    requirements
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegistryPackageKey {
    pub registry: String,
    pub name: String,
}

/// Exact immutable provider identity recorded in a lockfile.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegistryProviderIdentity {
    pub registry: String,
    pub snapshot: Sha256Digest,
    pub name: String,
    pub version: Version,
    pub archive_sha256: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRegistryDependency {
    pub alias: String,
    pub provider: RegistryProviderIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRegistryPackage {
    pub provider: RegistryProviderIdentity,
    pub dependencies: Vec<ResolvedRegistryDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRootRegistryDependency {
    pub owner_manifest_path: PathBuf,
    pub dependency: ResolvedRegistryDependency,
}

/// One deterministic, transitively complete registry provider graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryResolution {
    pub snapshots: Vec<(String, Sha256Digest)>,
    pub packages: Vec<ResolvedRegistryPackage>,
    pub roots: Vec<ResolvedRootRegistryDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryResolutionError {
    DuplicateSnapshot(String),
    MissingSnapshot(String),
    MissingPackage(RegistryPackageKey),
    NoCompatibleVersion {
        package: RegistryPackageKey,
        requirements: Vec<String>,
    },
    DependencyCycle(Vec<RegistryPackageKey>),
    SearchLimitExceeded(usize),
}

impl fmt::Display for RegistryResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateSnapshot(registry) => write!(
                formatter,
                "registry `{registry}` was supplied more than one index snapshot"
            ),
            Self::MissingSnapshot(registry) => {
                write!(
                    formatter,
                    "registry `{registry}` has no immutable index snapshot"
                )
            }
            Self::MissingPackage(package) => write!(
                formatter,
                "registry `{}` snapshot does not contain package `{}`",
                package.registry, package.name
            ),
            Self::NoCompatibleVersion {
                package,
                requirements,
            } => write!(
                formatter,
                "registry package `{}/{}` has no non-yanked version satisfying {}",
                package.registry,
                package.name,
                requirements.join(", ")
            ),
            Self::DependencyCycle(cycle) => {
                let cycle = cycle
                    .iter()
                    .map(|package| format!("{}/{}", package.registry, package.name))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                write!(formatter, "registry dependency cycle detected: {cycle}")
            }
            Self::SearchLimitExceeded(limit) => write!(
                formatter,
                "registry resolution exceeded its deterministic limit of {limit} candidate attempts"
            ),
        }
    }
}

impl Error for RegistryResolutionError {}

#[derive(Clone, Debug)]
struct Constraint {
    requirement: VersionReq,
    origin: String,
}

/// Resolve all roots against exactly one already-validated snapshot per
/// registry. The result is independent of root/snapshot input order.
///
/// A yanked release participates only when `previously_locked` contains its
/// complete provider identity. It is not preferred over a higher compatible
/// non-yanked release; the exception only keeps an otherwise unavailable
/// exact provider eligible.
pub fn resolve_registry_dependencies(
    roots: &[RootRegistryRequirement],
    snapshots: &[RegistrySnapshot],
    previously_locked: &BTreeSet<RegistryProviderIdentity>,
) -> Result<RegistryResolution, RegistryResolutionError> {
    let mut snapshots_by_registry = BTreeMap::new();
    for snapshot in snapshots {
        if snapshots_by_registry
            .insert(snapshot.registry.clone(), snapshot)
            .is_some()
        {
            return Err(RegistryResolutionError::DuplicateSnapshot(
                snapshot.registry.clone(),
            ));
        }
    }
    let snapshots = snapshots_by_registry;
    let mut constraints: BTreeMap<RegistryPackageKey, Vec<Constraint>> = BTreeMap::new();
    let mut ordered_roots = roots.to_vec();
    ordered_roots.sort_by(|left, right| {
        left.owner_manifest_path
            .cmp(&right.owner_manifest_path)
            .then_with(|| left.dependency.alias.cmp(&right.dependency.alias))
            .then_with(|| left.dependency.registry.cmp(&right.dependency.registry))
            .then_with(|| left.dependency.package.cmp(&right.dependency.package))
    });
    for root in &ordered_roots {
        if !snapshots.contains_key(&root.dependency.registry) {
            return Err(RegistryResolutionError::MissingSnapshot(
                root.dependency.registry.clone(),
            ));
        }
        constraints
            .entry(RegistryPackageKey {
                registry: root.dependency.registry.clone(),
                name: root.dependency.package.clone(),
            })
            .or_default()
            .push(Constraint {
                requirement: root.dependency.requirement.clone(),
                origin: format!(
                    "{} dependency `{}` requires `{}`",
                    root.owner_manifest_path.display(),
                    root.dependency.alias,
                    root.dependency.requirement
                ),
            });
    }

    let mut attempts = 0;
    let selected = solve_registry_constraints(
        &constraints,
        &BTreeMap::new(),
        &snapshots,
        previously_locked,
        &mut attempts,
    )?;

    let identities = selected
        .iter()
        .map(|(key, release)| {
            let snapshot = snapshots[&key.registry];
            (
                key.clone(),
                provider_identity(key, release, &snapshot.digest),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let packages = selected
        .iter()
        .map(|(key, release)| ResolvedRegistryPackage {
            provider: identities[key].clone(),
            dependencies: release
                .dependencies
                .iter()
                .map(|dependency| ResolvedRegistryDependency {
                    alias: dependency.alias.clone(),
                    provider: identities[&RegistryPackageKey {
                        registry: dependency.registry.clone(),
                        name: dependency.package.clone(),
                    }]
                        .clone(),
                })
                .collect(),
        })
        .collect();
    let roots = ordered_roots
        .into_iter()
        .map(|root| ResolvedRootRegistryDependency {
            owner_manifest_path: root.owner_manifest_path,
            dependency: ResolvedRegistryDependency {
                alias: root.dependency.alias,
                provider: identities[&RegistryPackageKey {
                    registry: root.dependency.registry,
                    name: root.dependency.package,
                }]
                    .clone(),
            },
        })
        .collect();
    let used_registries = identities
        .keys()
        .map(|key| key.registry.as_str())
        .collect::<BTreeSet<_>>();
    let snapshot_identities = snapshots
        .into_iter()
        .filter(|(registry, _)| used_registries.contains(registry.as_str()))
        .map(|(registry, snapshot)| (registry, snapshot.digest.clone()))
        .collect();
    Ok(RegistryResolution {
        snapshots: snapshot_identities,
        packages,
        roots,
    })
}

fn solve_registry_constraints<'a>(
    constraints: &BTreeMap<RegistryPackageKey, Vec<Constraint>>,
    selected: &BTreeMap<RegistryPackageKey, &'a RegistryRelease>,
    snapshots: &BTreeMap<String, &'a RegistrySnapshot>,
    previously_locked: &BTreeSet<RegistryProviderIdentity>,
    attempts: &mut usize,
) -> Result<BTreeMap<RegistryPackageKey, &'a RegistryRelease>, RegistryResolutionError> {
    for (key, release) in selected {
        if !constraints[key]
            .iter()
            .all(|constraint| constraint.requirement.matches(&release.version))
        {
            return Err(no_compatible_error(key, &constraints[key]));
        }
    }

    let Some(key) = constraints.keys().find(|key| !selected.contains_key(*key)) else {
        reject_registry_cycles(selected)?;
        return Ok(selected.clone());
    };
    let snapshot = snapshots
        .get(&key.registry)
        .ok_or_else(|| RegistryResolutionError::MissingSnapshot(key.registry.clone()))?;
    let package = snapshot
        .packages
        .iter()
        .find(|package| package.name == key.name)
        .ok_or_else(|| RegistryResolutionError::MissingPackage(key.clone()))?;
    let requirements = &constraints[key];
    let mut final_error = None;
    for release in package.releases.iter().rev() {
        if !requirements
            .iter()
            .all(|constraint| constraint.requirement.matches(&release.version))
        {
            continue;
        }
        let identity = provider_identity(key, release, &snapshot.digest);
        if release.yanked && !previously_locked.contains(&identity) {
            continue;
        }
        *attempts += 1;
        if *attempts > REGISTRY_RESOLUTION_ATTEMPT_LIMIT {
            return Err(RegistryResolutionError::SearchLimitExceeded(
                REGISTRY_RESOLUTION_ATTEMPT_LIMIT,
            ));
        }
        let mut next_selected = selected.clone();
        next_selected.insert(key.clone(), release);
        let mut next_constraints = constraints.clone();
        let mut missing_snapshot = None;
        for dependency in &release.dependencies {
            if !snapshots.contains_key(&dependency.registry) {
                missing_snapshot = Some(RegistryResolutionError::MissingSnapshot(
                    dependency.registry.clone(),
                ));
                break;
            }
            next_constraints
                .entry(RegistryPackageKey {
                    registry: dependency.registry.clone(),
                    name: dependency.package.clone(),
                })
                .or_default()
                .push(Constraint {
                    requirement: dependency.requirement.clone(),
                    origin: format!(
                        "{}/{}@{} dependency `{}` requires `{}`",
                        key.registry,
                        key.name,
                        release.version,
                        dependency.alias,
                        dependency.requirement
                    ),
                });
        }
        if let Some(error) = missing_snapshot {
            final_error = Some(error);
            continue;
        }
        match solve_registry_constraints(
            &next_constraints,
            &next_selected,
            snapshots,
            previously_locked,
            attempts,
        ) {
            Ok(solution) => return Ok(solution),
            Err(error @ RegistryResolutionError::SearchLimitExceeded(_)) => return Err(error),
            Err(error) => final_error = Some(error),
        }
    }
    Err(final_error.unwrap_or_else(|| no_compatible_error(key, requirements)))
}

fn provider_identity(
    key: &RegistryPackageKey,
    release: &RegistryRelease,
    snapshot: &Sha256Digest,
) -> RegistryProviderIdentity {
    RegistryProviderIdentity {
        registry: key.registry.clone(),
        snapshot: snapshot.clone(),
        name: key.name.clone(),
        version: release.version.clone(),
        archive_sha256: release.archive_sha256.clone(),
    }
}

fn no_compatible_error(
    key: &RegistryPackageKey,
    constraints: &[Constraint],
) -> RegistryResolutionError {
    let mut requirements = constraints
        .iter()
        .map(|constraint| constraint.origin.clone())
        .collect::<Vec<_>>();
    requirements.sort();
    requirements.dedup();
    RegistryResolutionError::NoCompatibleVersion {
        package: key.clone(),
        requirements,
    }
}

fn reject_registry_cycles(
    selected: &BTreeMap<RegistryPackageKey, &RegistryRelease>,
) -> Result<(), RegistryResolutionError> {
    fn visit(
        key: &RegistryPackageKey,
        selected: &BTreeMap<RegistryPackageKey, &RegistryRelease>,
        complete: &mut BTreeSet<RegistryPackageKey>,
        stack: &mut Vec<RegistryPackageKey>,
    ) -> Result<(), RegistryResolutionError> {
        if let Some(start) = stack.iter().position(|entry| entry == key) {
            let mut cycle = stack[start..].to_vec();
            cycle.push(key.clone());
            return Err(RegistryResolutionError::DependencyCycle(cycle));
        }
        if complete.contains(key) {
            return Ok(());
        }
        stack.push(key.clone());
        for dependency in &selected[key].dependencies {
            visit(
                &RegistryPackageKey {
                    registry: dependency.registry.clone(),
                    name: dependency.package.clone(),
                },
                selected,
                complete,
                stack,
            )?;
        }
        stack.pop();
        complete.insert(key.clone());
        Ok(())
    }

    let mut complete = BTreeSet::new();
    for key in selected.keys() {
        visit(key, selected, &mut complete, &mut Vec::new())?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum RegistryError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse(String),
    Invalid(String),
    DigestMismatch {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "could not read `{}`: {source}", path.display())
            }
            Self::Parse(message) | Self::Invalid(message) => formatter.write_str(message),
            Self::DigestMismatch { expected, actual } => write!(
                formatter,
                "registry snapshot checksum mismatch: expected {expected}, found {actual}"
            ),
        }
    }
}

impl Error for RegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Load strict registry configuration. This is the only PKG-1 operation that
/// interprets local filesystem endpoints; no endpoint is contacted.
pub fn load_registry_config(path: impl AsRef<Path>) -> Result<RegistryConfig, RegistryError> {
    let path = path.as_ref();
    let source = fs::read_to_string(path).map_err(|source| RegistryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let raw: RawConfig = toml::from_str(&source).map_err(|error| {
        RegistryError::Parse(format!("could not parse registry config: {error}"))
    })?;
    if raw.format != REGISTRY_CONFIG_FORMAT {
        return Err(RegistryError::Invalid(format!(
            "unsupported registry config format {}; expected {}",
            raw.format, REGISTRY_CONFIG_FORMAT
        )));
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut registries = BTreeMap::new();
    for (name, entry) in raw.registries {
        validate_registry_name(&name)?;
        let endpoint = match (entry.url, entry.fixture) {
            (Some(url), None) => {
                validate_https_root(&url)?;
                RegistryEndpoint::Https(url)
            }
            (None, Some(relative)) => {
                validate_fixture_path(&relative)?;
                RegistryEndpoint::LocalFixture(base.join(relative))
            }
            _ => {
                return Err(RegistryError::Invalid(format!(
                    "registry `{name}` must declare exactly one of `url` or `fixture`"
                )))
            }
        };
        registries.insert(name, endpoint);
    }
    Ok(RegistryConfig { registries })
}

/// Parse and validate one exact snapshot. The expected digest is checked before
/// JSON parsing so corrupt bytes never become package metadata.
pub fn parse_registry_snapshot(
    bytes: &[u8],
    expected_digest: &Sha256Digest,
    expected_registry: &str,
) -> Result<RegistrySnapshot, RegistryError> {
    let actual = Sha256Digest::of(bytes);
    if &actual != expected_digest {
        return Err(RegistryError::DigestMismatch {
            expected: expected_digest.clone(),
            actual,
        });
    }
    validate_registry_name(expected_registry)?;
    let raw: RawSnapshot = serde_json::from_slice(bytes).map_err(|error| {
        RegistryError::Parse(format!("could not parse registry snapshot: {error}"))
    })?;
    if raw.format != REGISTRY_SNAPSHOT_FORMAT {
        return Err(RegistryError::Invalid(format!(
            "unsupported registry snapshot format {}; expected {}",
            raw.format, REGISTRY_SNAPSHOT_FORMAT
        )));
    }
    if raw.registry != expected_registry {
        return Err(RegistryError::Invalid(format!(
            "registry snapshot identity `{}` does not match configured registry `{expected_registry}`",
            raw.registry
        )));
    }

    let mut packages = Vec::with_capacity(raw.packages.len());
    let mut previous_package: Option<String> = None;
    for package in raw.packages {
        validate_package_name(&package.name)?;
        if previous_package
            .as_deref()
            .is_some_and(|name| name >= package.name.as_str())
        {
            return Err(RegistryError::Invalid(
                "registry snapshot packages must be strictly sorted by name".into(),
            ));
        }
        previous_package = Some(package.name.clone());
        let mut releases = Vec::with_capacity(package.releases.len());
        let mut seen_versions = HashSet::new();
        let mut previous_version: Option<Version> = None;
        for release in package.releases {
            let version = parse_exact_version(&package.name, &release.version)?;
            let precedence = version.clone();
            if !seen_versions.insert((
                version.major,
                version.minor,
                version.patch,
                version.pre.clone(),
            )) {
                return Err(RegistryError::Invalid(format!(
                    "registry package `{}` repeats semantic version precedence `{version}`",
                    package.name
                )));
            }
            if previous_version
                .as_ref()
                .is_some_and(|prior| prior >= &precedence)
            {
                return Err(RegistryError::Invalid(format!(
                    "registry package `{}` releases must be strictly sorted by version",
                    package.name
                )));
            }
            previous_version = Some(precedence);
            let archive_sha256 = Sha256Digest::parse(&release.archive_sha256)?;
            validate_archive_path(&release.archive, &package.name, &version)?;
            let mut dependencies = Vec::with_capacity(release.dependencies.len());
            let mut previous_alias: Option<String> = None;
            for dependency in release.dependencies {
                validate_alias(&dependency.alias)?;
                validate_package_name(&dependency.package)?;
                validate_registry_name(&dependency.registry)?;
                if previous_alias
                    .as_deref()
                    .is_some_and(|alias| alias >= dependency.alias.as_str())
                {
                    return Err(RegistryError::Invalid(format!(
                        "dependencies of `{}@{version}` must be strictly sorted by alias",
                        package.name
                    )));
                }
                previous_alias = Some(dependency.alias.clone());
                let requirement = VersionReq::parse(&dependency.version).map_err(|error| {
                    RegistryError::Invalid(format!(
                        "dependency `{}` of `{}@{version}` has invalid version requirement `{}`: {error}",
                        dependency.alias, package.name, dependency.version
                    ))
                })?;
                dependencies.push(RegistryRequirement {
                    alias: dependency.alias,
                    package: dependency.package,
                    registry: dependency.registry,
                    requirement,
                });
            }
            releases.push(RegistryRelease {
                version,
                yanked: release.yanked,
                archive_sha256,
                archive: release.archive,
                dependencies,
            });
        }
        packages.push(RegistryPackage {
            name: package.name,
            releases,
        });
    }
    Ok(RegistrySnapshot {
        digest: expected_digest.clone(),
        registry: expected_registry.to_owned(),
        packages,
    })
}

/// Load the fixture snapshot at the protocol-defined content address:
/// `snapshots/sha256/<digest>.json`.
pub fn load_fixture_snapshot(
    fixture_root: &Path,
    digest: &Sha256Digest,
    registry: &str,
) -> Result<RegistrySnapshot, RegistryError> {
    let path = fixture_root
        .join("snapshots")
        .join("sha256")
        .join(format!("{digest}.json"));
    let bytes = fs::read(&path).map_err(|source| RegistryError::Io { path, source })?;
    parse_registry_snapshot(&bytes, digest, registry)
}

/// Platform cache root. `SALICIN_CACHE_HOME` is an explicit override; defaults
/// contain no registry URL or mutable registry name because all blobs are
/// addressed by checksum.
pub fn registry_cache_root() -> Result<PathBuf, RegistryError> {
    if let Some(root) = std::env::var_os("SALICIN_CACHE_HOME") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(RegistryError::Invalid(
                "SALICIN_CACHE_HOME must be an absolute path".into(),
            ));
        }
        return Ok(root.join("registry-v1"));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| RegistryError::Invalid("cannot determine the user cache root".into()))?;
    #[cfg(target_os = "macos")]
    let root = home.join("Library/Caches/salicin");
    #[cfg(not(target_os = "macos"))]
    let root = match std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from) {
        Some(root) if root.is_absolute() => root.join("salicin"),
        Some(_) => {
            return Err(RegistryError::Invalid(
                "XDG_CACHE_HOME must be an absolute path".into(),
            ))
        }
        None => home.join(".cache/salicin"),
    };
    Ok(root.join("registry-v1"))
}

pub fn snapshot_cache_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
    root.join("index")
        .join("sha256")
        .join(format!("{digest}.json"))
}

pub fn archive_cache_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
    root.join("archives")
        .join("sha256")
        .join(format!("{digest}.tar.gz"))
}

pub fn source_cache_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
    root.join("sources").join("sha256").join(digest.as_str())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    format: u32,
    #[serde(default)]
    registries: BTreeMap<String, RawRegistryEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegistryEntry {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    fixture: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshot {
    format: u32,
    registry: String,
    packages: Vec<RawPackage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    name: String,
    releases: Vec<RawRelease>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawRelease {
    version: String,
    #[serde(default)]
    yanked: bool,
    archive_sha256: String,
    archive: String,
    #[serde(default)]
    dependencies: Vec<RawRequirement>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawRequirement {
    alias: String,
    package: String,
    registry: String,
    version: String,
}

fn validate_https_root(url: &str) -> Result<(), RegistryError> {
    let tail = url
        .strip_prefix("https://")
        .ok_or_else(|| RegistryError::Invalid(format!("registry URL `{url}` must use HTTPS")))?;
    if tail.is_empty() || tail.contains(['#', '?']) || url.ends_with('/') {
        return Err(RegistryError::Invalid(format!(
            "registry URL `{url}` must be an HTTPS root without query, fragment, or trailing slash"
        )));
    }
    Ok(())
}

fn validate_fixture_path(path: &Path) -> Result<(), RegistryError> {
    let text = path.to_str().unwrap_or("");
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || text.contains(['\\', ':'])
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RegistryError::Invalid(format!(
            "registry fixture path `{}` must be a non-empty portable child path",
            path.display()
        )));
    }
    Ok(())
}

fn validate_archive_path(
    path: &str,
    package: &str,
    version: &Version,
) -> Result<(), RegistryError> {
    let expected = format!("archives/{package}/{version}/{package}-{version}.tar.gz");
    if path != expected {
        return Err(RegistryError::Invalid(format!(
            "archive path `{path}` must be exactly `{expected}`"
        )));
    }
    Ok(())
}

fn parse_exact_version(package: &str, value: &str) -> Result<Version, RegistryError> {
    let version = Version::parse(value).map_err(|error| {
        RegistryError::Invalid(format!(
            "registry package `{package}` has invalid version `{value}`: {error}"
        ))
    })?;
    if !version.build.is_empty() {
        return Err(RegistryError::Invalid(format!(
            "registry package `{package}` version `{value}` must not contain build metadata"
        )));
    }
    Ok(version)
}

pub(crate) fn validate_registry_name(name: &str) -> Result<(), RegistryError> {
    if !is_kebab_name(name) || name.len() > 64 {
        return Err(RegistryError::Invalid(format!(
            "registry identity `{name}` must be normalized ASCII kebab-case with at most 64 bytes"
        )));
    }
    Ok(())
}

pub(crate) fn validate_package_name(name: &str) -> Result<(), RegistryError> {
    if !is_kebab_name(name) {
        return Err(RegistryError::Invalid(format!(
            "registry package name `{name}` must be ASCII kebab-case"
        )));
    }
    Ok(())
}

pub(crate) fn validate_alias(alias: &str) -> Result<(), RegistryError> {
    let mut bytes = alias.bytes();
    let Some(first) = bytes.next() else {
        return Err(RegistryError::Invalid(
            "registry dependency alias must not be empty".into(),
        ));
    };
    if !(first.is_ascii_lowercase() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || matches!(alias, "_" | "self" | "core" | "alloc")
        || crate::lexer::is_keyword(alias)
    {
        return Err(RegistryError::Invalid(format!(
            "registry dependency alias `{alias}` must be ASCII snake_case"
        )));
    }
    Ok(())
}

fn is_kebab_name(name: &str) -> bool {
    let mut segments = name.split('-');
    let Some(first) = segments.next() else {
        return false;
    };
    if first.is_empty()
        || !first
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !first.as_bytes()[0].is_ascii_lowercase()
    {
        return false;
    }
    segments.all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "salicin-registry-contract-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn snapshot_bytes() -> Vec<u8> {
        br#"{"format":1,"registry":"local-test","packages":[{"name":"answer-kit","releases":[{"version":"1.2.3","archive_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","archive":"archives/answer-kit/1.2.3/answer-kit-1.2.3.tar.gz","dependencies":[{"alias":"base","package":"base-kit","registry":"local-test","version":"^1.0"}]}]}]}"#.to_vec()
    }

    fn digest(character: char) -> Sha256Digest {
        Sha256Digest::parse(&character.to_string().repeat(64)).unwrap()
    }

    fn requirement(alias: &str, package: &str, version: &str) -> RegistryRequirement {
        RegistryRequirement {
            alias: alias.into(),
            package: package.into(),
            registry: "community".into(),
            requirement: VersionReq::parse(version).unwrap(),
        }
    }

    fn release(
        version: &str,
        yanked: bool,
        dependencies: Vec<RegistryRequirement>,
    ) -> RegistryRelease {
        RegistryRelease {
            version: Version::parse(version).unwrap(),
            yanked,
            archive_sha256: digest(if version.starts_with('1') { '1' } else { '2' }),
            archive: format!("unused/{version}.tar.gz"),
            dependencies,
        }
    }

    fn root(alias: &str, package: &str, version: &str) -> RootRegistryRequirement {
        RootRegistryRequirement {
            owner_manifest_path: PathBuf::from("/workspace/salicin.toml"),
            dependency: RegistryDependency {
                alias: alias.into(),
                package: package.into(),
                registry: "community".into(),
                requirement: VersionReq::parse(version).unwrap(),
            },
        }
    }

    #[test]
    fn resolver_backtracks_to_the_highest_globally_compatible_graph() {
        let snapshot = RegistrySnapshot {
            digest: digest('a'),
            registry: "community".into(),
            packages: vec![
                RegistryPackage {
                    name: "app-kit".into(),
                    releases: vec![
                        release(
                            "1.0.0",
                            false,
                            vec![requirement("shared", "shared-kit", "^1")],
                        ),
                        release(
                            "2.0.0",
                            false,
                            vec![requirement("shared", "shared-kit", "^2")],
                        ),
                    ],
                },
                RegistryPackage {
                    name: "shared-kit".into(),
                    releases: vec![
                        release("1.5.0", false, vec![]),
                        release("2.5.0", false, vec![]),
                    ],
                },
            ],
        };
        let roots = vec![
            root("shared", "shared-kit", "<2"),
            root("app", "app-kit", ">=1"),
        ];
        let resolved = resolve_registry_dependencies(
            &roots,
            std::slice::from_ref(&snapshot),
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(
            resolved
                .packages
                .iter()
                .map(|package| (&package.provider.name, &package.provider.version))
                .collect::<Vec<_>>(),
            [
                (&"app-kit".to_owned(), &Version::new(1, 0, 0)),
                (&"shared-kit".to_owned(), &Version::new(1, 5, 0)),
            ]
        );

        let mut reversed_roots = roots;
        reversed_roots.reverse();
        assert_eq!(
            resolve_registry_dependencies(&reversed_roots, &[snapshot], &BTreeSet::new()).unwrap(),
            resolved
        );
    }

    #[test]
    fn resolver_admits_a_yanked_release_only_by_complete_locked_identity() {
        let snapshot = RegistrySnapshot {
            digest: digest('a'),
            registry: "community".into(),
            packages: vec![RegistryPackage {
                name: "answer-kit".into(),
                releases: vec![
                    release("1.0.0", false, vec![]),
                    release("2.0.0", true, vec![]),
                ],
            }],
        };
        let roots = [root("answer", "answer-kit", "=2.0.0")];
        let error = resolve_registry_dependencies(
            &roots,
            std::slice::from_ref(&snapshot),
            &BTreeSet::new(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RegistryResolutionError::NoCompatibleVersion { .. }
        ));

        let locked = RegistryProviderIdentity {
            registry: "community".into(),
            snapshot: snapshot.digest.clone(),
            name: "answer-kit".into(),
            version: Version::new(2, 0, 0),
            archive_sha256: digest('2'),
        };
        let resolved =
            resolve_registry_dependencies(&roots, &[snapshot], &BTreeSet::from([locked.clone()]))
                .unwrap();
        assert_eq!(resolved.packages[0].provider, locked);
    }

    #[test]
    fn resolver_reports_stable_conflicts_missing_snapshots_and_cycles() {
        let conflict = RegistrySnapshot {
            digest: digest('a'),
            registry: "community".into(),
            packages: vec![RegistryPackage {
                name: "answer-kit".into(),
                releases: vec![release("1.0.0", false, vec![])],
            }],
        };
        let roots = [
            root("old", "answer-kit", "<1"),
            root("new", "answer-kit", ">=2"),
        ];
        let error = resolve_registry_dependencies(&roots, &[conflict], &BTreeSet::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("answer-kit"), "{error}");
        assert!(error.contains("dependency `new`"), "{error}");
        assert!(error.contains("dependency `old`"), "{error}");

        let mut cross_registry = requirement("external", "external-kit", "^1");
        cross_registry.registry = "missing".into();
        let missing = RegistrySnapshot {
            digest: digest('a'),
            registry: "community".into(),
            packages: vec![RegistryPackage {
                name: "answer-kit".into(),
                releases: vec![release("1.0.0", false, vec![cross_registry])],
            }],
        };
        assert_eq!(
            resolve_registry_dependencies(
                &[root("answer", "answer-kit", "^1")],
                &[missing],
                &BTreeSet::new(),
            )
            .unwrap_err(),
            RegistryResolutionError::MissingSnapshot("missing".into())
        );

        let cycle = RegistrySnapshot {
            digest: digest('a'),
            registry: "community".into(),
            packages: vec![
                RegistryPackage {
                    name: "left-kit".into(),
                    releases: vec![release(
                        "1.0.0",
                        false,
                        vec![requirement("right", "right-kit", "^1")],
                    )],
                },
                RegistryPackage {
                    name: "right-kit".into(),
                    releases: vec![release(
                        "1.0.0",
                        false,
                        vec![requirement("left", "left-kit", "^1")],
                    )],
                },
            ],
        };
        assert!(matches!(
            resolve_registry_dependencies(
                &[root("left", "left-kit", "^1")],
                &[cycle],
                &BTreeSet::new(),
            ),
            Err(RegistryResolutionError::DependencyCycle(_))
        ));
    }

    #[test]
    fn configuration_separates_stable_identity_from_endpoint() {
        let root = temp_dir();
        let path = root.join(REGISTRY_CONFIG_FILE_NAME);
        fs::write(
            &path,
            "format = 1\n[registries.primary]\nurl = \"https://packages.example.test/v1\"\n[registries.local-test]\nfixture = \"fixtures/registry\"\n",
        )
        .unwrap();
        let config = load_registry_config(&path).unwrap();
        assert_eq!(
            config.registries["primary"],
            RegistryEndpoint::Https("https://packages.example.test/v1".into())
        );
        assert_eq!(
            config.registries["local-test"],
            RegistryEndpoint::LocalFixture(root.join("fixtures/registry"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_digest_is_checked_before_strict_metadata_validation() {
        let bytes = snapshot_bytes();
        let digest = Sha256Digest::of(&bytes);
        let snapshot = parse_registry_snapshot(&bytes, &digest, "local-test").unwrap();
        assert_eq!(snapshot.packages[0].name, "answer-kit");
        assert_eq!(
            snapshot.packages[0].releases[0].version,
            Version::new(1, 2, 3)
        );
        assert_eq!(
            snapshot.packages[0].releases[0].dependencies[0].alias,
            "base"
        );

        let wrong = Sha256Digest::parse(&"0".repeat(64)).unwrap();
        assert!(matches!(
            parse_registry_snapshot(&bytes, &wrong, "local-test"),
            Err(RegistryError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn fixture_protocol_loads_only_the_content_addressed_snapshot() {
        let root = temp_dir();
        let bytes = snapshot_bytes();
        let digest = Sha256Digest::of(&bytes);
        let path = root.join("snapshots/sha256").join(format!("{digest}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &bytes).unwrap();
        assert_eq!(
            load_fixture_snapshot(&root, &digest, "local-test")
                .unwrap()
                .digest,
            digest
        );
        assert_eq!(
            load_fixture_snapshot(&root, &digest, "local-test")
                .unwrap()
                .digest,
            digest,
            "a restarted client must address the same immutable bytes"
        );
        let path = root.join("snapshots/sha256").join(format!("{digest}.json"));
        fs::write(path, b"corrupt").unwrap();
        assert!(matches!(
            load_fixture_snapshot(&root, &digest, "local-test"),
            Err(RegistryError::DigestMismatch { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn strict_inputs_reject_ambiguous_endpoints_and_noncanonical_snapshots() {
        let root = temp_dir();
        let path = root.join(REGISTRY_CONFIG_FILE_NAME);
        fs::write(
            &path,
            "format = 1\n[registries.bad]\nurl = \"http://example.test\"\nfixture = \"../escape\"\n",
        )
        .unwrap();
        assert!(load_registry_config(&path)
            .unwrap_err()
            .to_string()
            .contains("exactly one"));

        let unsorted = br#"{"format":1,"registry":"local-test","packages":[{"name":"zeta","releases":[]},{"name":"alpha","releases":[]}]}"#;
        let digest = Sha256Digest::of(unsorted);
        assert!(parse_registry_snapshot(unsorted, &digest, "local-test")
            .unwrap_err()
            .to_string()
            .contains("strictly sorted"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_paths_are_content_addressed_and_source_independent() {
        let root = Path::new("/cache/salicin/registry-v1");
        let digest = Sha256Digest::parse(&"a".repeat(64)).unwrap();
        assert_eq!(
            snapshot_cache_path(root, &digest),
            root.join(format!("index/sha256/{digest}.json"))
        );
        assert_eq!(
            archive_cache_path(root, &digest),
            root.join(format!("archives/sha256/{digest}.tar.gz"))
        );
        assert_eq!(
            source_cache_path(root, &digest),
            root.join(format!("sources/sha256/{digest}"))
        );
    }
}
