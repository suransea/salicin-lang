# Allocation library

`library/alloc` contains owning types built on Salicin's raw allocation intrinsics and the
replaceable allocator ABI. `alloc.lib` is the root `pub let` alias facade, `alloc.boxed` and `alloc.vec`
hold the owning type implementations, and `alloc.raw` is reserved for package-internal allocation
helpers. The implementation package is not part of the intended prelude.

Owning container names are not implicitly visible. Alias the types a module uses:

```sc fragment
let Box = std.boxed.Box
let Vec = std.vec.Vec
```

Qualified paths such as `std.boxed.Box` are also valid. The underlying `alloc` layer is supplied by
the toolchain and does not need to appear in `salicin.toml`. Prefixed helpers such as `box_new` and
`vec_push` are private implementation details; `std.boxed` and `std.vec` export only the owning
types and their inherent APIs.

## `std.boxed`

`Box(T)` owns one heap allocation. `Box.new(value)` constructs it; `boxed.as_ref()` borrows the
pointee with shared access and `boxed.as_ref(mut)()` borrows it with exclusive access. The rest of
the API covers replacement, Copy reads and writes, and consuming extraction. `boxed.into_raw()`
consumes the owner without freeing its allocation; `unsafe { Box(T).from_raw(pointer) }` restores
unique ownership from a pointer produced by `into_raw`. The caller must not rebuild more than one
owner or pass any other pointer to `from_raw`. Destruction recursively drops the pointee before
releasing storage.

## `std.vec`

`Vec(T)` owns contiguous storage and supports both Copy and resource elements. Its API includes
construction, capacity management, push/pop, insertion/removal, append, truncation, swaps, and
in-place reversal. `values.at(index)` borrows an element with shared access and
`values.at(mut)(index)` borrows it with exclusive access. Bounds and allocation-layout failures
trap.

`Vec(T)` also implements `core.ops.Index(u64)` in source. `values[index]`,
`borrow(values[index])`, and `values[index] = replacement` share the same checked `at(A)`
implementation and preserve its receiver loan.

Container fields remain private so safe code cannot forge ownership metadata. Allocation operations
ultimately use the ABI documented in [runtime.md](../runtime.md).

See [standard-library organization](README.md) for the prelude and alias policy.
