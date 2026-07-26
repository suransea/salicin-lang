# Project Roadmap

Status: active project direction

This roadmap defines outcomes, order, and entry and exit gates. It does not
define language behavior; the [language specification](../language/specification.md)
and [grammar](../language/grammar.md) do that. Current implementation facts
belong in [status](status.md), executable work in [TODO](todo.md), and completed
work in the [changelog](../../CHANGELOG.md).

## Product Direction

Salicin's next phase turns the implemented language core into a compiler that
can support daily development. The near-term order is deliberately:

1. make ordinary programs practical with text, collections, host IO, and test
   support;
2. make unchanged builds reusable;
3. make source analysis continuously available to editors;
4. build source navigation on structured semantic identities;
5. make locked third-party source dependencies reproducible.

New language surface is not a near-term goal unless one of those outcomes
requires it. Every milestone must continue to preserve:

- deterministic left-to-right evaluation and exactly-once cleanup;
- source-level diagnostics without generated implementation names;
- static dispatch and bounded monomorphization;
- explicit authority for unsafe operations and effects;
- ordinary library declarations wherever compiler primitives are unnecessary;
- reproducible behavior independent of checkout path and traversal order.

## Planning Model

The roadmap uses ordered milestones rather than release dates. One milestone
is active at a time:

- **Now** is implementation-ready and owns the P0 queue.
- **Next** has an accepted outcome but starts only after the current exit gate.
- **Later** is ordered and accepted at milestone granularity; individual tasks
  may still need a design contract before implementation.
- **Design candidates** are real gaps but are not promises or active work.

Completed milestones are removed from this file. Their behavior is recorded
in [status](status.md), their contracts remain under `docs/project`, and their
history remains in the changelog.

## Now: Standard Library Usability

The language core can already express ownership-sensitive containers and
effectful programs, but the library surface is not yet sufficient for ordinary
command-line applications. `String` owns validated UTF-8 but lacks borrowed
text, character iteration, search, and formatting. `Array`, `Slice`, and
`Vec` do not yet share a consistent safe-access and algorithm vocabulary.
There is no host-facing `std` layer, and `test("name")` registrations return
only a boolean without standard assertions or structured failures.

This milestone fills those gaps before adding more language features. It is
delivered in small end-to-end slices:

1. fix the module, error, naming, and ownership contracts for the minimum
   standard-library surface;
2. complete UTF-8 text and common collection operations;
3. add parsing and source-backed formatting needed by IO and test messages;
4. add explicit synchronous console, process, and filesystem authority;
5. build assertion helpers and runner ergonomics on the same formatting and IO
   contracts.

The small prelude remains small. APIs live in explicit `std` modules, safe
operations preserve UTF-8 and collection initialization invariants, and host
operations require visible authority rather than introducing ambient IO.

Exit conditions:

- a standard-library surface document records the initial module map,
  naming conventions, ownership modes, error types, trap-versus-`Option` or
  `Result` behavior, and portability boundary;
- text has a zero-allocation runtime string literal, an invariant-preserving
  borrowed UTF-8 view, a Unicode scalar value, byte and scalar iteration,
  boundary-safe slicing, comparison, common search, and corresponding
  `String` construction and mutation operations;
- `Array`, `Slice`, and `Vec` share `len`, `is_empty`, checked access,
  first/last access, slicing or slice conversion, shared and mutable
  iteration, common mutation, and search/fold predicates where their ownership
  permits;
- `Option` and `Result` expose common inspection, borrowing, transformation,
  and fallback operations, and primitive numbers expose bounded conversion
  and basic utility functions without bypassing checked arithmetic;
- integer, boolean, Unicode scalar, and text parsing or formatting is
  sufficient for diagnostics, console output, and assertion messages without
  macros or reflection;
- a real `std` host layer provides process arguments, synchronous stdin,
  stdout and stderr, and deterministic file open/read/write/close with
  explicit IO authority and recoverable errors;
- standard test support provides `assert`, equality and inequality
  assertions, explicit failure, common `Option`/`Result` expectations, and
  messages that identify the failing registration without aborting the
  remaining runner;
- `salic test` can list and filter registrations, while dependency tests
  remain isolated unless their package is selected;
- native examples exercise text parsing, collection processing, file or
  console IO, and standard assertions with deterministic cleanup and no
  allocation leaks.

The first milestone does not include Unicode normalization, locale-sensitive
case mapping or collation, grapheme segmentation, regex, hash collections,
networking, asynchronous IO, formatting macros or interpolation syntax,
property testing, mocking, or benchmarking.

## Next: Persistent Incremental Builds

The existing schema-1 fingerprint already identifies the semantic and native
inputs to one selected package-graph target. This milestone turns that
read-only identity into a safe, content-addressed cache without changing
language semantics or freezing a precompiled package format.

The first cache is intentionally whole-graph and stores compiler-owned LLVM IR.
Manifest resolution and fingerprinting still run on every invocation; an
unchanged hit may skip semantic analysis and LLVM generation. Native linking
remains a separate step so output selection and host linker failures are not
hidden by the cache.

Exit conditions:

- cache location, key, payload schema, ownership, and invalidation rules are
  documented independently of output paths;
- `build`, `run`, `test`, and `emit-ir` can reuse valid cached IR, while
  `check` continues to perform source analysis;
