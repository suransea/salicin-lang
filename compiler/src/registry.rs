//! Validated input protocol for immutable source-package registries.
//!
//! This module owns PKG-1 only: configuration, snapshot metadata, archive
//! identity, and deterministic local-fixture loading. It performs no version
//! selection, network access, archive extraction, or cache publication.

use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REGISTRY_CONFIG_FILE_NAME: &str = "salicin-registries.toml";
pub const REGISTRY_CONFIG_FORMAT: u32 = 1;
pub const REGISTRY_SNAPSHOT_FORMAT: u32 = 1;

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

fn validate_registry_name(name: &str) -> Result<(), RegistryError> {
    if !is_kebab_name(name) || name.len() > 64 {
        return Err(RegistryError::Invalid(format!(
            "registry identity `{name}` must be normalized ASCII kebab-case with at most 64 bytes"
        )));
    }
    Ok(())
}

fn validate_package_name(name: &str) -> Result<(), RegistryError> {
    if !is_kebab_name(name) {
        return Err(RegistryError::Invalid(format!(
            "registry package name `{name}` must be ASCII kebab-case"
        )));
    }
    Ok(())
}

fn validate_alias(alias: &str) -> Result<(), RegistryError> {
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
