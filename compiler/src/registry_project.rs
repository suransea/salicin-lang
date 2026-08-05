//! End-to-end registry preparation for package commands.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::lockfile::{parse_lockfile, portable_relative_path, Lockfile};
use crate::manifest::{Dependency, DependencyGraph, Manifest};
use crate::registry::{
    archive_cache_path, load_registry_config, parse_registry_snapshot,
    registry_requirements_from_graph, resolve_registry_dependencies, snapshot_cache_path,
    RegistryEndpoint, RegistryProviderIdentity, RegistryRelease, RegistryResolution,
    RegistrySnapshot, ResolvedRegistryDependency, ResolvedRegistryPackage,
    ResolvedRootRegistryDependency, Sha256Digest, REGISTRY_CONFIG_FILE_NAME,
};
use crate::registry_cache::{RegistrySourceLookup, RegistrySourceStore, VerifiedRegistrySource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryAccessMode {
    Update,
    Locked,
    Frozen,
}

#[derive(Clone, Debug)]
pub struct PreparedRegistryGraph {
    pub graph: DependencyGraph,
    pub resolution: RegistryResolution,
}

pub fn prepare_registry_graph(
    resolution_graph: &DependencyGraph,
    compilation_graph: &DependencyGraph,
    project_root: &Path,
    cache_root: PathBuf,
    mode: RegistryAccessMode,
) -> Result<Option<PreparedRegistryGraph>, String> {
    let roots = registry_requirements_from_graph(resolution_graph);
    if roots.is_empty() {
        return Ok(None);
    }

    let lock_path = project_root.join(crate::lockfile::LOCKFILE_NAME);
    let prior_lock = read_prior_lock(&lock_path, mode)?;
    let config_path = project_root.join(REGISTRY_CONFIG_FILE_NAME);
    let config = load_registry_config(&config_path).map_err(|error| {
        format!(
            "registry dependencies require valid configuration '{}': {error}",
            config_path.display()
        )
    })?;
    let mut required_registries = roots
        .iter()
        .map(|root| root.dependency.registry.clone())
        .collect::<BTreeSet<_>>();
    if mode != RegistryAccessMode::Update {
        required_registries.extend(prior_lock.iter().flat_map(|lock| {
            lock.registry_snapshots
                .iter()
                .map(|entry| entry.registry.clone())
        }));
    }
    let store = RegistrySourceStore::open(cache_root.clone()).map_err(|error| error.to_string())?;
    let mut snapshots = BTreeMap::new();
    for registry in required_registries {
        load_named_snapshot(
            &mut snapshots,
            &registry,
            &config,
            prior_lock.as_ref(),
            &config_path,
            &cache_root,
            mode,
        )?;
    }

    let previously_locked = prior_lock
        .as_ref()
        .map(locked_provider_identities)
        .unwrap_or_default();
    let resolution = if mode == RegistryAccessMode::Update {
        loop {
            let supplied = snapshots.values().cloned().collect::<Vec<_>>();
            match resolve_registry_dependencies(&roots, &supplied, &previously_locked) {
                Ok(resolution) => break resolution,
                Err(crate::registry::RegistryResolutionError::MissingSnapshot(registry))
                    if !snapshots.contains_key(&registry) =>
                {
                    load_named_snapshot(
                        &mut snapshots,
                        &registry,
                        &config,
                        prior_lock.as_ref(),
                        &config_path,
                        &cache_root,
                        mode,
                    )?;
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    } else {
        resolution_from_lock(
            resolution_graph,
            project_root,
            prior_lock.as_ref().expect("locked mode has a lockfile"),
            &snapshots,
        )?
    };

    let mut verified = BTreeMap::new();
    for package in &resolution.packages {
        let snapshot = snapshots
            .get(&package.provider.registry)
            .filter(|snapshot| snapshot.digest == package.provider.snapshot)
            .expect("resolved providers reference supplied snapshots");
        let release = release_for(snapshot, &package.provider)?;
        let source = materialize_provider(
            &store,
            endpoint_for(&config.registries, &package.provider)?,
            release,
            &package.provider,
            mode,
        )?;
        verified.insert(package.provider.clone(), source);
    }

    let graph = compose_graph(compilation_graph, &resolution, &verified)?;
    Ok(Some(PreparedRegistryGraph { graph, resolution }))
}

fn resolution_from_lock(
    local_graph: &DependencyGraph,
    project_root: &Path,
    lock: &Lockfile,
    snapshots: &BTreeMap<String, RegistrySnapshot>,
) -> Result<RegistryResolution, String> {
    let providers = lock
        .registry_packages
        .iter()
        .map(|package| {
            let provider = RegistryProviderIdentity {
                registry: package.registry.clone(),
                snapshot: package.snapshot.clone(),
                name: package.name.clone(),
                version: package.version.clone(),
                archive_sha256: package.checksum.clone(),
            };
            (provider, package)
        })
        .collect::<BTreeMap<_, _>>();
    let mut roots = Vec::new();
    for manifest in &local_graph.packages {
        let path = portable_relative_path(project_root, &manifest.package_root);
        let locked = lock
            .packages
            .iter()
            .find(|package| {
                package.name == manifest.package.name
                    && package.version == manifest.package.version
                    && package.path == path
            })
            .ok_or_else(|| {
                format!(
                    "lockfile has no local package entry for `{}@{}` at `{path}`",
                    manifest.package.name, manifest.package.version
                )
            })?;
        if locked.registry_dependencies.len() != manifest.registry_dependencies.len() {
            return Err(format!(
                "lockfile registry dependencies for local package `{}` are out of date",
                manifest.package.name
            ));
        }
        for request in &manifest.registry_dependencies {
            let edge = locked
                .registry_dependencies
                .iter()
                .find(|edge| edge.alias == request.alias)
                .ok_or_else(|| {
                    format!(
                        "lockfile is missing registry dependency `{}`",
                        request.alias
                    )
                })?;
            let provider = provider_from_edge(edge);
            if provider.registry != request.registry
                || provider.name != request.package
                || !request.requirement.matches(&provider.version)
                || !providers.contains_key(&provider)
            {
                return Err(format!(
                    "locked provider for dependency `{}` no longer satisfies the manifest",
                    request.alias
                ));
            }
            roots.push(ResolvedRootRegistryDependency {
                owner_manifest_path: manifest.manifest_path.clone(),
                dependency: ResolvedRegistryDependency {
                    alias: request.alias.clone(),
                    provider,
                },
            });
        }
    }

    let mut packages = Vec::new();
    for (provider, locked) in &providers {
        let snapshot = snapshots.get(&provider.registry).ok_or_else(|| {
            format!(
                "locked registry `{}` has no cached snapshot",
                provider.registry
            )
        })?;
        if snapshot.digest != provider.snapshot {
            return Err(format!(
                "locked provider `{}/{}@{}` references the wrong snapshot",
                provider.registry, provider.name, provider.version
            ));
        }
        let release = release_for(snapshot, provider)?;
        if locked.dependencies.len() != release.dependencies.len() {
            return Err(format!(
                "locked dependencies of `{}/{}@{}` are out of date",
                provider.registry, provider.name, provider.version
            ));
        }
        let mut dependencies = Vec::new();
        for request in &release.dependencies {
            let edge = locked
                .dependencies
                .iter()
                .find(|edge| edge.alias == request.alias)
                .ok_or_else(|| {
                    format!(
                        "lockfile is missing dependency `{}` of `{}`",
                        request.alias, provider.name
                    )
                })?;
            let dependency_provider = provider_from_edge(edge);
            if dependency_provider.registry != request.registry
                || dependency_provider.name != request.package
                || !request.requirement.matches(&dependency_provider.version)
                || !providers.contains_key(&dependency_provider)
            {
                return Err(format!(
                    "locked dependency `{}` of `{}` no longer satisfies the immutable snapshot",
                    request.alias, provider.name
                ));
            }
            dependencies.push(ResolvedRegistryDependency {
                alias: request.alias.clone(),
                provider: dependency_provider,
            });
        }
        packages.push(ResolvedRegistryPackage {
            provider: provider.clone(),
            dependencies,
        });
    }
    packages.sort_by(|left, right| left.provider.cmp(&right.provider));
    roots.sort_by(|left, right| {
        left.owner_manifest_path
            .cmp(&right.owner_manifest_path)
            .then_with(|| left.dependency.alias.cmp(&right.dependency.alias))
    });
    validate_locked_closure(&roots, &packages)?;
    Ok(RegistryResolution {
        snapshots: lock
            .registry_snapshots
            .iter()
            .map(|entry| (entry.registry.clone(), entry.checksum.clone()))
            .collect(),
        packages,
        roots,
    })
}

fn provider_from_edge(
    edge: &crate::lockfile::LockedRegistryDependency,
) -> RegistryProviderIdentity {
    RegistryProviderIdentity {
        registry: edge.registry.clone(),
        snapshot: edge.snapshot.clone(),
        name: edge.name.clone(),
        version: edge.version.clone(),
        archive_sha256: edge.checksum.clone(),
    }
}

fn validate_locked_closure(
    roots: &[ResolvedRootRegistryDependency],
    packages: &[ResolvedRegistryPackage],
) -> Result<(), String> {
    let packages = packages
        .iter()
        .map(|package| (&package.provider, package))
        .collect::<BTreeMap<_, _>>();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    fn visit<'a>(
        provider: &'a RegistryProviderIdentity,
        packages: &BTreeMap<&'a RegistryProviderIdentity, &'a ResolvedRegistryPackage>,
        visiting: &mut BTreeSet<&'a RegistryProviderIdentity>,
        visited: &mut BTreeSet<&'a RegistryProviderIdentity>,
    ) -> Result<(), String> {
        if visited.contains(provider) {
            return Ok(());
        }
        if !visiting.insert(provider) {
            return Err(format!(
                "locked registry dependency cycle includes `{}/{}@{}`",
                provider.registry, provider.name, provider.version
            ));
        }
        let package = packages.get(provider).ok_or_else(|| {
            format!(
                "locked graph references missing provider `{}/{}@{}`",
                provider.registry, provider.name, provider.version
            )
        })?;
        for dependency in &package.dependencies {
            visit(&dependency.provider, packages, visiting, visited)?;
        }
        visiting.remove(provider);
        visited.insert(provider);
        Ok(())
    }
    for root in roots {
        visit(
            &root.dependency.provider,
            &packages,
            &mut visiting,
            &mut visited,
        )?;
    }
    if visited.len() != packages.len() {
        return Err("lockfile contains unreachable registry packages".into());
    }
    Ok(())
}

fn load_named_snapshot(
    snapshots: &mut BTreeMap<String, RegistrySnapshot>,
    registry: &str,
    config: &crate::registry::RegistryConfig,
    prior_lock: Option<&Lockfile>,
    config_path: &Path,
    cache_root: &Path,
    mode: RegistryAccessMode,
) -> Result<(), String> {
    let digest = selected_snapshot(registry, &config.snapshots, prior_lock, mode)?;
    let endpoint = config.registries.get(registry).ok_or_else(|| {
        format!(
            "registry `{registry}` is not configured in '{}'",
            config_path.display()
        )
    })?;
    let snapshot = load_snapshot(cache_root, registry, &digest, endpoint, mode)?;
    snapshots.insert(registry.to_owned(), snapshot);
    Ok(())
}

fn read_prior_lock(path: &Path, mode: RegistryAccessMode) -> Result<Option<Lockfile>, String> {
    match fs::read_to_string(path) {
        Ok(source) => match parse_lockfile(&source) {
            Ok(lock) => Ok(Some(lock)),
            Err(_) if mode == RegistryAccessMode::Update => Ok(None),
            Err(error) => Err(format!("lockfile '{}' is invalid: {error}", path.display())),
        },
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && mode == RegistryAccessMode::Update =>
        {
            Ok(None)
        }
        Err(error) => Err(format!(
            "lockfile '{}' is required but could not be read: {error}",
            path.display()
        )),
    }
}

fn selected_snapshot(
    registry: &str,
    configured: &BTreeMap<String, Sha256Digest>,
    lock: Option<&Lockfile>,
    mode: RegistryAccessMode,
) -> Result<Sha256Digest, String> {
    if mode == RegistryAccessMode::Update {
        return configured
            .get(registry)
            .cloned()
            .ok_or_else(|| format!("registry `{registry}` has no configured immutable snapshot"));
    }
    let entries = lock
        .expect("locked modes require a parsed lockfile")
        .registry_snapshots
        .iter()
        .filter(|entry| entry.registry == registry)
        .collect::<Vec<_>>();
    match entries.as_slice() {
        [entry] => Ok(entry.checksum.clone()),
        [] => Err(format!(
            "lockfile has no snapshot for registry `{registry}`"
        )),
        _ => Err(format!(
            "lockfile repeats snapshot for registry `{registry}`"
        )),
    }
}

fn load_snapshot(
    cache_root: &Path,
    registry: &str,
    digest: &Sha256Digest,
    endpoint: &RegistryEndpoint,
    mode: RegistryAccessMode,
) -> Result<RegistrySnapshot, String> {
    let cache_path = snapshot_cache_path(cache_root, digest);
    if let Ok(bytes) = fs::read(&cache_path) {
        match parse_registry_snapshot(&bytes, digest, registry) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if mode == RegistryAccessMode::Frozen => {
                return Err(format!(
                    "frozen registry snapshot cache '{}' is corrupt: {error}",
                    cache_path.display()
                ))
            }
            Err(_) => {}
        }
    } else if mode == RegistryAccessMode::Frozen {
        return Err(format!(
            "frozen registry snapshot cache '{}' is missing",
            cache_path.display()
        ));
    }

    let bytes = match endpoint {
        RegistryEndpoint::LocalFixture(root) => {
            let path = root.join("snapshots").join("sha256").join(format!("{digest}.json"));
            fs::read(&path).map_err(|error| format!("could not read registry snapshot '{}': {error}", path.display()))?
        }
        RegistryEndpoint::Https(url) => {
            return Err(format!("registry network transport for `{url}` is not standardized; use a local fixture or a populated cache"))
        }
    };
    let snapshot =
        parse_registry_snapshot(&bytes, digest, registry).map_err(|error| error.to_string())?;
    publish_blob(&cache_path, &bytes)?;
    Ok(snapshot)
}

fn locked_provider_identities(lock: &Lockfile) -> BTreeSet<RegistryProviderIdentity> {
    lock.registry_packages
        .iter()
        .map(|package| RegistryProviderIdentity {
            registry: package.registry.clone(),
            snapshot: package.snapshot.clone(),
            name: package.name.clone(),
            version: package.version.clone(),
            archive_sha256: package.checksum.clone(),
        })
        .collect()
}

fn endpoint_for<'a>(
    endpoints: &'a BTreeMap<String, RegistryEndpoint>,
    provider: &RegistryProviderIdentity,
) -> Result<&'a RegistryEndpoint, String> {
    endpoints.get(&provider.registry).ok_or_else(|| {
        format!(
            "registry `{}` required by the selected graph is not configured",
            provider.registry
        )
    })
}

