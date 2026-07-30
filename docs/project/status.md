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
  runner by `salic test`, with source-order execution, source-backed
  unit-returning `throwing(string)` bodies, owned UTF-8 failure messages,
  all-failure reporting,
  a dedicated framed parent channel, and `std.test` failure, boolean,
  equality, inequality, and `option`/`result` expectation helpers with static
  comparison/diagnostic-formatting bounds and single operand evaluation;
  source-order `--list`, case-sensitive UTF-8 `--filter`, package-wide
  duplicate-name diagnostics, one-runner selected execution, and stable
  selected/passed/failed summaries;
- source-declared pass-fixture tests batched into native runners by semantic
  group, while process-terminating fixtures remain isolated;
- package and virtual workspace manifests, explicit workspace members,
  `--package` selection, local path dependencies, shared build roots, and
  deterministic source-aware lockfiles;
- token and diagnostic editor analysis with UTF-8 byte ranges, zero-based
  UTF-16 positions, phased precision metadata, and multi-document routing;
- versioned SHA-256 incremental input fingerprints over compiler, target,
  standard-library, provider-graph, module, and source-byte inputs.

The compiler does not yet provide an LSP transport, incremental document
updates, completion, hover, or rename.
It also does not persist incremental compilation artifacts; the stable
fingerprint defines cache inputs without freezing a cache format.

The initial [standard-library usability surface](standard-library-surface.md)
is accepted. It fixes the `core`/`alloc`/host-`std` layers, all-`snake_case`
public naming, prelude exclusions, ownership modes, failure policy, error
families, explicit `io` authority, initial native target matrix, and minimum
text, collection, conversion, I/O, and test APIs. Its source-backed canonical
layers, target boundary, and console/process/filesystem APIs are implemented.
The accepted [synchronous host I/O contract](host-io.md) now fixes authority,
entry handling, errors, byte/text boundaries, partial progress, interruption,
resource cleanup, close behavior, and the two supported native targets.
Native `std.io` now provides single-attempt and exact/all stdin/stdout/stderr
operations, byte-exact text helpers, line input with checked UTF-8, explicit
flush points, and lossless-byte plus checked-text process arguments. The
launcher captures `argc`/`argv`, ignores `SIGPIPE`, and maps native failures
to portable `io_error_kind` values without exposing raw FFI authority.
Filesystem support provides validated open/create options, unique
deterministically dropped file owners, consuming close, short and exact/all
reads and writes, `fsync`, three-origin seek, and bounded whole-file helpers.
Paths are existing UTF-8 `str` views passed byte-exactly after embedded-NUL
validation.

The command-line surface is:

```text
salic check SOURCE
salic emit-ir SOURCE -o OUTPUT
salic fingerprint SOURCE
salic build SOURCE -o OUTPUT
salic run SOURCE -- ARGUMENTS
salic test SOURCE [--list] [--filter TEXT]
```

## Source Model

Implemented lexical and declaration features include:

- UTF-8 source and NFC-normalized Unicode XID identifiers;
- logical newlines, semicolons, line comments, and nested block comments;
- uniform `let` declarations and mutable local value bindings;
- prefix effect callable types `with(E)(F)` and effectful declaration
  boundaries, with compact boundary-free syntax retained for pure functions;
- the compiler-validated `std.io.io` authority identity, accepted only at the
  native `main` boundary, plus source-defined `io_error_kind` and `io_error`;
- private, package, and public visibility;
- contextual control, passing, sort, and borrow words;
- compiler-owned abstract sorts written `let name: sort(2)`;
- defined sorts written `let name = sort(1) { ... }`, including empty sorts;
- ordinary closed enums usable as compile-time value types;
- explicit core-private `builtin()` initializers for compiler-owned
  functions, types, type constructors, and extension methods.
- canonical private `builtin`, `foreign`, and `test` syntax declarations plus
  identity-validated passing and control-exit contracts;
- explicit erased inputs for those syntax declarations:
  the one- and two-argument `foreign` overloads select the finite
  `abi.c` value, while
  `pub let test(move body: with(core.error.throwing(core.string.string))((): ())): () = builtin()`
  receives the syntax-owned body after the compiler consumes UTF-8 name
  metadata.

