# Standard library

Salicin reserves three library layers and exposes the user-facing standard-library surface through
the `std` namespace:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` is the edition-matched user facade. It currently re-exports validated
  `core` and `alloc` identities; later host modules add files, process access,
  and standard streams at this boundary.

Source is organized around definition modules plus small `pub let` alias facades:

```text
library/
  core/src/
    lib.sc
    prelude.sc
    never.sc
    marker.sc
    option.sc
    result.sc
    error.sc
    cmp.sc
    flow.sc
    ops.sc
    ops/arith.sc
    ops/bit.sc
    ops/assign.sc
    effect.sc
    async.sc
    unsafe.sc
    sorts.sc
    borrow.sc
    memory.sc
    control.sc
    iter.sc
    algebra.sc
    functional.sc
  alloc/src/
    lib.sc
    boxed.sc
    vec.sc
    string.sc
    raw.sc
  std/src/
    lib.sc
    prelude.sc
    option.sc
    result.sc
    cmp.sc
    flow.sc
    ops.sc
    ops/
    effect.sc
    async.sc
    unsafe.sc
    sorts.sc
    foreign.sc
    passing.sc
    borrow.sc
    control.sc
    iter.sc
    algebra.sc
    functional.sc
    boxed.sc
    vec.sc
    string.sc
    array.sc
    slice.sc
```

## Prelude policy

The edition prelude must stay small. It contains the universal `never`, `copyable`, and `droppable`
contracts, primitive type names, and the `array`, `ptr`, `size_of`, and `align_of`
memory contracts that compiler-generated types and low-level library code routinely need.
`option` and `result` are fundamental `core` declarations, but ordinary source should name them
through the `std` facade:

```sc fragment
let option = std.option
let result = std.result
```

Operator traits are aliased from the `std.ops` facade, `?.`/`??` protocols from `std.flow`, generic
handler contracts from `std.effect`, typed failure from `std.error`, asynchronous computation from
`std.async`, unsafe authority from `std.unsafe`, compile-time sorts from `std.sorts`,
compiler-lowered control contracts from `std.control`, algebra protocols from
`std.algebra`, higher-kinded functional protocols from `std.functional`, iteration protocols from
`std.iter`, and owning containers from `std.boxed` and `std.vec`. The underlying implementation is
still split across `core` and `alloc`: `core.option` and `core.result` define `option` and `result`,
while the `core` root re-exports the root public surface. Standard declarations must be named
through their module or given transparent aliases with ordinary `let`; for example:

```sc fragment
let box = std.boxed.box
let vec = std.vec.vec
let string = std.string.string
```

The compiler validates and embeds the matching `library/std` source bundle,
then mounts its public aliases plus the lower-level `core` and `alloc`
namespaces in every package. An alias keeps the canonical identity of its
resolved target; ordinary declarations cannot obtain compiler authority by
copying a privileged name or shape. Unsupported hosts are rejected before
semantic analysis. The initial supported pairs are Linux/x86-64 and
macOS/arm64.
Non-prelude declarations have qualified internal identities, so a user declaration without such an alias may
still be named `add`, `box`, or `vec`. A project dependency or top-level file module cannot claim
any of these standard namespaces.
`std.ops` uses the same rule: `add`, `sub`, `mul`, `div`, `rem`, `eq`, `partial_ordering`,
`partial_ord`, `neg`, `not`, `bit_and`, `bit_or`, `bit_xor`, `shl`, `shr`, and their `_assign` mutation
traits require ordinary aliases when
named. Merely writing the corresponding operator token does not require importing its protocol.
`std.flow.chain` and `std.flow.coalesce` require ordinary aliases when named directly.
`throwing(e)`, `unsafety`, and `suspension` are ordinary standard effect declarations in `std.error`,
`std.unsafe`, and `std.async`. Source that names them binds them normally. `try` and `throw` target
`core.error`; `unsafe` targets `core.unsafe`; structural control spellings such as `do` and `loop`
target `core.control`. These contextual spellings do not inject module exports as ordinary
unqualified names.
Effect identities and row parameters use the same `snake_case` convention as every other source
identifier; for example, `comptime e: effects`.
Standard declaration names describe semantics rather than encoding their
kind: types use entity/state nouns, traits use capability/role/operation
names, and effects use abstract behavior or capability nouns such as
`throwing`, `suspension`, and `unsafety`. Embedded public names are ASCII
`snake_case` and may not use category suffixes such as `_type`, `_trait`, or
`_effect`; ordinary user declarations are not subject to this library gate.
The `effect` identity sort, `effects` row sort, finite `access` sort, and parameter modifier functions use
contextual names such as `pure`, `shared`, `mut`, `copy`, and `move` in parameter positions.
`semigroup` and `monoid` require aliases from `std.algebra` when named.
`functor`, `applicative`, and `monad` require aliases from `std.functional` when named.
`iterator` and `into_iterator` require ordinary aliases from `std.iter` when named in an implementation
or bound. Writing `for value { pattern -> ... }` binds to their validated lang-item identities
without aliasing them and cannot be redirected by same-named inherent methods or traits.

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations. `std`, `core`, and `alloc` are reserved top-level namespaces, not manifest
dependencies.

The complete [`examples/inventory`](../../examples/inventory) package exercises file modules,
validated owning strings, recoverable byte conversion, a vector of non-`copyable` values, consuming
iteration, and user trait dispatch.

The accepted [initial surface contract](../project/standard-library-surface.md) defines the next
modules, host boundary, failure policy, and minimum API matrix.
