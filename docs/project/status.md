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
- compile-time `test("name") { ... }` registrations collected into one native
  runner by `salic test`, with source-order execution and named failures;
- source-declared pass-fixture tests batched into native runners by semantic
  group, while process-terminating fixtures remain isolated;
- project manifests, local path dependencies, and deterministic lockfiles.

The command-line surface is:

```text
salic check SOURCE
salic emit-ir SOURCE -o OUTPUT
salic build SOURCE -o OUTPUT
salic run SOURCE -- ARGUMENTS
salic test SOURCE
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
- ordinary closed enums usable as compile-time value types;
- explicit core-private `builtin()` initializers for compiler-owned
  functions, types, type constructors, and extension methods.

An abstract domain is distinct from a defined empty domain. Bare `let Name = domain` and the former
top-level `= type` forms are rejected. Primitive integer types use declarations such as
`pub let i32: type = builtin()`; `type` is an abstract domain, not a
type-construction expression. The marker is unavailable to user packages and
is distinct from bodyless abstract interfaces.

## Types and Static Abstraction

Implemented type-system features include:

- unit and uninhabited enum types;
- all fixed-width signed and unsigned integers plus pointer-width `isize` and `usize`;
- tuples, arrays, borrows, raw pointers, function types, structs, and enums;
- transparent type aliases and partially applied type constructors;
- compile-time `type`, `usize`, `region`, `effect`, `access`, closed-value, constructor, and
  parameter-schema arguments;
- source-level compile-time diagnostics that identify binder, kind, owner, and parameter group;
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

The current static-abstraction surface is complete for the standard-library requirements tracked by
the project roadmap.

## Ownership and Borrowing

The semantic analyzer implements:

- explicit `copy`, `move`, shared borrow, and mutable borrow parameter modes;
- type-directed default copy or move behavior;
- source-backed structural `Move`, with `Copy` inheriting relocation capability;
- relocation checks at owned place reads while preserving direct in-place initialization;
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

- parenthesis-free application for one-parameter runtime groups, including
  curried groups, methods, and trailing closures, with application binding
  above infix operators and logical newlines ending the call;
- nominal structs and closed enums;
- target-layout `struct(c)` data with recursive field validation for integers,
  raw pointers, non-zero fixed arrays, nested C structs, and concrete generic
  instances;
- per-declaration `foreign(c)` and `foreign(c, "symbol")` definitions with
  default linker names, bounded scalar/raw-pointer C signatures, and implicit
  `Unsafe` call requirements;
- tuple, struct, enum, literal, binding, and wildcard patterns;
- exhaustive `match` with guards;
- `if`, `loop`, `while`, post-test loops, and `for`;
- `break`, `continue`, and `return`;
- lexical `defer` with LIFO execution on normal, loop, return, and error exits;
- cold compiler-generated futures with a typed pure `Future` implementation, one-shot
  `Poll.Ready` transition, inferred residual `Unsafe`, state-aware capture transfer, cancellation
  cleanup, completed-state repoll traps, and one tail-position child suspension;
- the explicit allocation-free `core.async.Spin` executor for one owned future;
- handler specialization for non-suspending futures with a custom residual
  effect, including standard `Throws(Error)`, and by-value `Copy`, move-only,
  shared-borrow, or mutable-borrow captures, including exact once-only
  move/drop behavior, retained borrow exclusion, `Future(E)` where-predicate
  inference, and effectful trait-method inlining;
- handler specialization for a suspended await with a finite sequence of pure
  linear continuation segments and a residual effect in the first segment,
  including standard `Throws(Error)`, by-value `Copy`, move-only,
  shared-borrow, and mutable-borrow captures and retained locals, Pending
  repoll without replaying earlier transitions, and exact completion, error,
  and cancellation cleanup;
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

Cold `async` blocks without suspension materialize compiler-generated nominal state containing an
explicit state word and their captured fields. The generated state satisfies structural `Move`;
relocating or cancelling an unpolled future transfers or drops owned captures exactly once.
The no-suspension polling transition returns `Poll.Ready` once, traps on repoll, and enforces an
inferred residual `Unsafe` requirement. Standard residual `Throws(Error)` polling specializes
through `try` or its underlying handler; success, error, and move-capture cleanup paths run
natively. An await may retain custom residual effects when the cold segment
and its finite linear continuation segments capture by-value `Copy`,
move-only, or region-checked shared or mutable references, retained state
remains structural `Move`, and later child poll rows have no custom effect or
`Throws`. Its handler-specialized
first poll transfers factory captures before evaluating the child factory. A
distinct starting state retains move-only continuation captures if the
factory aborts; factory locals still use ordinary lexical cleanup. Pending
repolls only the active stored child, each Ready transition destroys that
child before constructing the next, and completion, error, or cancellation
cleans each initialized field once. A one-shot `if` or `match` can select
between direct-tail children of one concrete future type when the selected
factory retains the residual row. A direct two-way `if` may select
heterogeneous concrete children: condition and factory helpers retain source
types through handler specialization, while a pure bridge initializes the
private active-variant state. Selection runs once and cancellation drops only
the selected child. General heterogeneous `match` and wrapped branch state
remain diagnostic before LLVM generation.
One tail-position `await` stores its child across Pending,
resumes from Ready, and drops the child exactly once on completion or cancellation. A single
non-tail await may bind the Ready output and run a linear continuation with state-owned captures.
Multiple sequential awaits compose while retaining earlier outputs and dropping only the active
segment on cancellation. Ordinary locals live across a sequential await are stored in generated
state and transferred into the continuation; owned resources are dropped exactly once on Ready or
cancellation. Borrow chains whose referent would be stored in the same future are rejected because
the generated state could not implement `Move`, while region-checked borrows of external storage
remain valid. An `if` or `match` whose every branch is a single tail await can suspend when all
branch futures have the same Output; child types may differ. Selection is evaluated once and a
private active-variant future polls or cancels only the selected child. Branch-local linear
prefixes and continuations retain their own suspension state; a branch without await becomes an
immediate Ready future. A `loop` or `while` proven to exit on its first entered iteration hoists
its suspension into the same state machine; false pre-test conditions complete immediately, and a
pre-test condition may itself suspend. A child Output may differ from the enclosing future Output.
Recurring suspension is classified by loop kind, condition/body location, `continue`, fallthrough,
and value-producing `break`. A `loop` with one await followed by a boolean
`break`/`continue()` decision now uses a private `Continue(next_child) | Break(Output)` step enum.
The break output is inferred from the source expression and may be move-only. Its poll transition
reinitializes one child slot and consumes consecutive immediately-ready iterations in an HIR loop.
Completed children are destroyed before reuse, while cancellation drops only the active suspended
child. An omitted `else` and non-suspending branch bodies execute as fallthrough before creating
the next child. Recurring pre-test and post-test `while` loops invoke a reusable iteration factory:
the pre-test condition can finish without constructing a child, a Pending child does not recheck
the condition, and each completed backedge rechecks it before constructing the next child.
Conditions are currently pure and `while` remains unit-valued. Move-only continuation captures are
now packed into `Continue(Carry)` and restored into their parent fields before the next iteration;
completion and cancellation consume or drop each field once. Move-only values required by the
iteration factory or condition still require a more general carry transform. Residual effects in
later sequential segments, general heterogeneous `match`, wrapped-state branches, and loops are
not implemented.
Iterations with multiple top-level sequential awaits use a private iteration future; its final
`Break(Output)` may depend on any awaited binding, and cancellation follows its nested active-child
chain without retaining completed children. A recurring loop with no break uses the standard
uninhabited `Never` as its output.
For unit-valued general iteration bodies, the compiler rewrites control exits at the current loop
depth into early iteration-future step returns and distributes normal fallthrough across nested
`if` and `match` exits. Nested loops and nested async blocks remain separate control boundaries.
`core.async.Spin` is an ordinary zero-field library value implementing `Executor`; it repeatedly
polls one owned future until `Ready` and introduces no implicit allocation or runtime selection.

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
- batched native execution fixtures that use one generated test runner and
  link per compatible group, with independent processes for expected traps;
- cleanup, alias, escape, and allocation behavior;
- deterministic diagnostics, IR, symbol ordering, and lockfiles;
- classified documentation examples;
- formatting and warning-free Clippy.

The `examples/inventory` package is the current nontrivial library acceptance program. It combines
modules, owning strings, vectors, results, user traits, resource transfer, iteration, and cleanup.

## Known Boundaries

The principal incomplete areas are:

- host-facing `std` APIs;
- complete asynchronous execution;
- stable ABI and package distribution.

These boundaries are intentionally explicit. Passing tests for an implemented subset do not imply
stability or support for adjacent syntax.
