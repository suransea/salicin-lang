# Standard library

Salicin reserves three library layers and exposes the user-facing standard-library surface through
the `std` namespace:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` will contain host facilities such as files, processes, networking, and threading.

Source is organized by public module rather than accumulated in a release-oriented prelude file:

```text
library/
  core/src/
    root.sc
    prelude.sc
    ops.sc
    flow.sc
    effects.sc
    domains.sc
    control.sc
    iter.sc
    algebra.sc
    functional.sc
    option.sc
    result.sc
  alloc/src/
    boxed.sc
    vec.sc
```

## Prelude policy

The edition prelude must stay small. It is intended only for names that the language constantly
produces or needs as universal contracts, currently `Never`, `Copy`, and `Drop`. `Option` and
`Result` are fundamental `core` declarations, but ordinary source should name them through the
`std` facade:

```sc
use std.Option
use std.Result
```

Operator traits are imported from `std.ops`, `?.`/`??` protocols from `std.flow`, effect identities
from `std.effect`, compile-time domains from `std.domains`, compiler-lowered control contracts from
`std.control`, algebra protocols from `std.algebra`, higher-kinded functional protocols from
`std.functional`, iteration protocols from `std.iter`, and owning containers from `std.boxed` and
`std.vec`. The underlying implementation is still split across `core` and `alloc`: `core.option`
and `core.result` hold standard implementations for the root `Option` and `Result` types. Standard
declarations must be named through their module or imported explicitly with ordinary `use`; for
example:

```sc
use std.boxed.Box
use std.vec.{Vec, vec_at}
```

The compiler mounts `std` plus the lower-level `core` and `alloc` namespaces in every package.
Non-prelude declarations have qualified internal identities, so an unimported user declaration may
still be named `Add`, `Box`, or `Vec`. A project dependency or top-level file module cannot claim
any of these standard namespaces.
`std.ops` uses the same rule: `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `PartialOrdering`,
`PartialOrd`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`, and their `*Assign` mutation
traits require ordinary imports when
named. Merely writing the corresponding operator token does not require importing its protocol.
`std.flow.Chain` and `std.flow.Coalesce` require ordinary imports when named directly; the older
`std.ops.Chain` and `std.ops.Coalesce` paths remain accepted as compatibility aliases.
`Throws(E)`, `Unsafe`, and `Async` are ordinary standard effect declarations imported from
`std.effect`;
source that names them imports them normally. The control spellings `do`, `try`, `throw`, `unsafe`,
and `loop` bind directly to validated lang-item declarations in `core.control`; they do not inject
those module exports as ordinary unqualified names. The former control-container protocols have
been removed.
Effect identities use uppercase nominal spelling, including user-defined effects; row parameters
such as `E: effect` remain ordinary parameter names.
The `effect`, `access`, and `passing` compile-time domains use contextual names such as `pure`,
`shared`, `mut`, `auto`, `copy`, and `move` in parameter positions.
`Semigroup` and `Monoid` require `use std.algebra...` when named.
`Functor`, `Applicative`, and `Monad` require `use std.functional...` when named.
`Iterator` and `IntoIterator` require an ordinary `use std.iter...` when named in an implementation
or bound. Writing `for pattern in value { ... }` binds to their validated lang-item identities
without importing them and cannot be redirected by same-named inherent methods or traits.

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations. `std`, `core`, and `alloc` are reserved top-level namespaces, not manifest
dependencies.
