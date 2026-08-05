//! Verified, atomic source materialization for registry archives.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use unicode_normalization::UnicodeNormalization;

use crate::manifest::{load_manifest, Manifest, MANIFEST_FILE_NAME};
use crate::registry::{
    archive_cache_path, source_cache_path, RegistryProviderIdentity, RegistryRelease, Sha256Digest,
};

pub const MAX_REGISTRY_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_REGISTRY_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_REGISTRY_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_REGISTRY_ARCHIVE_ENTRIES: usize = 16_384;

const SOURCE_METADATA_FILE: &str = ".salicin-registry-source.toml";
const SOURCE_METADATA_FORMAT: u32 = 1;
const MAX_PUBLICATION_ATTEMPTS: usize = 16;
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryMaterializeOutcome {
    Published,
    AlreadyPresent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRegistrySource {
    pub provider: RegistryProviderIdentity,
    pub root: PathBuf,
    pub manifest: Manifest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrySourceLookup {
    Hit(Box<VerifiedRegistrySource>),
    Missing,
    Corrupt(String),
}

#[derive(Debug)]
pub enum RegistryCacheError {
    RootMustBeAbsolute,
    UnsafeCachePath(String),
    ArchiveChecksumMismatch {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
    ArchiveTooLarge(usize),
    Archive(String),
    Manifest(String),
    Io(io::Error),
    PublicationContended,
}

impl fmt::Display for RegistryCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootMustBeAbsolute => formatter.write_str("registry cache root must be absolute"),
            Self::UnsafeCachePath(message) | Self::Archive(message) | Self::Manifest(message) => {
                formatter.write_str(message)
            }
            Self::ArchiveChecksumMismatch { expected, actual } => write!(
                formatter,
                "registry archive checksum mismatch: expected {expected}, found {actual}"
            ),
            Self::ArchiveTooLarge(size) => write!(
                formatter,
                "registry archive is {size} bytes; maximum compressed size is {MAX_REGISTRY_ARCHIVE_BYTES}"
            ),
            Self::Io(error) => write!(formatter, "registry cache I/O failed: {error}"),
            Self::PublicationContended => {
                formatter.write_str("registry source publication remained contended")
            }
        }
    }
}

impl Error for RegistryCacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for RegistryCacheError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub struct RegistrySourceStore {
    root: PathBuf,
}

