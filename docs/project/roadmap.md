# Language roadmap

Status: living project direction

This roadmap orders Salicin's language work. It does not redefine implemented behavior, promise
release dates, or expand the frozen M0 scope. Use the project documents as follows:

| Document | Responsibility |
|---|---|
| [M0 core scope](core-scope.md) | Frozen completion target and feature-admission gate |
| [Implementation status](status.md) | Facts about what the current compiler accepts |
| [Roadmap](roadmap.md) | Milestone order, dependencies, and exit conditions |
| [TODO](todo.md) | Current executable work queue |
| [Changelog](../../CHANGELOG.md) | Completed release history |

## Direction

Salicin aims to be a coherent native language with deterministic ownership and cleanup, explicit
effects, static abstraction, and source-backed control/library contracts. The project should favor a
small complete vertical slice over a broad set of partially interacting features.

The current strategic order is:

1. close the already-advanced algebraic-handler implementation at a bounded, documented boundary;
2. audit and harden the frozen M0 core as a release-quality baseline;
3. make ordinary library programming practical;
4. complete the static abstraction model where real library code requires it;
5. start asynchronous state-machine lowering only after continuation ownership is stable;
6. add ecosystem and compatibility commitments after the language/runtime boundary settles.

## Working rules

1. **M0 regressions block all work.** Implemented extensions may not weaken M0 ownership, cleanup,
   diagnostics, or native execution.
2. **No adjacent feature expansion during a closure milestone.** Handler work may complete handler
   ownership and callable transport, but may not introduce unrelated syntax.
3. **Source-backed before compiler-only.** New control and protocol surfaces should be ordinary
   library declarations validated as lang items unless primitive authority or lowering requires
   compiler support.
4. **Every capability needs four kinds of evidence.** Positive typing, negative rejection,
   diagnostic quality, and native cleanup/execution tests are required before promotion.
5. **Evaluation order is part of semantics.** Rewrites must preserve single evaluation and
   left-to-right argument order.
6. **Exploration does not enter the active queue without its gate.** In particular, async work does
   not start merely because the generic handler machinery can model suspension.

## Milestone EH1: Close the algebraic-handler extension

Priority: active, bounded implementation extension

Purpose: finish ownership transport through the handler paths already implemented, then stop adding
handler surface until M0 hardening is complete.

Scope:

- single-evaluation staging for indexed borrow places entering fused effectful calls;
- disjoint same-root projection analysis without permitting overlapping mutable aliases;
- owned state through recursive named calls and calls with concrete residual effects;
- loan-aware staging for borrowed arguments preceding reusable handler actions;
- one general erased `EffectCallable` path for escaping capturing callables and open target sets;
- consistent source diagnostics for every unsupported handler shape.

Exit conditions:

- direct operations, loops, sequential calls, recursion, and supported residual rows preserve owned
  state and exactly-once cleanup on resume and abandon;
- root, field, and indexed borrow arguments have explicit single-evaluation and alias rules;
- known and erased callable paths share one documented ownership contract;
- unsupported generic residual rows or self-referential callable shapes fail before LLVM emission;
- `examples/ledger.sc` remains the complete native acceptance program;
- the handler section of [implementation status](status.md) contains no untracked structural gap.

Non-goals:

- async/Future lowering;
- multi-shot continuations;
- effect inference that silently changes public function rows;
- new handler syntax.

## Milestone M0-RC: Freeze a release-quality core

Priority: blocks expansion beyond EH1

Purpose: turn the frozen M0 scope from a feature list into a verified baseline suitable for larger
library and tooling work.

Scope:

- a conformance matrix mapping every M0 item to positive, negative, diagnostic, and native tests;
- source locations on semantic and ownership diagnostics;
- deterministic diagnostics and generated symbols across repeated builds;
- a warning-free `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` gate;
- compiler phase cleanup that keeps source rewrite, typed HIR, cleanup planning, and emission
  boundaries explicit;
- documentation examples checked as part of CI.

Exit conditions:

- every frozen M0 item has an owner, test evidence, and an explicit status;
- no M0 feature depends on an exploration-only implementation path;
- the complete ledger program and package/dependency fixtures pass from a clean checkout;
- known compiler panics, locationless user errors, and nondeterministic diagnostics have tracked
  zero-count gates;
