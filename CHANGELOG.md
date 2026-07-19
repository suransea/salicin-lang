# Changelog

Salicin follows semantic versioning while the compiler is experimental. Minor releases may extend
the accepted language, while patch releases preserve source semantics within the implemented subset.

## Unreleased

M2 development follows the v0.2 language foundation.

- Added compile-time `type` parameter groups and explicit generic named-function application.
- Added deterministic, cached, on-demand monomorphization while preserving runtime parameter groups
  and local partial application.
- Added definition-time abstract checking for generic function bodies, including return types and
  ownership, so invalid unused templates are diagnosed before instantiation.

## 0.2.0 - 2026-07-20

- Added nominal structs and enums with positional or labeled construction.
- Added field access and mutable field assignment.
- Added exhaustive enum matching, payload bindings, guards and nested matches.
- Added local non-escaping partial application for multi-group named functions.
- Added place-aware `copy` and `move` checking, including use-after-move diagnostics.
- Added shared and mutable borrows, explicit borrow values, and pointer-based borrow parameters.
- Added path-sensitive move-state joins for conditionals, short-circuit expressions and matches.
- Added lambda lifting for local, non-escaping closures with shared/mutable scalar captures and
  single-use nominal move captures.
- Added complete grouped calls for curried capturing closures, with explicit diagnostics for
  unsupported partial application.
- Added `while`, value-yielding `loop`, and value-carrying `break` expressions.
- Added conservative loop back-edge checking that rejects moving bindings declared outside the
  loop body.
- Added inline fixed `Array(T, N)` values, array literals, and runtime-checked `i32` indexing;
  the initial subset is limited to `Copy` elements and read-only indexing.
- Added inherent `extend` blocks with statically dispatched `borrow self`, `mut borrow self`, and
  `move self` methods, plus type-namespace associated functions and constants.
- Added separate instance/type member lookup and conservative diagnostics for temporary receivers
  and non-`Copy` bound-method partial application; trait-backed extensions remain reserved for M2.

## 0.1.0 - 2026-07-20

- Added the `salic` single-file compiler and `.sali` source format.
- Added scalar types, named functions, parameter groups, local bindings, control flow and operators.
- Added textual LLVM IR generation and native linking through Clang.
- Added `build`, `check`, `emit-ir` and `run` commands.
