//! Deterministic `salicin.lock` generation for local and registry providers.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as IoWrite};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::Deserialize;

use crate::manifest::{DependencyGraph, Edition};
use crate::registry::{
    validate_alias, validate_package_name, validate_registry_name, RegistryResolution, Sha256Digest,
};

pub const LOCKFILE_NAME: &str = "salicin.lock";
pub const LOCKFILE_FORMAT_VERSION: u32 = 3;

/// Deterministic lock data derived from a complete dependency graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lockfile {
    pub format: u32,
    pub registry_snapshots: Vec<LockedRegistrySnapshot>,
    pub registry_packages: Vec<LockedRegistryPackage>,
    pub packages: Vec<LockedPackage>,
}

impl Lockfile {
    /// Build sorted lock data. Package paths are relative to the root package
    /// whenever both paths share a filesystem root.
    pub fn from_graph(graph: &DependencyGraph) -> Self {
        Self::from_graph_root(graph, &graph.root().package_root)
    }

    /// Build sorted lock data relative to an explicit package or workspace root.
    pub fn from_graph_root(graph: &DependencyGraph, root: &Path) -> Self {
        Self::from_graph_root_with_workspace_members(graph, root, &HashSet::new())
    }

    pub fn from_graph_root_with_workspace_members(
        graph: &DependencyGraph,
        root: &Path,
        workspace_members: &HashSet<PathBuf>,
    ) -> Self {
        let mut packages: Vec<LockedPackage> = graph
            .packages
            .iter()
            .map(|manifest| {
                let mut dependencies: Vec<LockedDependency> = manifest
                    .dependencies
                    .iter()
                    .map(|dependency| LockedDependency {
                        alias: dependency.alias.clone(),
                        name: dependency.package.name.clone(),
                        version: dependency.package.version.clone(),
                        path: portable_relative_path(root, &dependency.path),
                        source: package_source(root, &dependency.path, workspace_members),
                    })
                    .collect();
                dependencies.sort_by(|left, right| {
                    left.alias
                        .cmp(&right.alias)
                        .then_with(|| left.name.cmp(&right.name))
                        .then_with(|| left.version.cmp(&right.version))
                        .then_with(|| left.path.cmp(&right.path))
                });
                LockedPackage {
                    name: manifest.package.name.clone(),
                    version: manifest.package.version.clone(),
                    edition: manifest.package.edition,
                    path: portable_relative_path(root, &manifest.package_root),
                    source: package_source(root, &manifest.package_root, workspace_members),
                    dependencies,
                    registry_dependencies: Vec::new(),
                }
            })
            .collect();
        packages.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.path.cmp(&right.path))
        });
        Self {
            format: LOCKFILE_FORMAT_VERSION,
            registry_snapshots: Vec::new(),
            registry_packages: Vec::new(),
            packages,
        }
    }

    /// Attach a previously solved registry graph without reading package
    /// archives. Every root edge is tied back to its owning local manifest and
    /// every transitive edge records the complete immutable provider identity.
    pub fn from_graph_root_with_registry(
        graph: &DependencyGraph,
        root: &Path,
        workspace_members: &HashSet<PathBuf>,
        resolution: &RegistryResolution,
    ) -> Result<Self, String> {
        let mut lockfile =
            Self::from_graph_root_with_workspace_members(graph, root, workspace_members);
        for root_dependency in &resolution.roots {
            let manifest = graph
                .package(&root_dependency.owner_manifest_path)
                .ok_or_else(|| {
                    format!(
                        "registry resolution references unknown local manifest `{}`",
                        root_dependency.owner_manifest_path.display()
                    )
                })?;
            let path = portable_relative_path(root, &manifest.package_root);
            let package = lockfile
                .packages
                .iter_mut()
                .find(|package| {
                    package.name == manifest.package.name
                        && package.version == manifest.package.version
                        && package.path == path
                })
                .expect("a lockfile contains every graph package");
            package
                .registry_dependencies
                .push(LockedRegistryDependency::from_resolved(
                    &root_dependency.dependency,
                ));
        }
        for package in &mut lockfile.packages {
            package
                .registry_dependencies
                .sort_by(LockedRegistryDependency::compare);
        }
        lockfile.registry_snapshots = resolution
            .snapshots
            .iter()
            .map(|(registry, checksum)| LockedRegistrySnapshot {
                registry: registry.clone(),
                checksum: checksum.clone(),
            })
            .collect();
        lockfile.registry_packages = resolution
            .packages
            .iter()
            .map(|package| LockedRegistryPackage {
                registry: package.provider.registry.clone(),
                name: package.provider.name.clone(),
                version: package.provider.version.clone(),
                snapshot: package.provider.snapshot.clone(),
                checksum: package.provider.archive_sha256.clone(),
                dependencies: package
                    .dependencies
                    .iter()
                    .map(LockedRegistryDependency::from_resolved)
                    .collect(),
            })
            .collect();
        lockfile.registry_packages.sort_by(|left, right| {
            left.registry
                .cmp(&right.registry)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.checksum.cmp(&right.checksum))
        });
        Ok(lockfile)
    }

    /// Render canonical UTF-8 TOML with stable ordering and whitespace.
    pub fn to_text(&self) -> String {
        let mut output = String::from(
            "# This file is automatically generated by salic.\n\
             # Do not edit it by hand.\n",
        );
        writeln!(output, "format = {}", self.format).expect("writing to a string cannot fail");

        for snapshot in &self.registry_snapshots {
            output.push_str("\n[[registry-snapshot]]\n");
            write_toml_field(&mut output, "registry", &snapshot.registry);
            write_toml_field(&mut output, "checksum", snapshot.checksum.as_str());
        }

        for package in &self.registry_packages {
            output.push_str("\n[[registry-package]]\n");
            write_toml_field(&mut output, "registry", &package.registry);
            write_toml_field(&mut output, "name", &package.name);
            write_toml_field(&mut output, "version", &package.version.to_string());
            write_toml_field(&mut output, "snapshot", package.snapshot.as_str());
            write_toml_field(&mut output, "checksum", package.checksum.as_str());

            for dependency in &package.dependencies {
                output.push_str("\n[[registry-package.dependencies]]\n");
                dependency.write_to(&mut output);
            }
        }

        for package in &self.packages {
            output.push_str("\n[[package]]\n");
            write_toml_field(&mut output, "name", &package.name);
            write_toml_field(&mut output, "version", &package.version.to_string());
            write_toml_field(&mut output, "edition", package.edition.as_str());
            write_toml_field(&mut output, "source", &package.source);
            write_toml_field(&mut output, "path", &package.path);

            for dependency in &package.dependencies {
                output.push_str("\n[[package.dependencies]]\n");
                write_toml_field(&mut output, "alias", &dependency.alias);
                write_toml_field(&mut output, "name", &dependency.name);
                write_toml_field(&mut output, "version", &dependency.version.to_string());
                write_toml_field(&mut output, "source", &dependency.source);
                write_toml_field(&mut output, "path", &dependency.path);
            }
            for dependency in &package.registry_dependencies {
                output.push_str("\n[[package.registry-dependencies]]\n");
                dependency.write_to(&mut output);
            }
        }
        output
    }
}

