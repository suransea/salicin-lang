//! Loading and validation for `salicin.toml` package manifests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use semver::Version;
use serde::Deserialize;

/// The package manifest file name recognized by `salic`.
pub const MANIFEST_FILE_NAME: &str = "salicin.toml";

/// A validated Salicin package manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    /// Package identity and language edition.
    pub package: Package,
    /// The optional library target.
    pub lib: Option<Target>,
    /// Binary targets in declaration order.
    pub bins: Vec<Target>,
    /// Validated local path dependencies, sorted by alias.
    pub dependencies: Vec<Dependency>,
    /// Canonical absolute path to `salicin.toml`.
    pub manifest_path: PathBuf,
    /// Canonical absolute path to the directory containing the manifest.
    pub package_root: PathBuf,
}

/// One validated local path dependency from `[dependencies]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dependency {
    /// Source-level name used as the dependency's package module.
    pub alias: String,
    /// Canonical absolute path to the dependency package root.
    pub path: PathBuf,
    /// Canonical absolute path to the dependency's `salicin.toml`.
    pub manifest_path: PathBuf,
    /// Identity read and validated from the dependency manifest.
    pub package: Package,
}

impl Manifest {
    /// Iterate over the library target, when present, followed by binary targets.
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.lib.iter().chain(self.bins.iter())
    }
}

/// Package identity from the `[package]` table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub edition: Edition,
}

/// A language edition supported by this compiler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Edition {
    Edition2026,
}

impl Edition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edition2026 => "2026",
        }
    }
}

impl fmt::Display for Edition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A source target declared by or discovered from a manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub name: String,
    /// Canonical absolute path to the target's `.sali` source file.
    pub path: PathBuf,
    pub kind: TargetKind,
}

/// The kind of package target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetKind {
    Lib,
    Bin,
}

impl fmt::Display for TargetKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lib => formatter.write_str("library"),
            Self::Bin => formatter.write_str("binary"),
        }
    }
}

/// An error produced while reading, parsing, or validating a manifest.
#[derive(Debug)]
pub enum ManifestError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        message: String,
    },
}

impl ManifestError {
    /// Path most directly associated with this diagnostic.
    pub fn path(&self) -> &Path {
        match self {
            Self::Io { path, .. } | Self::Parse { path, .. } | Self::Invalid { path, .. } => path,
        }
    }

