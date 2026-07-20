# Salicin `alloc`

This directory contains the edition-pinned, compiler-embedded `alloc` bootstrap source. Unlike
`core`, it depends on the replaceable allocator ABI and target-layout intrinsics.

The bootstrap defines `Box(T)`, `box_new`, `box_ptr`, `box_into_inner`, and `box_replace` in ordinary
Salicin source. The compiler recognizes the validated `Box` representation only to supply recursive
heap drop/deallocation glue; construction and access still pass through the normal parser, generic
monomorphizer, visibility checker, ownership analysis, and LLVM lowering.

`box_into_inner` consumes the unique owner, moves the pointee back to the caller, and frees only the
allocation. `box_replace` requires a mutable borrow and exchanges the heap value without duplicating
ownership. Their small unsafe regions use the reserved `raw_take`, `raw_init`, and allocator
intrinsics; callers see safe owning operations and ordinary move diagnostics.
