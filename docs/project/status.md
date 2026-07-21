# Implementation status

Salicin is an experimental compiler and language design. The repository currently includes a native
compiler pipeline, project manifests and local dependencies, ownership and borrow analysis,
source-backed core traits and containers, cleanup lowering, raw allocation primitives, and a growing
`Box`/`Vec` allocation library.

Structured control flow includes `while`, value-producing `loop`, `break`, and `continue`.
`continue` targets the nearest loop, participates in loop-backedge ownership validation, and runs
all lexical cleanup required when leaving nested scopes before starting the next iteration.

Access keyword generics are implemented for functions and generic inherent members: `A: access` accepts `shared` or `mut`,
defaults to shared when omitted, participates in monomorphization, and can drive parameter modes,
borrow types, borrow expressions, and raw pointer borrows. The alloc free functions and methods use
this path. Mutable borrowing has one source spelling, `borrow(mut)`; separately named mutable alloc
aliases and the former prefix spelling are intentionally absent before 1.0.
Passing keyword generics are also implemented for functions and generic inherent members:
`P: passing` accepts `auto`, `copy`, or `move` and can be referenced directly in parameter keyword
position. Functions and trait methods place a contextual `with(...)` clause after the result type:
`: T with(unsafe)` adds the checked unsafe call requirement, while `: T with(throws(E))` declares an
automatically propagated error effect and uses `Result(T, E)` as its current ABI carrier. `try { ... }`
handles that effect and produces an explicit `Result`. Without a contextual result type, a handler
infers `Result(T, E)` from one unique escaping `throws(E)` source across direct, method, and
non-capturing indirect calls; nested handlers do not leak handled errors. Postfix `.try` and
`with(try...)` are removed.
Callable source types use the same shape, such as
`(i32): i32 with(unsafe)`; the clause is not a runtime or currying group. Complete direct, method,
aliased, and partially applied unsafe calls require an
enclosing unsafe function or `unsafe { ... }` handler. `do` forwards the implemented unsafe effect
into nested immediate calls. `let UI = effect` declares a nominal, module-visible marker effect.
Function and generic inherent-member `E: effect` parameters represent complete rows, default to pure,
participate in monomorphization, forward through ordinary compile-time calls such as
`callee(E)(value)`, and infer pure, unsafe, custom, or `throws(Error)` rows from higher-order callable
arguments. A selected `throws(Error)` row preserves both its error type and the current `Result`
carrier ABI through forwarding and specialization.
Named non-capturing functions can be passed and invoked through the native function-pointer ABI.
Concrete and generic top-level functions, concrete-nominal inherent members, and trait requirements may form label-directed
overload sets. Their runtime parameter-label shapes must differ, and at least one explicit named
call argument must select a unique candidate; a method's implicit receiver is not disambiguating
evidence. Trait conformance, default and blanket implementations, where-bound assumptions, curried
groups, module resolution, imports, type and optional-chain probing, closure lowering, effects, and
native mangling preserve that choice. Generic templates are selected by runtime labels before their
compile-time groups are inferred or consumed. Blanket generic inherent extensions preserve the same
overload set across every applicable concrete nominal instance.
Callable effect rows support requirement subtyping: a pure function value can fill an unsafe or
custom-effect slot, while a value requiring additional effects cannot fill a narrower slot. The
slot's widened requirements remain checked at indirect calls, and generic row inference retains the
callable's exact source row.
Fixed and effect-parameterized `throws(E)` are implemented for direct, method, partial, and
non-capturing indirect calls. Ordinary `Option` and `Result` functions require explicit variant
construction; the removed `Try`, `FromResidual`, `FromError`, and `ControlFlow` language protocols no
longer participate in return completion or propagation. `do` transparently forwards the complete
active row through its immediate closure boundary, including `throws` carrier lowering, `unsafe`,
and nominal marker effects. User-defined handlers, capturing closure values, generic trait methods,
and async color lowering remain design or implementation work.

`core` and `alloc` are mounted in ordinary module resolution. `core.ops` traits and alloc containers
are not part of the prelude. `Box`, `Vec`, and their free functions require
`use alloc.boxed...` / `use alloc.vec...` (or a qualified path), while operator traits require
`use core.ops...` when named. Their internal identities remain isolated from same-named user
declarations; operator syntax continues to dispatch through validated lang items.

The implementation is broad but not stable. Important incomplete boundaries include:

- `core` provides the initial prelude and arithmetic, bitwise, unary, equality, and partial-ordering
  protocols; language error propagation is the built-in `throws(E)` effect. Slices, iteration,
  indexing, and `Future` remain to be implemented;
- `std` host APIs have not been started;
- registry dependencies, workspaces, stable ABI guarantees, and a package distribution format are
  not defined;
- asynchronous syntax is designed but Future lowering and an executor interface are not complete;
- diagnostics, source locations, tooling, and incremental compilation need substantial work.

The [language specification](../language/specification.md) states intended language rules. This file
records implementation state. Release-specific additions and fixes are recorded only in the
[changelog](../../CHANGELOG.md).
