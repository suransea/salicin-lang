# Language Roadmap

Status: living project direction

This roadmap records sequencing and exit conditions. It does not define language behavior; the
[language specification](../language/specification.md) and [grammar](../language/grammar.md) do
that. Current implementation facts belong in [status](status.md), actionable work in
[TODO](todo.md), and completed work in the [changelog](../../CHANGELOG.md).

## Direction

Salicin is moving toward a coherent native language built around deterministic ownership, explicit
effects, static abstraction, and source-backed library contracts. Work should complete one
end-to-end capability at a time and preserve:

- deterministic left-to-right evaluation and exactly-once cleanup;
- source-level diagnostics without generated names;
- static dispatch and bounded monomorphization;
- explicit authority for unsafe operations and effects;
- ordinary library declarations wherever compiler primitives are unnecessary.

## Current Milestone: Async Foundations

Async work starts only after callable and continuation ownership is closed.

Required design:

- source contracts for `Move`, `Future`, polling, cancellation, and executor interaction;
- cold futures lowered to explicit state machines;
- deterministic drop of initialized state on cancellation;
- a first-version rejection rule for self-referential borrowed state;
- one explicit executor interface, with no implicit allocation or runtime selection.

Exit conditions:

- ready, pending, cancellation, and drop paths run natively;
- nested async and error handlers preserve effect order;
- recursive async calls require explicit indirection;
- suspension cannot duplicate continuations or owned state.

## Ecosystem Milestone

Tooling and package compatibility follow stabilization of source semantics and runtime
representations:

- a formatter based on parser-preserving syntax invariants;
- an LSP over stable parser and semantic spans;
- workspaces and reproducible dependency resolution;
- an external ABI designed after representation review;
- incremental compilation keyed by stable semantic inputs.

No milestone may freeze a public ABI, package registry protocol, or compatibility promise while the
corresponding language representation is still changing.

## Deferred

The following require separate accepted designs and are not active work:

- multi-shot continuations;
- implicit ambient IO or allocation authority;
- garbage collection as a second ownership model;
- runtime trait objects and open-world dispatch;
- macros, reflection, and general compile-time execution;
- a public package registry;
- a stable 1.0 compatibility promise.

## Change Gate

A language change enters the roadmap only when it:

1. solves a concrete language or standard-library requirement;
2. states its interaction with ownership, effects, evaluation order, and cleanup;
3. defines rejection boundaries and source-level diagnostics;
4. has positive, negative, cross-module when relevant, and native evidence;
5. leaves the full formatting, lint, test, and documentation gates clean.
