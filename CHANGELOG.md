# Changelog

Salicin follows semantic versioning while the compiler is experimental. Minor releases may extend
the accepted language, while patch releases preserve source semantics within the implemented subset.

## Unreleased

M1 development is in progress.

- Added nominal structs and enums with positional or labeled construction.
- Added field access and mutable field assignment.
- Added exhaustive enum matching, payload bindings, guards and nested matches.
- Added local non-escaping partial application for multi-group named functions.
- Added place-aware `copy` and `move` checking, including use-after-move diagnostics.
- Added shared and mutable borrows, explicit borrow values, and pointer-based borrow parameters.

## 0.1.0 - 2026-07-20

- Added the `salic` single-file compiler and `.sali` source format.
- Added scalar types, named functions, parameter groups, local bindings, control flow and operators.
- Added textual LLVM IR generation and native linking through Clang.
- Added `build`, `check`, `emit-ir` and `run` commands.
