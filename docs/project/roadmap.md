# Project Roadmap

Status: active project direction

This roadmap defines outcomes, order, and entry and exit gates. It does not
define language behavior; the [language specification](../language/specification.md)
and [grammar](../language/grammar.md) do that. Current implementation facts
belong in [status](status.md), executable work in [TODO](todo.md), and completed
work in the [changelog](../../CHANGELOG.md).

## Product Direction

Salicin's next phase turns the implemented language core into a compiler that
can support daily development. The near-term order is deliberately:

1. make compile-time evaluation understand ordinary scalar and composite
   values;
2. make ordinary programs practical with text, collections, host I/O, and test
   support;
3. make unchanged builds reusable;
4. make source analysis continuously available to editors;
5. make parser-owned declaration forms obey source-visible static contracts;
6. build source navigation on structured semantic identities;
7. make locked third-party source dependencies reproducible.

New language surface is not a near-term goal unless one of those outcomes
requires it. Every milestone must continue to preserve:

- deterministic left-to-right evaluation and exactly-once cleanup;
- source-level diagnostics without generated implementation names;
- static dispatch and bounded monomorphization;
- explicit authority for unsafe operations and effects;
- ordinary library declarations wherever compiler primitives are unnecessary;
- reproducible behavior independent of checkout path and traversal order.

## Planning Model

The roadmap uses ordered milestones rather than release dates. One milestone
is active at a time:

- **Now** is implementation-ready and owns the P0 queue.
- **Next** has an accepted outcome but starts only after the current exit gate.
- **Later** is ordered and accepted at milestone granularity; individual tasks
  may still need a design contract before implementation.
- **Design candidates** are real gaps but are not promises or active work.

Completed milestones are removed from this file. Their behavior is recorded
in [status](status.md), their contracts remain under `docs/project`, and their
history remains in the changelog.

## Now: Syntax Declaration Contracts

`test("name") { ... }`, `extend(...) { ... }`, and
`requires(goals) expression` now use the minimal source-backed model. `test`
passes its static string name and a unit-returning `throwing(string)` closure.
Function-body `requires` passes a compile-time `bool` and a delayed closure.
Trait and extension requirements are labeled compile-time boolean header
parameters; `extend` itself has no callable contract.

The erased `constraint` classifier remains the solver boundary. The premature
`declaration` classifier has been removed: the current fixed syntax does not
need first-class declaration fragments. Active work is the acceptance pass
across formatting, diagnostics, fixtures, and documentation.

Exit conditions:

- constraint fragments have explicit normalization and phase separation;
- the `test` contract includes its static string name and accepts only a
  unit-returning closure with exactly `throwing(string)`;
- function-body `requires` uses its validated boolean/closure contract, while
  trait and extension requirements remain boolean header parameters;
- `extend` has no decorative builtin signature;
- missing or malformed edition contracts fail core-bundle validation;
- grammar, formatter preservation, diagnostics, core sources, fixtures, and
  documentation agree on one contract shape.

Additional reflection and syntax-fragment sorts are ordered later under
META-1 rather than being inferred from these fixed forms.

## Later: Semantic Navigation

Navigation follows the LSP baseline because it needs long-lived analysis
snapshots and stable source identities. The compiler will expose a semantic
occurrence index that distinguishes declarations, references, overloads,
aliases, fields, variants, traits, implementations, and generated
specializations without exposing compiler-generated names.

Exit conditions:

- source declarations and uses have stable identities within one snapshot;
- go-to-definition, references, and hover work across modules and packages;
- rename produces a complete, non-overlapping workspace edit or refuses the
  operation when identity or visibility is ambiguous;
- aliases, shadowing, overloads, Unicode identifiers, and dependency-owned
  read-only source have explicit tests and rejection behavior.

Completion remains a separate follow-up because candidate ranking and partial
syntax recovery require their own contract.

## Later: Registry Source Dependencies

The implemented resolver already fixes package provider identity, lockfile
semantics, and workspace/path resolution. The
[dependency resolution contract](dependency-resolution.md) also defines the
registry selection algorithm. This milestone implements a registry client
against immutable index snapshots and verified source archives; it does not
create or standardize a public registry service.

Exit conditions:

- manifests can declare registry source dependencies without weakening
  workspace/path identity rules;
- resolution selects the highest compatible non-yanked version from one
  identified snapshot and records exact provider and checksum data;
- archives are verified before manifest or source consumption and extracted
  without path traversal or partial-cache visibility;
- `--locked` cannot change the selected graph, and `--frozen` succeeds only
  from verified local index and archive data;
- local fixture registries cover version conflicts, yanking, checksum
  mismatch, cache corruption, offline operation, and dependency cycles.

Publishing, credentials, mirrors, a hosted service, precompiled interfaces,
and a stable package protocol remain outside this milestone.

## Later: Compile-Time Metaprogramming Foundations

After the daily-development milestones, Salicin may add more static sorts for
typed reflection and metaprogramming. META-1 must first define scope safety,
phase separation, equality and normalization, permitted fragment producers,
resource limits, and source diagnostics. No unused `declaration` sort is
reserved in advance.

## Design Candidates

These gaps need an accepted contract and sequencing decision before entering
the executable queue:

- per-package incremental reuse based on dependency interface digests;
- compile-time mutation, loop normalization, allocation, and resource values;
- runtime nominal types as compile-parameter classifiers;
- networking, asynchronous I/O, time, subprocess, and platform-service APIs;
- Unicode normalization, grapheme segmentation, locale-sensitive text, and
  regular expressions;
- hash maps, hash sets, and a stable hashing contract;
- completion and partial-program analysis;
- the currently diagnosed async shapes: recursive erasure, effectful loop
  conditions, nested residual iteration, and move-only backedge factories;
- a wake-aware executor and host async runtime;
- non-host targets and a target-aware ABI;
- broader by-value C interoperability;
- precompiled package interfaces and distribution artifacts;
- analyzer decomposition along the existing semantic phase boundaries.
- typed compile-time reflection and syntax/declaration fragment classifiers.

Refactoring may accompany an active milestone when it creates a narrow
boundary required by that milestone. A repository-wide rewrite is not itself
a roadmap outcome.

## Deferred

The following remain intentionally outside the accepted roadmap:

- multi-shot continuations;
- implicit ambient I/O or allocation authority;
- garbage collection as a second ownership model;
- runtime trait objects and open-world dispatch;
- macros and general compile-time execution before META-1;
- a public package registry service;
- a stable ABI or 1.0 compatibility promise.

## Change Gate

A milestone or language change enters the executable queue only when it:

1. names a user-visible or compiler-operational outcome;
2. identifies affected ownership, effects, evaluation, cleanup, source, and
   package boundaries;
3. defines failure behavior and source-level diagnostics;
4. has positive, negative, restart or corruption, cross-module, and native
   evidence where relevant;
5. states what is deliberately not included;
6. leaves formatting, Clippy, tests, and documentation clean.
