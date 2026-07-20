# Salicin core bootstrap

This directory contains the compiler-matched Salicin sources that begin the `core` standard-library
layer. The compiler embeds `src/prelude.sali`, parses it with the ordinary frontend, validates every
lang-item declaration, and shares the resulting identities across the complete package graph.

The bootstrap bundle currently defines `Option`, `Result`, `never`, the canonical empty marker trait
`pub let Copy = trait {}`, and the five arithmetic operator traits `Add`, `Sub`, `Mul`, `Div`, and
`Rem`. Each operator trait takes `Rhs`, defines an `Output` associated type, and moves both `self`
and `rhs`. It deliberately has no package manifest yet:
mounting `core` as an explicitly addressable virtual package, compiling it without its own prelude,
and selecting per-package edition preludes are later bootstrap steps.

The v0.6 compiler closes field and exported-signature visibility before this source surface grows:
core types may now expose selected fields without making representation details constructible, and
public declarations cannot accidentally name narrower implementation types.

The v0.7 compiler validates the identity and exact shape of all five source-backed arithmetic traits.
Nominal left operands use unique static trait dispatch with expected-output and integer-literal range
filtering; built-in integers keep direct lowering, and same-named user traits cannot spoof lang-item
semantics.

The v0.8 compiler likewise validates `Copy` by canonical identity and exact shape. Primitive types,
unit, `never`, the internal error-recovery type, and arrays of `Copy` elements are intrinsic. A
nominal struct or enum opts in with `extend T: Copy {}` only in its defining package, and every field
or variant payload must recursively be `Copy`. An implementation for a concrete generic instance
does not generalize; blanket/generic implementations and `where` proofs are not supported yet.
Validated nominal `Copy` participates in inferred parameter passing, ordinary reads, closure
captures, and partial application, while explicit `move` still consumes. Function and closure types
remain non-`Copy`, and same-named user traits cannot spoof the lang item.

`Drop` is not public in v0.8. The next ownership layer is internal scope cleanup and drop flags,
followed by `Drop`, raw pointers, and an allocator ABI. Only then will `alloc` be added; the platform
`std` layer follows C-ABI and runtime interfaces.
