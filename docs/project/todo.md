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

- [ ] **ASYNC-EFFECT-1: Specialize generated polling through residual handlers**

`MOVE-TRAIT-1`, `ASYNC-STATE-1`, `ASYNC-POLL-1`, `ASYNC-CANCEL-1`, `ASYNC-BORROW-1`, and
`ASYNC-CONTROL-1`, and `ASYNC-EXEC-1` are complete. Generated futures preserve relocation, branch-local state,
sequential and nested suspension, reusable loop iterations, move-only carry, and exact completion
or cancellation cleanup. Recurring `loop`, pre-test `while`, and post-test `while` cover false
exits, explicit `continue`, fallthrough, value output, `Never` output, heterogeneous branch
children, multiple suspension points, and allocation-free child-slot reuse.
The ordinary zero-field `core.async.Spin` executor polls one owned `Future(E)` until `Ready`
without implicit allocation or runtime selection.

The current task must remove the source-level rejection of residual `Throws` and custom algebraic
effects in async bodies and continuations. Generated `Future(E)` implementations must specialize
their poll/resume boundary through the concrete residual row, preserve handler ownership and
one-shot continuation rules, and keep construction cold. `Unsafe` already propagates as a residual
poll requirement and remains the baseline behavior.

## Later

- [ ] **TOOL-FMT-1: Define formatter-preserving syntax invariants**
- [ ] **TOOL-LSP-1: Expose parser and semantic spans for an LSP**
- [ ] **PKG-WORKSPACE-1: Design workspaces and registry dependency identities**
- [ ] **PKG-REPRO-1: Specify reproducible dependency resolution**
- [ ] **ABI-REP-1: Replace legacy C representation syntax with `struct(c)`**
- [ ] **ABI-FOREIGN-1: Replace grouped extern declarations with per-declaration `foreign(c, ...)`**
- [ ] **BUILTIN-1: Mark every compiler-owned core definition with private `builtin()` initializers**
- [ ] **INCR-1: Define stable incremental-compilation inputs**

The ABI tasks must remove `rep c`, `@link_name`, and `extern "C"` without introducing `@` syntax.
`foreign` calls implicitly require `Unsafe`; `builtin()` is a complete declaration marker typed by
the declaration annotation and must be eliminated before code generation. Trait requirements
remain bodyless, and user opaque types are outside `BUILTIN-1`.

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