/// One locked local package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedPackage {
    pub name: String,
    pub version: Version,
    pub edition: Edition,
    pub source: String,
    pub path: String,
    pub dependencies: Vec<LockedDependency>,
    pub registry_dependencies: Vec<LockedRegistryDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedRegistrySnapshot {
    pub registry: String,
    pub checksum: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedRegistryPackage {
    pub registry: String,
    pub name: String,
    pub version: Version,
    pub snapshot: Sha256Digest,
    pub checksum: Sha256Digest,
    pub dependencies: Vec<LockedRegistryDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedRegistryDependency {
    pub alias: String,
    pub registry: String,
    pub name: String,
    pub version: Version,
    pub snapshot: Sha256Digest,
    pub checksum: Sha256Digest,
}

impl LockedRegistryDependency {
    fn from_resolved(dependency: &crate::registry::ResolvedRegistryDependency) -> Self {
        Self {
            alias: dependency.alias.clone(),
            registry: dependency.provider.registry.clone(),
            name: dependency.provider.name.clone(),
            version: dependency.provider.version.clone(),
            snapshot: dependency.provider.snapshot.clone(),
            checksum: dependency.provider.archive_sha256.clone(),
        }
    }

    fn compare(left: &Self, right: &Self) -> std::cmp::Ordering {
        left.alias
            .cmp(&right.alias)
            .then_with(|| left.registry.cmp(&right.registry))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.checksum.cmp(&right.checksum))
    }

    fn write_to(&self, output: &mut String) {
        write_toml_field(output, "alias", &self.alias);
        write_toml_field(output, "registry", &self.registry);
        write_toml_field(output, "name", &self.name);
        write_toml_field(output, "version", &self.version.to_string());
        write_toml_field(output, "snapshot", self.snapshot.as_str());
        write_toml_field(output, "checksum", self.checksum.as_str());
    }
}

/// One named dependency edge from a locked package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedDependency {
    pub alias: String,
    pub name: String,
    pub version: Version,
    pub source: String,
    pub path: String,
}

