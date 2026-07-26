# Experimental ABI Review

Status: implemented representation audit for the native compiler target

This document records the runtime representations emitted by the current
compiler. It is evidence for the ABI milestone, not a source-language promise
or a frozen 1.0 ABI. The [native calling convention](native-calling-convention.md)
and [native linkage contract](native-linkage.md) build on this audit.

## Target Model

The compiler emits LLVM IR for the host target and leaves target triple and
data layout selection to the native LLVM/Clang toolchain. `usize` and `isize`
use the compiler host pointer width. The allocator and slice-length records
currently use `i64`, so the supported native target is 64-bit. Supporting a
32-bit or cross-compilation target requires an explicit target description
rather than host `usize::BITS`.

## Value Representations

| Salicin value | Current LLVM representation | Boundary status |
| --- | --- | --- |
| `i8`/`u8` through `i128`/`u128` | same-width LLVM integer | native and bounded C |
| `isize`/`usize` | host pointer-width LLVM integer | native and bounded C |
| `bool` | `i1` | native only |
| `()` | erased parameter, `void` result, `[0 x i8]` aggregate field | native only |
| `Never` | no first-class value; terminating path is `unreachable` | native control only |
| `Ptr(A)(T)` | opaque LLVM `ptr`; access and pointee are static | native and bounded C |
| `borrow` / `borrow(mut)` | opaque LLVM `ptr`; region and access are static | native only |
| `borrow(Slice(T))` | `{ ptr, i64 }` | native only, 64-bit target |
| `(A, B, ...)` | literal LLVM struct in source field order | native, experimental |
| `Array(T)(N)` | `[N x T]` | native; C only inside validated `struct(c)` |
| Salicin `struct` | named unpacked LLVM struct in declaration order | native, experimental |
| `struct(c)` | named unpacked LLVM struct after recursive C-field validation | C data boundary |
| Salicin `enum` | `{ i32 tag, all variant payload fields... }` | native, experimental |
| noncapturing function value | opaque LLVM `ptr` | native, experimental |
| concrete closure/partial | statically named struct of captures | native, compiler-private |
| `Continuation(I, O)` | `{ entry ptr, drop ptr, environment ptr, active-flag ptr }` | native, compiler-private |
| `EffectCallable(I, O, A)` | `{ entry ptr, drop ptr, environment ptr, active-flag ptr }` | native, compiler-private |

Salicin structs and enums deliberately have no C status. `struct(c)` is the
only aggregate admitted to the C data model, while the current foreign-call
surface accepts only integers, raw pointers, and `()` results.

## Function Boundaries

Runtime parameter groups are flattened in source order. Unit parameters and
borrows of unit are erased. Other `borrow` and `borrow(mut)` parameters pass one pointer; inferred,
`copy`, and `move` parameters pass the value representation directly.
Aggregate returns are direct LLVM aggregate returns.

Passing an owned value transfers cleanup responsibility to the callee.
Borrowed parameters remain caller-owned. A successful by-value return
transfers ownership to the caller. These rules are now the native calling
contract; exported definitions include their ownership modes in the linkage
fingerprint.

Effect rows have no standalone runtime argument in direct specialized calls.
`Unsafe` is static authority. Algebraic effects are specialized into
continuation-bearing control flow. `Throws(Error)` uses the corresponding
`Result(Error)(Output)` enum as its runtime return boundary.

## Module And Symbol Boundaries

The selected primary package's concrete `pub` functions and non-Unit globals
receive package-qualified external symbols with ABI fingerprints. Other
source and generated definitions retain deterministic compiler-private
`sali.*` symbols and internal linkage. The
[native linkage contract](native-linkage.md) defines declaration agreement,
generic specialization ownership, and collision behavior.

## C Boundary

`foreign(c)` and `foreign(c, "symbol")` declare one external C symbol and
implicitly require `Unsafe` at calls. A declaration must have exactly one
runtime parameter group, no compile-time runtime residue, no `Throws` or
custom effect row, integer or raw-pointer parameters, and an integer,
raw-pointer, or unit result. `bool`, borrows, slices, Salicin aggregates,
callables, continuations, and effect callables are rejected at the source
declaration.

`struct(c)` validates non-empty field lists recursively. Accepted fields are
integers, raw pointers, non-zero fixed arrays of accepted fields, and nested
concrete `struct(c)` instances. Cross-language conformance and the final
supported scalar/aggregate call surface belong to `ABI-C-1`.

## Audit Conclusions

- The compiler has one deterministic whole-program native representation for
  every currently emitted first-class type.
- Unsupported unsized and compile-time-only values fail before LLVM value
  emission.
- The 64-bit host-target assumption is explicit and must become a target
  descriptor before cross-compilation.
- Native call ownership participates in the separately emitted linkage
  contract.
- Concrete primary-package `pub` definitions have experimental native exports.
- The C surface is bounded and source-validated, but aggregate calls still
  require cross-language verification.