impl RegistrySourceStore {
    pub fn open(root: PathBuf) -> Result<Self, RegistryCacheError> {
        if !root.is_absolute() {
            return Err(RegistryCacheError::RootMustBeAbsolute);
        }
        fs::create_dir_all(&root)?;
        let metadata = fs::symlink_metadata(&root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RegistryCacheError::UnsafeCachePath(format!(
                "registry cache root `{}` must be a real directory",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lookup(
        &self,
        provider: &RegistryProviderIdentity,
        release: &RegistryRelease,
    ) -> RegistrySourceLookup {
        let path = source_cache_path(&self.root, &provider.archive_sha256);
        match validate_published_source(&path, provider, release) {
            Ok(source) => RegistrySourceLookup::Hit(Box::new(source)),
            Err(ValidationFailure::Missing) => RegistrySourceLookup::Missing,
            Err(ValidationFailure::Corrupt(message)) => RegistrySourceLookup::Corrupt(message),
        }
    }

    /// Verify the compressed bytes and complete extraction tree before either
    /// archive or source data becomes visible at its content address.
    pub fn materialize(
        &self,
        provider: &RegistryProviderIdentity,
        release: &RegistryRelease,
        archive_bytes: &[u8],
    ) -> Result<(RegistryMaterializeOutcome, VerifiedRegistrySource), RegistryCacheError> {
        validate_provider_release(provider, release)?;
        if archive_bytes.len() > MAX_REGISTRY_ARCHIVE_BYTES {
            return Err(RegistryCacheError::ArchiveTooLarge(archive_bytes.len()));
        }
        let actual = Sha256Digest::of(archive_bytes);
        if actual != provider.archive_sha256 {
            return Err(RegistryCacheError::ArchiveChecksumMismatch {
                expected: provider.archive_sha256.clone(),
                actual,
            });
        }

        let tree = parse_archive(provider, archive_bytes)?;
        let source_path = source_cache_path(&self.root, &provider.archive_sha256);
        if let RegistrySourceLookup::Hit(source) = self.lookup(provider, release) {
            publish_verified_archive(&self.root, provider, archive_bytes)?;
            return Ok((RegistryMaterializeOutcome::AlreadyPresent, *source));
        }

        let parent = source_path
            .parent()
            .expect("source cache mapping always has a parent");
        ensure_private_directory_path(&self.root, parent)?;
        let staging = create_unique_directory(parent, "source-tmp")?;
        let mut staging_guard = RemoveGuard::new(staging.clone());
        write_tree(&staging, &tree)?;
        let staging_manifest = validate_manifest_identity(&staging, provider, release)?;
        let metadata = SourceMetadata::new(provider, &tree);
        write_private_new_file(
            &staging.join(SOURCE_METADATA_FILE),
            canonical_metadata(&metadata)?.as_bytes(),
        )?;
        sync_directory_best_effort(&staging);
        validate_tree_and_metadata(&staging, provider, release, &metadata)
            .map_err(validation_as_cache_error)?;
        drop(staging_manifest);

        publish_verified_archive(&self.root, provider, archive_bytes)?;
        for _ in 0..MAX_PUBLICATION_ATTEMPTS {
            match self.lookup(provider, release) {
                RegistrySourceLookup::Hit(source) => {
                    return Ok((RegistryMaterializeOutcome::AlreadyPresent, *source));
                }
                RegistrySourceLookup::Missing => {}
                RegistrySourceLookup::Corrupt(_) => {
                    detach_invalid_path(parent, &source_path)?;
                }
            }
            match fs::rename(&staging, &source_path) {
                Ok(()) => {
                    staging_guard.disarm();
                    sync_directory_best_effort(parent);
                    let source = match self.lookup(provider, release) {
                        RegistrySourceLookup::Hit(source) => *source,
                        RegistrySourceLookup::Missing => {
                            return Err(RegistryCacheError::Io(io::Error::new(
                                io::ErrorKind::NotFound,
                                "published registry source disappeared",
                            )))
                        }
                        RegistrySourceLookup::Corrupt(message) => {
                            return Err(RegistryCacheError::Archive(format!(
                                "published registry source did not revalidate: {message}"
                            )))
                        }
                    };
                    return Ok((RegistryMaterializeOutcome::Published, source));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(RegistryCacheError::Io(error)),
            }
        }
        Err(RegistryCacheError::PublicationContended)
    }
}

#[derive(Debug)]
struct ValidatedTree {
    directories: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, Vec<u8>>,
    digest: Sha256Digest,
    total_bytes: u64,
}

fn parse_archive(
    provider: &RegistryProviderIdentity,
    archive_bytes: &[u8],
) -> Result<ValidatedTree, RegistryCacheError> {
    let expected_root = format!("{}-{}", provider.name, provider.version);
    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(decoder);
    let entries = archive.entries().map_err(|error| {
        RegistryCacheError::Archive(format!("invalid registry tar.gz: {error}"))
    })?;
    let mut directories = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total_bytes = 0u64;
    let mut count = 0usize;
    for entry in entries {
        count += 1;
        if count > MAX_REGISTRY_ARCHIVE_ENTRIES {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive has more than {MAX_REGISTRY_ARCHIVE_ENTRIES} entries"
            )));
        }
        let mut entry = entry.map_err(|error| {
            RegistryCacheError::Archive(format!("invalid registry tar entry: {error}"))
        })?;
        let path = entry.path().map_err(|error| {
            RegistryCacheError::Archive(format!("invalid registry archive path: {error}"))
        })?;
        let relative = validate_archive_entry_path(&path, &expected_root)?;
        let entry_type = entry.header().entry_type();
        if relative.as_os_str().is_empty() {
            if entry_type.is_dir() {
                continue;
            }
            return Err(RegistryCacheError::Archive(format!(
                "registry archive root `{expected_root}` must be a directory"
            )));
        }
        if !seen.insert(relative.clone()) {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive repeats path `{}`",
                relative.display()
            )));
        }
        for parent in relative
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if files.contains_key(parent) {
                return Err(RegistryCacheError::Archive(format!(
                    "registry archive path `{}` traverses file `{}`",
                    relative.display(),
                    parent.display()
                )));
            }
        }
        add_parent_directories(&relative, &mut directories);
        if entry_type.is_dir() {
            if files.contains_key(&relative) {
                return Err(RegistryCacheError::Archive(format!(
                    "registry archive path `{}` is both a file and directory",
                    relative.display()
                )));
            }
            directories.insert(relative);
            continue;
        }
        if entry_type != tar::EntryType::Regular {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive entry `{}` is not a regular file or directory; links and special files are forbidden",
                relative.display()
            )));
        }
        let declared_size = entry.header().size().map_err(|error| {
            RegistryCacheError::Archive(format!(
                "registry archive entry `{}` has invalid size: {error}",
                relative.display()
            ))
        })?;
        if declared_size > MAX_REGISTRY_FILE_BYTES {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive file `{}` exceeds the per-file limit of {MAX_REGISTRY_FILE_BYTES} bytes",
                relative.display()
            )));
        }
        total_bytes = total_bytes
            .checked_add(declared_size)
            .ok_or_else(|| RegistryCacheError::Archive("registry archive size overflow".into()))?;
        if total_bytes > MAX_REGISTRY_SOURCE_BYTES {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive exceeds the expanded-source limit of {MAX_REGISTRY_SOURCE_BYTES} bytes"
            )));
        }
        let mut contents = Vec::with_capacity(declared_size as usize);
        entry
            .by_ref()
            .take(MAX_REGISTRY_FILE_BYTES + 1)
            .read_to_end(&mut contents)
            .map_err(|error| {
                RegistryCacheError::Archive(format!(
                    "could not read registry archive entry `{}`: {error}",
                    relative.display()
                ))
            })?;
        if contents.len() as u64 != declared_size {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive entry `{}` size does not match its header",
                relative.display()
            )));
        }
        if directories.contains(&relative) {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive path `{}` is both a file and directory",
                relative.display()
            )));
        }
        files.insert(relative, contents);
    }
    if !files.contains_key(Path::new(MANIFEST_FILE_NAME)) {
        return Err(RegistryCacheError::Archive(format!(
            "registry archive must contain `{expected_root}/{MANIFEST_FILE_NAME}`"
        )));
    }
    let digest = tree_digest(&directories, &files);
    Ok(ValidatedTree {
        directories,
        files,
        digest,
        total_bytes,
    })
}

