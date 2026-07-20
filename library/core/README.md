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

The v0.9 compiler normalizes move-path initialization alternatives, permits sound root and field
reinitialization, conservatively widens after 64 exact alternatives, and prevents non-`Copy`
pattern bindings from being moved in match guards. It also builds and verifies a type-independent
`CleanupPlan` from each function's real HIR scopes and control-flow edges.

That plan is a boundary, not executable destruction. It explicitly keeps pending cases for
unmaterialized resource results, move-state dataflow, temporary liveness, loop-break transfer,
borrowed mutation, maybe-overwrite state, match/pattern transfer, and partial or closure captures.
There is no `needs_drop`, runtime drop flag, public source-backed `Drop`, drop glue, or LLVM cleanup
emission yet. Compile-time globals are independently materialized at each use and are outside the
cleanup plan; resource-bearing global semantics must be settled before `Drop` is exposed.

The adjacent standard-library route is therefore: finish cleanup materialization and dataflow in
`core`; add `needs_drop` and drop flags; expose source-backed `Drop` and emit glue; then define raw
pointers and the allocator ABI. Only after those boundaries are real will `alloc` be added, followed
by platform `std` over the C ABI and minimal runtime.
