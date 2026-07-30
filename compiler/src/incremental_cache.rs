//! Strict local storage for whole-graph incremental LLVM-IR artifacts.

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::incremental::{
    cache_entry_relative_path, IncrementalFingerprint, CACHE_ARTIFACT_SCHEMA_VERSION,
    CACHE_DIRECTORY_ENV, CACHE_METADATA_FILE, CACHE_PAYLOAD_FILE,
};
use crate::manifest::Edition;

const CACHE_KIND: &str = "llvm-ir";
const CACHE_ROOT_MARKER: &str = ".salicin-cache-root";
const CACHE_ROOT_MARKER_CONTENT: &str = "salicin-cache-root-v1\n";
const MAX_METADATA_BYTES: u64 = 16 * 1024;
const MAX_PUBLICATION_ATTEMPTS: usize = 16;
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheRootResolution {
    Available(PathBuf),
    Disabled(CacheRootDisabled),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheRootDisabled {
    RelativeOverride,
    NoAbsolutePlatformDirectory,
}

/// Resolve the cache root without creating it.
pub fn resolve_cache_root() -> CacheRootResolution {
    resolve_cache_root_with(|name| env::var_os(name))
}

fn resolve_cache_root_with(
    mut variable: impl FnMut(&str) -> Option<OsString>,
) -> CacheRootResolution {
    if let Some(override_path) = variable(CACHE_DIRECTORY_ENV).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(override_path);
        return if path.is_absolute() {
            CacheRootResolution::Available(path)
        } else {
            CacheRootResolution::Disabled(CacheRootDisabled::RelativeOverride)
        };
    }

    #[cfg(target_os = "macos")]
    let candidate = variable("HOME").map(|home| PathBuf::from(home).join("Library/Caches/salicin"));

    #[cfg(windows)]
    let candidate =
        variable("LOCALAPPDATA").map(|root| PathBuf::from(root).join("salicin").join("cache"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let candidate = variable("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| variable("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|root| root.join("salicin"));

    #[cfg(not(any(unix, windows)))]
    let candidate: Option<PathBuf> = None;

    match candidate.filter(|path| path.is_absolute()) {
        Some(path) => CacheRootResolution::Available(path),
        None => CacheRootResolution::Disabled(CacheRootDisabled::NoAbsolutePlatformDirectory),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheTarget {
    Binary,
    Library,
    Test,
}

impl CacheTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Library => "library",
            Self::Test => "test",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheEntryIdentity {
    pub fingerprint: IncrementalFingerprint,
    pub edition: Edition,
    pub target: CacheTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheMissReason {
    Missing,
    Unreadable,
    InvalidEntryKind,
    InvalidMetadata,
    IncompatibleMetadata,
    InvalidPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheArtifact {
    pub llvm_ir: String,
    pub test_names: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheLookup {
    Hit(CacheArtifact),
    Miss(CacheMissReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachePublishOutcome {
    Published,
    AlreadyPresent,
}

#[derive(Debug)]
pub enum CacheStoreError {
    RootMustBeAbsolute,
    RootIsSymbolicLink,
    InvalidRootMarker,
    Io(io::Error),
    PublicationContended,
}

impl fmt::Display for CacheStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootMustBeAbsolute => formatter.write_str("cache root must be absolute"),
            Self::RootIsSymbolicLink => {
                formatter.write_str("cache root must not be a symbolic link")
            }
            Self::InvalidRootMarker => {
                formatter.write_str("cache root ownership marker is invalid")
            }
            Self::Io(error) => write!(formatter, "cache storage I/O failed: {error}"),
            Self::PublicationContended => {
                formatter.write_str("cache entry publication remained contended")
            }
        }
    }
}

impl std::error::Error for CacheStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CacheStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub struct CacheStore {
    root: PathBuf,
}

impl CacheStore {
    pub fn open(root: PathBuf) -> Result<Self, CacheStoreError> {
        if !root.is_absolute() {
            return Err(CacheStoreError::RootMustBeAbsolute);
        }
        ensure_private_directory(&root)?;
        let root_metadata = fs::symlink_metadata(&root)?;
        if root_metadata.file_type().is_symlink() {
            return Err(CacheStoreError::RootIsSymbolicLink);
        }
        if !root_metadata.is_dir() {
            return Err(CacheStoreError::Io(io::Error::new(
                io::ErrorKind::NotADirectory,
                "cache root is not a directory",
            )));
        }
        ensure_root_marker(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lookup(&self, identity: CacheEntryIdentity) -> CacheLookup {
        read_entry_at(
            &self
                .root
                .join(cache_entry_relative_path(identity.fingerprint)),
            identity,
        )
    }

    pub fn publish(
        &self,
        identity: CacheEntryIdentity,
        artifact: &CacheArtifact,
    ) -> Result<CachePublishOutcome, CacheStoreError> {
        let relative = cache_entry_relative_path(identity.fingerprint);
        let final_path = self.root.join(&relative);
        let parent = final_path
            .parent()
            .expect("cache entry mapping always has a parent");
        ensure_owned_directory_path(&self.root, parent)?;

        let temporary = create_unique_directory(parent, "tmp")?;
        let temporary_guard = RemoveDirectoryGuard::new(temporary.clone());
        let payload_hash = sha256_hex(artifact.llvm_ir.as_bytes());
        let metadata = CacheMetadata::for_entry(
            identity,
            artifact.llvm_ir.len() as u64,
            payload_hash,
            artifact.test_names.clone(),
        );

        write_private_file(
            &temporary.join(CACHE_PAYLOAD_FILE),
            artifact.llvm_ir.as_bytes(),
        )?;
        write_private_file(
            &temporary.join(CACHE_METADATA_FILE),
            canonical_metadata(&metadata)?.as_bytes(),
        )?;
        sync_directory_best_effort(&temporary);

        if read_entry_at(&temporary, identity) != CacheLookup::Hit(artifact.clone()) {
            return Err(CacheStoreError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "new cache entry did not validate",
            )));
        }

        for _ in 0..MAX_PUBLICATION_ATTEMPTS {
            match read_entry_at(&final_path, identity) {
                CacheLookup::Hit(_) => return Ok(CachePublishOutcome::AlreadyPresent),
                CacheLookup::Miss(CacheMissReason::Missing) => {}
                CacheLookup::Miss(_) => {
                    let invalid = unique_path(parent, "invalid");
                    match fs::rename(&final_path, &invalid) {
                        Ok(()) => remove_path_no_follow(&invalid),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) => {
                            if matches!(read_entry_at(&final_path, identity), CacheLookup::Hit(_)) {
                                return Ok(CachePublishOutcome::AlreadyPresent);
                            }
                            return Err(CacheStoreError::Io(error));
                        }
                    }
                }
            }

            match fs::rename(&temporary, &final_path) {
                Ok(()) => {
                    temporary_guard.disarm();
                    sync_directory_best_effort(parent);
                    return Ok(CachePublishOutcome::Published);
                }
                Err(error) => {
                    if matches!(read_entry_at(&final_path, identity), CacheLookup::Hit(_)) {
                        return Ok(CachePublishOutcome::AlreadyPresent);
                    }
                    if matches!(
                        error.kind(),
                        io::ErrorKind::AlreadyExists
                            | io::ErrorKind::PermissionDenied
                            | io::ErrorKind::DirectoryNotEmpty
                    ) {
                        continue;
                    }
                    return Err(CacheStoreError::Io(error));
                }
            }
        }
        Err(CacheStoreError::PublicationContended)
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CacheMetadata {
    schema: u32,
    kind: String,
    fingerprint: String,
    compiler: String,
    host_os: String,
    host_arch: String,
    edition: String,
    target: String,
    test_names: Vec<String>,
    test_names_sha256: String,
    payload_bytes: u64,
    payload_sha256: String,
}

impl CacheMetadata {
    fn for_entry(
        identity: CacheEntryIdentity,
        payload_bytes: u64,
        payload_sha256: String,
        test_names: Vec<String>,
    ) -> Self {
        let test_names_sha256 = test_names_digest(&test_names);
        Self {
            schema: CACHE_ARTIFACT_SCHEMA_VERSION,
            kind: CACHE_KIND.into(),
            fingerprint: identity.fingerprint.to_hex(),
            compiler: env!("CARGO_PKG_VERSION").into(),
            host_os: env::consts::OS.into(),
            host_arch: env::consts::ARCH.into(),
            edition: identity.edition.as_str().into(),
            target: identity.target.as_str().into(),
            test_names,
            test_names_sha256,
            payload_bytes,
            payload_sha256,
        }
    }

    fn is_compatible_with(&self, identity: CacheEntryIdentity) -> bool {
        self.schema == CACHE_ARTIFACT_SCHEMA_VERSION
            && self.kind == CACHE_KIND
            && self.fingerprint == identity.fingerprint.to_hex()
            && self.compiler == env!("CARGO_PKG_VERSION")
            && self.host_os == env::consts::OS
            && self.host_arch == env::consts::ARCH
            && self.edition == identity.edition.as_str()
            && self.target == identity.target.as_str()
            && test_names_are_valid(identity.target, &self.test_names)
            && self.test_names_sha256 == test_names_digest(&self.test_names)
            && is_lower_hex_digest(&self.payload_sha256)
    }
}

fn canonical_metadata(metadata: &CacheMetadata) -> Result<String, CacheStoreError> {
    toml::to_string(metadata).map_err(|error| {
        CacheStoreError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not encode cache metadata: {error}"),
        ))
    })
}

fn read_entry_at(path: &Path, identity: CacheEntryIdentity) -> CacheLookup {
    let entry_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return CacheLookup::Miss(CacheMissReason::Missing);
        }
        Err(_) => return CacheLookup::Miss(CacheMissReason::Unreadable),
    };
    if entry_metadata.file_type().is_symlink() || !entry_metadata.is_dir() {
        return CacheLookup::Miss(CacheMissReason::InvalidEntryKind);
    }

    let metadata_path = path.join(CACHE_METADATA_FILE);
    let metadata_bytes = match read_regular_file(&metadata_path, Some(MAX_METADATA_BYTES)) {
        Ok(bytes) => bytes,
        Err(reason) => return CacheLookup::Miss(reason),
    };
    let metadata_text = match std::str::from_utf8(&metadata_bytes) {
        Ok(text) => text,
        Err(_) => return CacheLookup::Miss(CacheMissReason::InvalidMetadata),
    };
    let metadata = match toml::from_str::<CacheMetadata>(metadata_text) {
        Ok(metadata) => metadata,
        Err(_) => return CacheLookup::Miss(CacheMissReason::InvalidMetadata),
    };
    if canonical_metadata(&metadata).ok().as_deref() != Some(metadata_text) {
        return CacheLookup::Miss(CacheMissReason::InvalidMetadata);
    }
    if !metadata.is_compatible_with(identity) {
        return CacheLookup::Miss(CacheMissReason::IncompatibleMetadata);
    }

    let payload_path = path.join(CACHE_PAYLOAD_FILE);
    let payload_bytes = match read_regular_file(&payload_path, None) {
        Ok(bytes) => bytes,
        Err(_) => return CacheLookup::Miss(CacheMissReason::InvalidPayload),
    };
    if payload_bytes.len() as u64 != metadata.payload_bytes
        || sha256_hex(&payload_bytes) != metadata.payload_sha256
    {
        return CacheLookup::Miss(CacheMissReason::InvalidPayload);
    }
    match String::from_utf8(payload_bytes) {
        Ok(llvm_ir) => CacheLookup::Hit(CacheArtifact {
            llvm_ir,
            test_names: metadata.test_names,
        }),
        Err(_) => CacheLookup::Miss(CacheMissReason::InvalidPayload),
    }
}

fn read_regular_file(path: &Path, maximum_bytes: Option<u64>) -> Result<Vec<u8>, CacheMissReason> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CacheMissReason::Unreadable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CacheMissReason::InvalidEntryKind);
    }
    if maximum_bytes.is_some_and(|maximum| metadata.len() > maximum) {
        return Err(CacheMissReason::InvalidMetadata);
    }
    fs::read(path).map_err(|_| CacheMissReason::Unreadable)
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    digest_hex(Sha256::digest(bytes))
}

