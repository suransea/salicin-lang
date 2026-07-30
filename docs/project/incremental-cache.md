# Persistent LLVM-IR Cache Contract

Status: implemented storage contract

This contract defines Salicin's first persistent compilation cache. The cache
is a local performance optimization: deleting it, bypassing it, or missing an
entry cannot change language semantics, diagnostics, program output, or exit
status.

## Root and Ownership

`SALICIN_CACHE_DIR`, when set to a non-empty absolute path, selects the Salicin
cache root. A relative override is rejected rather than interpreted relative
to the invocation directory. Without an override, the platform user-cache
location is used:

- macOS: `$HOME/Library/Caches/salicin`;
- Linux and other Unix: `$XDG_CACHE_HOME/salicin` when `XDG_CACHE_HOME` is
  absolute, otherwise `$HOME/.cache/salicin`;
- Windows: `%LOCALAPPDATA%\salicin\cache`.

If no absolute user-cache root can be established, caching is disabled for
that invocation. Compilation continues normally. Cache implementation must
create private directories where the platform supports permissions and must
not follow symbolic links while validating or publishing an entry. The cache
is not a security boundary against another process running as the same user.

Only descendants of the selected root are compiler-owned. Build outputs,
source trees, and the separate temporary native runtime-object cache are never
cache entries. Cleanup must first validate the exact root and a
compiler-created ownership marker; it must never recursively delete an
unvalidated path.

Opening a store creates `.salicin-cache-root` atomically with the exact
contents `salicin-cache-root-v1\n`. An existing symbolic-link, non-regular, or
byte-different marker is rejected. A nonempty directory without the marker is
not claimed, so a mistaken override cannot turn pre-existing user data into a
future cleanup target. The storage API exposes root unavailability and storage
errors separately; later pipeline integration turns either into disabled
caching rather than a source diagnostic.

## Identity and Layout

Input schema 2 is the SHA-256 contract in
[Stable Incremental Inputs](incremental-inputs.md). Test compilation identity
includes whether `--filter` was supplied and its exact UTF-8 bytes, because
the current test compiler emits only selected registrations.

Artifact schema 2 maps a 64-character lowercase input fingerprint `H` to:

```text
<root>/llvm-ir/v2/sha256/<H[0..2]>/<H[2..64]>/
  metadata.toml
  module.ll
```

The mapping is implemented by `cache_entry_relative_path`. It contains no
output path, checkout path, graph-local package ID, diagnostic path, timestamp,
or directory enumeration order. The artifact schema is independent of the
input schema so storage representation can change without silently reusing an
old payload.

## Metadata and Payload

`module.ll` is the exact UTF-8 LLVM text emitted after successful semantic
analysis, constant evaluation, ownership cleanup verification, and
deterministic code generation. It is the whole selected package graph, not a
native object or executable.

`metadata.toml` is canonical UTF-8 with LF line endings and these fields:

```toml
schema = 2
kind = "llvm-ir"
fingerprint = "<64 lowercase hexadecimal characters>"
compiler = "<exact Salicin package version>"
host_os = "<std::env::consts::OS>"
host_arch = "<std::env::consts::ARCH>"
edition = "2026"
target = "binary" # or "library" / "test"
test_names = [] # ordered selected names for test entries
test_names_sha256 = "<SHA-256 of the length-framed name sequence>"
payload_bytes = 1234
payload_sha256 = "<64 lowercase hexadecimal characters>"
```

Readers reject missing, duplicate, unknown, incorrectly typed, or noncanonical
fields. They also require regular files, the exact artifact schema and kind,
metadata matching the directory key and current invocation, the exact payload
length, the payload SHA-256, and valid UTF-8. Validation happens before cached
IR reaches Clang. A digest detects damage; it does not authenticate writes by
another same-user process.

Binary and library entries require an empty `test_names` list. Test entries
store the selected names in source order, including an empty selection, so
`test --list` and native failure-index reporting remain correct on a hit. The
name digest hashes the list length followed by each UTF-8 byte length and byte
sequence; empty or duplicate names are rejected.

## Publication and Concurrency

A writer constructs both files in a uniquely named sibling temporary
directory, validates its own completed entry, synchronizes files where the
platform supports it, and makes the directory visible with one atomic rename.
Readers inspect only the final fingerprint directory and therefore cannot see
a partial metadata/payload pair.

Concurrent writers need no global lock. The first valid publication wins; a
loser validates the winner and discards its temporary directory. If the final
entry is invalid, a writer atomically moves it to a unique compiler-owned
invalid sibling before attempting publication. Races revalidate the current
winner. Abandoned temporary and invalid siblings are never readable entries
and may be removed by bounded maintenance.

Atomic visibility is required. Crash durability beyond the filesystem's
documented rename and synchronization guarantees is best effort.

## Misses, Corruption, and Bypass

A nonexistent, unreadable, malformed, truncated, incompatible, mismatched, or
non-regular entry is an ordinary cache miss. It never becomes a source
diagnostic and is never passed to the native toolchain. Compilation proceeds
from source and may replace the invalid entry after successful emission.
Failed parsing, analysis, code generation, or native linking cannot publish a
new entry. A native-link failure does not invalidate already validated LLVM
IR.

`emit-ir`, `build`, `run`, and `test` perform lookup after complete input
resolution and publish only successfully emitted IR. `--no-cache` on those
commands forbids both lookup and publication for that invocation. It still
resolves and fingerprints the complete input and does not alter generated IR.
`check` always analyzes source and rejects cache-control flags.

`--cache-trace` writes decisions only to stderr. Each line identifies the
binary, library, or test target, its complete lowercase fingerprint, and one
of: `bypassed`, `hit`, a structured miss reason, disabled caching with its
cause, publication, concurrent-winner reuse, or a nonfatal publication error.
It never changes ordinary stdout, generated IR, native program output, or exit
status.

`salic cache clean` first resolves and opens the exact cache root, including
ownership-marker validation. It atomically renames only the `llvm-ir`
namespace to a unique detached sibling and removes that detached directory
without following a namespace symbolic link. The root marker and unrelated
root children remain. A missing namespace succeeds as already empty; an
unowned root, invalid marker, symbolic link, non-directory namespace, or I/O
failure is reported and nothing outside the validated namespace is removed.
Concurrent compilations may recreate the namespace after detachment; a writer
racing inside the detached snapshot may miss publication and continue
normally from its compiled result.

## Explicit Non-Goals

Artifact schema 2 does not provide:

- per-package, function, query, or dependency-interface reuse;
- native object, executable, linker, CTFE-result, or diagnostic caching;
- remote cache protocol, sharing between users, authentication, eviction, or
  size policy;
- compatibility across compiler versions, artifact schemas, host OS or
  architecture, editions, command targets, or test selections;
- a stable serialized Salicin IR, package ABI, or public artifact format.

Whole-graph LLVM IR is deliberately conservative. Finer reuse requires stable
subcomputation identities and dependency tracking rather than weakening this
entry's validation.
