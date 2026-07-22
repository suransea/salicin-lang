# Standard library

Salicin reserves three library layers:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` will contain host facilities such as files, processes, networking, and threading.

Source is organized by public module rather than accumulated in a release-oriented prelude file:

```text
library/
  core/src/
    prelude.sc
    ops.sc
    effects.sc
    access.sc
    control.sc
    iter.sc
    algebra.sc
    functional.sc
  alloc/src/
    boxed.sc
    vec.sc
```

## Prelude policy

The edition prelude must stay small. It is intended for universally useful language-level names,
currently `Option`, `Result`, `Never`, `Copy`, and `Drop`. Operator traits belong to `core.ops`,
effect identities belong to `core.effects`, access identities belong to `core.access`,
compiler-lowered control contracts belong to `core.control`, algebra protocols belong to
`core.algebra`, higher-kinded functional protocols belong to `core.functional`, iteration protocols
belong to `core.iter`, and owning containers belong to `alloc.boxed` and `alloc.vec`. Alloc
declarations must be named through their module or imported
explicitly with ordinary `use`; for example:

```sali
use alloc.boxed.Box
use alloc.vec.{Vec, vec_at}
```

The compiler mounts `core` and `alloc` as reserved standard-library namespaces in every package.
Their non-prelude declarations have qualified internal identities, so an unimported user declaration
may still be named `Add`, `Box`, or `Vec`. A project dependency or top-level file module cannot claim
either standard namespace.
`core.ops` uses the same rule: `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `PartialOrdering`,
`PartialOrd`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`, and their `*Assign` mutation
traits require ordinary imports when
named. Merely writing the corresponding operator token does not require importing its protocol.
`Throws(E)`, `Unsafe`, and `Async` are ordinary standard effect declarations in `core.effects`;
source that names them imports them normally. The control spellings `do`, `try`, `unsafe`, and
`loop` bind directly to validated lang-item declarations in `core.control`; they do not inject those
module exports as ordinary unqualified names. The former control-container protocols have been
removed.
`Shared`/`Mutable` require `use core.access...` when named as standard-library declarations.
`Semigroup` and `Monoid` require `use core.algebra...` when named.
`Functor`, `Applicative`, and `Monad` require `use core.functional...` when named.
`Iterator` and `IntoIterator` require an ordinary `use core.iter...` when named in an implementation
or bound. Writing `for pattern in value { ... }` binds to their validated lang-item identities
without importing them and cannot be redirected by same-named inherent methods or traits.

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations. `core` and `alloc` are reserved top-level namespaces, not manifest dependencies.
