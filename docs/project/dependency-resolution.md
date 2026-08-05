# Reproducible Dependency Resolution

Status: implemented for workspace and path sources; registry input contract
implemented and selection algorithm specified

Dependency resolution produces one exact provider graph before source
compilation. The graph is serialized in canonical `salicin.lock` format 2 and
is compared structurally when resolution is locked.

## Local Sources

Workspace members are the canonical manifests listed by the workspace root.
Path dependencies resolve relative to the declaring package manifest and are
canonicalized before graph traversal. Traversal rejects cycles, packages
without library targets, and inaccessible manifests. Collection and lock
serialization sort canonical paths, package identities, and dependency
aliases; filesystem enumeration order does not affect the result.

The lockfile records every workspace member and the complete dependency
closure of every member, even when one command compiles only a selected
member. Each package records its exact name, semantic version, edition,
portable source identity, and lock-root-relative path. Each dependency edge
records its local alias and exact resolved provider.

Local source contents are intentionally not checksummed. They are editable
inputs, like the selected package itself. Reproducible resolution means the
same provider graph is selected; byte-reproducible compilation is tracked
separately by the incremental-input milestone.

## Lock Modes

Default commands compute the provider graph and atomically create or replace
`salicin.lock` only when canonical bytes differ.

`--locked` requires an existing format-2 lockfile. The compiler strictly
parses it, rejects unknown fields and invalid versions, and compares the full
typed graph with current manifest resolution. Missing, malformed, or stale
lock data is an error and is never rewritten.

`--frozen` includes every `--locked` rule and additionally forbids dependency
network access. Current workspace/path resolution performs no network access,
so the two modes differ only in their forward contract.

## Registry Algorithm

Registry dependencies now name a normalized ASCII registry identity, package
name, and semantic-version requirement according to the strict
[registry source input contract](registry-source-contract.md). PKG-1 validates
immutable checksum-addressed snapshots without selecting or fetching a
provider. Resolution must:

1. read one immutable, revision-identified registry index snapshot;
2. discard yanked releases unless an existing lock already selects one;
3. select the highest non-yanked version satisfying every requirement;
4. use package name, exact version, registry identity, and archive SHA-256 as
   the locked provider;
5. verify the archive checksum before reading its manifest or sources;
6. reject a release whose manifest identity differs from the index entry.

Default mode may refresh the index and cache. `--locked` may fetch only the
already selected exact provider and may not change it. `--frozen` may read
only verified local index and archive cache entries. Registry credentials,
mirrors, transport, publishing, and the public service remain outside the
language and are not yet implemented.

## Failure Atomicity

Manifest, graph, lock parsing, and frozen-state validation complete before
native output or lockfile replacement. A failed resolution leaves the prior
lockfile and build outputs unchanged.
