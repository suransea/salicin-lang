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
owning containers belong to `alloc.boxed` and `alloc.vec`. Those non-prelude modules are intended to
be imported explicitly with `use`.

The source tree and provenance metadata already follow this boundary. The current bootstrap analyzer
still injects all validated `core` and `alloc` declarations for compatibility, so explicit standard
package imports are not enforced yet. Mounting compiler-owned packages in normal module resolution,
then removing those compatibility aliases, is the remaining semantic migration; see
[implementation status](../project/status.md).

The compiler, library sources, and edition form one toolchain unit. Compiler-matched language items
must come from the matching `core`, while user declarations with the same spelling remain ordinary
declarations.