fn release_for<'a>(
    snapshot: &'a RegistrySnapshot,
    provider: &RegistryProviderIdentity,
) -> Result<&'a RegistryRelease, String> {
    snapshot
        .packages
        .iter()
        .find(|package| package.name == provider.name)
        .and_then(|package| {
            package.releases.iter().find(|release| {
                release.version == provider.version
                    && release.archive_sha256 == provider.archive_sha256
            })
        })
        .ok_or_else(|| {
            format!(
                "selected provider `{}/{}@{}` disappeared from snapshot {}",
                provider.registry, provider.name, provider.version, provider.snapshot
            )
        })
}

fn materialize_provider(
    store: &RegistrySourceStore,
    endpoint: &RegistryEndpoint,
    release: &RegistryRelease,
    provider: &RegistryProviderIdentity,
    mode: RegistryAccessMode,
) -> Result<VerifiedRegistrySource, String> {
    if let RegistrySourceLookup::Hit(source) = store.lookup(provider, release) {
        return Ok(*source);
    }
    let archive_path = archive_cache_path(store.root(), &provider.archive_sha256);
    let cached = fs::read(&archive_path).ok();
    let bytes = match (cached, mode) {
        (Some(bytes), _) if Sha256Digest::of(&bytes) == provider.archive_sha256 => bytes,
        (_, RegistryAccessMode::Frozen) => {
            return Err(format!("frozen registry archive cache '{}' is missing or corrupt", archive_path.display()))
        }
        (_, _) => match endpoint {
            RegistryEndpoint::LocalFixture(root) => {
                let path = root.join(&release.archive);
                fs::read(&path).map_err(|error| format!("could not read registry archive '{}': {error}", path.display()))?
            }
            RegistryEndpoint::Https(url) => {
                return Err(format!("registry network transport for `{url}` is not standardized; use a local fixture or a populated cache"))
            }
        },
    };
    store
        .materialize(provider, release, &bytes)
        .map(|(_, source)| source)
        .map_err(|error| error.to_string())
}

