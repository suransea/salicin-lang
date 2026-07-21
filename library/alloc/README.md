# Salicin `alloc`

This directory contains the edition-pinned, compiler-embedded `alloc` bootstrap source. Unlike
`core`, it depends on the replaceable allocator ABI and target-layout intrinsics.

The bootstrap defines `Box(T)` and the first `Vec(T)` in ordinary Salicin source. The compiler
recognizes the validated `Box` representation only to supply recursive heap drop/deallocation glue;
construction and access still pass through the normal parser, generic monomorphizer, visibility
checker, ownership analysis, and LLVM lowering. `Vec` uses an ordinary source-backed `Drop`
implementation and needs no container-specific compiler glue.

`box_into_inner` consumes the unique owner, moves the pointee back to the caller, and frees only the
allocation. `box_replace` requires a mutable borrow and exchanges the heap value without duplicating
ownership. Their small unsafe regions use the reserved `raw_take`, `raw_init`, and allocator
intrinsics; callers see safe owning operations and ordinary move diagnostics.

`box_read` and `box_write` require `T: Copy` and access the pointee through small internal unsafe
regions. Their `boxed.read()` and `boxed.write(value)` methods live in a separate constrained
extension, so non-Copy resource boxes never gain copying APIs; those use ownership-aware `replace`.
This is value access, not a reference-returning operation; safe Box borrows
remain reserved for the future explicit reference and lifetime type system.

Since v0.32 the bundle also defines `extend(T: type) Box(T)` in source. This supplies inferred
`Box.new`, `as_mut_ptr`, `into_inner`, and `replace` members through the compiler's general generic
inherent-extension monomorphizer. Since v0.45 a second `where T: Copy` extension supplies `read` and `write`;
the free functions remain the bootstrap and compatibility layer.

Since v0.61 the bundle also defines a private three-field `Vec(T)` representation and safe APIs for
`T: Copy`: `new`, `with_capacity`, `len`, `capacity`, `push`, `read`, and `write`. Growth doubles the
capacity, copies initialized elements with `raw_offset`, and releases the old allocation. Both
capacity doubling and `capacity * size_of(T)` are checked before allocator entry; bounds and layout
failures use the diverging unsafe `raw_trap()` intrinsic inside the safe wrapper. Zero-sized elements
retain normal length/capacity behavior. Resource-element move/drop support remains intentionally
outside this first API rather than treating resources as copyable.

Since v0.62 construction and `push` work for resource elements as well. The inferred push parameter
copies a Copy value and moves a resource value. Growth move-initializes the new allocation from
`raw_take` results, `replace` and `pop` return ownership to the caller, and Vec's source-backed
destructor drops only the elements that remain within its logical length before deallocation.
Copy-only `read` and `write` stay in their constrained extension; resource access never duplicates
an owner.

Since v0.63 `reserve(additional)` owns the checked growth path used by `push`. `truncate` and `clear`
drop removed elements immediately while retaining storage, `is_empty` reports logical emptiness,
and `swap_remove` returns the selected owner while moving the last element into its slot. These
operations support resource elements without a `Copy` bound; capacity addition, layout size, and
indices are checked before entering raw storage operations.

Since v0.64 ordered `insert` and `remove` shift initialized suffix elements using ownership moves.
`append` mutably borrows two vectors, transfers every source owner, and leaves the source empty with
its allocation intact; ordinary alias checking rejects self-append. `shrink_to_fit` similarly moves
elements into exact-length storage. None of these APIs require `Copy` or destroy elements merely
because their storage location changes.

Since v0.65 `at(index)` and `at_mut(index)` return bounds-checked shared and mutable element
references for Copy and resource types. Their inferred regions are tied to the receiver borrow, so
live references prevent vector mutation or conflicting access. Small internal unsafe regions use
`raw_borrow`/`raw_mut_borrow` with the Vec receiver as the explicit lifetime anchor; users never
receive an unanchored raw storage reference through the safe API.
