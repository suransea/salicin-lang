# Project TODO

Status: executable queue

This file tracks work that can be started. It is intentionally narrower than the
[roadmap](roadmap.md). Keep at most one language task in progress, preserve stable task IDs in
commits and discussions, and remove completed details after recording them in the changelog.

Priority meanings:

- **P0**: current milestone or regression blocker;
- **P1**: next milestone preparation;
- **P2**: accepted later work whose entry gate is not yet open;
- **Deferred**: not actionable without a new design decision.

## Current focus

Current milestone: **EH1, close the algebraic-handler extension**

Next task: **EFF-DIAG-1**

### P0 handler queue

- [ ] **EFF-DIAG-1: Inventory handler rejection boundaries**
  - Give each intentional rejection a source location, stable message, and negative fixture.
  - Remove diagnostics for shapes completed by the tasks above.
  - Link every remaining rejection to the implementation-status boundary it enforces.

### EH1 stop condition

After the P0 handler queue passes its exit conditions, stop adding handler features. Move to
**M0-AUDIT-1** even if adjacent handler or async work looks convenient.

## P1 M0 release baseline

- [ ] **M0-AUDIT-1: Build the M0 conformance matrix**
  - Map every bullet in `core-scope.md` to positive, negative, diagnostic, and native tests.
  - Mark missing evidence as a task; do not silently reclassify it as an extension.

- [ ] **M0-DIAG-1: Attach source spans to semantic errors**
  - Prioritize ownership, borrow, handler, trait selection, and generic inference errors.
  - Ensure generated internal names do not leak into user-facing diagnostics.

- [ ] **M0-QUALITY-1: Make the repository quality gate clean**
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - Run the ledger acceptance program from a clean build.

- [ ] **M0-DETERMINISM-1: Verify deterministic compiler output**
  - Compare diagnostics, LLVM symbols, lockfiles, and generated IR across repeated clean builds.
  - Remove hash-map iteration order from user-visible output.

- [ ] **COMPILER-SPLIT-1: Continue the Analyzer boundary split**
  - Move behavior behind existing `codegen/` module ownership boundaries.
  - Keep source rewriting, HIR construction, cleanup planning, and emission phase-separated.
  - Require behavior-preserving tests before data-ownership refactors.

- [ ] **DOC-CHECK-1: Compile documentation examples**
  - Extract or mirror normative Salicin snippets as check fixtures.
  - Distinguish exploration snippets that are intentionally not executable.

## P1 library usability

Entry gate: **M0-QUALITY-1** and **M0-AUDIT-1**

- [ ] **LIB-SLICE-1: Specify and implement slices**
- [ ] **LIB-INDEX-1: Route indexing through source-backed traits**
- [ ] **LIB-ITER-1: Add array, slice, and Vec iterator implementations**
- [ ] **LIB-VEC-1: Complete Vec mutation and consuming-iterator cleanup**
- [ ] **LIB-STRING-DESIGN-1: Decide the minimum owning string model**
- [ ] **LIB-EXAMPLE-1: Add a nontrivial library-style native example**

Each library task requires ownership, aliasing, allocation-failure, bounds, and cleanup behavior to
be explicit where applicable.

## P2 static abstraction

Entry gate: concrete requirements from LIB1

- [ ] **TYPE-GAT-1: Lower generic associated constructors**
- [ ] **TYPE-EQ-1: Add bounded constructor-equation solving**
- [ ] **TYPE-CALLABLE-1: Bridge capturing callables into source protocols**
- [ ] **TYPE-TRAIT-METHOD-1: Support coherent generic trait methods**
- [ ] **TYPE-CONST-NOMINAL-1: Extend compile-time scalar arguments to nominal types and type aliases**
- [ ] **TYPE-DIAG-1: Improve kind and constructor inference diagnostics**

## P2 async exploration

Entry gate: all ASYNC1 prerequisites in the roadmap

- [ ] **ASYNC-CONTRACT-1: Finalize Future, Pin, and executor source contracts**
- [ ] **ASYNC-STATE-1: Lower async handlers to anonymous state machines**
- [ ] **ASYNC-POLL-1: Implement typed poll transitions**
- [ ] **ASYNC-CANCEL-1: Drop initialized state on cancellation**
- [ ] **ASYNC-BORROW-1: Reject first-version self-referential states**
- [ ] **ASYNC-EXEC-1: Provide one explicit minimal executor**

## P2 ecosystem

- [ ] **TOOL-FMT-1: Define formatter-preserving syntax invariants**
- [ ] **TOOL-LSP-1: Expose parser and semantic spans for an LSP**
- [ ] **PKG-WORKSPACE-1: Design workspaces and registry dependency identities**
- [ ] **PKG-REPRO-1: Specify reproducible dependency resolution**
- [ ] **ABI-1: Design an external ABI only after representation review**
- [ ] **INCR-1: Define stable incremental-compilation inputs**

## Recently completed

- [x] **SCOPE-M0-1:** Freeze the M0 core scope and change gate.
- [x] **EFF-FOR-1:** Carry iterator ownership through effectful `for` loops.
- [x] **EFF-OWNED-1:** Replace iterator-name capture exceptions with handler-owned capture policy.
- [x] **EFF-FRAME-1:** Fuse distinct borrowed roots into eligible non-recursive handler frames.
- [x] **EFF-FIELD-1:** Extend frame fusion to stable nested field places.
- [x] **EFF-INDEX-1:** Stage indexed borrow places once and rebuild them from frame-owned roots.
- [x] **EFF-ALIAS-1:** Fuse statically disjoint same-root projections into handler frames.
- [x] **EFF-RESIDUAL-1:** Share owned roots through concrete residual effect rows.
- [x] **EFF-RECUR-1:** Share owned roots through direct and mutually recursive effectful calls.
- [x] **EFF-ACTION-1:** Stage borrowed root and field arguments before direct handler actions.
- [x] **EFF-CALLABLE-1:** Carry open one-shot actions through the erased `EffectCallable` ABI.
- [x] **CORE-MEMORY-1:** Source-back raw pointer and layout-query contracts in `core.memory`.
- [x] **TYPE-CONST-1:** Add `usize` compile-time values and source-back curried `Array(T)(L)`.
- [x] **CORE-BORROW-1:** Move borrow contracts from `core.domains` into `core.borrow`.
- [x] **LIB-ALLOC-API-1:** Keep Box/Vec in the allocation-capable layer and expose only their inherent APIs through std.
- [x] **LIB-BOX-1:** Replace `Box.as_mut_ptr` with consuming `into_raw` and unsafe `from_raw`.
- [x] **EXAMPLE-LEDGER-1:** Process non-`Copy` transactions in an effectful native ledger loop.

## Definition of done

A task is complete only when:

1. source semantics and explicit rejection boundaries are documented;
2. positive and negative tests cover typing and ownership;
3. diagnostics identify source-level constructs rather than generated internals;
4. native tests cover relevant success, trap, resume, abandon, and cleanup paths;
5. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, or an
   already-tracked unrelated gate is called out explicitly;
6. the changelog and implementation status describe the resulting behavior;
7. the commit is pushed with a clean worktree.

## Deferred

Do not turn these into implementation tasks without an accepted design and roadmap gate:

- multi-shot continuations;
- implicit IO or allocation effects;
- garbage collection;
- runtime trait objects and open-world dispatch;
- macros, reflection, or compile-time execution;
- a public registry service;
- a stable ABI or 1.0 compatibility promise.
