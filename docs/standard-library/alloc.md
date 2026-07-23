# Allocation library

`library/alloc` contains owning types built on Salicin's raw allocation intrinsics and the
replaceable allocator ABI. `alloc.lib` is the root `pub use` facade, `alloc.boxed` and `alloc.vec`
hold the public definitions, and `alloc.raw` is reserved for package-internal allocation helpers
once the language has finer-grained visibility. Alloc is not part of the intended prelude.

Alloc names are not implicitly visible. Import only the declarations a module uses:

```sc
use std.boxed.{Box, box_as_ref}
use std.vec.Vec
```

Qualified paths such as `std.boxed.Box` are also valid. The underlying `alloc` layer is supplied by
the toolchain and does not need to appear in `salicin.toml`.

## `std.boxed`

`Box(T)` owns one heap allocation. `box_as_ref(A: access, R: region, T: type)` is the canonical free
borrow operation: omitted `A` selects shared access and `A: mut` selects exclusive access.
The method form is `boxed.as_ref()` for shared access and `boxed.as_ref(mut)()` for exclusive access.
There is no separately named mutable alias. The rest of the API covers construction, pointer access,
replacement, Copy reads and writes, and consuming extraction. Destruction recursively drops the
pointee before releasing storage.

## `std.vec`

`Vec(T)` owns contiguous storage and supports both Copy and resource elements. Its API includes
construction, capacity management, push/pop, insertion/removal, append, truncation, swaps, and
in-place reversal. `vec_at(A: access, R: region, T: type)` is the canonical element-borrow function;
the method forms are `values.at(index)` and `values.at(mut)(index)`. There is no separately named
mutable alias. Bounds and allocation-layout failures trap.

Container fields remain private so safe code cannot forge ownership metadata. Allocation operations
ultimately use the ABI documented in [runtime.md](../runtime.md).

See [standard-library organization](README.md) for the prelude and import policy.