    fn invalid(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "could not read `{}`: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "could not parse `{}`: {source}", path.display())
            }
            Self::Invalid { path, message } => {
                write!(
                    formatter,
                    "invalid manifest `{}`: {message}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

/// Load and validate a `salicin.toml` manifest.
///
/// `path` may point directly to a manifest or to a package directory. All
/// returned paths are canonical absolute paths. Local path dependency
/// manifests are read to validate their package identity; use
/// [`load_dependency_graph`] to recursively load the complete graph.
pub fn load_manifest(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
    let manifest_path = resolve_manifest_path(path.as_ref())?;
    let raw = read_raw_manifest(&manifest_path)?;

    validate_manifest(raw, manifest_path)
}

fn resolve_manifest_path(path: &Path) -> Result<PathBuf, ManifestError> {
    let manifest_path = if path.is_dir() {
        path.join(MANIFEST_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    fs::canonicalize(&manifest_path).map_err(|source| ManifestError::Io {
        path: manifest_path,
        source,
    })
}

fn read_raw_manifest(manifest_path: &Path) -> Result<RawManifest, ManifestError> {
    let source = fs::read_to_string(manifest_path).map_err(|source| ManifestError::Io {
        path: manifest_path.to_path_buf(),
        source,
    })?;
    toml::from_str(&source).map_err(|source| ManifestError::Parse {
        path: manifest_path.to_path_buf(),
        source,
    })
}

fn validate_manifest(raw: RawManifest, manifest_path: PathBuf) -> Result<Manifest, ManifestError> {
    let package_root = manifest_path
        .parent()
        .expect("a canonical file path always has a parent")
        .to_path_buf();

    let package = validate_package(&raw.package, &manifest_path)?;

    let dependencies = validate_dependencies(raw.dependencies, &package_root, &manifest_path)?;

    let lib = match raw.lib {
        Some(raw_lib) => Some(Target {
            name: package.name.replace('-', "_"),
            path: resolve_target_path(
                &package_root,
                &manifest_path,
                &raw_lib.path,
                TargetKind::Lib,
            )?,
            kind: TargetKind::Lib,
        }),
        None => discover_default_target(
            &package_root,
            &manifest_path,
            Path::new("src/lib.sali"),
            package.name.replace('-', "_"),
            TargetKind::Lib,
        )?,
    };

    let bins = if raw.bin.is_empty() {
        discover_default_target(
            &package_root,
            &manifest_path,
            Path::new("src/main.sali"),
            package.name.clone(),
            TargetKind::Bin,
        )?
        .into_iter()
        .collect()
    } else {
        let mut names = HashSet::new();
        let mut bins = Vec::with_capacity(raw.bin.len());
        for raw_bin in raw.bin {
            if !is_ascii_kebab_case(&raw_bin.name) {
                return Err(ManifestError::invalid(
                    &manifest_path,
                    format!(
                        "binary target name `{}` must be ASCII kebab-case",
                        raw_bin.name
                    ),
                ));
            }
            if !names.insert(raw_bin.name.clone()) {
                return Err(ManifestError::invalid(
                    &manifest_path,
                    format!(
                        "binary target name `{}` is declared more than once",
                        raw_bin.name
                    ),
                ));
            }
            bins.push(Target {
                name: raw_bin.name,
                path: resolve_target_path(
                    &package_root,
                    &manifest_path,
                    &raw_bin.path,
                    TargetKind::Bin,
                )?,
                kind: TargetKind::Bin,
            });
        }
        bins
    };

    if lib.is_none() && bins.is_empty() {
        return Err(ManifestError::invalid(
            &manifest_path,
            "package has no targets; add `src/lib.sali`, `src/main.sali`, `[lib]`, or `[[bin]]`",
        ));
    }

    Ok(Manifest {
        package,
        lib,
        bins,
        dependencies,
        manifest_path,
        package_root,
    })
}

fn validate_package(raw: &RawPackage, manifest_path: &Path) -> Result<Package, ManifestError> {
    if !is_ascii_kebab_case(&raw.name) {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "package name `{}` must be ASCII kebab-case (for example `hello-salicin`)",
                raw.name
            ),
        ));
    }

    let version = Version::parse(&raw.version).map_err(|error| {
        ManifestError::invalid(
            manifest_path,
            format!(
                "package version `{}` is not valid semantic versioning: {error}",
                raw.version
            ),
        )
    })?;

    let edition = match raw.edition.as_str() {
        "2026" => Edition::Edition2026,
        edition => {
            return Err(ManifestError::invalid(
                manifest_path,
                format!("edition `{edition}` is not supported; expected `2026`"),
            ));
        }
    };

    Ok(Package {
        name: raw.name.clone(),
        version,
        edition,
    })
}

fn validate_dependencies(
    raw_dependencies: BTreeMap<String, RawDependency>,
    package_root: &Path,
    manifest_path: &Path,
) -> Result<Vec<Dependency>, ManifestError> {
    let mut dependencies = Vec::with_capacity(raw_dependencies.len());
    for (alias, raw) in raw_dependencies {
        if !is_ascii_snake_case_module_name(&alias) {
            return Err(ManifestError::invalid(
                manifest_path,
                format!(
                    "dependency alias `{alias}` must be a non-reserved ASCII snake_case module name"
                ),
            ));
        }
        if !is_portable_relative_dependency_path(&raw.path) {
            return Err(ManifestError::invalid(
                manifest_path,
                format!(
                    "dependency `{alias}` path `{}` must be a non-empty portable relative path using `/` separators",
                    raw.path.display()
                ),
            ));
        }

        let dependency_manifest =
            resolve_dependency_manifest_path(package_root, manifest_path, &alias, &raw.path)?;
        let dependency_raw = read_raw_manifest(&dependency_manifest)?;
        let package = validate_package(&dependency_raw.package, &dependency_manifest)?;
        let dependency_root = dependency_manifest
            .parent()
            .expect("a canonical manifest path always has a parent")
            .to_path_buf();
        dependencies.push(Dependency {
            alias,
            path: dependency_root,
            manifest_path: dependency_manifest,
            package,
        });
    }
    Ok(dependencies)
}

fn is_portable_relative_dependency_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    if text.is_empty() || text.contains(['\\', ':']) {
        return false;
    }

    path.components().all(|component| {
        matches!(
            component,
            Component::Normal(_) | Component::ParentDir | Component::CurDir
        )
    })
}

fn resolve_dependency_manifest_path(
    package_root: &Path,
    manifest_path: &Path,
    alias: &str,
    dependency_path: &Path,
) -> Result<PathBuf, ManifestError> {
    let joined = package_root.join(dependency_path);
    let candidate = if joined.is_dir() {
        joined.join(MANIFEST_FILE_NAME)
    } else {
        joined
    };
    let canonical = fs::canonicalize(&candidate).map_err(|source| {
        ManifestError::invalid(
            manifest_path,
            format!(
                "dependency `{alias}` path `{}` does not contain an accessible `{MANIFEST_FILE_NAME}`: {source}",
                dependency_path.display()
            ),
        )
    })?;
    if canonical.file_name().and_then(|name| name.to_str()) != Some(MANIFEST_FILE_NAME)
        || !canonical.is_file()
    {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "dependency `{alias}` path `{}` must name a package directory or `{MANIFEST_FILE_NAME}`",
                dependency_path.display()
            ),
        ));
    }
    Ok(canonical)
}