Types, traits, functions, values, modules, parameters, and ordinary sorts use
`snake_case`.

An abstract sort is distinct from a defined empty sort. Bare `let name = sort` and the former
top-level `= type` forms are rejected. Primitive integer types use declarations such as
`pub let i32: type = builtin()`; `type` is an abstract sort, not a
type-construction expression. The marker is unavailable to user packages and
is distinct from bodyless abstract interfaces.

## Types and Static Abstraction

Implemented type-system features include:

- unit and uninhabited enum types;
- all fixed-width signed and unsigned integers plus pointer-width `isize` and `usize`;
- tuples, arrays, borrows, raw pointers, function types, structs, and enums;
- transparent type aliases and partially applied type constructors;
- compile-time `type`, `usize`, `string`, `region`, `effect`, `effects`, `access`,
  closed-value, constructor, and
  parameter-schema arguments;
- source-level compile-time diagnostics that identify binder, sort, owner, and parameter group;
- curried compile-time and runtime parameter groups;
- labeled arguments, overload selection, and trailing closures;
- generic nominal types, aliases, inherent extensions, and trait implementations;
- call-shaped `extend(target) { ... }` and `extend(target, trait) { ... }`
  declarations whose generic binders and sorts are inferred by destructuring
  the target type constructor, including after cross-module resolution;
- associated types and generic associated constructors;
- bounded generic associated-constructor equality predicates;
- static trait and operator dispatch;
- trait inheritance predicates and associated-type equality predicates;
- alpha-equivalent generic trait methods across concrete, blanket, constructor, and default
  implementations;
- static specialization of capturing callables passed to known higher-order callees.

Generic associated constructors preserve parameter sorts and groups in trait declarations and
implementations. Standard iterator contracts use `item(comptime r: region): type`, allowing an item type to
depend on the receiver-borrow region.

Ordinary pure scalar functions can be evaluated in dependent array-length
expressions. Unit `()`, `bool`, every fixed-width signed and unsigned integer, and
`isize`/`usize` are supported as parameters, immutable locals, operator
operands, literal or irrefutable patterns, and results. Integer literals are
converted fallibly to their expected exact type; arithmetic, division,
remainder, bitwise operations, comparisons, negation, and shifts are checked
at that width. The static expression IR excludes runtime-only operations,
substitutes generic `usize` values before evaluation, and diagnoses
nontermination through a bounded evaluator.

Every primitive integer has source-declared `min`, `max`, `clamp`, and `sign`
methods. Signed integers expose a total same-width unsigned `magnitude`, so
the signed minimum is representable. Explicit
`checked_into(output: target)()` conversion accepts only integer targets and
returns `option(target)` without truncating on failure. CTFE and LLVM lowering
share the same signed and width boundaries; LLVM uses defined comparisons,
extensions, and truncations without overflow flags or backend undefined
behavior.

Dependent-expression evaluation and global constant normalization now share
one recursive typed CTFE value. It retains the exact integer type,
distinguishes tuples from arrays, records canonical struct and enum identity,
and stores an enum's source variant plus active payload rather than backend
padding or discriminants. Erased sort metadata remains in `StaticValue`.

The current backend has an explicit native target description with a 64-bit
pointer width. CTFE, literal validation, runtime guards, LLVM scalar types,
and constant encoding use that description rather than Rust's host
`usize`. Global constant normalization and dependent-expression evaluation
share exact signed/unsigned integer operations, including the full `u128`
domain and signed minima.

The type-level evaluator also admits recursively typed tuples and fixed
arrays as function parameters, immutable locals, and results. It evaluates
tuple/array literals, decimal tuple projection, `usize` array indexing,
nested tuple patterns and bindings, and bounds failures. Aggregate types are
rejected before construction beyond 64 nesting levels, 65,536 elements in one
aggregate, or 65,536 normalized nodes. Fixed-array bracket syntax now uses
`usize` at runtime as well as during CTFE.

