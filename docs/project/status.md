# Implementation Status

Salicin is experimental and has no source, library, or ABI stability guarantee. This document is a
current capability inventory. It does not record release history; see the
[changelog](../../CHANGELOG.md) for that. Planned work belongs in the
[roadmap](roadmap.md) and [TODO](todo.md).

## Compiler Pipeline

`salic` provides:

- lexing, parsing, module resolution, and static semantic analysis;
- ownership, borrow, visibility, effect, and trait checks;
- monomorphization of generic functions, nominals, extensions, and trait implementations;
- deterministic HIR and LLVM IR generation;
- native checking, IR emission, building, and running;
- project manifests, local path dependencies, and deterministic lockfiles.

The command-line surface is:

```text
salic check SOURCE
salic emit-ir SOURCE -o OUTPUT
salic build SOURCE -o OUTPUT
salic run SOURCE -- ARGUMENTS
```

## Source Model

Implemented lexical and declaration features include:

- UTF-8 source and NFC-normalized Unicode XID identifiers;
- logical newlines, semicolons, line comments, and nested block comments;
- uniform `let` declarations and mutable local value bindings;
- private, package, and public visibility;
- contextual control, passing, kind, and borrow words;
- abstract domains written `let Name: domain`;
- defined domains written `let Name = domain { ... }`, including empty domains;
- ordinary closed enums usable as compile-time value types.

An abstract domain is distinct from a defined empty domain. Bare `let Name = domain` and the former
top-level `= type` forms are rejected. Primitive integer types use declarations such as
`pub let i32: type`; `type` is an abstract domain, not a type-construction expression.

## Types and Static Abstraction

Implemented type-system features include:

- unit and uninhabited enum types;
- all fixed-width signed and unsigned integers plus pointer-width `isize` and `usize`;
- tuples, arrays, borrows, raw pointers, function types, structs, and enums;
- transparent type aliases and partially applied type constructors;
- compile-time `type`, `usize`, `region`, `effect`, `access`, closed-value, constructor, and
  parameter-schema arguments;
- curried compile-time and runtime parameter groups;
- labeled arguments, overload selection, and trailing closures;
- generic nominal types, aliases, inherent extensions, and trait implementations;
- associated types and generic associated constructors;
- bounded generic associated-constructor equality predicates;
- static trait and operator dispatch;
- trait inheritance predicates and associated-type equality predicates;
- alpha-equivalent generic trait methods across concrete, blanket, constructor, and default
  implementations;
- static specialization of capturing callables passed to known higher-order callees.

Generic associated constructors preserve parameter kinds and groups in trait declarations and
implementations. Standard iterator contracts use `Item(R: region): type`, allowing an item type to
depend on the receiver-borrow region.

The remaining static-abstraction work is generic custom-effect callable materialization and clearer
kind and constructor diagnostics.

## Ownership and Borrowing

The semantic analyzer implements:

- explicit `copy`, `move`, shared borrow, and mutable borrow parameter modes;
- type-directed default copy or move behavior;
- whole-value and field-sensitive move tracking;
- shared-loan overlap and mutable-loan exclusion;
- reborrowing with region shortening;
- escape checks for local and temporary references;
- mutation and move invalidation checks;
- deterministic, exactly-once cleanup for initialized resources;
- cleanup across returns, loop exits, handled effects, partial calls, and partial aggregate
  construction.

The implementation rejects overlapping mutable iterator yields and references that outlive their
source. Mutable iteration can yield access-preserving element borrows without moving elements from
their container.

## Data and Control

Implemented data and control features include:

- nominal structs and closed enums;
- tuple, struct, enum, literal, binding, and wildcard patterns;
- exhaustive `match` with guards;
- `if`, `loop`, `while`, post-test loops, and `for`;
- `break`, `continue`, and `return`;
- checked arithmetic, comparisons, bitwise operations, shifts, and compound assignment;
- deterministic left-to-right evaluation;
- optional chaining, coalescing, error propagation, and forced unwrap.

Control forms are validated against source declarations in `core` where compiler authority is not
intrinsically required. User declarations with matching names cannot impersonate a lang item.

## Effects

Implemented algebraic-effect support includes:

- source-declared effects and operations;
- effect rows and compile-time effect parameters;
- resumable and abortive handlers;
- single-use continuations;
- cleanup on resumption and abandonment;
- captured effectful closures;
- capturing callable arguments specialized after generic custom-effect rows become concrete;
- source-backed `Throws(Error)`, `throw`, and `try`;
- composition of standard error and unsafe effects.

`Unsafe` is an authority effect used by raw memory and foreign operations. It does not disable
typing, ownership, or cleanup checks.

Complete `Future` contracts, async state-machine lowering, polling, cancellation, and
self-reference rules are not implemented.

## Modules, Packages, and FFI

Implemented package features include:

- file and directory modules;
- `self`, `super`, `root`, package, and dependency paths;
- entity aliases and explicit re-exports;
- `salicin.toml` projects with library and binary roots;
- local path dependencies and `salicin.lock`;
- package ownership and trait coherence boundaries.

The C import boundary supports validated ASCII link names and the documented integer and raw-pointer
subset. Foreign calls require `unsafe`. Stable exported aggregates, a frozen Salicin ABI, registry
dependencies, workspaces, and a distribution format are not defined.

## Standard Library

The source library is split into:

- `core`: allocation-free language contracts and primitives;
- `alloc`: owning heap-backed containers;
- `std`: target and host facilities, not yet populated.

Implemented `core` facilities include:

- primitive declarations and compile-time domains;
- `borrow`, `Ptr`, `Array`, `Slice`, `size_of`, and `align_of`;
- ownership markers and operator traits;
- `Option`, `Result`, iteration, indexing, and flow protocols;
- functional constructor traits;
- effects, handlers, and control contracts.

Implemented `alloc` facilities include:

- `Box(T)`;
- `Vec(T)` with mutation and consuming iteration;
- owning UTF-8 `String`;
- recoverable UTF-8 validation errors.

Safe `String` APIs preserve valid UTF-8 and do not expose mutable bytes. Text slicing, character
iteration, Unicode algorithms, and host I/O are not yet library features.

Borrowed `SliceIter(A)(T)` preserves shared or mutable source access and yields
`borrow(A)(R)(T)`. `Vec` iteration consumes elements and drops an unyielded suffix exactly once on
early exit.

## Quality Gates

Repository gates cover:

- parser and semantic unit tests;
- positive and negative CLI fixtures;
- native execution and exit-status checks;
- cleanup, alias, escape, and allocation behavior;
- deterministic diagnostics, IR, symbol ordering, and lockfiles;
- classified documentation examples;
- formatting and warning-free Clippy.

The `examples/inventory` package is the current nontrivial library acceptance program. It combines
modules, owning strings, vectors, results, user traits, resource transfer, iteration, and cleanup.

## Known Boundaries

The principal incomplete areas are:

- concise diagnostics for underconstrained constructor and effect inference;
- host-facing `std` APIs;
- complete asynchronous execution;
- stable ABI and package distribution.

These boundaries are intentionally explicit. Passing tests for an implemented subset do not imply
stability or support for adjacent syntax.
