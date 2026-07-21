# Salicin `alloc`

This directory contains the edition-pinned, compiler-embedded `alloc` bootstrap source. Unlike
`core`, it depends on the replaceable allocator ABI and target-layout intrinsics.

The bootstrap defines `Box(T)`, `box_new`, `box_ptr`, `box_read`, `box_into_inner`, and `box_replace`
in ordinary Salicin source. The compiler recognizes the validated `Box` representation only to supply recursive
heap drop/deallocation glue; construction and access still pass through the normal parser, generic
monomorphizer, visibility checker, ownership analysis, and LLVM lowering.

`box_into_inner` consumes the unique owner, moves the pointee back to the caller, and frees only the
allocation. `box_replace` requires a mutable borrow and exchanges the heap value without duplicating
ownership. Their small unsafe regions use the reserved `raw_take`, `raw_init`, and allocator
intrinsics; callers see safe owning operations and ordinary move diagnostics.

`box_read` requires `T: Copy` and copies the pointee through a small internal unsafe region. Its
`boxed.read()` method lives in a separate constrained extension, so non-Copy resource boxes never
gain a copying API. This is value access, not a reference-returning operation; safe Box borrows
remain reserved for the future explicit reference and lifetime type system.

Since v0.32 the bundle also defines `extend(T: type) Box(T)` in source. This supplies inferred
`Box.new`, `as_mut_ptr`, `into_inner`, and `replace` members through the compiler's general generic
inherent-extension monomorphizer. Since v0.44 a second `where T: Copy` extension supplies `read`;
the free functions remain the bootstrap and compatibility layer.