Concrete generic and non-generic structs are also CTFE values. Construction
evaluates labeled fields once in source order and normalizes them in
declaration order. Nested structs, immutable inferred or annotated locals,
function parameters and results, field projection, structural equality, and
nested labeled destructuring patterns retain canonical nominal identity.
Struct patterns now travel through ordinary runtime `match` lowering and LLVM
emission as well as through CTFE. Before construction, recursive type
validation rejects slices and other unsized storage, pointers and borrows,
allocation-backed or custom-`droppable` fields, callable/address-dependent
values, recursive nominal layouts, and the existing aggregate budgets.

Closed enums are intermediate dependent-expression values too. Unit,
positional, and named variants preserve canonical enum identity, source
variant position, and declaration-order payload fields rather than inspecting
an LLVM tag or padding. Generic enum instances are materialized on first CTFE
use even when they occur only in a function-body local. Exhaustive matches
support payload binding, nested tuple/struct/enum patterns, literal tests,
guards, unit short patterns, and structural equality. Standard `option` and
`result` construction and matching execute through the same path. Resource
exclusion recursively checks every possible variant before constructing even
a resource-free unit variant.

Dependent expressions admit fully applied ordinary pure source functions with
multiple runtime groups and labeled arguments. Explicit or inferred generic
arguments select concrete source instances before interpretation. Direct
dependent syntax and calls nested in interpreted bodies share this path.
Statically resolved inherent or unique trait methods and associated functions,
overload selection by runtime labels, and canonical cross-module declarations
are supported. Immutable blocks, `if`, exhaustive `match`, guards, and
function `return` preserve selected-path evaluation. Repeated equal calls fail
as cycles; value-changing recursion may proceed within the fixed 16,384-step
and 128-active-call limits.

Effectful calls, borrow parameters or expressions, closures, mutation,
foreign or builtin bodies without a specified CTFE rule, and unavailable
source remain outside CTFE. Dependent array lengths and global constants use
the same source evaluator and typed normalized values. Globals can call the
same eligible ordinary functions, including generic and cross-module
instances; generic `size_of` and `align_of` retain the concrete queried type
for target-layout encoding.

The accepted [composite CTFE contract](composite-ctfe.md) fixes the typed
value domain, phase and function-eligibility rules, strict evaluation order,
resource exclusion, structural normalization, deterministic complexity
budgets, diagnostics, and the boundary from erased `StaticValue` metadata.
Implementation now has the unified typed value IR, evaluator, and constant
consumer boundary. The remaining milestone work is its complete acceptance
matrix.

## Ownership and Borrowing

The semantic analyzer implements:

- explicit `copy`, `move`, shared borrow, and mutable borrow parameter modes;
- type-directed default copy or move behavior;
- source-backed structural `movable`, with `copyable` inheriting relocation capability;
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
  `unsafety` call requirements;
- tuple, struct, enum, literal, binding, and wildcard patterns;
- exhaustive `match` with guards;
- `if`, `loop`, `while`, post-test loops, and `for`;
- `break`, `continue`, and `return`;
- lexical `defer` with LIFO execution on normal, loop, return, and error exits;
- cold compiler-generated futures with a typed pure `future` implementation, one-shot
  `poll.ready` transition, inferred residual `unsafety`, state-aware capture transfer, cancellation
  cleanup, completed-state repoll traps, and one tail-position child suspension;
- a direct intrinsic `core.async.async` entry point for anonymous future state
  and a source-defined `core.async.await` over the ordinary
  `poll`/`suspension.suspend` protocol;
- the explicit allocation-free `std.async.spin` executor for one owned future;
- handler specialization for non-suspending futures with a custom residual
  effect, including standard `throwing(error)`, and by-value `copyable`, move-only,
  shared-borrow, or mutable-borrow captures, including exact once-only
  move/drop behavior, retained borrow exclusion, `future(e)` where-predicate
  inference, and effectful trait-method inlining;
