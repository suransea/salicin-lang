# Allocation library

`library/alloc` contains owning types built on Salicin's raw allocation intrinsics and the
replaceable allocator ABI. It is split by public module and is not part of the intended prelude.

## `alloc.boxed`

`Box(T)` owns one heap allocation. Its API supports construction, pointer access, shared and mutable
borrows, replacement, Copy reads and writes, and consuming extraction. Destruction recursively drops
the pointee before releasing storage.

## `alloc.vec`

`Vec(T)` owns contiguous storage and supports both Copy and resource elements. Its API includes
construction, capacity management, push/pop, insertion/removal, append, truncation, shared and
mutable element borrows, swaps, and in-place reversal. Bounds and allocation-layout failures trap.

Container fields remain private so safe code cannot forge ownership metadata. Allocation operations
ultimately use the ABI documented in [runtime.md](../runtime.md).

See [standard-library organization](README.md) for the current import policy and compatibility note.
