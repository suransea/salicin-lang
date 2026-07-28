# Project TODO

Status: executable queue

This file contains only unfinished work accepted by the
[roadmap](roadmap.md). [Status](status.md) records implemented behavior and the
[changelog](../../CHANGELOG.md) records completed work.

Priority meanings:

- **P0**: the active milestone; each workstream runs in listed dependency
  order;
- **P1**: the accepted next milestone; design work may start, implementation
  waits for the P0 exit gate;
- **P2**: ordered later work; tasks may be refined before their milestone
  opens;
- **Design candidate**: not executable until its contract and roadmap position
  are accepted.

Task IDs are stable. A completed item leaves this queue and is recorded in the
status and changelog instead of remaining as a checked archive.

## P0: Standard Library Usability

### Arrays, slices, vectors, and iteration

- [ ] **COLL-1 — Consistent contiguous access.** Give `array`, `slice`, and
  `vec` a consistent `len`, `is_empty`, checked `get`, trapping `at`/index,
  first/last, slice conversion, and shared or mutable access contract. Checked
  operations return `option` and never form an out-of-bounds borrow.

- [ ] **COLL-2 — `slice` and `array` iteration.** Complete shared and mutable
  iteration for arrays and slices, remove the current copy-only limitation
  from borrowed array traversal, and preserve exclusive yielded-borrow rules.

- [ ] **COLL-3 — `array` and `slice` mutation.** Add swap, reverse, copy/fill
  where element bounds permit, and overlap-safe copy behavior for mutable
  arrays and slices. Validate bounds before mutation and define partial
  progress and cleanup for every effectful operation.

- [ ] **COLL-4 — Common `vec` operations.** Add checked access and slice-based
  extension/copy operations that complement the existing push, insert,
  remove, append, truncate, reverse, and capacity APIs. Allocation failure,
  partial copy, overlap, and move-only element behavior must be explicit.

- [ ] **ITER-1 — Common algorithms.** Provide source-backed `find`,
  `position`, `contains`, `any`, `all`, and `fold` over the narrowest usable
  iterator or slice contracts, forwarding callback effects and preserving
  early-exit cleanup.

### Synchronous host I/O

- [ ] **IO-1 — Explicit host I/O contract.** Define the `io` authority effect,
  entry-point handling, `io_error`, byte-versus-text boundaries, partial
  operations, interruption, resource ownership, close behavior, and the
  supported host matrix. No safe host operation may silently acquire
  `unsafety` or ambient authority.

- [ ] **IO-2 — Console and process support.** Implement process arguments and
  synchronous stdin, stdout, and stderr byte/text operations, including flush,
  EOF, invalid UTF-8, broken pipes, short reads/writes, `read_line`, and
  `print`/`println` plus stderr counterparts. Keep program output separate
  from compiler diagnostics.

- [ ] **IO-3 — Filesystem basics.** Implement owned file handles with
  deterministic cleanup, open/create options, read, write, flush, seek where
  supported, and whole-file convenience functions with bounded allocation and
  recoverable path/permission/encoding errors.

### Test support

- [ ] **TEST-1 — Structured test failure.** Replace boolean-only failure as
  the sole test contract with a source-backed failure path that carries an
  optional formatted message, is interpreted per registration, cleans test
  resources exactly once, and allows later registrations to run. Preserve a
  simple boolean migration path while the compiler is experimental.

- [ ] **TEST-2 — Common assertions.** Add `assert`, `assert_eq`, `assert_ne`,
  `fail`, and common `option`/`result` expectation helpers with static
  `eq`/formatting bounds, single evaluation of operands, useful failure
  messages, and no generated names in output.

- [ ] **TEST-3 — Runner selection and reporting.** Add deterministic
  `salic test --list` and name filtering, selected/failed/passed counts, and
  clear exit behavior. Keep source order, duplicate-name diagnostics, package
  selection, dependency isolation, and one-runner batching.

### Acceptance

- [ ] **STD-3 — Practical standard-library acceptance.** Add a multi-module
  command-line example and native suites that read arguments or input, parse
  text, process arrays/slices/vectors, format output, use files where
  available, and exercise standard assertions. Verify Unicode boundaries,
  errors, early exits, allocation balance, deterministic output, and
  documentation examples.

P1 is complete only when every workstream above and the roadmap milestone exit
conditions are satisfied.

## P2: Persistent Incremental Builds

- [ ] **INCR-2 — Persistent cache contract.** Specify the cache root, schema
  version, fingerprint mapping, LLVM IR payload, metadata, atomic publication,
  concurrent access, corruption handling, bypass behavior, and explicit
  non-goals. Keep output paths and graph-local IDs out of cache identity.