- handler specialization for a suspended await with a finite sequence of pure
  linear continuation segments and a residual effect in the first segment,
  including standard `throwing(error)`, by-value `copyable`, move-only,
  shared-borrow, and mutable-borrow captures and retained locals, pending
  repoll without replaying earlier transitions, and exact completion, error,
  and cancellation cleanup;
- handler specialization for a final non-suspending continuation that retains
  a custom effect or `throwing` after a pure child becomes ready, including
  atomic transfer of move-only retained state, no execution on pending or
  cancellation, and exact cleanup on resume, error, or abandonment;
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
- source-backed `throwing(error)`, `throw`, and `try`;
- composition of standard error and unsafe effects.

`unsafety` is an authority effect used by raw memory and foreign operations. It does not disable
typing, ownership, or cleanup checks.

Cold `async` blocks without suspension materialize compiler-generated nominal state containing an
explicit state word and their captured fields. The generated state satisfies structural `movable`;
relocating or cancelling an unpolled future transfers or drops owned captures exactly once.
The no-suspension polling transition returns `poll.ready` once, traps on repoll, and enforces an
inferred residual `unsafety` requirement. Standard residual `throwing(error)` polling specializes
through `try` or its underlying handler; success, error, and move-capture cleanup paths run
natively. An await may retain custom residual effects when the cold segment
and its finite linear continuation segments capture by-value `copyable`,
move-only, or region-checked shared or mutable references, retained state
remains structural `movable`, and later child poll rows have no custom effect or
`throwing`. Its handler-specialized
first poll transfers factory captures before evaluating the child factory. A
distinct starting state retains move-only continuation captures if the
factory aborts; factory locals still use ordinary lexical cleanup. pending
repolls only the active stored child, each ready transition destroys that
child before constructing the next, and completion, error, or cancellation
cleans each initialized field once. A one-shot `if` or `match` can select
between direct-tail children of one concrete future type when the selected
factory retains the residual row. Direct `if` and `match` selection may use
heterogeneous concrete children: the complete source selection retains
pattern payload scope and move-only selector ownership through handler
specialization, while pure bridges initialize the private active-variant
state. Those bridges also assemble retained continuation locals with the
selected child before the existing atomic start transition. Selection runs
once; ready and cancellation drop only the selected child and each
initialized retained value once.
When a pure child becomes ready, a final continuation may itself retain a
custom effect or `throwing` if it does not suspend again. A pure transition
destroys the child and packages its output with continuation captures and
retained locals; the source poll wrapper consumes that package under the
handler. This prevents replay on pending and preserves exactly-once cleanup
when the continuation resumes, failure, or is abandoned.
One tail-position `await` stores its child across pending,
resumes from ready, and drops the child exactly once on completion or cancellation. A single
non-tail await may bind the ready output and run a linear continuation with state-owned captures.
Multiple sequential awaits compose while retaining earlier outputs and dropping only the active
segment on cancellation. Ordinary locals live across a sequential await are stored in generated
state and transferred into the continuation; owned resources are dropped exactly once on ready or
cancellation. Borrow chains whose referent would be stored in the same future are rejected because
the generated state could not implement `movable`, while region-checked borrows of external storage
remain valid. An `if` or `match` whose every branch is a single tail await can suspend when all
branch futures have the same output; child types may differ. Selection is evaluated once and a
private active-variant future polls or cancels only the selected child. Branch-local linear
prefixes and continuations retain their own suspension state; a branch without await becomes an
immediate ready future. A `loop` or `while` proven to exit on its first entered iteration hoists
its suspension into the same state machine; false pre-test conditions complete immediately, and a
pre-test condition may itself suspend. A child output may differ from the enclosing future output.
Recurring suspension is classified by loop kind, condition/body location, `continue`, fallthrough,
and value-producing `break`. A `loop` with one await followed by a boolean
`break`/`continue()` decision now uses a private `iteration_skip(next_child) | loop_exit(output)` step enum.
The break output is inferred from the source expression and may be move-only. Its poll transition
reinitializes one child slot and consumes consecutive immediately-ready iterations in an HIR loop.
Completed children are destroyed before reuse, while cancellation drops only the active suspended
child. An omitted `else` and non-suspending branch bodies execute as fallthrough before creating
the next child. Recurring pre-test and post-test `while` loops invoke a reusable iteration factory:
the pre-test condition can finish without constructing a child, a pending child does not recheck
the condition, and each completed backedge rechecks it before constructing the next child.
Conditions are currently pure and `while` remains unit-valued. Move-only continuation captures are
now packed into `iteration_skip(carry)` and restored into their parent fields before the next iteration;
completion and cancellation consume or drop each field once. Move-only values required by the
iteration factory or condition still require a more general carry transform.
Later sequential segments may construct and poll residual custom-effect or
`throwing` children. Ownership moves through the active segment only; pending,
ready, cancellation, error, and handler abandonment do not replay earlier
segments and clean each initialized child once. Recurring `loop`, pre-test
`while`, and post-test `while` support one residual child factory per
iteration. A false pre-test condition constructs no child; a completed
backedge yields before the next effectful factory is invoked. Multiple
sequential awaits whose generated iteration future itself has a residual
`poll`, effectful recurring conditions, and move-only factory or condition
backedge state remain explicit diagnostics.
Iterations with multiple top-level sequential awaits use a private iteration future; its final
`loop_exit(output)` may depend on any awaited binding, and cancellation follows its nested active-child
chain without retaining completed children. A recurring loop with no break uses the standard
uninhabited `never` as its output.
For unit-valued general iteration bodies, the compiler rewrites control exits at the current loop
depth into early iteration-future step returns and distributes normal fallthrough across nested
`if` and `match` exits. Nested loops and nested async blocks remain separate control boundaries.
`std.async.spin` is an ordinary zero-field library value implementing `executor`; it repeatedly
polls one owned future until `ready` and introduces no implicit allocation or runtime selection.

