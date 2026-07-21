# Allocation library

`library/alloc` contains owning types built on Salicin's raw allocation intrinsics and the
replaceable allocator ABI. It is split by public module and is not part of the intended prelude.

## `alloc.boxed`

`Box(T)` owns one heap allocation. `box_as_ref(A: access, 'a: region, T: type)` is the canonical free
borrow operation: omitted `A` selects shared access and `A: mut` selects exclusive access.
`box_as_mut` remains a compatibility wrapper while generic methods are not yet supported inside a
generic inherent extension. The rest of the API covers construction, pointer access, replacement,
Copy reads and writes, and consuming extraction. Destruction recursively drops the pointee before
releasing storage.

## `alloc.vec`

`Vec(T)` owns contiguous storage and supports both Copy and resource elements. Its API includes
construction, capacity management, push/pop, insertion/removal, append, truncation, swaps, and
in-place reversal. `vec_at(A: access, 'a: region, T: type)` is the canonical element-borrow function;
`vec_at_mut` is its compatibility wrapper. Bounds and allocation-layout failures trap.

Container fields remain private so safe code cannot forge ownership metadata. Allocation operations
ultimately use the ABI documented in [runtime.md](../runtime.md).

See [standard-library organization](README.md) for the current import policy and compatibility note.