- misses, disabled caching, corrupt or truncated entries, compiler/schema
  changes, source changes, target changes, and dependency changes behave
  deterministically;
- entries are written atomically, a failed compilation cannot publish an
  entry, and concurrent readers never observe partial data;
- CLI output makes cache use inspectable without changing ordinary program
  stdout or exit status;
- cold and warm outputs are byte-equivalent where the current compiler
  promises deterministic IR, and the complete repository quality gate passes.

This milestone does not promise per-package reuse, cross-compiler cache
compatibility, remote caching, eviction policy, or a stable binary artifact
format.

## Later: LSP Diagnostics Baseline

The transport-independent editor API already exposes UTF-8 byte ranges,
UTF-16 positions, tokens, and phased diagnostics. The next milestone adds a
stateful workspace session and a minimal Language Server Protocol transport.

The baseline covers workspace discovery, full-document synchronization,
cancellation or supersession of stale analyses, diagnostics, and semantic
tokens. In-memory editor buffers take precedence over disk without mutating
source files. Resolver and semantic diagnostics must carry structured source
origins; the server must not recover locations by parsing rendered messages.

Exit conditions:

- a versioned workspace snapshot can overlay opened documents and reanalyze a
  complete source graph;
- stale results cannot replace diagnostics for a newer document version;
- `salic lsp` supports initialize, shutdown, open, change, save, and close over
  stdio JSON-RPC;
- lexer, parser, resolver, and semantic failures publish to the correct URI
  with UTF-16 ranges;
- semantic tokens use the compiler token model and remain stable for Unicode
  source;
- protocol transcript tests cover malformed messages, multiple files,
  out-of-order edits, cancellation, and clean shutdown.

Incremental parsing, completion, hover, references, rename, and editor-specific
extensions are not part of this baseline.

## Later: Semantic Navigation

Navigation follows the LSP baseline because it needs long-lived analysis
snapshots and stable source identities. The compiler will expose a semantic
occurrence index that distinguishes declarations, references, overloads,
aliases, fields, variants, traits, implementations, and generated
specializations without exposing compiler-generated names.

Exit conditions:

- source declarations and uses have stable identities within one snapshot;
- go-to-definition, references, and hover work across modules and packages;
- rename produces a complete, non-overlapping workspace edit or refuses the
  operation when identity or visibility is ambiguous;
- aliases, shadowing, overloads, Unicode identifiers, and dependency-owned
  read-only source have explicit tests and rejection behavior.

Completion remains a separate follow-up because candidate ranking and partial
syntax recovery require their own contract.

## Later: Registry Source Dependencies

The implemented resolver already fixes package provider identity, lockfile
semantics, and workspace/path resolution. The
[dependency resolution contract](dependency-resolution.md) also defines the
registry selection algorithm. This milestone implements a registry client
against immutable index snapshots and verified source archives; it does not
create or standardize a public registry service.

Exit conditions:

- manifests can declare registry source dependencies without weakening
  workspace/path identity rules;
- resolution selects the highest compatible non-yanked version from one
  identified snapshot and records exact provider and checksum data;
- archives are verified before manifest or source consumption and extracted
  without path traversal or partial-cache visibility;
- `--locked` cannot change the selected graph, and `--frozen` succeeds only
  from verified local index and archive data;
- local fixture registries cover version conflicts, yanking, checksum
  mismatch, cache corruption, offline operation, and dependency cycles.

Publishing, credentials, mirrors, a hosted service, precompiled interfaces,
and a stable package protocol remain outside this milestone.

## Design Candidates

These gaps need an accepted contract and sequencing decision before entering
the executable queue:

- per-package incremental reuse based on dependency interface digests;
- networking, asynchronous IO, time, subprocess, and platform-service APIs;
- Unicode normalization, grapheme segmentation, locale-sensitive text, and
  regular expressions;
- hash maps, hash sets, and a stable hashing contract;
- completion and partial-program analysis;
- the currently diagnosed async shapes: recursive erasure, effectful loop
  conditions, nested residual iteration, and move-only backedge factories;
- a wake-aware executor and host async runtime;
- non-host targets and a target-aware ABI;
- broader by-value C interoperability;
- precompiled package interfaces and distribution artifacts;
- analyzer decomposition along the existing semantic phase boundaries.

Refactoring may accompany an active milestone when it creates a narrow
boundary required by that milestone. A repository-wide rewrite is not itself
a roadmap outcome.

## Deferred

The following remain intentionally outside the accepted roadmap:

- multi-shot continuations;
- implicit ambient IO or allocation authority;
- garbage collection as a second ownership model;
- runtime trait objects and open-world dispatch;
- macros, reflection, and general compile-time execution;
- a public package registry service;
- a stable ABI or 1.0 compatibility promise.

## Change Gate

A milestone or language change enters the executable queue only when it:

1. names a user-visible or compiler-operational outcome;
2. identifies affected ownership, effects, evaluation, cleanup, source, and
   package boundaries;
3. defines failure behavior and source-level diagnostics;
4. has positive, negative, restart or corruption, cross-module, and native
   evidence where relevant;
5. states what is deliberately not included;
6. leaves formatting, Clippy, tests, and documentation clean.