## Modules, Packages, and FFI

Implemented package features include:

- file and directory modules;
- `self`, `super`, `root`, package, and dependency paths;
- entity aliases and explicit re-exports;
- `salicin.toml` projects with library and binary roots;
- rooted and virtual workspaces with explicit non-nested members;
- local path dependencies and workspace-root `salicin.lock` format 2;
- resolved provider identities separating workspace, path, registry, and
  compiler-owned sources from package name and exact version;
- strict typed lockfile parsing plus `--locked` and `--frozen` graph
  validation for workspace and path dependencies;
- package ownership and trait coherence boundaries.

The verified [C interoperability boundary](c-interoperability.md) supports
validated ASCII link names, every Salicin integer width, raw pointers, and
Unit results. Foreign calls require `unsafe`. `struct(c)` layout is verified
against host Clang through nested, array, integer, and pointer fields; C reads
and writes those records behind raw pointers. By-value aggregates, arrays,
bool, borrows, and typed function pointers remain rejected. A frozen Salicin
Registry transport, a frozen ABI, and a precompiled distribution format are
not implemented.

The experimental native [ABI representation audit](abi-review.md) specifies
the current 64-bit host-target mapping for every emitted first-class value.
Unit parameters are erased, borrows are pointers, owned values and aggregates
pass directly, effect rows are specialized out of direct calls, `throwing` uses
its `result` return boundary, and compiler-owned continuation records contain
entry, drop, environment, and active-flag pointers. Native calling agreement
is implemented.

The experimental [native calling convention](native-calling-convention.md)
defines flattened runtime groups, erased Unit and borrowed-Unit parameters,
direct value or pointer passing, owned argument and return transfer, cleanup
on every exit, static effect authority, algebraic continuation lowering, and
`result`-based `throwing` propagation. Unsized value parameters and returns are
rejected at source declarations.

The experimental [native linkage contract](native-linkage.md) exports concrete
primary-package `pub` functions and non-Unit globals under stable
package-qualified identities with ABI fingerprints. Private, package-visible,
dependency-owned, generic-specialization, and generated definitions remain
internal. Package graph order does not affect nominal identities;
incompatible signatures cannot bind to one symbol; independent LLVM modules
link through the contract. Precompiled package interfaces remain undefined.

## Standard Library