- adding a new language feature requires passing the change gate in `core-scope.md`.

## Milestone LIB1: Practical allocation and collection APIs

Priority: after M0-RC

Purpose: make ordinary programs useful without expanding the language for library conveniences.

Scope:

- keep allocator ABI and raw helpers below the public container API, while retaining `Box` and
  `Vec` implementations in the allocation-capable layer for freestanding use;
- expose owning containers through stable `std.boxed` and `std.vec` type APIs without duplicating
  every inherent operation as a public prefixed free function;
- make raw ownership conversion explicit and unsafe instead of exposing ordinary mutable-pointer
  accessors from `Box`;
- slices and trait-based indexing;
- standard iteration for arrays, slices, `Box`, and `Vec` where semantically applicable;
- complete `Vec` ownership, borrowing, growth, and iterator cleanup behavior;
- the [accepted minimum owning-string model](string-design.md): private `Vec(u8)` storage with an
  always-valid UTF-8 invariant, before adding literal or character syntax;
- small host-independent utilities that can be expressed over the existing runtime ABI.

Exit conditions:

- collection APIs use source-backed traits rather than compiler name checks;
- mutable and consuming iterators have alias, invalidation, and cleanup tests;
- no container operation requires implicit allocation or hidden effect-row widening;
- owning strings validate byte input, preserve failed input ownership, and expose no safe mutable
  byte view;
- at least one nontrivial library-style example uses collections, errors, traits, and modules.

## Milestone TYPE1: Complete static abstraction where libraries need it

Priority: active after LIB1 exit conditions were met

Purpose: finish the higher-kinded and callable bridges already justified by standard-library code.

Scope:

- extend bounded compile-time scalar arguments from functions and `Array` to nominal types and type
  aliases when library code demonstrates the required inference and constructor semantics;
- generic associated constructor lowering and constructor equality solving;
- captured callable-to-function bridging for `Chain`, `Coalesce`, and higher-order protocols;
- generic trait methods where coherent static dispatch can be preserved;
- clearer inference boundaries and diagnostics for constructor kinds and effect-row parameters.

Exit conditions:

- `Functor`, `Applicative`, `Monad`, `Chain`, and `Coalesce` need no compiler-only special cases
  beyond validated lang-item identity;
- constructor equations terminate under documented complexity limits;
- ambiguous or underconstrained programs receive source-level diagnostics;
- no runtime dictionary or open-world dispatch is introduced accidentally.

## Milestone ASYNC1: Minimal cancellable Future lowering

Priority: gated exploration

Entry gate:

- EH1 erased continuation/callable ownership is complete;
- M0-RC cleanup and diagnostics gates pass;
- `Future`, `Pin`, cancellation, and executor boundaries have accepted source contracts.

Scope:

- `async { ... }` as a handler that produces a cold anonymous Future;
- state-machine fields for values live across suspension;
- cancellation by ordinary deterministic drop;
- explicit rejection of self-referential borrowed state in the first slice;
- one minimal executor interface in `std.async`, not a language-selected runtime.

Exit conditions:

- poll, ready, pending, cancellation, and drop paths are native-tested;
- nested `async` and `try` preserve handler order;
- direct async recursion requires an explicit indirection such as `BoxFuture`;
- no implicit heap allocation or executor selection occurs.

## Milestone ECO1: Tooling, packages, and compatibility

Priority: after language/runtime boundaries stabilize

Scope:

- formatter and language-server foundations;
- registry dependency model, workspaces, and reproducible package resolution;
- external ABI and package artifact design;
- compatibility policy and deprecation process;
- incremental compilation based on stable semantic inputs.

This milestone must not freeze an ABI or package format while core type, ownership, and callable
representations are still changing.

## Explicitly deferred

The following are not on the active roadmap:

- multi-shot or delimited continuation variants beyond the current one-shot model;
- garbage collection as a second ownership model;
- implicit effect handling or ambient IO authority;
- runtime trait objects or open-world dynamic dispatch without a separate design;
- macros, reflection, compile-time execution, or metaprogramming;
- distributed package infrastructure before local package semantics are stable.

Any proposal to promote a deferred item must identify the milestone it depends on, satisfy the M0
change gate when applicable, and remove or simplify an existing limitation rather than merely add
surface area.
