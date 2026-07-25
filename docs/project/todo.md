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

## P0: Async Completion

- [ ] **ASYNC-EFFECT-1: Extend residual specialization beyond direct tail await**

Non-suspending futures support `Copy`, move-only, shared-borrow, and
mutable-borrow captures. Suspended residual specialization supports a
residual first segment followed by finite pure linear await segments; captures
accept by-value `Copy`, move-only, shared-borrow, or mutable-borrow state, and
retained locals must preserve structural `Move`. One-shot branches support
same-type direct-tail child factories; heterogeneous child enums and wrapped
branch-local state remain. Remaining work also covers residual effects in
later segments and loops while preserving handler ownership, cold
construction, and one-shot cleanup.

## P1: ABI Review And Interoperability

Begin this milestone immediately after `ASYNC-EFFECT-1`.

- [ ] **ABI-REVIEW-1: Audit runtime representations and calling boundaries**

Specify and verify the target-dependent representation passed across function,
module, and foreign boundaries for primitives, pointers, aggregates, enums,
callables, effects, and ownership modes. Keep this an experimental ABI review,
not a stability promise.

- [ ] **ABI-CALL-1: Define the native Salicin calling convention**

Make runtime parameter groups, return values, ownership transfer, cleanup
responsibility, effect rows, and error propagation explicit across separately
compiled modules. Reject unsupported boundary types at their declarations.

- [ ] **ABI-LINK-1: Define exported symbols and cross-module linkage**

Specify source visibility, symbol identity, declaration/definition agreement,
generic specialization ownership, and duplicate or incompatible export
diagnostics without freezing symbol names for 1.0.

- [ ] **ABI-C-1: Complete the verified C interoperability surface**

Review `struct(c)` and `foreign(c, ...)` against supported targets, document
which scalar, pointer, array, aggregate, and function signatures are accepted,
and add cross-language layout and call tests. Keep compiler-owned `builtin()`
definitions orthogonal to this boundary.

## P2: Tooling And Packages

- [ ] **TOOL-FMT-1: Define formatter-preserving syntax invariants**
- [ ] **TOOL-LSP-1: Expose parser and semantic spans for an LSP**
- [ ] **PKG-WORKSPACE-1: Design workspaces and registry dependency identities**
- [ ] **PKG-REPRO-1: Specify reproducible dependency resolution**
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
