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

## P0: No Active Tasks

All accepted implementation tasks are complete. New work enters this queue
only after its contract and roadmap position pass the change gate below.

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
- analyzer decomposition not required by an active outcome;
- typed compile-time reflection and syntax/declaration fragment sorts.

## Deferred

- multi-shot continuations;
- implicit I/O or allocation effects;
- garbage collection;
- runtime trait objects and open-world dispatch;
- macros or general compile-time execution without a separately accepted
  staging and reflection contract;
- a public registry service;
- a stable ABI or 1.0 compatibility promise.