fn validate_archive_entry_path(
    path: &Path,
    expected_root: &str,
) -> Result<PathBuf, RegistryCacheError> {
    if path.is_absolute() {
        return Err(RegistryCacheError::Archive(format!(
            "registry archive path `{}` must be relative",
            path.display()
        )));
    }
    let mut components = path.components();
    let Some(Component::Normal(root)) = components.next() else {
        return Err(RegistryCacheError::Archive(format!(
            "registry archive path `{}` has no package root",
            path.display()
        )));
    };
    if root.to_str() != Some(expected_root) {
        return Err(RegistryCacheError::Archive(format!(
            "registry archive path `{}` must be rooted at `{expected_root}/`",
            path.display()
        )));
    }
    let mut relative = PathBuf::new();
    for component in components {
        let Component::Normal(component) = component else {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive path `{}` contains traversal or a non-portable component",
                path.display()
            )));
        };
        let text = component.to_str().ok_or_else(|| {
            RegistryCacheError::Archive(format!(
                "registry archive path `{}` is not valid UTF-8",
                path.display()
            ))
        })?;
        if text.is_empty()
            || text.contains(['\\', ':', '\0'])
            || text.chars().any(char::is_control)
            || text.nfc().collect::<String>() != text
        {
            return Err(RegistryCacheError::Archive(format!(
                "registry archive path component `{text}` is not portable NFC text"
            )));
        }
        relative.push(text);
    }
    Ok(relative)
}

