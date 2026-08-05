# Registry Source Input Contract

Status: PKG-1 input, PKG-2 resolution, and PKG-3 verified source cache
implemented; transport and locked/frozen CLI acceptance remain PKG-4

This contract fixes the source-visible and on-disk inputs for registry packages
without defining a public registry service. Registry inputs are strict and
versioned so later resolver and cache work cannot silently reinterpret an
existing manifest, snapshot, or checksum.

## Manifest Requests

A registry dependency uses the same `[dependencies]` alias namespace as a path
dependency and declares all three registry fields:

```toml
[dependencies]
http = { package = "http-kit", version = "^1.2", registry = "community" }
```

The alias is an ASCII `snake_case` Salicin module name. Package and registry
identities are normalized ASCII `kebab-case`; the version is a SemVer
requirement. A declaration must contain exactly one source: `path`, or the
complete `package`/`version`/`registry` group. Git dependencies, implicit
default registries, URL-valued manifest sources, and mixed path/registry
fallbacks are rejected.

The manifest loader preserves an unresolved typed request. The implemented
registry solver can collect those requests across the complete local graph and
produce exact providers. The source-store API materializes selected bytes and
returns a source root only after verification; ordinary CLI compilation still
refuses registry requests until PKG-4 wires transport and lock modes to it.

## Registry Identity and Configuration

Endpoints are deployment configuration, not package identity. The strict
`salicin-registries.toml` format 1 maps a stable registry identity to exactly
one endpoint:

```toml
format = 1

[registries.community]
url = "https://packages.example.org/v1"

[registries.local-test]
fixture = "fixtures/registry"
```

Production endpoints must be HTTPS roots without query, fragment, or trailing
slash. `fixture` is a relocatable child path anchored at the configuration
file; it cannot be absolute or contain `.`/`..`. Credentials, mirrors,
endpoint fallback, discovery, publishing, and trust-policy distribution are
not part of this format.

## Immutable Snapshot

An index snapshot is exact UTF-8 JSON whose SHA-256 is supplied by the caller
or later lock/resolution state. Digest verification happens before JSON
parsing. A complete format-1 shape is:

```json
{
  "format": 1,
  "registry": "community",
  "packages": [
    {
      "name": "http-kit",
      "releases": [
        {
          "version": "1.2.3",
          "yanked": false,
          "archive_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          "archive": "archives/http-kit/1.2.3/http-kit-1.2.3.tar.gz",
          "dependencies": [
            {
              "alias": "bytes",
              "package": "byte-kit",
              "registry": "community",
              "version": "^2.0"
            }
          ]
        }
      ]
    }
  ]
}
```

Whitespace is part of the digested bytes; the indentation above is
illustrative rather than a second canonicalization rule. Format 1 contains:

- `format = 1` and the exact registry identity;
- packages strictly sorted by normalized name;
- releases strictly sorted by SemVer, with no build metadata or duplicate
  SemVer precedence;
- `yanked`, exact archive path, SHA-256 of compressed archive bytes, and
  dependencies strictly sorted by alias;
- each transitive request's explicit alias, package, registry, and SemVer
  requirement.

Unknown fields, unsorted or duplicate entries, malformed identities, checksum
spelling other than 64 lowercase hexadecimal digits, and registry-identity
replay are rejected. Strict ordering makes traversal deterministic; the digest
owns the exact serialized snapshot, so no canonicalization is performed after
verification.

## Archive and Checksum Ownership

The index entry owns the SHA-256 of the exact `.tar.gz` bytes and the only
accepted archive path:

```text
archives/<package>/<version>/<package>-<version>.tar.gz
```

The verified source store requires one top-level `<package>-<version>/`
directory containing `salicin.toml`; only regular files and directories are
extractable.
Absolute paths, traversal, links, devices, duplicate normalized paths,
identity mismatch, and bounded-size violations will fail before source
publication. Compressed archives are limited to 64 MiB, individual files to
16 MiB, expanded regular-file bytes to 256 MiB, and entries to 16,384.
Attestations and transparency proofs may later supplement this
digest, but never replace local digest verification.

## Cache and Fixture Protocol

`SALICIN_CACHE_HOME`, when set, must be absolute. Otherwise macOS uses
`~/Library/Caches/salicin`; other supported hosts use
`$XDG_CACHE_HOME/salicin` or `~/.cache/salicin`. Registry data belongs below
`registry-v1` and is content-addressed rather than endpoint-addressed:

```text
registry-v1/index/sha256/<snapshot-digest>.json
registry-v1/archives/sha256/<archive-digest>.tar.gz
registry-v1/sources/sha256/<archive-digest>/...
```

Local fixtures expose the same immutable snapshot namespace at
`snapshots/sha256/<digest>.json` plus the archive path recorded inside it.
The implemented fixture loader reads only the requested digest path and then
rechecks its bytes. It does not scan a mutable directory or choose a “latest”
file, which makes offline, corruption, and restart tests deterministic.

The source store verifies the compressed SHA-256 before parsing, validates the
entire archive into a bounded in-memory tree, then writes a private staging
directory. It rejects absolute or non-NFC paths, traversal, backslashes,
links, special files, duplicates, and file/directory conflicts. The extracted
manifest must match the selected package/version, provide a library target,
contain no path dependencies, and repeat exactly the immutable index
dependencies. Only then are archive and source entries published atomically.

Each published source contains canonical compiler-owned metadata with its
provider identity, bounds, and a deterministic tree digest. Lookup reads with
no link following and recomputes the tree digest and manifest identity every
time; corrupt entries are never returned and can be replaced from the already
verified archive bytes. Empty or partial staging directories never occupy the
content address.

## Research Basis

The 2025 lockfile survey separates exact versions, integrity, and temporal
reproducibility instead of treating a lockfile as one undifferentiated cache.
HyperRes models provider namespaces separately from package/version vertices,
supporting Salicin's explicit registry identity on every request. Go's module
protocol authenticates source before cache extraction and specifies portable
archive structure; TUF consistent snapshots motivate digest-named immutable
metadata. Salicin starts with a smaller checksum-pinned snapshot protocol and
leaves signatures, transparency, and hosted policy outside this milestone.

- [The Design Space of Lockfiles Across Package Managers (2025)](https://arxiv.org/abs/2505.04834)
- [Solving Package Management via Hypergraph Dependency Resolution (2025)](https://arxiv.org/abs/2506.10803)
- [Does Functional Package Management Enable Reproducible Builds at Scale? Yes (2025)](https://arxiv.org/abs/2501.15919)
- [Go Modules Reference](https://go.dev/ref/mod)
- [TUF roles and metadata](https://theupdateframework.io/docs/metadata/)
- [Securing Packages in npm, Homebrew, PyPI, Maven Central, and RubyGems (USENIX Security 2025)](https://www.usenix.org/conference/usenixsecurity25/presentation/steindler)
- [Wormholes in the File System: Understanding the Misunderstanding of Symlinks (USENIX Security 2026)](https://www.usenix.org/conference/usenixsecurity26/presentation/liu-yongheng)
- [Securing the Software Package Supply Chain for Critical Systems (2025)](https://arxiv.org/abs/2505.22023)

## Non-goals

The current registry subsystem does not refresh an index, access a network,
wire registry packages into CLI compilation, distribute trust roots, or
standardize a public service. Those boundaries remain explicit PKG-4 or later
work rather than partially enabled behavior.
