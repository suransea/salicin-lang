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

## P0: LSP Diagnostics Baseline

- [ ] **LSP-5 — Protocol acceptance suite.** Test recorded client transcripts,
  malformed requests, Unicode, multiple documents, stale versions,
  cancellation, server restart, and clean shutdown without depending on a
  particular editor.

## P1: Semantic Navigation

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

- language consistency: define static declaration-former and constraint-guard
  sorts, then add source-defined contracts for parser-owned `extend` and
  `requires`; keep the existing source-declared `test` contract unit-returning
  with only `throwing(string)`, and do not model either missing form as an
  ordinary runtime function;
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