/// Parse and validate canonical lock data.
pub fn parse_lockfile(source: &str) -> Result<Lockfile, String> {
    let raw: RawLockfile =
        toml::from_str(source).map_err(|error| format!("could not parse lockfile: {error}"))?;
    if raw.format != LOCKFILE_FORMAT_VERSION {
        return Err(format!(
            "unsupported lockfile format {}; expected {}",
            raw.format, LOCKFILE_FORMAT_VERSION
        ));
    }
    let registry_snapshots = raw
        .registry_snapshot
        .into_iter()
        .map(|snapshot| {
            validate_registry_name(&snapshot.registry).map_err(|error| error.to_string())?;
            Ok(LockedRegistrySnapshot {
                registry: snapshot.registry,
                checksum: Sha256Digest::parse(&snapshot.checksum)
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let registry_packages = raw
        .registry_package
        .into_iter()
        .map(parse_locked_registry_package)
        .collect::<Result<Vec<_>, String>>()?;
    let mut packages = Vec::with_capacity(raw.package.len());
    for package in raw.package {
        let version = Version::parse(&package.version).map_err(|error| {
            format!(
                "package `{}` has invalid locked version `{}`: {error}",
                package.name, package.version
            )
        })?;
        let edition = parse_locked_edition(&package.edition)?;
        validate_locked_source(&package.source, &package.path)?;
        let mut dependencies = Vec::with_capacity(package.dependencies.len());
        for dependency in package.dependencies {
            let dependency_version = Version::parse(&dependency.version).map_err(|error| {
                format!(
                    "dependency `{}` has invalid locked version `{}`: {error}",
                    dependency.alias, dependency.version
                )
            })?;
            validate_locked_source(&dependency.source, &dependency.path)?;
            dependencies.push(LockedDependency {
                alias: dependency.alias,
                name: dependency.name,
                version: dependency_version,
                source: dependency.source,
                path: dependency.path,
            });
        }
        let registry_dependencies = package
            .registry_dependencies
            .into_iter()
            .map(parse_locked_registry_dependency)
            .collect::<Result<Vec<_>, String>>()?;
        packages.push(LockedPackage {
            name: package.name,
            version,
            edition,
            source: package.source,
            path: package.path,
            dependencies,
            registry_dependencies,
        });
    }
    let lockfile = Lockfile {
        format: raw.format,
        registry_snapshots,
        registry_packages,
        packages,
    };
    validate_registry_lock_graph(&lockfile)?;
    Ok(lockfile)
}

fn parse_locked_registry_package(
    raw: RawLockedRegistryPackage,
) -> Result<LockedRegistryPackage, String> {
    validate_registry_name(&raw.registry).map_err(|error| error.to_string())?;
    validate_package_name(&raw.name).map_err(|error| error.to_string())?;
    let version = Version::parse(&raw.version).map_err(|error| {
        format!(
            "registry package `{}` has invalid locked version `{}`: {error}",
            raw.name, raw.version
        )
    })?;
    if !version.build.is_empty() {
        return Err(format!(
            "registry package `{}` locked version `{}` must not contain build metadata",
            raw.name, raw.version
        ));
    }
    Ok(LockedRegistryPackage {
        registry: raw.registry,
        name: raw.name,
        version,
        snapshot: Sha256Digest::parse(&raw.snapshot).map_err(|error| error.to_string())?,
        checksum: Sha256Digest::parse(&raw.checksum).map_err(|error| error.to_string())?,
        dependencies: raw
            .dependencies
            .into_iter()
            .map(parse_locked_registry_dependency)
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn parse_locked_registry_dependency(
    raw: RawLockedRegistryDependency,
) -> Result<LockedRegistryDependency, String> {
    validate_alias(&raw.alias).map_err(|error| error.to_string())?;
    validate_registry_name(&raw.registry).map_err(|error| error.to_string())?;
    validate_package_name(&raw.name).map_err(|error| error.to_string())?;
    let version = Version::parse(&raw.version).map_err(|error| {
        format!(
            "registry dependency `{}` has invalid locked version `{}`: {error}",
            raw.alias, raw.version
        )
    })?;
    if !version.build.is_empty() {
        return Err(format!(
            "registry dependency `{}` locked version `{}` must not contain build metadata",
            raw.alias, raw.version
        ));
    }
    Ok(LockedRegistryDependency {
        alias: raw.alias,
        registry: raw.registry,
        name: raw.name,
        version,
        snapshot: Sha256Digest::parse(&raw.snapshot).map_err(|error| error.to_string())?,
        checksum: Sha256Digest::parse(&raw.checksum).map_err(|error| error.to_string())?,
    })
}

fn validate_registry_lock_graph(lockfile: &Lockfile) -> Result<(), String> {
    let snapshots = lockfile
        .registry_snapshots
        .iter()
        .map(|snapshot| (snapshot.registry.as_str(), &snapshot.checksum))
        .collect::<BTreeMap<_, _>>();
    if snapshots.len() != lockfile.registry_snapshots.len() {
        return Err("lockfile repeats a registry snapshot identity".into());
    }
    let providers = lockfile
        .registry_packages
        .iter()
        .map(|package| {
            (
                (
                    package.registry.as_str(),
                    package.name.as_str(),
                    &package.version,
                    &package.snapshot,
                    &package.checksum,
                ),
                package,
            )
        })
        .collect::<BTreeMap<_, _>>();
    if providers.len() != lockfile.registry_packages.len() {
        return Err("lockfile repeats an exact registry provider identity".into());
    }
    let package_keys = lockfile
        .registry_packages
        .iter()
        .map(|package| (package.registry.as_str(), package.name.as_str()))
        .collect::<HashSet<_>>();
    if package_keys.len() != lockfile.registry_packages.len() {
        return Err("lockfile selects more than one version of a registry package identity".into());
    }
    for package in &lockfile.registry_packages {
        if snapshots.get(package.registry.as_str()) != Some(&&package.snapshot) {
            return Err(format!(
                "registry package `{}/{}` references an unknown snapshot",
                package.registry, package.name
            ));
        }
        for dependency in &package.dependencies {
            validate_locked_registry_edge(dependency, &providers)?;
        }
    }
    for package in &lockfile.packages {
        for dependency in &package.registry_dependencies {
            validate_locked_registry_edge(dependency, &providers)?;
        }
    }
    Ok(())
}

fn validate_locked_registry_edge(
    dependency: &LockedRegistryDependency,
    providers: &BTreeMap<
        (&str, &str, &Version, &Sha256Digest, &Sha256Digest),
        &LockedRegistryPackage,
    >,
) -> Result<(), String> {
    if !providers.contains_key(&(
        dependency.registry.as_str(),
        dependency.name.as_str(),
        &dependency.version,
        &dependency.snapshot,
        &dependency.checksum,
    )) {
        return Err(format!(
            "registry dependency `{}` references missing exact provider `{}/{}@{}`",
            dependency.alias, dependency.registry, dependency.name, dependency.version
        ));
    }
    Ok(())
}

fn parse_locked_edition(edition: &str) -> Result<Edition, String> {
    match edition {
        "2026" => Ok(Edition::Edition2026),
        edition => Err(format!("unsupported locked edition `{edition}`")),
    }
}

fn validate_locked_source(source: &str, path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || Path::new(path).is_absolute()
        || Path::new(path).components().any(|component| {
            !matches!(
                component,
                Component::Normal(_) | Component::ParentDir | Component::CurDir
            )
        })
    {
        return Err(format!(
            "locked package path `{path}` must be a portable relative path"
        ));
    }
    let expected = source
        .strip_prefix("workspace:")
        .or_else(|| source.strip_prefix("path:"))
        .ok_or_else(|| format!("unsupported locked package source `{source}`"))?;
    if expected != path {
        return Err(format!(
            "locked source `{source}` does not match package path `{path}`"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockfile {
    format: u32,
    #[serde(default, rename = "registry-snapshot")]
    registry_snapshot: Vec<RawLockedRegistrySnapshot>,
    #[serde(default, rename = "registry-package")]
    registry_package: Vec<RawLockedRegistryPackage>,
    #[serde(default)]
    package: Vec<RawLockedPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedPackage {
    name: String,
    version: String,
    edition: String,
    source: String,
    path: String,
    #[serde(default)]
    dependencies: Vec<RawLockedDependency>,
    #[serde(default, rename = "registry-dependencies")]
    registry_dependencies: Vec<RawLockedRegistryDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedRegistrySnapshot {
    registry: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedRegistryPackage {
    registry: String,
    name: String,
    version: String,
    snapshot: String,
    checksum: String,
    #[serde(default)]
    dependencies: Vec<RawLockedRegistryDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedRegistryDependency {
    alias: String,
    registry: String,
    name: String,
    version: String,
    snapshot: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedDependency {
    alias: String,
    name: String,
    version: String,
    source: String,
    path: String,
}

/// Generate deterministic lock data from a previously validated graph.
pub fn generate_lockfile(graph: &DependencyGraph) -> Lockfile {
    Lockfile::from_graph(graph)
}

pub fn generate_lockfile_at(graph: &DependencyGraph, root: &Path) -> Lockfile {
    Lockfile::from_graph_root(graph, root)
}

pub fn generate_workspace_lockfile(
    graph: &DependencyGraph,
    root: &Path,
    workspace_members: &HashSet<PathBuf>,
) -> Lockfile {
    Lockfile::from_graph_root_with_workspace_members(graph, root, workspace_members)
}

/// Generate canonical lockfile text from a previously validated graph.
pub fn render_lockfile(graph: &DependencyGraph) -> String {
    Lockfile::from_graph(graph).to_text()
}

/// Atomically replace a lockfile only when its bytes differ.
///
/// Returns `true` when the file changed and `false` when the existing content
/// was already current.
pub fn write_lockfile_if_changed(path: impl AsRef<Path>, lockfile: &Lockfile) -> io::Result<bool> {
    let path = path.as_ref();
    let contents = lockfile.to_text();
    match fs::read(path) {
        Ok(existing) if existing == contents.as_bytes() => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    atomic_write(path, contents.as_bytes())?;
    Ok(true)
}

/// Write `<root>/salicin.lock` only when the generated content changed.
pub fn write_package_lockfile(graph: &DependencyGraph) -> io::Result<bool> {
    let lockfile = Lockfile::from_graph(graph);
    write_lockfile_if_changed(graph.root().package_root.join(LOCKFILE_NAME), &lockfile)
}

fn write_toml_field(output: &mut String, name: &str, value: &str) {
    writeln!(output, "{name} = {}", toml_string(value)).expect("writing to a string cannot fail");
}

fn toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\u{:04X}", character as u32)
                    .expect("writing to a string cannot fail");
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

pub fn portable_relative_path(root: &Path, target: &Path) -> String {
    let path = relative_path(root, target).unwrap_or_else(|| target.to_path_buf());
    if path.as_os_str().is_empty() {
        return ".".to_owned();
    }
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn package_source(root: &Path, target: &Path, workspace_members: &HashSet<PathBuf>) -> String {
    let category = if workspace_members.contains(target) {
        "workspace"
    } else {
        "path"
    };
    format!("{category}:{}", portable_relative_path(root, target))
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<Component<'_>> = from.components().collect();
    let to_components: Vec<Component<'_>> = to.components().collect();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();

    if common == 0
        && (from.is_absolute()
            || to.is_absolute()
            || from_components.first() != to_components.first())
    {
        return None;
    }
    if from_components[common..]
        .iter()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &from_components[common..] {
        if !matches!(component, Component::CurDir) {
            relative.push("..");
        }
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(LOCKFILE_NAME);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for _ in 0..100 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{id}",
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.write_all(contents)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a temporary lockfile name",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::load_dependency_graph;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("salicin-lockfile-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative: impl AsRef<Path>, contents: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }

        fn package(&self, directory: &str, name: &str, dependencies: &str) {
            self.write(format!("{directory}/src/lib.sc"), "let answer = 42\n");
            self.write(
                format!("{directory}/salicin.toml"),
                &format!(
                    "[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nedition = \"2026\"\n{dependencies}"
                ),
            );
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn renders_sorted_stable_relative_lock_data() {
        let temp = TempDir::new();
        temp.package("shared", "shared", "");
        temp.package(
            "zeta",
            "zeta",
            "\n[dependencies]\nshared = { path = \"../shared\" }\n",
        );
        temp.package("alpha", "alpha", "");
        temp.package(
            "root",
            "root",
            "\n[dependencies]\nz = { path = \"../zeta\" }\na = { path = \"../alpha\" }\n",
        );
        let graph = load_dependency_graph(temp.0.join("root")).unwrap();

        let lockfile = Lockfile::from_graph(&graph);
        let first = lockfile.to_text();
        let second = Lockfile::from_graph(&graph).to_text();

        assert_eq!(first, second);
        assert_eq!(
            lockfile
                .packages
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "root", "shared", "zeta"]
        );
        assert!(first.contains("path = \".\""));
        assert!(first.contains("path = \"../shared\""));
        assert!(first.contains("source = \"path:.\""));
        assert!(first.contains("source = \"path:../shared\""));
        assert!(!first.contains(&temp.0.display().to_string()));
        let parsed: toml::Value = toml::from_str(&first).unwrap();
        assert_eq!(parsed["format"].as_integer(), Some(3));
    }

    #[test]
    fn writes_atomically_only_when_content_changes() {
        let temp = TempDir::new();
        temp.package("root", "root", "");
        let graph = load_dependency_graph(temp.0.join("root")).unwrap();
        let lockfile = generate_lockfile(&graph);
        let path = temp.0.join("root").join(LOCKFILE_NAME);

        assert!(write_lockfile_if_changed(&path, &lockfile).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), lockfile.to_text());
        assert!(!write_lockfile_if_changed(&path, &lockfile).unwrap());

        fs::write(&path, "stale\n").unwrap();
        assert!(write_package_lockfile(&graph).unwrap());
        assert_eq!(fs::read_to_string(path).unwrap(), lockfile.to_text());
    }

    #[test]
    fn parses_generated_lockfiles_and_rejects_stale_shapes() {
        let temp = TempDir::new();
        temp.package("dependency", "dependency", "");
        temp.package(
            "root",
            "root",
            "\n[dependencies]\ndep = { path = \"../dependency\" }\n",
        );
        let graph = load_dependency_graph(temp.0.join("root")).unwrap();
        let expected = generate_lockfile(&graph);
        assert_eq!(parse_lockfile(&expected.to_text()).unwrap(), expected);

        for (source, expected) in [
            ("format = 1\n", "unsupported lockfile format"),
            (
                "format = 3\nunknown = true\n",
                "unknown field `unknown`",
            ),
            (
                "format = 3\n[[package]]\nname = \"p\"\nversion = \"bad\"\nedition = \"2026\"\nsource = \"path:.\"\npath = \".\"\n",
                "invalid locked version",
            ),
            (
                "format = 3\n[[package]]\nname = \"p\"\nversion = \"1.0.0\"\nedition = \"2026\"\nsource = \"path:other\"\npath = \".\"\n",
                "does not match package path",
            ),
        ] {
            let error = parse_lockfile(source).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn registry_resolution_renders_and_parses_complete_exact_identities() {
        use crate::registry::{
            RegistryProviderIdentity, ResolvedRegistryDependency, ResolvedRegistryPackage,
            ResolvedRootRegistryDependency,
        };

        let temp = TempDir::new();
        temp.package("root", "root", "");
        let graph = load_dependency_graph(temp.0.join("root")).unwrap();
        let snapshot = Sha256Digest::parse(&"a".repeat(64)).unwrap();
        let checksum = Sha256Digest::parse(&"b".repeat(64)).unwrap();
        let provider = RegistryProviderIdentity {
            registry: "community".into(),
            snapshot: snapshot.clone(),
            name: "answer-kit".into(),
            version: Version::new(1, 2, 3),
            archive_sha256: checksum.clone(),
        };
        let dependency = ResolvedRegistryDependency {
            alias: "answer".into(),
            provider: provider.clone(),
        };
        let resolution = RegistryResolution {
            snapshots: vec![("community".into(), snapshot)],
            packages: vec![ResolvedRegistryPackage {
                provider,
                dependencies: vec![],
            }],
            roots: vec![ResolvedRootRegistryDependency {
                owner_manifest_path: graph.root_manifest_path.clone(),
                dependency,
            }],
        };
        let lockfile = Lockfile::from_graph_root_with_registry(
            &graph,
            &graph.root().package_root,
            &HashSet::new(),
            &resolution,
        )
        .unwrap();
        let text = lockfile.to_text();
        assert!(text.contains("[[registry-snapshot]]"), "{text}");
        assert!(text.contains("[[registry-package]]"), "{text}");
        assert!(text.contains("[[package.registry-dependencies]]"), "{text}");
        assert_eq!(parse_lockfile(&text).unwrap(), lockfile);

        let broken = text.replacen("name = \"answer-kit\"", "name = \"missing-kit\"", 1);
        let error = parse_lockfile(&broken).unwrap_err();
        assert!(error.contains("missing exact provider"), "{error}");
    }

    #[test]
    fn escapes_toml_strings_and_relativizes_paths() {
        assert_eq!(toml_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(
            relative_path(Path::new("/a/root"), Path::new("/a/dep")),
            Some(PathBuf::from("../dep"))
        );
        assert_eq!(
            relative_path(Path::new("/a/root"), Path::new("/a/root")),
            Some(PathBuf::new())
        );
    }
}