The source library is split into:

- `core`: allocation-free language contracts and primitives;
- `alloc`: owning heap-backed containers;
- `std`: an edition-matched source-owning layer above `core` and `alloc`, with
  host facilities still to be added.

The compiler embeds `library/std`. Ordinary public definitions receive `std`
identities, while both private shortcuts and public re-exports of another
canonical path are rejected as mirrors. Standard-library implementations use
qualified lower-layer paths directly. The bundle participates in incremental
fingerprints and semantic preprocessing. It cannot define language items or
acquire authority by name, and ordinary source cannot reserve the `std`,
`core`, or `alloc` namespaces. Host-library loading is accepted only on
Linux/x86-64 and macOS/arm64; other host pairs receive a target-specific
diagnostic.

The enforced dependency order is `core ← alloc ← std`. Algebraic and
higher-kinded functional protocols plus their `option`/`result`
implementations are owned by `std`; the concrete `spin` executor is likewise
in `std.async`. Freestanding data types, operator/iteration/control
protocols, cold futures, and the executor protocol remain in `core`.

Public embedded-library declarations use strict ASCII `snake_case` and
semantic category vocabulary: value types use entity or state nouns, traits
use capability, role, or operation names, and effects use behavior or
capability nouns. Category suffixes such as `_type`, `_trait`, and `_effect`
are rejected. The standard effect identities are `throwing`, `suspension`,
`unsafety`, `loop_exit`, `iteration_skip`, and `function_exit`; user packages
are not subjected to this library-only naming gate.

Implemented `core` facilities include:

- primitive declarations and compile-time sorts;
- primitive integer bounds, sign and total magnitude helpers, and checked
  conversions in `core.numeric`;
- `borrow`, `ptr`, `array`, `slice`, `size_of`, and `align_of`;
- consistent `array` and `slice` length, emptiness, checked `get`,
  trapping `at`/index, first/last, and access-preserving array-to-slice views.
  Checked misses test bounds before forming a borrow, and mutable views retain
  the original array loan;
- shared left-to-right `find`, `position`, `contains`, `any`, `all`, and
  `fold` algorithms for slices and arrays. Predicate and fold callbacks borrow
  elements and forward their exact effect row; source-returning search retains
  the collection loan;
- ownership markers and operator traits;
- `option` and `result`, including inspection, region-preserving views,
  transformations, fallback, and extraction helpers;
- iteration, indexing, and flow protocols;
- source-backed `parse`, effect-parameterized scalar/ASCII text writers, strict
  radix-2-through-36 `u64`/`i64` parsing with structured byte-offset errors,
  and statically dispatched display/debug formatting for 64-bit and 128-bit
  integers, booleans, Unicode scalars, and text in `core.fmt`;
- zero-allocation runtime UTF-8 string literals backed by immutable private
  globals and owning `string` values that distinguish literal from allocated
  storage;
- dynamically sized immutable `str` views represented through the same
  `{address, byte length}` borrowed-view ABI as slices; checked
  `borrow(slice(u8))` validation rejects malformed, truncated, overlong,
  surrogate, and out-of-range UTF-8, while `string.as_str` and `str.as_bytes`
  retain the source region and loan;
- UTF-8 byte-boundary queries and checked `str` subviews, including empty
  one-past-end views; the internal subview projection is shared with slices
  while the safe text wrapper alone enforces UTF-8 endpoints;
- exact allocation-free byte equality for `str` views and owning `string`
  values, with borrowed dynamically sized operands dispatched through the
  source-defined equality protocol;
- owning `string` construction from empty capacity, borrowed `str`, and
  `unicode_scalar`; capacity reservation; scalar and text append; checked
  boundary-preserving truncation; and clearing without releasing capacity.
  Growing static literal storage first copies it into a uniquely owned
  allocation, and safe mutation never exposes writable bytes;
- checked owning substring copies, lexicographic `str` and `string` ordering,
  prefix and suffix checks, containment, and first-match search. All range and
  search positions are UTF-8 byte offsets; substring endpoints and returned
  match offsets are scalar boundaries;
