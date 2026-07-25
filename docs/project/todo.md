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

- [ ] **ASYNC-CONTROL-1: Lower suspension nested in control flow**

## Next

- [ ] **ASYNC-EFFECT-1: Specialize generated polling through residual handlers**
- [ ] **ASYNC-EXEC-1: Provide one explicit minimal executor**

`MOVE-TRAIT-1`, `ASYNC-STATE-1`, `ASYNC-POLL-1`, `ASYNC-CANCEL-1`, and `ASYNC-BORROW-1` are
complete. Cold async blocks materialize compiler-owned nominal state, preserve owned captures and
live sequential locals across relocation, and drop initialized cold or suspended state exactly
once. Typed polling returns `Poll.Ready(T)` once, retains a child across `Pending`, resumes linear
sequential awaits, and traps on completed-state repoll. State-internal borrow chains live across an
await are rejected as non-`Move`; region-checked borrows of external storage remain supported. An
unhandled `Unsafe` requirement is inferred onto `poll`.

The current task must lower suspension points nested in `if`, `match`, and loop control flow while
preserving branch-local liveness and deterministic cancellation. Homogeneous `if` and `match`
branches whose bodies are a single tail await now hoist selection before one shared suspension.
Branches may use different concrete child-future types when their Output agrees; a private
active-variant future dispatches polling and cancellation. Branches may contain linear local
prefixes and continuations, and branches without an await become immediate Ready futures. Loop
suspension with a reachable backedge remains. Loops proven to exit on their first entered iteration
already hoist their suspension without recursive state, including false or suspended pre-test
`while` conditions. Recurring loops are classified by loop kind, suspension location, `continue`,
fallthrough, and value-producing `break`. A `loop` with one await and a boolean
break/continue decision now reuses one child slot through a private step enum and poll-local HIR
loop. Its `Break(Output)` path now preserves inferred value outputs, including move-only values;
an omitted `else` or a non-suspending branch body forms the fallthrough backedge. Generalize that
path to multiple suspension points and explicit move-only `Carry`. Recurring pre-test and post-test
`while` loops re-evaluate a pure condition before constructing the next child; value-producing
`break` remains invalid because `while` is unit-valued. Residual algebraic-effect specialization is
a separate follow-up task.

Loop completion requires an allocation-free reusable iteration state. The generated iteration
returns an internal `Continue(Carry)` or `Break(Output)` outcome, transfers loop-carried values by
value, polls consecutive immediately-ready iterations without recursion, and retains only the
active iteration across `Pending`. Tests must cover `while` false exits, `continue`, fallthrough,
value-producing `break`, loop-carried ownership, child-slot reuse, and cancellation cleanup.

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