fn add_parent_directories(path: &Path, directories: &mut BTreeSet<PathBuf>) {
    let mut parent = path.parent();
    while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
        directories.insert(path.to_path_buf());
        parent = path.parent();
    }
}

fn tree_digest(
    directories: &BTreeSet<PathBuf>,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Sha256Digest {
    let mut digest = Sha256::new();
    digest.update(b"salicin-registry-source-tree-v1\0");
    for directory in directories {
        digest.update(b"D");
        update_path_digest(&mut digest, directory);
    }
    for (path, contents) in files {
        digest.update(b"F");
        update_path_digest(&mut digest, path);
        digest.update((contents.len() as u64).to_be_bytes());
        digest.update(contents);
    }
    Sha256Digest::from_sha256(digest)
}

fn update_path_digest(digest: &mut Sha256, path: &Path) {
    let path = path
        .to_str()
        .expect("validated archive paths are UTF-8")
        .replace(std::path::MAIN_SEPARATOR, "/");
    digest.update((path.len() as u64).to_be_bytes());
    digest.update(path.as_bytes());
}

fn write_tree(root: &Path, tree: &ValidatedTree) -> Result<(), RegistryCacheError> {
    for directory in &tree.directories {
        ensure_private_directory_path(root, &root.join(directory))?;
    }
    for (path, contents) in &tree.files {
        let target = root.join(path);
        let parent = target.parent().expect("a source file has a parent");
        ensure_private_directory_path(root, parent)?;
        write_private_new_file(&target, contents)?;
    }
    Ok(())
}

fn validate_provider_release(
    provider: &RegistryProviderIdentity,
    release: &RegistryRelease,
) -> Result<(), RegistryCacheError> {
    if provider.version != release.version || provider.archive_sha256 != release.archive_sha256 {
        return Err(RegistryCacheError::Archive(format!(
            "selected provider `{}/{}@{}` does not match its registry release metadata",
            provider.registry, provider.name, provider.version
        )));
    }
    Ok(())
}

fn validate_manifest_identity(
    root: &Path,
    provider: &RegistryProviderIdentity,
    release: &RegistryRelease,
) -> Result<Manifest, RegistryCacheError> {
    let manifest = load_manifest(root).map_err(|error| {
        RegistryCacheError::Manifest(format!(
            "registry package `{}/{}@{}` has invalid manifest: {error}",
            provider.registry, provider.name, provider.version
        ))
    })?;
    if manifest.package.name != provider.name || manifest.package.version != provider.version {
        return Err(RegistryCacheError::Manifest(format!(
            "registry package manifest identity `{}@{}` does not match selected provider `{}@{}`",
            manifest.package.name, manifest.package.version, provider.name, provider.version
        )));
    }
    if manifest.lib.is_none() {
        return Err(RegistryCacheError::Manifest(format!(
            "registry package `{}@{}` does not provide a library target",
            provider.name, provider.version
        )));
    }
    if !manifest.dependencies.is_empty() {
        return Err(RegistryCacheError::Manifest(format!(
            "registry package `{}@{}` may not contain local path dependencies",
            provider.name, provider.version
        )));
    }
    let manifest_dependencies = manifest
        .registry_dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.alias.as_str(),
                dependency.package.as_str(),
                dependency.registry.as_str(),
                dependency.requirement.to_string(),
            )
        })
        .collect::<Vec<_>>();
    let index_dependencies = release
        .dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.alias.as_str(),
                dependency.package.as_str(),
                dependency.registry.as_str(),
                dependency.requirement.to_string(),
            )
        })
        .collect::<Vec<_>>();
    if manifest_dependencies != index_dependencies {
        return Err(RegistryCacheError::Manifest(format!(
            "registry package `{}@{}` manifest dependencies do not match the immutable index release",
            provider.name, provider.version
        )));
    }
    Ok(manifest)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceMetadata {
    format: u32,
    registry: String,
    snapshot: String,
    name: String,
    version: String,
    archive_sha256: String,
    tree_sha256: String,
    files: usize,
    directories: usize,
    total_bytes: u64,
}