- borrowed byte and Unicode-scalar iterators that retain the source text loan
  and yield copied values, plus scalar counting and checked scalar lookup;
- copyable `unicode_scalar` values with checked `u32` construction, numeric
  projection, code-point equality, and exact canonical UTF-8 byte length;
- effects, handlers, and control contracts.

Implemented `alloc` facilities include:

- `box(t)`;
- `vec(t)` with consistent checked/trapping shared or mutable access,
  first/last, slice conversion, copyable slice extension, equal-length and
  overlap-safe copy mutation, resource-preserving owned append, and consuming
  iteration, plus the common slice-backed search and fold vocabulary;
- ownership-preserving `vec(u8)`/`string` conversion: success transfers the
  allocation, failure retains the original vector and its valid-prefix length,
  and static strings copy into owned bytes;
- `string_writer`, an allocation-backed pure formatting sink whose empty state
  and ordered scalar/ASCII appends finish as one owning `string`;

Safe `string` and `str` APIs preserve valid UTF-8 and do not expose mutable
bytes. Borrowed-view
escape analysis follows reference loans through raw view casts, calls,
`option` payloads, and matches, so a view cannot outlive local bytes or overlap
a mutable write. Broader Unicode algorithms and host I/O are not yet library
features.

Borrowed `array.iter` and `slice.iter` both produce `slice_iter(a)(t)` without
a `copyable` element bound. The iterator preserves shared or mutable source
access and yields `borrow(a)(r)(t)` tied to one mutable `next` borrow, so a
yielded mutable element must end before the iterator advances. Resource
elements remain in place and are dropped by their owner. `vec` iteration
consumes elements and drops an unyielded suffix exactly once on early exit.

Arrays and slices expose in-place `swap` and `reverse` for concrete sized
elements, including resources. Copyable elements additionally expose `fill`,
equal-length `copy_from`, and overlap-safe `copy_within`. Every fallible bound
or length precondition is checked before the first write. `copy_within`
selects backward copying whenever the destination starts after the source, so
overlap has memmove semantics. These operations do not allocate or invoke user
effects; resource reordering neither copies nor drops elements.

Vectors additionally expose `extend_from_slice` for copyable elements. It
reserves the complete additional capacity before initializing the tail,
advances the initialized length after every write, and rejects a source view
borrowed from the same mutable vector. Allocation and layout failure terminate
before any tail copy. Move-only elements use ownership-transferring `push` and
`append`; borrowed slices never copy or consume them. The accepted
[vector-operations contract](vector-operations.md) records these guarantees.

Arrays, slices, and vectors expose common `find`, `position`, `contains`,
`any`, `all`, and `fold` operations through one borrowed-slice kernel.
Search and predicates short-circuit in source order, empty inputs use the
usual logical and fold identities, and callback effects remain visible to the
caller. `find` returns a source-retaining element borrow; `contains` requires
copyable equality while move-only callers use a borrowing predicate. The
accepted [collection-algorithms contract](collection-algorithms.md) records
the ownership, effect, cleanup, and iterator-boundary decisions.

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

## Tooling

The conservative [source formatter](formatter.md) provides `salic fmt` and
`salic fmt --check` for individual files and root packages. It preserves the
existing physical line boundaries, expands directly nested block boundaries,
and uses parser-provided source roles for two-space brace, delimiter,
declaration, `where`, trailing-closure, and match indentation. Comments and
dependencies remain source-owned; the passing fixture corpus is idempotent
under repeated formatting.

The editor API remains transport-independent. Structured resolver diagnostics,
versioned in-memory document state, incremental parsing, and an LSP transport
are not yet implemented.

The `examples/inventory` package is the current nontrivial library acceptance program. It combines
modules, owning strings, vectors, results, user traits, resource transfer, iteration, and cleanup.

## Known Boundaries

The principal incomplete areas are:

- host-facing modules within `std`;
- complete asynchronous execution;
- stable ABI and package distribution.

These boundaries are intentionally explicit. Passing tests for an implemented subset do not imply
stability or support for adjacent syntax.
