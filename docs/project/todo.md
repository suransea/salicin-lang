# Project TODO

Status: executable queue

This file contains only unfinished or immediately preparatory work. The
[roadmap](roadmap.md) defines milestone order, [status](status.md) records implemented behavior,
and the [changelog](../../CHANGELOG.md) records completed work.

Priority meanings:

- **P0**: current task or regression blocker;
- **P1**: next milestone preparation;
- **P2**: accepted later work whose entry gate is not open;
- **Deferred**: requires a new design decision.

## Current

- [ ] **MOVE-TRAIT-1: Add the source-backed structural Move marker**
  - Validate `core.marker.Move` and make `Copy` inherit it.
  - Require `Move` for relocation through parameters, returns, assignment, and storage growth.
  - Preserve direct in-place initialization as the future boundary for non-`Move` values.
  - Cover structural derivation, generic bounds, diagnostics, and existing resource behavior.

## Next

- [ ] **ASYNC-STATE-1: Lower cold async blocks to state machines**
- [ ] **ASYNC-POLL-1: Implement typed polling transitions**
- [ ] **ASYNC-CANCEL-1: Drop initialized state on cancellation**
- [ ] **ASYNC-BORROW-1: Reject first-version self-referential states**
- [ ] **ASYNC-EXEC-1: Provide one explicit minimal executor**

Async state-machine lowering starts only after `MOVE-TRAIT-1` is complete.

## Later

- [ ] **TOOL-FMT-1: Define formatter-preserving syntax invariants**
- [ ] **TOOL-LSP-1: Expose parser and semantic spans for an LSP**
- [ ] **PKG-WORKSPACE-1: Design workspaces and registry dependency identities**
- [ ] **PKG-REPRO-1: Specify reproducible dependency resolution**
- [ ] **ABI-1: Design an external ABI after representation review**
- [ ] **INCR-1: Define stable incremental-compilation inputs**

## Definition of Done

A task is complete only when:

1. its source semantics and rejection boundaries are documented;
2. positive and negative tests cover typing, ownership, and effects;
3. diagnostics identify source constructs rather than generated internals;
4. native tests cover relevant execution, trap, resume, abandon, and cleanup paths;
5. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the full test suite pass;
6. status and changelog entries are updated;
7. the commit is pushed with a clean worktree.

## Deferred

- multi-shot continuations;
- implicit IO or allocation effects;
- garbage collection;
- runtime trait objects and open-world dispatch;
- macros, reflection, or general compile-time execution;
- a public registry service;
- a stable ABI or 1.0 compatibility promise.
