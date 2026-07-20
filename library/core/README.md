# Salicin core bootstrap

This directory contains the compiler-matched Salicin sources that begin the `core` standard-library
layer. The compiler embeds `src/prelude.sali`, parses it with the ordinary frontend, validates every
lang-item declaration, and shares the resulting identities across the complete package graph.

The bootstrap bundle currently defines `Option`, `Result`, `never`, and `Add`. It deliberately has no
package manifest yet: mounting `core` as an explicitly addressable virtual package, compiling it
without its own prelude, and selecting per-package edition preludes are later bootstrap steps.

The v0.6 compiler closes field and exported-signature visibility before this source surface grows:
core types may now expose selected fields without making representation details constructible, and
public declarations cannot accidentally name narrower implementation types.

`alloc` will be added only after pointer, allocator, and destruction support exists. The platform
`std` layer will follow raw-pointer, C-ABI, and runtime interfaces.
