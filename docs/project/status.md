# Implementation status

Salicin is an experimental compiler and language design. The repository currently includes a native
compiler pipeline, project manifests and local dependencies, ownership and borrow analysis,
source-backed core traits and containers, cleanup lowering, raw allocation primitives, and a growing
`Box`/`Vec` allocation library.

Access keyword generics are implemented for functions and generic inherent members: `A: access` accepts `shared` or `mut`,
defaults to shared when omitted, participates in monomorphization, and can drive parameter modes,
borrow types, borrow expressions, and raw pointer borrows. The alloc free functions and methods use
this path. Mutable borrowing has one source spelling, `borrow(mut)`; separately named mutable alloc
aliases and the former prefix spelling are intentionally absent before 1.0.
Passing keyword generics are also implemented for functions and generic inherent members:
`P: passing` accepts `auto`, `copy`, or `move` and can be referenced directly in parameter keyword
position. Functions and trait methods place a contextual `with(...)` clause after the result type:
`: T with(unsafe)` adds the checked unsafe call requirement, `: T with(try)` normalizes to `Option(T)`, and
`: T with(try(E))` normalizes to `Result(T, E)`. Callable source types use the same shape, such as
`(i32): i32 with(unsafe)`; the clause is not a runtime or currying group. Complete direct, method,
aliased, and partially applied unsafe calls require an
enclosing unsafe function or `unsafe { ... }` handler. `do` forwards the implemented unsafe effect
into nested immediate calls. `let UI = effect` declares a nominal, module-visible marker effect.
Function and generic inherent-member `E: effect` parameters represent complete rows, default to pure,
participate in monomorphization, forward through ordinary compile-time calls such as
`callee(E)(value)`, and infer pure, unsafe, or custom rows from higher-order callable arguments.
Named non-capturing functions can be passed and invoked through the native function-pointer ABI.
User-defined handlers/context lowering, capturing closure values, generic trait methods, and async
color polymorphism remain design work.

`core` and `alloc` are mounted in ordinary module resolution. `core.ops` traits and alloc containers
are not part of the prelude. `Box`, `Vec`, and their free functions require
`use alloc.boxed...` / `use alloc.vec...` (or a qualified path), while operator traits require
`use core.ops...` when named. Their internal identities remain isolated from same-named user
declarations; operator syntax continues to dispatch through validated lang items.

The implementation is broad but not stable. Important incomplete boundaries include:

- `core` provides the initial prelude, arithmetic, bitwise, unary, equality, and partial-ordering protocols, and generalized error
  propagation through `Try`, `FromResidual`, and `FromError`; slices, iteration, indexing,
  and `Future` remain to be implemented;
- `std` host APIs have not been started;
- registry dependencies, workspaces, stable ABI guarantees, and a package distribution format are
  not defined;
- asynchronous syntax is designed but Future lowering and an executor interface are not complete;
- diagnostics, source locations, tooling, and incremental compilation need substantial work.

The [language specification](../language/specification.md) states intended language rules. This file
records implementation state. Release-specific additions and fixes are recorded only in the
[changelog](../../CHANGELOG.md).