impl SourceMetadata {
    fn new(provider: &RegistryProviderIdentity, tree: &ValidatedTree) -> Self {
        Self {
            format: SOURCE_METADATA_FORMAT,
            registry: provider.registry.clone(),
            snapshot: provider.snapshot.to_string(),
            name: provider.name.clone(),
            version: provider.version.to_string(),
            archive_sha256: provider.archive_sha256.to_string(),
            tree_sha256: tree.digest.to_string(),
            files: tree.files.len(),
            directories: tree.directories.len(),
            total_bytes: tree.total_bytes,
        }
    }
}

#[derive(Debug)]
enum ValidationFailure {
    Missing,
    Corrupt(String),
}

fn validate_published_source(
    root: &Path,
    provider: &RegistryProviderIdentity,
    release: &RegistryRelease,
) -> Result<VerifiedRegistrySource, ValidationFailure> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ValidationFailure::Missing
        } else {
            ValidationFailure::Corrupt(error.to_string())
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ValidationFailure::Corrupt(
            "source cache entry is not a real directory".into(),
        ));
    }
    let metadata_path = root.join(SOURCE_METADATA_FILE);
    let metadata_text = fs::read_to_string(&metadata_path)
        .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
    let source_metadata: SourceMetadata = toml::from_str(&metadata_text)
        .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
    if canonical_metadata(&source_metadata).ok().as_deref() != Some(metadata_text.as_str()) {
        return Err(ValidationFailure::Corrupt(
            "source cache metadata is not canonical".into(),
        ));
    }
    validate_tree_and_metadata(root, provider, release, &source_metadata)?;
    let manifest = validate_manifest_identity(root, provider, release)
        .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
    Ok(VerifiedRegistrySource {
        provider: provider.clone(),
        root: root.to_path_buf(),
        manifest,
    })
}

fn validate_tree_and_metadata(
    root: &Path,
    provider: &RegistryProviderIdentity,
    release: &RegistryRelease,
    metadata: &SourceMetadata,
) -> Result<(), ValidationFailure> {
    validate_provider_release(provider, release)
        .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
    let expected = SourceMetadata {
        format: SOURCE_METADATA_FORMAT,
        registry: provider.registry.clone(),
        snapshot: provider.snapshot.to_string(),
        name: provider.name.clone(),
        version: provider.version.to_string(),
        archive_sha256: provider.archive_sha256.to_string(),
        tree_sha256: metadata.tree_sha256.clone(),
        files: metadata.files,
        directories: metadata.directories,
        total_bytes: metadata.total_bytes,
    };
    if metadata != &expected {
        return Err(ValidationFailure::Corrupt(
            "source cache metadata does not match the selected provider".into(),
        ));
    }
    let tree = read_tree(root)?;
    if tree.digest.to_string() != metadata.tree_sha256
        || tree.files.len() != metadata.files
        || tree.directories.len() != metadata.directories
        || tree.total_bytes != metadata.total_bytes
    {
        return Err(ValidationFailure::Corrupt(
            "source cache tree digest or bounds metadata does not match".into(),
        ));
    }
    Ok(())
}

