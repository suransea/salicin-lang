# Stable Incremental Inputs

Status: implemented schema-2 fingerprint contract

Salicin defines a versioned SHA-256 input fingerprint before defining any
on-disk incremental artifact format. `salic fingerprint` resolves the same
package, workspace, target, and lock state as compilation and prints the
64-character lowercase hexadecimal fingerprint without generating LLVM or
native output.

## Schema

Schema version 2 length-prefixes every variable-width field before hashing.
It includes:

- schema and compiler versions;
- host operating system and architecture;
- binary, library, or test target mode;
- for a test target, the presence and exact UTF-8 bytes of its selection
  filter;
- language edition;
- every compiler-owned `core` and `alloc` source module;
- every resolved package provider identity, declared name, exact version, and
  primary-package role;
- every direct dependency alias and target provider identity;
- every source module path, root role, and exact UTF-8 source bytes.

Package and source collections are sorted by semantic identity before
encoding. Dependency edges resolve graph-local IDs to provider identities
before encoding.

## Deliberate Exclusions

The fingerprint excludes:

- graph-local numeric package IDs;
- absolute source and manifest paths used only for diagnostics;
- checkout root, file modification times, directory enumeration order, and
  process environment unrelated to the selected target;
- build/output paths;
- lockfile whitespace and comments.

Moving an unchanged project therefore preserves its key. Reordering package
or source vectors also preserves it. Changing source bytes, provider identity,
dependency aliases, target mode, edition, standard-library source, compiler
version, OS, or architecture invalidates it. Supplying, removing, or changing
a test filter also invalidates it, including the distinction between no filter
and an explicitly empty filter.

Source hashing is intentionally conservative: comments and formatting are
source bytes and invalidate the key even when semantics are unchanged. A
future query-level cache may introduce finer subkeys, but cannot omit any
input represented here.

## Boundary

This fingerprint identifies the compiler's resolved source-to-LLVM input. It
does not claim that LLVM text, native objects, linker output, or executable
artifacts are compatible across compiler versions or host targets. A future
native artifact cache must additionally key Clang/LLVM version, target triple,
link options, allocator runtime, and external native libraries.

The accepted [persistent LLVM-IR cache contract](incremental-cache.md) maps
this fingerprint to artifact-schema-2 entries. Persistent storage and compile
pipeline integration remain separate implementation workstreams.