fn digest_hex(digest: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_ref() {
        use fmt::Write;
        write!(output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

fn test_names_digest(names: &[String]) -> String {
    let mut encoder = Sha256::new();
    encoder.update((names.len() as u64).to_le_bytes());
    for name in names {
        encoder.update((name.len() as u64).to_le_bytes());
        encoder.update(name.as_bytes());
    }
    digest_hex(encoder.finalize())
}

fn test_names_are_valid(target: CacheTarget, names: &[String]) -> bool {
    if target != CacheTarget::Test {
        return names.is_empty();
    }
    let mut unique = std::collections::HashSet::new();
    names
        .iter()
        .all(|name| !name.is_empty() && unique.insert(name.as_str()))
}

fn ensure_root_marker(root: &Path) -> Result<(), CacheStoreError> {
    let marker = root.join(CACHE_ROOT_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) => {
            if !valid_root_marker(&marker, &metadata) {
                return Err(CacheStoreError::InvalidRootMarker);
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if !unowned_root_is_claimable(root)? {
                return match fs::symlink_metadata(&marker) {
                    Ok(metadata) if valid_root_marker(&marker, &metadata) => Ok(()),
                    _ => Err(CacheStoreError::InvalidRootMarker),
                };
            }
            let temporary = unique_path(root, "marker");
            let temporary_guard = RemoveFileGuard::new(temporary.clone());
            write_private_file_create_new(&temporary, CACHE_ROOT_MARKER_CONTENT.as_bytes())?;
            match fs::rename(&temporary, &marker) {
                Ok(()) => {
                    temporary_guard.disarm();
                    sync_directory_best_effort(root);
                    Ok(())
                }
                Err(_) => {
                    let metadata = fs::symlink_metadata(&marker)
                        .map_err(|_| CacheStoreError::InvalidRootMarker)?;
                    if valid_root_marker(&marker, &metadata) {
                        Ok(())
                    } else {
                        Err(CacheStoreError::InvalidRootMarker)
                    }
                }
            }
        }
        Err(error) => Err(CacheStoreError::Io(error)),
    }
}

fn unowned_root_is_claimable(root: &Path) -> Result<bool, CacheStoreError> {
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        if !name.to_string_lossy().starts_with(".marker-") {
            return Ok(false);
        }
    }
    Ok(true)
}

fn valid_root_marker(marker: &Path, metadata: &fs::Metadata) -> bool {
    !metadata.file_type().is_symlink()
        && metadata.is_file()
        && matches!(
            fs::read(marker),
            Ok(bytes) if bytes == CACHE_ROOT_MARKER_CONTENT.as_bytes()
        )
}

fn ensure_owned_directory_path(root: &Path, target: &Path) -> Result<(), CacheStoreError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        CacheStoreError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache directory escaped its root",
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        ensure_private_directory(&current)?;
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(CacheStoreError::Io(io::Error::new(
                io::ErrorKind::NotADirectory,
                "cache-owned path contains a non-directory or symbolic link",
            )));
        }
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match create_private_directory(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            match create_private_directory(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = private_file_options(false).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn write_private_file_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = private_file_options(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn private_file_options(create_new: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(!create_new);
    if create_new {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn create_unique_directory(parent: &Path, role: &str) -> io::Result<PathBuf> {
    for _ in 0..MAX_PUBLICATION_ATTEMPTS {
        let path = unique_path(parent, role);
        match create_private_directory(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique cache staging directory",
    ))
}

fn unique_path(parent: &Path, role: &str) -> PathBuf {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(
        ".{role}-{}-{time:x}-{counter:x}",
        std::process::id()
    ))
}

fn sync_directory_best_effort(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
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

struct RemoveDirectoryGuard(Option<PathBuf>);

impl RemoveDirectoryGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn disarm(mut self) {
        self.0 = None;
    }
}

struct RemoveFileGuard(Option<PathBuf>);

impl RemoveFileGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for RemoveFileGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

impl Drop for RemoveDirectoryGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            remove_path_no_follow(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;
    use crate::incremental::{fingerprint_package_graph, IncrementalTarget};
    use crate::modules::{PackageId, SourcePackage, SourceUnit};

    fn temporary_root() -> PathBuf {
        let root = env::temp_dir().join(format!(
            "salicin-incremental-cache-test-{}",
            unique_path(Path::new(""), "root")
                .file_name()
                .expect("unique name")
                .to_string_lossy()
        ));
        fs::create_dir(&root).expect("create temporary cache parent");
        root
    }

    fn identity(target: CacheTarget) -> CacheEntryIdentity {
        let package = SourcePackage {
            id: PackageId(0),
            name: "cache-test".into(),
            version: "1.0.0".into(),
            identity: "cache-test@1.0.0".into(),
            is_primary: true,
            dependencies: Default::default(),
            sources: vec![SourceUnit {
                path: "/checkout/main.sc".into(),
                module_path: Vec::new(),
                source: "let main(): i32 = { 42 }\n".into(),
                is_root: true,
            }],
        };
        let incremental_target = match target {
            CacheTarget::Binary => IncrementalTarget::Binary,
            CacheTarget::Library => IncrementalTarget::Library,
            CacheTarget::Test => IncrementalTarget::Test { filter: None },
        };
        CacheEntryIdentity {
            fingerprint: fingerprint_package_graph(
                &[package],
                Edition::Edition2026,
                incremental_target,
            )
            .expect("fingerprint"),
            edition: Edition::Edition2026,
            target,
        }
    }

    fn with_store(test: impl FnOnce(&CacheStore, &Path)) {
        let root = temporary_root();
        let store = CacheStore::open(root.clone()).expect("open cache store");
        test(&store, &root);
        fs::remove_dir_all(root).expect("remove temporary cache");
    }

    fn artifact(llvm_ir: &str, test_names: &[&str]) -> CacheArtifact {
        CacheArtifact {
            llvm_ir: llvm_ir.into(),
            test_names: test_names.iter().map(|name| (*name).into()).collect(),
        }
    }

    #[test]
    fn root_resolution_prefers_absolute_override_and_rejects_relative_override() {
        let mut variables = HashMap::new();
        variables.insert(CACHE_DIRECTORY_ENV, OsString::from("/cache/salicin"));
        assert_eq!(
            resolve_cache_root_with(|name| variables.get(name).cloned()),
            CacheRootResolution::Available(PathBuf::from("/cache/salicin"))
        );
        variables.insert(CACHE_DIRECTORY_ENV, OsString::from("relative/cache"));
        assert_eq!(
            resolve_cache_root_with(|name| variables.get(name).cloned()),
            CacheRootResolution::Disabled(CacheRootDisabled::RelativeOverride)
        );
    }

    #[test]
    fn store_requires_an_absolute_root_and_validates_its_marker() {
        assert!(matches!(
            CacheStore::open(PathBuf::from("relative")),
            Err(CacheStoreError::RootMustBeAbsolute)
        ));
        let root = temporary_root();
        fs::write(root.join(CACHE_ROOT_MARKER), "not salicin\n").expect("write malformed marker");
        assert!(matches!(
            CacheStore::open(root.clone()),
            Err(CacheStoreError::InvalidRootMarker)
        ));
        fs::remove_dir_all(root).expect("remove temporary cache");

        let unowned = temporary_root();
        fs::write(unowned.join("user-data"), "keep").expect("write unrelated data");
        assert!(matches!(
            CacheStore::open(unowned.clone()),
            Err(CacheStoreError::InvalidRootMarker)
        ));
        assert_eq!(
            fs::read_to_string(unowned.join("user-data")).unwrap(),
            "keep"
        );
        fs::remove_dir_all(unowned).expect("remove unowned directory");
    }

    #[test]
    fn missing_publish_and_warm_lookup_round_trip_exact_ir() {
        with_store(|store, root| {
            let identity = identity(CacheTarget::Binary);
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Miss(CacheMissReason::Missing)
            );
            assert_eq!(
                store
                    .publish(identity, &artifact("; module\n", &[]))
                    .unwrap(),
                CachePublishOutcome::Published
            );
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Hit(artifact("; module\n", &[]))
            );
            assert_eq!(
                store
                    .publish(identity, &artifact("; different but same key\n", &[]))
                    .unwrap(),
                CachePublishOutcome::AlreadyPresent
            );
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Hit(artifact("; module\n", &[]))
            );
            assert!(root.join(CACHE_ROOT_MARKER).is_file());
        });
    }

    #[test]
    fn malformed_metadata_and_payload_damage_are_misses_and_replaceable() {
        with_store(|store, _| {
            let identity = identity(CacheTarget::Library);
            store
                .publish(identity, &artifact("; valid\n", &[]))
                .unwrap();
            let entry = store
                .root()
                .join(cache_entry_relative_path(identity.fingerprint));
            fs::write(
                entry.join(CACHE_METADATA_FILE),
                "schema = 1\nunknown = true\n",
            )
            .expect("damage metadata");
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Miss(CacheMissReason::InvalidMetadata)
            );
            assert_eq!(
                store
                    .publish(identity, &artifact("; repaired metadata\n", &[]))
                    .unwrap(),
                CachePublishOutcome::Published
            );
            fs::write(entry.join(CACHE_PAYLOAD_FILE), "; truncated").expect("truncate payload");
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Miss(CacheMissReason::InvalidPayload)
            );
            assert_eq!(
                store
                    .publish(identity, &artifact("; repaired payload\n", &[]))
                    .unwrap(),
                CachePublishOutcome::Published
            );
            assert_eq!(
                store.lookup(identity),
                CacheLookup::Hit(artifact("; repaired payload\n", &[]))
            );
        });
    }

    #[test]
    fn incompatible_identity_and_non_regular_files_never_hit() {
        with_store(|store, _| {
            let binary = identity(CacheTarget::Binary);
            store.publish(binary, &artifact("; binary\n", &[])).unwrap();
            let library = CacheEntryIdentity {
                target: CacheTarget::Library,
                ..binary
            };
            assert_eq!(
                store.lookup(library),
                CacheLookup::Miss(CacheMissReason::IncompatibleMetadata)
            );

            let test = identity(CacheTarget::Test);
            let entry = store
                .root()
                .join(cache_entry_relative_path(test.fingerprint));
            ensure_owned_directory_path(store.root(), entry.parent().unwrap()).unwrap();
            fs::write(&entry, "not a directory").expect("create non-directory entry");
            assert_eq!(
                store.lookup(test),
                CacheLookup::Miss(CacheMissReason::InvalidEntryKind)
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_roots_entries_and_payloads_are_rejected() {
        use std::os::unix::fs::symlink;

        let real_root = temporary_root();
        let linked_root = unique_path(real_root.parent().unwrap(), "root-link");
        symlink(&real_root, &linked_root).expect("link cache root");
        assert!(matches!(
            CacheStore::open(linked_root.clone()),
            Err(CacheStoreError::RootIsSymbolicLink)
        ));
        fs::remove_file(linked_root).expect("remove root link");

        let store = CacheStore::open(real_root.clone()).expect("open real cache root");
        let identity = identity(CacheTarget::Binary);
        store
            .publish(identity, &artifact("; valid\n", &[]))
            .unwrap();
        let entry = store
            .root()
            .join(cache_entry_relative_path(identity.fingerprint));
        fs::remove_file(entry.join(CACHE_PAYLOAD_FILE)).expect("remove payload");
        symlink("/dev/null", entry.join(CACHE_PAYLOAD_FILE)).expect("link payload");
        assert_eq!(
            store.lookup(identity),
            CacheLookup::Miss(CacheMissReason::InvalidPayload)
        );

        fs::remove_dir_all(real_root).expect("remove temporary cache");
    }

    #[test]
    fn concurrent_writers_publish_one_complete_entry() {
        let root = temporary_root();
        let identity = identity(CacheTarget::Test);
        let barrier = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for index in 0..8 {
            let root = root.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                let store = CacheStore::open(root).expect("open concurrent store");
                barrier.wait();
                let payload = CacheArtifact {
                    llvm_ir: format!("; writer {index}\n"),
                    test_names: vec![format!("writer-{index}")],
                };
                store.publish(identity, &payload).expect("publish")
            }));
        }
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("join writer"))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| **outcome == CachePublishOutcome::Published)
                .count(),
            1
        );
        let store = CacheStore::open(root.clone()).expect("reopen store");
        let CacheLookup::Hit(payload) = store.lookup(identity) else {
            panic!("one complete concurrent payload must be readable");
        };
        assert!(payload.llvm_ir.starts_with("; writer "));
        assert_eq!(payload.test_names.len(), 1);
        fs::remove_dir_all(root).expect("remove temporary cache");
    }
}
