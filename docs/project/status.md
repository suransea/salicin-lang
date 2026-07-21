# Implementation status

Salicin is an experimental compiler and language design. The repository currently includes a native
compiler pipeline, project manifests and local dependencies, ownership and borrow analysis,
source-backed core traits and containers, cleanup lowering, raw allocation primitives, and a growing
`Box`/`Vec` allocation library.

Access keyword generics are implemented for functions: `A: access` accepts `shared` or `mut`,
defaults to shared when omitted, participates in monomorphization, and can drive parameter modes,
borrow types, borrow expressions, and raw pointer borrows. The alloc free functions use this path.
Generic methods nested in generic inherent extensions still use shared/mutable compatibility names.
General effect rows, effect polymorphism, and a generalized `passing` kind remain design work rather
than source-language features.

The implementation is broad but not stable. Important incomplete boundaries include:

- compiler-owned `core` and `alloc` modules are tracked separately, but normal `use core...` and
  `use alloc...` resolution has not replaced compatibility injection yet;
- `std` host APIs have not been started;
- registry dependencies, workspaces, stable ABI guarantees, and a package distribution format are
  not defined;
- asynchronous syntax is designed but Future lowering and an executor interface are not complete;
- diagnostics, source locations, tooling, and incremental compilation need substantial work.

The [language specification](../language/specification.md) states intended language rules. This file
records implementation state. Release-specific additions and fixes are recorded only in the
[changelog](../../CHANGELOG.md).
