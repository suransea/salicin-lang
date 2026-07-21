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
  alloc/src/
    boxed.sali
    vec.sali
```

## Prelude policy

The edition prelude must stay small. It is intended for universally useful language-level names,
currently `Option`, `Result`, `never`, `Copy`, and `Drop`. Operator traits belong to `core.ops`;
owning containers belong to `alloc.boxed` and `alloc.vec`. Alloc declarations must be named through
their module or imported explicitly with ordinary `use`; for example:

```sali
use alloc.boxed.Box
use alloc.vec.{Vec, vec_at}
```

The compiler mounts `alloc` as a reserved standard-library namespace in every package. Its
declarations have qualified internal identities, so an unimported user declaration may still be
named `Box` or `Vec`. A project dependency or top-level file module cannot claim the name `alloc`.
`core.ops` still has a bootstrap visibility migration remaining before its traits require ordinary
imports; see [implementation status](../project/status.md).

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations.
