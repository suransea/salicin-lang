# Salicin `alloc`

This directory contains the edition-pinned, compiler-embedded `alloc` bootstrap source. Unlike
`core`, it depends on the replaceable allocator ABI and target-layout intrinsics.

The first slice defines `Box(T)`, `box_new`, and `box_ptr` in ordinary Salicin source. The compiler
recognizes the validated `Box` representation only to supply recursive heap drop/deallocation glue;
construction and pointer access still pass through the normal parser, generic monomorphizer,
visibility checker, ownership analysis, and LLVM lowering.