fn is_ascii_snake_case_module_name(name: &str) -> bool {
    if name == "_" || name == "self" || crate::lexer::is_keyword(name) {
        return false;
    }
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A recursively loaded, canonicalized local dependency graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyGraph {
    /// Canonical path of the root package manifest.
    pub root_manifest_path: PathBuf,
    /// Every unique package in canonical manifest-path order.
    pub packages: Vec<Manifest>,
}

impl DependencyGraph {
    /// Return the root package manifest.
    pub fn root(&self) -> &Manifest {
        self.packages
            .iter()
            .find(|manifest| manifest.manifest_path == self.root_manifest_path)
            .expect("a dependency graph always contains its root")
    }

    /// Find a package by its canonical manifest path.
    pub fn package(&self, manifest_path: &Path) -> Option<&Manifest> {
        self.packages
            .iter()
            .find(|manifest| manifest.manifest_path == manifest_path)
    }
}

/// Recursively load all local path dependencies and reject canonical-path cycles.
pub fn load_dependency_graph(path: impl AsRef<Path>) -> Result<DependencyGraph, ManifestError> {
    let root = load_manifest(path)?;
    let root_manifest_path = root.manifest_path.clone();
    let mut builder = GraphBuilder {
        states: HashMap::new(),
        manifests: BTreeMap::new(),
        stack: Vec::new(),
    };
    builder.visit(root)?;
    Ok(DependencyGraph {
        root_manifest_path,
        packages: builder.manifests.into_values().collect(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Complete,
}

struct GraphBuilder {
    states: HashMap<PathBuf, VisitState>,
    manifests: BTreeMap<PathBuf, Manifest>,
    stack: Vec<PathBuf>,
}

impl GraphBuilder {
    fn visit(&mut self, manifest: Manifest) -> Result<(), ManifestError> {
        let path = manifest.manifest_path.clone();
        match self.states.get(&path) {
            Some(VisitState::Complete) => return Ok(()),
            Some(VisitState::Visiting) => {
                let start = self
                    .stack
                    .iter()
                    .position(|entry| entry == &path)
                    .unwrap_or(0);
                let mut cycle = self.stack[start..].to_vec();
                cycle.push(path.clone());
                let cycle = cycle
                    .iter()
                    .map(|entry| entry.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(ManifestError::invalid(
                    &path,
                    format!("local dependency cycle detected: {cycle}"),
                ));
            }
            None => {}
        }

        self.states.insert(path.clone(), VisitState::Visiting);
        self.stack.push(path.clone());
        let dependencies: Vec<(String, PathBuf)> = manifest
            .dependencies
            .iter()
            .map(|dependency| (dependency.alias.clone(), dependency.manifest_path.clone()))
            .collect();
        self.manifests.insert(path.clone(), manifest);
        for (alias, dependency_path) in dependencies {
            let dependency = load_manifest(&dependency_path)?;
            if dependency.lib.is_none() {
                return Err(ManifestError::invalid(
                    &dependency_path,
                    format!(
                        "dependency `{alias}` package `{}` does not provide a library target",
                        dependency.package.name
                    ),
                ));
            }
            self.visit(dependency)?;
        }
        self.stack.pop();
        self.states.insert(path, VisitState::Complete);
        Ok(())
    }
}

fn discover_default_target(
    package_root: &Path,
    manifest_path: &Path,
    relative_path: &Path,
    name: String,
    kind: TargetKind,
) -> Result<Option<Target>, ManifestError> {
    let path = package_root.join(relative_path);
    if !path.try_exists().map_err(|source| ManifestError::Io {
        path: path.clone(),
        source,
    })? {
        return Ok(None);
    }

    Ok(Some(Target {
        name,
        path: resolve_target_path(package_root, manifest_path, relative_path, kind)?,
        kind,
    }))
}

fn resolve_target_path(
    package_root: &Path,
    manifest_path: &Path,
    relative_path: &Path,
    kind: TargetKind,
) -> Result<PathBuf, ManifestError> {
    if relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` must be a non-empty relative path",
                relative_path.display()
            ),
        ));
    }

    if relative_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("sali")
    {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` must have the `.sali` extension",
                relative_path.display()
            ),
        ));
    }

    if lexically_escapes_root(relative_path) {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` escapes the package root",
                relative_path.display()
            ),
        ));
    }

    let joined = package_root.join(relative_path);
    let canonical = fs::canonicalize(&joined).map_err(|source| {
        ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` does not exist or cannot be accessed: {source}",
                relative_path.display()
            ),
        )
    })?;

    if !canonical.starts_with(package_root) {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` resolves outside the package root",
                relative_path.display()
            ),
        ));
    }

    if !canonical.is_file() {
        return Err(ManifestError::invalid(
            manifest_path,
            format!(
                "{kind} target path `{}` is not a file",
                relative_path.display()
            ),
        ));
    }

    Ok(canonical)
}

fn lexically_escapes_root(path: &Path) -> bool {
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth == 0 => return true,
            Component::ParentDir => depth -= 1,
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return true,
        }
    }
    false
}

fn is_ascii_kebab_case(name: &str) -> bool {
    let mut segments = name.split('-');
    let Some(first) = segments.next() else {
        return false;
    };
    is_kebab_segment(first, true) && segments.all(|segment| is_kebab_segment(segment, false))
}

fn is_kebab_segment(segment: &str, require_leading_letter: bool) -> bool {
    let mut bytes = segment.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if (require_leading_letter && !first.is_ascii_lowercase())
        || (!require_leading_letter && !first.is_ascii_lowercase() && !first.is_ascii_digit())
    {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    package: RawPackage,
    #[serde(default)]
    lib: Option<RawLib>,
    #[serde(default)]
    bin: Vec<RawBin>,
    #[serde(default)]
    dependencies: BTreeMap<String, RawDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    name: String,
    version: String,
    edition: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLib {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBin {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDependency {
    path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("salicin-manifest-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: impl AsRef<Path>, contents: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn basic_manifest(extra: &str) -> String {
        format!(
            r#"[package]
name = "hello-salicin"
version = "1.2.3"
edition = "2026"
{extra}
"#
        )
    }

    #[test]
    fn loads_explicit_targets_and_exposes_validated_metadata() {
        let temp = TempDir::new();
        temp.write("source/library.sali", "let answer = 42\n");
        temp.write("source/tool.sali", "let main() = 0\n");
        temp.write(
            MANIFEST_FILE_NAME,
            &basic_manifest(
                r#"
[lib]
path = "source/library.sali"

[[bin]]
name = "salicin-tool"
path = "source/tool.sali"

[dependencies]
"#,
            ),
        );

        let manifest = load_manifest(temp.path()).unwrap();

        assert_eq!(manifest.package.name, "hello-salicin");
        assert_eq!(manifest.package.version, Version::new(1, 2, 3));
        assert_eq!(manifest.package.edition, Edition::Edition2026);
        assert_eq!(manifest.lib.as_ref().unwrap().name, "hello_salicin");
        assert_eq!(manifest.lib.as_ref().unwrap().kind, TargetKind::Lib);
        assert_eq!(manifest.bins[0].name, "salicin-tool");
        assert_eq!(manifest.bins[0].kind, TargetKind::Bin);
        assert!(manifest.targets().all(|target| target.path.is_absolute()));
    }

    #[test]
    fn discovers_default_library_and_binary_and_allows_the_same_name() {
        let temp = TempDir::new();
        temp.write("src/lib.sali", "let answer = 42\n");
        temp.write("src/main.sali", "let main() = 0\n");
        temp.write(MANIFEST_FILE_NAME, &basic_manifest("\n[dependencies]\n"));

        let manifest = load_manifest(temp.path().join(MANIFEST_FILE_NAME)).unwrap();

        assert_eq!(manifest.lib.unwrap().name, "hello_salicin");
        assert_eq!(manifest.bins[0].name, "hello-salicin");
    }

    #[test]
    fn rejects_unknown_fields_at_every_manifest_level() {
        let temp = TempDir::new();
        temp.write("src/main.sali", "let main() = 0\n");
        temp.write(
            MANIFEST_FILE_NAME,
            r#"[package]
name = "hello"
version = "1.0.0"
edition = "2026"
license = "MIT"
"#,
        );

        let error = load_manifest(temp.path()).unwrap_err().to_string();
        assert!(error.contains("unknown field `license`"), "{error}");
    }

    #[test]
    fn validates_package_name_version_and_edition() {
        for (name, version, edition, expected) in [
            ("Hello", "1.0.0", "2026", "ASCII kebab-case"),
            ("hello", "one", "2026", "semantic versioning"),
            ("hello", "1.0.0", "2025", "not supported"),
        ] {
            let temp = TempDir::new();
            temp.write("src/main.sali", "let main() = 0\n");
            temp.write(
                MANIFEST_FILE_NAME,
                &format!(
                    "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"{edition}\"\n"
                ),
            );

            let error = load_manifest(temp.path()).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn rejects_packages_without_targets() {
        let temp = TempDir::new();
        temp.write(MANIFEST_FILE_NAME, &basic_manifest(""));

        let error = load_manifest(temp.path()).unwrap_err().to_string();
        assert!(error.contains("package has no targets"), "{error}");
    }

    #[test]
    fn rejects_non_path_dependency_sources_and_unknown_fields() {
        let temp = TempDir::new();
        temp.write("src/main.sali", "let main() = 0\n");
        for (field, value) in [
            ("version", "\"1.2\""),
            ("git", "\"https://example.invalid/repo\""),
            ("branch", "\"main\""),
        ] {
            temp.write(
                MANIFEST_FILE_NAME,
                &basic_manifest(&format!(
                    "\n[dependencies]\nhttp = {{ {field} = {value} }}\n"
                )),
            );

            let error = load_manifest(temp.path()).unwrap_err().to_string();
            assert!(
                error.contains(&format!("unknown field `{field}`")),
                "{error}"
            );
        }
    }

    #[test]
    fn loads_and_sorts_validated_local_path_dependencies() {
        let temp = TempDir::new();
        write_test_package(&temp, "alpha", "alpha-package", "");
        write_test_package(&temp, "zeta", "zeta-package", "");
        temp.write("root/src/main.sali", "let main() = 0\n");
        temp.write(
            "root/salicin.toml",
            r#"[package]
name = "root-package"
version = "1.0.0"
edition = "2026"

[dependencies]
zeta = { path = "../zeta/salicin.toml" }
alpha_util = { path = "../alpha" }
"#,
        );

        let manifest = load_manifest(temp.path().join("root")).unwrap();

        assert_eq!(
            manifest
                .dependencies
                .iter()
                .map(|dependency| dependency.alias.as_str())
                .collect::<Vec<_>>(),
            ["alpha_util", "zeta"]
        );
        assert_eq!(manifest.dependencies[0].package.name, "alpha-package");
        assert!(manifest
            .dependencies
            .iter()
            .all(|dependency| dependency.path.is_absolute()
                && dependency.manifest_path.is_absolute()));
    }

    #[test]
    fn rejects_invalid_dependency_aliases_paths_and_manifests() {
        for alias in ["Upper", "has-dash", "self", "_", "let"] {
            let temp = TempDir::new();
            temp.write("root/src/main.sali", "let main() = 0\n");
            temp.write(
                "root/salicin.toml",
                &format!(
                    "[package]\nname = \"root\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\n[dependencies]\n{alias} = {{ path = \"../dep\" }}\n"
                ),
            );
            let error = load_manifest(temp.path().join("root"))
                .unwrap_err()
                .to_string();
            assert!(error.contains("ASCII snake_case"), "{error}");
        }

        let temp = TempDir::new();
        temp.write("root/src/main.sali", "let main() = 0\n");
        for path in [
            "/dep",
            "C:/dep",
            r"C:dep",
            r"\dep",
            r"\\server\share",
            r"..\dep",
            "named:stream",
        ] {
            temp.write(
                "root/salicin.toml",
                &format!(
                    "[package]\nname = \"root\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\n[dependencies]\ndep = {{ path = '{path}' }}\n"
                ),
            );
            let non_portable = load_manifest(temp.path().join("root"))
                .unwrap_err()
                .to_string();
            assert!(
                non_portable.contains("portable relative path"),
                "{non_portable}"
            );
        }

        temp.write(
            "root/salicin.toml",
            "[package]\nname = \"root\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\n[dependencies]\ndep = { path = \"../missing\" }\n",
        );
        let missing = load_manifest(temp.path().join("root"))
            .unwrap_err()
            .to_string();
        assert!(missing.contains("does not contain"), "{missing}");
    }

    #[test]
    fn recursively_loads_diamond_graph_once_and_rejects_canonical_cycles() {
        let temp = TempDir::new();
        write_test_package(&temp, "shared", "shared", "");
        write_test_package(
            &temp,
            "left",
            "left",
            "\n[dependencies]\nshared = { path = \"../shared\" }\n",
        );
        write_test_package(
            &temp,
            "right",
            "right",
            "\n[dependencies]\nshared = { path = \"../shared/salicin.toml\" }\n",
        );
        write_test_package(
            &temp,
            "root",
            "root",
            "\n[dependencies]\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n",
        );

        let graph = load_dependency_graph(temp.path().join("root")).unwrap();
        assert_eq!(graph.packages.len(), 4);
        assert_eq!(graph.root().package.name, "root");
        assert_eq!(
            graph
                .packages
                .iter()
                .filter(|manifest| manifest.package.name == "shared")
                .count(),
            1
        );

        write_test_package(
            &temp,
            "cycle-a",
            "cycle-a",
            "\n[dependencies]\nb = { path = \"../cycle-b\" }\n",
        );
        write_test_package(
            &temp,
            "cycle-b",
            "cycle-b",
            "\n[dependencies]\na = { path = \"../cycle-a/salicin.toml\" }\n",
        );
        let cycle = load_dependency_graph(temp.path().join("cycle-a"))
            .unwrap_err()
            .to_string();
        assert!(cycle.contains("dependency cycle"), "{cycle}");
        assert!(
            cycle.contains("cycle-a") && cycle.contains("cycle-b"),
            "{cycle}"
        );
    }

    #[test]
    fn dependency_graph_requires_library_targets() {
        let temp = TempDir::new();
        temp.write("app/src/main.sali", "let main() = 0\n");
        temp.write(
            "app/salicin.toml",
            "[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\n[dependencies]\ntool = { path = \"../tool\" }\n",
        );
        temp.write("tool/src/main.sali", "let main() = 0\n");
        temp.write(
            "tool/salicin.toml",
            "[package]\nname = \"tool\"\nversion = \"1.0.0\"\nedition = \"2026\"\n",
        );

        let error = load_dependency_graph(temp.path().join("app"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("library target"), "{error}");
    }

    fn write_test_package(temp: &TempDir, directory: &str, name: &str, extra: &str) {
        temp.write(format!("{directory}/src/lib.sali"), "let answer = 42\n");
        temp.write(
            format!("{directory}/salicin.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nedition = \"2026\"\n{extra}"
            ),
        );
    }

    #[test]
    fn rejects_duplicate_binary_target_names() {
        let temp = TempDir::new();
        temp.write("src/one.sali", "let main() = 0\n");
        temp.write("src/two.sali", "let main() = 0\n");
        temp.write(
            MANIFEST_FILE_NAME,
            &basic_manifest(
                r#"
[[bin]]
name = "tool"
path = "src/one.sali"

[[bin]]
name = "tool"
path = "src/two.sali"
"#,
            ),
        );

        let error = load_manifest(temp.path()).unwrap_err().to_string();
        assert!(error.contains("declared more than once"), "{error}");
    }

    #[test]
    fn rejects_binary_names_that_could_escape_the_build_directory() {
        let temp = TempDir::new();
        temp.write("src/main.sali", "let main() = 0\n");
        temp.write(
            MANIFEST_FILE_NAME,
            &basic_manifest(
                r#"
[[bin]]
name = "../outside"
path = "src/main.sali"
"#,
            ),
        );

        let error = load_manifest(temp.path()).unwrap_err().to_string();
        assert!(error.contains("ASCII kebab-case"), "{error}");
    }

    #[test]
    fn validates_target_paths() {
        let temp = TempDir::new();
        temp.write("outside.sali", "let main() = 0\n");
        temp.write("package/src/not-salicin.txt", "text\n");

        for (path, expected) in [
            ("../outside.sali", "escapes the package root"),
            ("src/missing.sali", "does not exist"),
            ("src/not-salicin.txt", "`.sali` extension"),
        ] {
            temp.write(
                "package/salicin.toml",
                &format!(
                    "[package]\nname = \"hello\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\n[[bin]]\nname = \"hello\"\npath = \"{path}\"\n"
                ),
            );

            let error = load_manifest(temp.path().join("package"))
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error}");
        }
    }
}
