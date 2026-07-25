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

## Current: ABI And Compiler Definitions

- [ ] **BUILTIN-1: Mark every compiler-owned core definition with private `builtin()` initializers**

`ABI-REP-1` is complete: `struct(c)` is the only C data representation
constructor, composes with ordinary struct options, preserves target C
alignment and padding, validates concrete generic instances, and rejects
empty or representation-unstable fields with source-level diagnostics.

`ABI-FOREIGN-1` is complete: every foreign-owned function uses a complete
`foreign(c)` or `foreign(c, "symbol")` initializer, omitted symbols default
to the declaration name, calls implicitly require `Unsafe`, and grouped
`extern` plus all `@` syntax have no accepted grammar path.

The remaining compiler-definition task introduces no `@` syntax.
`builtin()` is a complete declaration marker typed by the declaration annotation and must be
eliminated before code generation. Trait requirements remain bodyless, and user opaque types are
outside `BUILTIN-1`.

## Async Follow-Up

- [ ] **ASYNC-EFFECT-1: Extend residual specialization beyond direct tail await**

Non-suspending futures support `Copy`, move-only, shared-borrow, and
mutable-borrow captures. The first suspended slice supports one direct tail
await with no retained local or continuation state and by-value `Copy` or
move-only captures. Remaining work covers post-await continuations, retained
locals, branches, loops, and borrowed suspended captures while preserving
handler ownership, cold construction, and one-shot cleanup.

## Later

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
