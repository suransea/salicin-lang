# Allocation library

`library/alloc` contains owning types built on Salicin's raw allocation intrinsics and the
replaceable allocator ABI. `alloc.lib` is the root `pub let` alias facade, `alloc.boxed` and `alloc.vec`
hold the owning type implementations, and `alloc.raw` is reserved for package-internal allocation
helpers. The implementation package is not part of the intended prelude.

Owning container names are not implicitly visible. Alias the types a module uses:

```sc fragment
let box = alloc.boxed.box
let vec = alloc.vec.vec
```

Qualified paths such as `alloc.boxed.box` are also valid. The underlying `alloc` layer is supplied by
the toolchain and does not need to appear in `salicin.toml`. Prefixed helpers such as `box_new` and
`vec_push` are private implementation details. Owning types keep their
canonical `alloc` paths rather than acquiring mirror paths in `std`.

## `alloc.boxed`

`box(t)` owns one heap allocation. `box.new(value)` constructs it; `boxed.as_ref()` borrows the
pointee with shared access and `boxed.as_ref(mut)()` borrows it with exclusive access. The rest of
the API covers replacement, `copyable` reads and writes, and consuming extraction. `boxed.into_raw()`
consumes the owner without freeing its allocation; `unsafe { box(t).from_raw(pointer) }` restores
unique ownership from a pointer produced by `into_raw`. The caller must not rebuild more than one
owner or pass any other pointer to `from_raw`. Destruction recursively drops the pointee before
releasing storage.

## `alloc.vec`

`vec(t)` owns contiguous storage and supports both copyable and resource elements. Its API includes
construction, capacity management, push/pop, insertion/removal, append, truncation, swaps, and
in-place reversal. `values.at(index)` borrows an element with shared access and
`values.at(mut)(index)` borrows it with exclusive access. Bounds and allocation-layout failures
trap.

`vec(t)` also implements `core.ops.index(u64)` in source. `values[index]`,
`borrow(values[index])`, and `values[index] = replacement` share the same checked `at(a)`
implementation and preserve its receiver loan.

`values.take()` replaces a vector with an empty vector and returns ownership of its previous
allocation without copying elements. Consuming iteration transfers the allocation into
`vec_into_iter(t)` and invalidates the original
vector. Each `next` moves one initialized element in source order. If iteration stops early, the
The iterator drops only the unyielded suffix and then releases the allocation; yielded values remain
owned by the loop body. Capacity arithmetic, layout overflow, invalid bounds, invalid allocator
layouts, and allocation failure terminate the process rather than returning a recoverable error or
widening the caller's effect row.

Container fields remain private so safe code cannot forge ownership metadata. Allocation operations
ultimately use the ABI documented in [runtime.md](../runtime.md).

## `alloc.string`

`string` is a private `vec(u8)` wrapper whose initialized bytes are
always valid UTF-8. Length and capacity are byte-based; safe code receives only shared byte views,
and invalid consuming conversion preserves the original vector in `from_utf8_error`. Construction,
validation, byte recovery, capacity management, clearing, and append are ordinary source-backed
methods. `from_utf8_unchecked` requires the standard `unsafety` effect. Unicode scalars, a
borrowed `str` type, indexing, Unicode algorithms, and general string literal expressions are
specified by the initial surface contract but remain unimplemented.
`string` owns a private `vec(u8)`, maintains valid UTF-8, measures length and capacity in bytes,
and exposes no safe mutable byte view. Failed UTF-8 conversion returns ownership through
`from_utf8_error`.

See [standard-library organization](README.md) for the prelude and alias policy.
