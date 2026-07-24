# M0 core scope

Status: frozen implementation target

M0 is the smallest language slice that Salicin intends to make coherent before adding another major
language capability. "Frozen" applies to the scope of the implementation target: it does not promise
source or ABI compatibility while Salicin remains a pre-1.0 experimental language.

New syntax or semantic machinery belongs outside M0 unless it is required to make an item below
sound, implementable, diagnosable, or usable in a complete program.

## Maturity labels

Project documentation uses three labels:

- **M0 core**: part of the frozen implementation target. Regressions block changes.
- **Implemented extension**: available and tested, but not required to complete M0. Its surface may
  still be narrowed when it complicates the core.
- **Exploration**: design work or a partial implementation. Programs must not rely on the complete
  documented design being available.

These labels describe project maturity, not safety. Implemented code must uphold Salicin's safety
rules regardless of its label.

## M0 core

M0 contains:

- Unicode source, logical newlines, declarations, lexical scopes, modules, packages, and explicit
  visibility;
- immutable and mutable value bindings, primitive scalars, tuples, fixed arrays, nominal structs,
  enums, exhaustive pattern matching, and structured control flow;
- named functions, parameter groups, complete and partial application, non-capturing function
  values, and closures needed by ordinary control APIs;
- static generic functions and nominal types over `type`, first-order traits, associated types,
  coherent static dispatch, and source-backed operator protocols;
- deterministic left-to-right evaluation, explicit `copy`, `move`, `borrow`, and `borrow(mut)`
  passing, lexical borrow checking, move checking, and deterministic cleanup;
- `Option`, `Result`, `Throws(Error)`, `try`, `throw`, `Unsafe`, `unsafe`, raw primitives behind
  that authority boundary, and C FFI;
- binary and library targets, native LLVM emission, project manifests, local dependencies, and
  diagnostics suitable for source-level debugging.

M0 explicitly does not require general algebraic handlers, higher-kinded traits, generic effect
rows, dynamic effectful callables, asynchronous state-machine lowering, owning strings, dynamic
vectors, or a particular executor. Existing implementations of those capabilities are extensions.

## Implemented extensions

The repository currently includes tested implementation slices for:

- user-defined algebraic effects, one-shot handlers, residual rows, and several statically
  specialized effectful callable forms;
- the compile-time `effect` domain and closed `access` and `passing` types;
- constructor-kinded generic parameters and the standard `Functor`, `Applicative`, and `Monad`
  protocols;
- source-backed `Chain` and `Coalesce` protocols;
- allocation primitives plus initial `Box` and `Vec` library implementations.

An implemented extension may have explicit rejection boundaries. Those boundaries belong in
[implementation status](status.md) and should have negative tests.

## Exploration

The following remain exploratory:

- complete `Async`/`Future` state-machine lowering, cancellation, pinning, and executor libraries;
- open-ended runtime effectful callable dispatch;
- complete higher-kinded associated-type and constructor-equation solving;
- general handler composition across fully effect-parameterized residual rows;
- a stable external ABI, stable package ecosystem, and compatibility guarantees.

The language specification may describe the intended semantics of an exploration item. Such a
section must identify itself as exploratory and link back to this document.

## Change gate

A proposal that expands M0 must provide:

1. a motivating complete program that cannot be expressed reasonably with the frozen core;
2. interaction rules for ownership, effects, cleanup, inference, modules, and FFI where applicable;
3. positive, negative, diagnostic, and native execution tests;
4. an implementation plan that preserves the compiler phase boundaries;
5. removal or simplification alternatives considered first.

Bug fixes, diagnostic improvements, library additions expressible in the existing language, and
behavior-preserving compiler refactors do not expand M0.