fn compose_graph(
    local_graph: &DependencyGraph,
    resolution: &RegistryResolution,
    sources: &BTreeMap<RegistryProviderIdentity, VerifiedRegistrySource>,
) -> Result<DependencyGraph, String> {
    let mut manifests = local_graph.packages.clone();
    let local_paths = manifests
        .iter()
        .map(|manifest| manifest.manifest_path.clone())
        .collect::<BTreeSet<_>>();
    let packages_by_provider = resolution
        .packages
        .iter()
        .map(|package| (&package.provider, package))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = BTreeSet::new();
    let mut pending = resolution
        .roots
        .iter()
        .filter(|root| local_paths.contains(&root.owner_manifest_path))
        .map(|root| root.dependency.provider.clone())
        .collect::<Vec<_>>();
    while let Some(provider) = pending.pop() {
        if !reachable.insert(provider.clone()) {
            continue;
        }
        let package = packages_by_provider.get(&provider).ok_or_else(|| {
            format!(
                "resolved graph references unavailable provider `{}/{}@{}`",
                provider.registry, provider.name, provider.version
            )
        })?;
        pending.extend(
            package
                .dependencies
                .iter()
                .map(|dependency| dependency.provider.clone()),
        );
    }
    let source_by_provider = sources
        .iter()
        .map(|(provider, source)| (provider, &source.manifest))
        .collect::<BTreeMap<_, _>>();
    for root in &resolution.roots {
        let Some(owner) = manifests
            .iter_mut()
            .find(|manifest| manifest.manifest_path == root.owner_manifest_path)
        else {
            continue;
        };
        attach_dependency(
            owner,
            &root.dependency.alias,
            &root.dependency.provider,
            &source_by_provider,
        )?;
    }
    for package in &resolution.packages {
        if !reachable.contains(&package.provider) {
            continue;
        }
        let mut manifest = sources
            .get(&package.provider)
            .ok_or_else(|| {
                format!(
                    "selected registry package `{}` was not materialized",
                    package.provider.name
                )
            })?
            .manifest
            .clone();
        for dependency in &package.dependencies {
            attach_dependency(
                &mut manifest,
                &dependency.alias,
                &dependency.provider,
                &source_by_provider,
            )?;
        }
        manifests.push(manifest);
    }
    manifests.sort_by(|left, right| left.manifest_path.cmp(&right.manifest_path));
    Ok(DependencyGraph {
        root_manifest_path: local_graph.root_manifest_path.clone(),
        packages: manifests,
    })
}

fn attach_dependency(
    owner: &mut Manifest,
    alias: &str,
    provider: &RegistryProviderIdentity,
    sources: &BTreeMap<&RegistryProviderIdentity, &Manifest>,
) -> Result<(), String> {
    let target = sources.get(provider).ok_or_else(|| {
        format!(
            "registry dependency `{alias}` references unavailable provider `{}/{}@{}`",
            provider.registry, provider.name, provider.version
        )
    })?;
    owner.dependencies.push(Dependency {
        alias: alias.to_owned(),
        path: target.package_root.clone(),
        manifest_path: target.manifest_path.clone(),
        package: target.package.clone(),
    });
    owner
        .dependencies
        .sort_by(|left, right| left.alias.cmp(&right.alias));
    Ok(())
}

fn publish_blob(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().expect("cache blob has a parent");
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create cache '{}': {error}", parent.display()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".snapshot-tmp-{}-{nonce}", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if fs::read(path).ok().as_deref() == Some(bytes) {
                Ok(())
            } else {
                Err(error.to_string())
            }
        }
    }
}
