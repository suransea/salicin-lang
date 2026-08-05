# Reproducible Dependency Resolution

Status: implemented for workspace, path, and immutable-snapshot registry
provider selection; verified registry source materialization remains pending

Dependency resolution produces one exact provider graph before source
compilation. The graph is serialized in canonical `salicin.lock` format 3 and
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

`--locked` requires an existing format-3 lockfile. The compiler strictly
parses it, rejects unknown fields and invalid versions, and compares the full
typed graph with current manifest resolution. Missing, malformed, or stale
lock data is an error and is never rewritten.

`--frozen` includes every `--locked` rule and additionally forbids dependency
network access. Current workspace/path resolution performs no network access,
so the two modes differ only in their forward contract.

## Registry Algorithm

Registry dependencies now name a normalized ASCII registry identity, package
name, and semantic-version requirement according to the strict
[registry source input contract](registry-source-contract.md). The resolver
consumes validated checksum-addressed snapshots without fetching or extracting
archives. It:

1. uses at most one immutable digest-identified snapshot per registry;
2. unifies all constraints for each `(registry, package)` identity;
3. tries candidates in descending SemVer order and backtracks across
   transitive conflicts, yielding the highest graph under that deterministic
   decision order;
4. discards yanked releases unless the complete snapshot/version/archive
   identity already exists in prior lock data;
5. requires root inclusion, transitive dependency closure, one selected
   version per registry/package identity, and an acyclic provider graph;
6. emits stable package/root ordering and exact registry, snapshot, name,
   version, and archive SHA-256 identities into lockfile format 3.

Missing snapshots/packages, incompatible constraint intersections, dependency
cycles, duplicate snapshots, and dangling exact lock edges are explicit
errors. Input ordering cannot change the solution or serialized lock graph.
Because the accepted constraint language is NP-complete, resolution has a
fixed compiler-owned limit of 100,000 candidate attempts; exhaustion is a
stable error rather than an unbounded search or a silently greedy answer.
The resolver does not read manifests from archives; PKG-3 must verify the
archive checksum and package identity before sources enter compilation.

Default mode may refresh the index and cache. `--locked` may fetch only the
already selected exact provider and may not change it. `--frozen` may read
only verified local index and archive cache entries. Registry credentials,
mirrors, transport, publishing, and the public service remain outside the
language and are not yet implemented. CLI compilation still refuses registry
requests until PKG-3 can materialize verified sources; the implemented
resolver and lock builder are transport-independent APIs.

## Failure Atomicity

Manifest, graph, lock parsing, and frozen-state validation complete before
native output or lockfile replacement. A failed resolution leaves the prior
lockfile and build outputs unchanged.
