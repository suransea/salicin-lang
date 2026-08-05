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

## Now: Compile-Time Metaprogramming Foundations

With the daily-development milestones complete, Salicin may add more static
sorts for typed reflection and metaprogramming. META-1 must first define scope safety,
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
