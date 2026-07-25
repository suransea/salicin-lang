# Standard library

Salicin reserves three library layers and exposes the user-facing standard-library surface through
the `std` namespace:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` will contain host facilities such as files, processes, networking, and threading.

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
    cmp.sc
    flow.sc
    ops.sc
    ops/arith.sc
    ops/bit.sc
    ops/assign.sc
    effect.sc
    effect/handler.sc
    domains.sc
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
```

## Prelude policy

The edition prelude must stay small. It contains the universal `Never`, `Copy`, and `Drop`
contracts, primitive type names, and the `Array`, `Ptr`, `size_of`, and `align_of`
memory contracts that compiler-generated types and low-level library code routinely need.
`Option` and `Result` are fundamental `core` declarations, but ordinary source should name them
through the `std` facade:

```sc fragment
let Option = std.Option
let Result = std.Result
```

Operator traits are aliased from the `std.ops` facade, `?.`/`??` protocols from `std.flow`, effect
identities from `std.effect`, handler contracts from `std.effect.handler`, compile-time domains from
`std.domains`, compiler-lowered control contracts from `std.control`, algebra protocols from
`std.algebra`, higher-kinded functional protocols from `std.functional`, iteration protocols from
`std.iter`, and owning containers from `std.boxed` and `std.vec`. The underlying implementation is
still split across `core` and `alloc`: `core.option` and `core.result` define `Option` and `Result`,
while the `core` root re-exports the root public surface. Standard declarations must be named
through their module or given transparent aliases with ordinary `let`; for example:

```sc fragment
let Box = std.boxed.Box
let Vec = std.vec.Vec
let String = std.string.String
```

The compiler mounts `std` plus the lower-level `core` and `alloc` namespaces in every package.
Non-prelude declarations have qualified internal identities, so a user declaration without such an alias may
still be named `Add`, `Box`, or `Vec`. A project dependency or top-level file module cannot claim
any of these standard namespaces.
`std.ops` uses the same rule: `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `PartialOrdering`,
`PartialOrd`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`, and their `*Assign` mutation
traits require ordinary aliases when
named. Merely writing the corresponding operator token does not require importing its protocol.
`std.flow.Chain` and `std.flow.Coalesce` require ordinary aliases when named directly; the older
`std.ops.Chain` and `std.ops.Coalesce` paths remain accepted as compatibility aliases.
`Throws(E)`, `Unsafe`, and `Async` are ordinary standard effect declarations aliased from
`std.effect`;
source that names them binds them normally. The control spellings `do`, `try`, `throw`, `unsafe`,
and `loop` bind directly to validated lang-item declarations in `core.control`; they do not inject
those module exports as ordinary unqualified names. The former control-container protocols have
been removed.
Effect identities use uppercase nominal spelling, including user-defined effects; row parameters
such as `E: effect` remain ordinary parameter names.
The `effect` compile-time domain, closed `access` type, and parameter modifier functions use
contextual names such as `pure`, `shared`, `mut`, `copy`, and `move` in parameter positions.
`Semigroup` and `Monoid` require aliases from `std.algebra` when named.
`Functor`, `Applicative`, and `Monad` require aliases from `std.functional` when named.
`Iterator` and `IntoIterator` require ordinary aliases from `std.iter` when named in an implementation
or bound. Writing `for value { pattern -> ... }` binds to their validated lang-item identities
without aliasing them and cannot be redirected by same-named inherent methods or traits.

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations. `std`, `core`, and `alloc` are reserved top-level namespaces, not manifest
dependencies.

The complete [`examples/inventory`](../../examples/inventory) package demonstrates the LIB1
surface across file modules: validated owning strings, recoverable byte conversion, a vector of
non-`Copy` values, consuming iteration, and user trait dispatch.
