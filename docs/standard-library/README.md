# Standard library

Salicin reserves three library layers:

- `core` contains allocation-free language protocols and fundamental types.
- `alloc` contains owning heap types and depends on the allocator ABI.
- `std` will contain host facilities such as files, processes, networking, and threading.

Source is organized by public module rather than accumulated in a release-oriented prelude file:

```text
library/
  core/src/
    prelude.sali
    ops.sali
    control.sali
  alloc/src/
    boxed.sali
    vec.sali
```

## Prelude policy

The edition prelude must stay small. It is intended for universally useful language-level names,
currently `Option`, `Result`, `never`, `Copy`, and `Drop`. Operator traits belong to `core.ops`,
error-control traits belong to `core.control`, and
owning containers belong to `alloc.boxed` and `alloc.vec`. Alloc declarations must be named through
their module or imported explicitly with ordinary `use`; for example:

```sali
use alloc.boxed.Box
use alloc.vec.{Vec, vec_at}
```

The compiler mounts `core` and `alloc` as reserved standard-library namespaces in every package.
Their non-prelude declarations have qualified internal identities, so an unimported user declaration
may still be named `Add`, `Box`, or `Vec`. A project dependency or top-level file module cannot claim
either standard namespace.
`core.ops` uses the same rule: `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `PartialOrdering`, and
`PartialOrd` require ordinary imports when
named. Merely writing the corresponding operator token does not require importing its protocol.
Likewise, `ControlFlow`, `Try`, `FromResidual`, and `FromError` require imports when named; `.try`
and `throw` do not inject those names into source scope.

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations. `core` and `alloc` are reserved top-level namespaces, not manifest dependencies.
