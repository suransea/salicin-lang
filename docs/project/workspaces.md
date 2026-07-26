# Workspace And Package Identity Contract

Status: accepted implementation contract

This document defines workspace membership and resolved package-provider
identity. Dependency version selection and registry transport belong to the
following reproducible-resolution milestone.

## Manifest Shape

A `salicin.toml` may contain `[package]`, `[workspace]`, or both. A manifest
with only `[workspace]` is a virtual workspace root and is not itself a
package.

```toml
[workspace]
members = ["packages/app", "packages/math"]
```

The first implementation accepts explicit portable relative member paths.
It does not expand globs. Each path must resolve to a package
`salicin.toml`, remain beneath the workspace root, and identify a unique
canonical manifest. A member cannot declare another `[workspace]`. When the
root also contains `[package]`, that root package is automatically a member
and must not appear in `members`.

Workspace membership does not create dependency edges. Packages continue to
declare every dependency explicitly.

## Command Selection

Passing a member package path selects that member directly. Passing a
workspace root selects its root package when one exists. A virtual workspace
with one member selects that member. A virtual workspace with multiple
members requires `--package <name>` or `-p <name>`.

Target selectors apply after package selection. Package names must therefore
be unique within one workspace.

Workspace commands write one `salicin.lock` and use one `build` directory at
the workspace root. They never write member-local lockfiles as a side effect.

## Provider Identity

A package declaration supplies a package name and version. Those fields alone
do not identify its provider. After dependency resolution, compiler and
lockfile identity is:

```text
(source, package name, exact version)
```

Source is one of:

- `workspace:<member-relative-path>` for workspace members;
- `path:<lockfile-relative-path>` for external path dependencies;
- `registry:<registry-name>` for registry dependencies;
- a compiler-owned source identity for edition-pinned libraries.

Workspace and path identities use normalized `/` separators in lock data.
Absolute machine paths never enter native symbols or serialized identities.
Registry names are ASCII lowercase identifiers declared by toolchain
configuration; URLs and credentials are resolution configuration rather than
package identity.

Dependency aliases remain local source names and are not provider identity.
Two aliases may name the same resolved provider. Two distinct providers with
the same package name and version remain distinct. One compilation rejects
ambiguous duplicate providers only when source dependency paths would make
their public names indistinguishable.

## Deferred Resolution Rules

The next milestone defines registry dependency syntax, semantic-version
requirement matching, registry indexes, checksums, offline behavior, lockfile
reuse, and update selection. Workspace support does not guess those rules or
perform network access.