fn read_tree(root: &Path) -> Result<ValidatedTree, ValidationFailure> {
    fn visit(
        root: &Path,
        directory: &Path,
        directories: &mut BTreeSet<PathBuf>,
        files: &mut BTreeMap<PathBuf, Vec<u8>>,
        total_bytes: &mut u64,
        entries: &mut usize,
    ) -> Result<(), ValidationFailure> {
        let mut children = fs::read_dir(directory)
            .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            if directory == root && child.file_name() == SOURCE_METADATA_FILE {
                continue;
            }
            *entries += 1;
            if *entries > MAX_REGISTRY_ARCHIVE_ENTRIES {
                return Err(ValidationFailure::Corrupt(
                    "source cache tree exceeds the entry limit".into(),
                ));
            }
            let relative = path.strip_prefix(root).map_err(|_| {
                ValidationFailure::Corrupt("source cache path escaped its root".into())
            })?;
            validate_cached_relative_path(relative)?;
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(ValidationFailure::Corrupt(format!(
                    "source cache path `{}` is a symbolic link",
                    relative.display()
                )));
            }
            if metadata.is_dir() {
                directories.insert(relative.to_path_buf());
                visit(root, &path, directories, files, total_bytes, entries)?;
            } else if metadata.is_file() {
                if metadata.len() > MAX_REGISTRY_FILE_BYTES {
                    return Err(ValidationFailure::Corrupt(format!(
                        "source cache file `{}` exceeds the size limit",
                        relative.display()
                    )));
                }
                *total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
                    ValidationFailure::Corrupt("source cache size overflow".into())
                })?;
                if *total_bytes > MAX_REGISTRY_SOURCE_BYTES {
                    return Err(ValidationFailure::Corrupt(
                        "source cache tree exceeds the total size limit".into(),
                    ));
                }
                let contents = fs::read(&path)
                    .map_err(|error| ValidationFailure::Corrupt(error.to_string()))?;
                files.insert(relative.to_path_buf(), contents);
            } else {
                return Err(ValidationFailure::Corrupt(format!(
                    "source cache path `{}` is not a regular file or directory",
                    relative.display()
                )));
            }
        }
        Ok(())
    }

    let mut directories = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut total_bytes = 0;
    let mut entries = 0;
    visit(
        root,
        root,
        &mut directories,
        &mut files,
        &mut total_bytes,
        &mut entries,
    )?;
    let digest = tree_digest(&directories, &files);
    Ok(ValidatedTree {
        directories,
        files,
        digest,
        total_bytes,
    })
}

fn validate_cached_relative_path(path: &Path) -> Result<(), ValidationFailure> {
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(ValidationFailure::Corrupt(
                "source cache contains a non-portable path".into(),
            ));
        };
        let text = component
            .to_str()
            .ok_or_else(|| ValidationFailure::Corrupt("source cache path is not UTF-8".into()))?;
        if text.contains(['\\', ':', '\0'])
            || text.chars().any(char::is_control)
            || text.nfc().collect::<String>() != text
        {
            return Err(ValidationFailure::Corrupt(
                "source cache contains a non-portable path".into(),
            ));
        }
    }
    Ok(())
}

fn validation_as_cache_error(error: ValidationFailure) -> RegistryCacheError {
    match error {
        ValidationFailure::Missing => {
            RegistryCacheError::Archive("new registry source staging directory disappeared".into())
        }
        ValidationFailure::Corrupt(message) => RegistryCacheError::Archive(message),
    }
}

fn canonical_metadata(metadata: &SourceMetadata) -> Result<String, RegistryCacheError> {
    toml::to_string(metadata)
        .map_err(|error| RegistryCacheError::Archive(format!("metadata encoding failed: {error}")))
}

fn publish_verified_archive(
    root: &Path,
    provider: &RegistryProviderIdentity,
    bytes: &[u8],
) -> Result<(), RegistryCacheError> {
    let final_path = archive_cache_path(root, &provider.archive_sha256);
    let parent = final_path
        .parent()
        .expect("archive cache mapping always has a parent");
    ensure_private_directory_path(root, parent)?;
    if archive_file_is_valid(&final_path, &provider.archive_sha256) {
        return Ok(());
    }
    if final_path.exists() {
        detach_invalid_path(parent, &final_path)?;
    }
    let temporary = unique_path(parent, "archive-tmp");
    let mut guard = RemoveGuard::new(temporary.clone());
    write_private_new_file(&temporary, bytes)?;
    if !archive_file_is_valid(&temporary, &provider.archive_sha256) {
        return Err(RegistryCacheError::Archive(
            "new archive cache file did not revalidate".into(),
        ));
    }
    match fs::rename(&temporary, &final_path) {
        Ok(()) => {
            guard.disarm();
            sync_directory_best_effort(parent);
            Ok(())
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) && archive_file_is_valid(&final_path, &provider.archive_sha256) =>
        {
            Ok(())
        }
        Err(error) => Err(RegistryCacheError::Io(error)),
    }
}

fn archive_file_is_valid(path: &Path, expected: &Sha256Digest) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_REGISTRY_ARCHIVE_BYTES as u64
    {
        return false;
    }
    fs::read(path)
        .ok()
        .is_some_and(|bytes| Sha256Digest::of(&bytes) == *expected)
}

