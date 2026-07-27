# Standard library

Salicin reserves three canonical library layers:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` owns policy-bearing or host-facing abstractions and does not mirror
  lower-layer modules.

The dependency order is `core ← alloc ← std`: `alloc` correctly depends on
`core`, while `core` never depends on allocation or host services.

Source is organized around canonical definition modules:

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
  alloc/src/
    lib.sc
    boxed.sc
    vec.sc
    string.sc
    raw.sc
  std/src/
    lib.sc
    async.sc
    algebra.sc
    functional.sc
```

## Prelude policy

The edition prelude must stay small. It contains the universal `never`, `copyable`, and `droppable`
contracts, primitive type names, and the `array`, `ptr`, `size_of`, and `align_of`
memory contracts that compiler-generated types and low-level library code routinely need.
`option` and `result` are fundamental `core` declarations:

```sc fragment
let option = core.option
let result = core.result
```

Operator traits are aliased from the `core.ops` facade, `?.`/`??` protocols from `core.flow`, generic
handler contracts from `core.effect`, typed failure from `core.error`, asynchronous computation from
`core.async`, unsafe authority from `core.unsafe`, compile-time sorts from `core.sorts`,
compiler-lowered control contracts from `core.control`, algebra protocols from
`std.algebra`, higher-kinded functional protocols from `std.functional`, iteration protocols from
`core.iter`, and owning containers from `alloc.boxed` and `alloc.vec`.
Declarations should be named through their canonical layer or given
transparent aliases with ordinary `let`; for example:

```sc fragment
let box = alloc.boxed.box
let vec = alloc.vec.vec
let string = alloc.string.string
```

The compiler validates and embeds the matching `library/std` source bundle
alongside the lower-level `core` and `alloc` namespaces. `std` contains only
its own declarations; it does not manufacture duplicate lower-layer paths.
No declaration can obtain compiler authority by copying a privileged name or
shape.
Unsupported hosts are rejected before semantic analysis. The initial
supported pairs are Linux/x86-64 and macOS/arm64.
Non-prelude declarations have qualified internal identities, so a user declaration without such an alias may
still be named `add`, `box`, or `vec`. A project dependency or top-level file module cannot claim
any of these standard namespaces.
`core.ops` uses the same rule: `add`, `sub`, `mul`, `div`, `rem`, `eq`, `partial_ordering`,
`partial_ord`, `neg`, `not`, `bit_and`, `bit_or`, `bit_xor`, `shl`, `shr`, and their `_assign` mutation
traits require ordinary aliases when
named. Merely writing the corresponding operator token does not require importing its protocol.
`core.flow.chain` and `core.flow.coalesce` require ordinary aliases when named directly.
`throwing(e)`, `unsafety`, and `suspension` are ordinary standard effect declarations in `core.error`,
`core.unsafe`, and `core.async`. Source that names them binds them normally. `try` and `throw` target
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
`iterator` and `into_iterator` require ordinary aliases from `core.iter` when named in an implementation
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