- [ ] **INCR-3 — Cache storage layer.** Implement content-addressed lookup and
  atomic write/replace with strict metadata validation. A missing, malformed,
  truncated, or incompatible entry is a miss; it must never be executed or
  reported as a compiler diagnostic.

- [ ] **INCR-4 — Compile pipeline integration.** Reuse cached IR for
  `emit-ir`, `build`, `run`, and `test` after manifest resolution and
  fingerprinting. Publish only after semantic analysis, cleanup verification,
  constant evaluation, and deterministic LLVM emission succeed. Keep native
  linking and `check` behavior outside the first cache.

- [ ] **INCR-5 — Cache control and observability.** Add a documented way to
  bypass the cache and an inspectable hit/miss reason that does not pollute
  program stdout. Define safe cleanup of compiler-owned entries without
  deleting user outputs.

- [ ] **INCR-6 — End-to-end invalidation proof.** Cover cold and warm
  equivalence, checkout relocation, compiler and schema changes, target and
  command targets, embedded libraries, provider identities, dependency
  aliases, module paths, source bytes, corrupt entries, failed compilation,
  and concurrent readers.

## P2: LSP Diagnostics Baseline

- [ ] **LSP-1 — Structured diagnostic origins.** Replace resolver
  message-parsing and remaining location fallbacks with structured document
  identity, phase, source range, severity, and stable diagnostic code.

- [ ] **LSP-2 — Versioned workspace snapshots.** Add a stateful analysis
  session that overlays open buffers on the resolved package graph, tracks
  document versions, and discards superseded results without writing files.

- [ ] **LSP-3 — Minimal stdio transport.** Implement `salic lsp` with JSON-RPC
  framing, initialize/shutdown lifecycle, workspace selection, and full-text
  open/change/save/close synchronization.

- [ ] **LSP-4 — Diagnostics and semantic tokens.** Publish phased diagnostics
  and compiler-derived tokens with exact URI and UTF-16 ranges across a
  multi-file package.

- [ ] **LSP-5 — Protocol acceptance suite.** Test recorded client transcripts,
  malformed requests, Unicode, multiple documents, stale versions,
  cancellation, server restart, and clean shutdown without depending on a
  particular editor.

## P2: Semantic Navigation

- [ ] **NAV-1 — Semantic occurrence index.** Define stable snapshot-local
  identities and source occurrences for declarations, aliases, fields,
  variants, overloads, trait members, implementations, and references.

- [ ] **NAV-2 — Definition, references, and hover.** Expose cross-module and
  cross-package navigation while keeping dependency-owned source read-only and
  generated specialization names private.

- [ ] **NAV-3 — Safe rename.** Produce complete non-overlapping workspace edits
  with explicit refusal for ambiguous, generated, foreign-symbol, or
  dependency-owned targets.

## P2: Registry Source Dependencies

- [ ] **PKG-1 — Registry input contract.** Finalize manifest spelling, registry
  identity/configuration, immutable index snapshot format, archive layout,
  checksum ownership, cache roots, and local-fixture protocol.

- [ ] **PKG-2 — Registry resolution.** Extend the provider graph with
  highest-compatible non-yanked selection while preserving exact lockfile
  identities and deterministic graph ordering.

- [ ] **PKG-3 — Verified source cache.** Download or load archives into an
  atomic checksum-addressed cache, reject traversal and identity mismatch, and
  expose sources only after verification.

- [ ] **PKG-4 — Locked and frozen acceptance.** Prove that `--locked` cannot
  change selection and `--frozen` performs no network access, including
  yanked locks, missing entries, corrupt archives, conflicts, and cycles.

## Definition of Done

A task is complete only when:

1. its contract, failure behavior, and non-goals are documented;
2. unit, integration, CLI, cross-module, restart, corruption, and native tests
   cover the relevant boundary;
3. diagnostics identify source constructs or operational causes without
   leaking generated internals;
4. any performance claim has a reproducible measurement or observable proof;
5. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the
   full test suite pass;
6. status, architecture or contract docs, and changelog are updated together;
7. the change is reviewable in isolation and does not include unrelated
   worktree edits.

## Design Candidates

- per-package incremental compilation and dependency interface hashes;
- compile-time mutation, loops, allocation, and resource-bearing values;
- runtime nominal values as compile-parameter classifiers;
- networking, asynchronous I/O, time, subprocess, and platform services;
- advanced Unicode, regex, hashing, and unordered collections;
- completion and partial-program recovery;
- unsupported async and executor shapes;
- non-host targets and broader C ABI lowering;
- precompiled package distribution;
- analyzer decomposition not required by an active outcome.

## Deferred

- multi-shot continuations;
- implicit I/O or allocation effects;
- garbage collection;
- runtime trait objects and open-world dispatch;
- macros, reflection, or general compile-time execution;
- a public registry service;
- a stable ABI or 1.0 compatibility promise.