fn ensure_private_directory_path(root: &Path, target: &Path) -> Result<(), RegistryCacheError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        RegistryCacheError::UnsafeCachePath(format!(
            "registry cache directory `{}` escaped `{}`",
            target.display(),
            root.display()
        ))
    })?;
    let root_metadata = fs::symlink_metadata(root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(RegistryCacheError::UnsafeCachePath(format!(
            "registry cache root `{}` is a symlink or non-directory",
            root.display()
        )));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(_) = component else {
            return Err(RegistryCacheError::UnsafeCachePath(format!(
                "registry cache directory `{}` is not beneath `{}`",
                target.display(),
                root.display()
            )));
        };
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(RegistryCacheError::UnsafeCachePath(format!(
                        "registry cache path `{}` contains a symlink or non-directory",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                create_private_directory(&current)?;
            }
            Err(error) => return Err(RegistryCacheError::Io(error)),
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_private_new_file(path: &Path, contents: &[u8]) -> Result<(), RegistryCacheError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

fn create_unique_directory(parent: &Path, kind: &str) -> Result<PathBuf, RegistryCacheError> {
    for _ in 0..100 {
        let path = unique_path(parent, kind);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(RegistryCacheError::Io(error)),
        }
    }
    Err(RegistryCacheError::PublicationContended)
}

fn unique_path(parent: &Path, kind: &str) -> PathBuf {
    let id = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{kind}-{}-{nonce}-{id}", std::process::id()))
}

fn detach_invalid_path(parent: &Path, path: &Path) -> Result<(), RegistryCacheError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(RegistryCacheError::Io(error)),
        Ok(_) => {}
    }
    let detached = unique_path(parent, "invalid");
    match fs::rename(path, &detached) {
        Ok(()) => {
            remove_path_no_follow(&detached);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RegistryCacheError::Io(error)),
    }
}

fn remove_path_no_follow(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn sync_directory_best_effort(path: &Path) {
    let _ = File::open(path).and_then(|file| file.sync_all());
}

struct RemoveGuard {
    path: PathBuf,
    armed: bool,
}

impl RemoveGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RemoveGuard {
    fn drop(&mut self) {
        if self.armed {
            remove_path_no_follow(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use flate2::write::GzEncoder;
    use flate2::Compression;
    use semver::{Version, VersionReq};
    use tar::{Builder, EntryType, Header};

    use crate::registry::RegistryRequirement;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "salicin-registry-cache-test-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);
        for (path, contents) in entries {
            let mut header = Header::new_gnu();
            header.set_entry_type(EntryType::Regular);
            header.set_mode(0o644);
            header.set_size(contents.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, path, Cursor::new(contents))
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn linked_archive() -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_mode(0o777);
        header.set_size(0);
        header.set_link_name("../../escape").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "answer-kit-1.2.3/src/link", io::empty())
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn manifest(name: &str) -> String {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"1.2.3\"\nedition = \"2026\"\n\n[lib]\npath = \"src/lib.sc\"\n"
        )
    }

    fn identities(
        bytes: &[u8],
        dependencies: Vec<RegistryRequirement>,
    ) -> (RegistryProviderIdentity, RegistryRelease) {
        let checksum = Sha256Digest::of(bytes);
        let version = Version::parse("1.2.3").unwrap();
        let provider = RegistryProviderIdentity {
            registry: "main".into(),
            snapshot: Sha256Digest::parse(&"a".repeat(64)).unwrap(),
            name: "answer-kit".into(),
            version: version.clone(),
            archive_sha256: checksum.clone(),
        };
        let release = RegistryRelease {
            version,
            yanked: false,
            archive_sha256: checksum,
            archive: "archives/answer-kit-1.2.3.tar.gz".into(),
            dependencies,
        };
        (provider, release)
    }

    fn valid_archive() -> Vec<u8> {
        let manifest = manifest("answer-kit");
        archive(&[
            ("answer-kit-1.2.3/salicin.toml", manifest.as_bytes()),
            (
                "answer-kit-1.2.3/src/lib.sc",
                b"pub let answer(): i32 = { 42 }",
            ),
        ])
    }

    #[test]
    fn materializes_revalidates_and_repairs_content_addressed_source() {
        let temp = TempDir::new();
        let store = RegistrySourceStore::open(temp.path().join("cache")).unwrap();
        let bytes = valid_archive();
        let (provider, release) = identities(&bytes, vec![]);

        let (outcome, source) = store.materialize(&provider, &release, &bytes).unwrap();
        assert_eq!(outcome, RegistryMaterializeOutcome::Published);
        assert_eq!(source.manifest.package.name, "answer-kit");
        assert!(archive_cache_path(store.root(), &provider.archive_sha256).is_file());
        assert!(matches!(
            store.lookup(&provider, &release),
            RegistrySourceLookup::Hit(_)
        ));
        let reopened = RegistrySourceStore::open(store.root().to_path_buf()).unwrap();
        assert!(matches!(
            reopened.lookup(&provider, &release),
            RegistrySourceLookup::Hit(_)
        ));

        let (outcome, _) = store.materialize(&provider, &release, &bytes).unwrap();
        assert_eq!(outcome, RegistryMaterializeOutcome::AlreadyPresent);

        fs::write(source.root.join("src/lib.sc"), "tampered").unwrap();
        assert!(matches!(
            store.lookup(&provider, &release),
            RegistrySourceLookup::Corrupt(_)
        ));
        let (outcome, repaired) = store.materialize(&provider, &release, &bytes).unwrap();
        assert_eq!(outcome, RegistryMaterializeOutcome::Published);
        assert_eq!(
            fs::read_to_string(repaired.root.join("src/lib.sc")).unwrap(),
            "pub let answer(): i32 = { 42 }"
        );
    }

    #[test]
    fn rejects_checksum_links_and_manifest_identity_mismatch() {
        let temp = TempDir::new();
        let store = RegistrySourceStore::open(temp.path().join("cache")).unwrap();
        let valid = valid_archive();
        let (provider, release) = identities(&valid, vec![]);
        let error = store
            .materialize(&provider, &release, b"not the archive")
            .unwrap_err();
        assert!(matches!(
            error,
            RegistryCacheError::ArchiveChecksumMismatch { .. }
        ));

        let linked = linked_archive();
        let (provider, release) = identities(&linked, vec![]);
        assert!(store
            .materialize(&provider, &release, &linked)
            .unwrap_err()
            .to_string()
            .contains("links and special files"));

        let wrong_manifest = manifest("other-name");
        let wrong = archive(&[
            ("answer-kit-1.2.3/salicin.toml", wrong_manifest.as_bytes()),
            (
                "answer-kit-1.2.3/src/lib.sc",
                b"pub let answer(): i32 = { 42 }",
            ),
        ]);
        let (provider, release) = identities(&wrong, vec![]);
        assert!(store
            .materialize(&provider, &release, &wrong)
            .unwrap_err()
            .to_string()
            .contains("does not match selected provider"));
    }

    #[test]
    fn rejects_index_manifest_dependency_mismatch() {
        let temp = TempDir::new();
        let store = RegistrySourceStore::open(temp.path().join("cache")).unwrap();
        let dependency = RegistryRequirement {
            alias: "util".into(),
            package: "util".into(),
            registry: "main".into(),
            requirement: VersionReq::parse("^1").unwrap(),
        };
        let bytes = valid_archive();
        let (provider, release) = identities(&bytes, vec![dependency]);
        assert!(store
            .materialize(&provider, &release, &bytes)
            .unwrap_err()
            .to_string()
            .contains("do not match the immutable index"));
    }

    #[test]
    fn rejects_traversal_and_non_portable_archive_paths() {
        assert!(validate_archive_entry_path(
            Path::new("answer-kit-1.2.3/../escape"),
            "answer-kit-1.2.3"
        )
        .is_err());
        assert!(validate_archive_entry_path(
            Path::new("answer-kit-1.2.3/src\\escape"),
            "answer-kit-1.2.3"
        )
        .is_err());
        assert!(validate_archive_entry_path(
            Path::new("other-1.2.3/src/lib.sc"),
            "answer-kit-1.2.3"
        )
        .is_err());
    }
}
