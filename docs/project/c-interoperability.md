# C Interoperability

Status: implemented and verified bounded C ABI

Salicin exposes C-owned functions through `foreign(c, ...)` and C-compatible
data through `struct(c)`. These forms share the native Clang target selected
by the compiler driver, but they are intentionally separate: a type having C
layout does not imply that it may be passed by value through a foreign call.

## Target

The compiler currently supports only a native 64-bit host target. It emits
LLVM IR without a cross-target description and delegates the target triple,
data layout, object format, and system linker to the host Clang installation.
Consequently, C layout and calls are defined against that same Clang target.

Cross-compilation and 32-bit targets require an explicit compiler target model
and are not inferred from these rules.

## Scalar Mapping

The following foreign parameter and result mappings are accepted:

| Salicin | C spelling used by conformance tests |
| --- | --- |
| `i8`, `i16`, `i32`, `i64` | `int8_t`, `int16_t`, `int32_t`, `int64_t` |
| `u8`, `u16`, `u32`, `u64` | `uint8_t`, `uint16_t`, `uint32_t`, `uint64_t` |
| `i128`, `u128` | Clang `__int128`, `unsigned __int128` extension |
| `isize`, `usize` | `intptr_t`, `uintptr_t` |
| `Ptr(A)(T)` | a C object pointer with compatible pointee use |
| result `()` | C `void` |

`i128` and `u128` are supported because the current boundary is explicitly a
Clang ABI, not portable ISO C source. Parameters of type `()` are rejected;
an empty Salicin parameter group represents a C `(void)` parameter list.

`bool` is not accepted. Correct C `_Bool` calls require target ABI extension
attributes that the current foreign lowering does not yet model.

## Foreign Signatures

A foreign declaration:

- has exactly one runtime parameter group;
- has no compile-time parameter group;
- uses only inferred parameter modes;
- has an explicit result;
- has no explicit `Throws` or custom effect;
- implicitly requires `Unsafe` at every call;
- uses `foreign(c)` for the local declaration name or
  `foreign(c, "symbol")` for an explicit validated ASCII C symbol.

Variadic functions are not supported. Duplicate C link names and names
reserved for the Salicin runtime are rejected before LLVM emission.

Arrays, tuples, enums, Salicin structs, `struct(c)` values, borrows, slices,
function values, closures, continuations, and effect callables are rejected
as by-value foreign parameters and results.

C array parameters decay to pointers and must therefore be declared as
`Ptr(T)` or `Ptr(mut)(T)`. C aggregates must likewise cross the current
function boundary behind a raw pointer. Typed C function pointers are not yet
part of the foreign surface; an opaque address may be stored in `Ptr`, but
Salicin does not infer a callable C signature from it.

## C Data Layout

`struct(c)` admits a non-empty field list containing only:

- signed or unsigned Salicin integers;
- raw pointers;
- non-zero fixed arrays whose element is recursively valid;
- concrete nested `struct(c)` values.

Field order is source order. The host LLVM data layout determines padding,
size, and alignment exactly as for the corresponding C declaration. Generic
`struct(c)` instances are checked after compile-time substitution, so an
invalid concrete field type is rejected at its instantiation.

`bool`, Unit, Never, borrows, slices, tuples, enums, ordinary Salicin structs,
callables, and zero-length arrays are not valid fields. Salicin does not
provide packed C structs, unions, bit-fields, flexible arrays, explicit
alignment overrides, or C enums.

## Why Aggregates Are Pointer-Only

Target C ABIs do not pass aggregates using their in-memory LLVM struct type in
all cases. For example, the current AArch64 Darwin Clang ABI coerces some
small records to integer arrays and lowers larger returns through `sret`.
Salicin therefore rejects by-value aggregate signatures until it has an
explicit target ABI classifier. Passing a `struct(c)` behind `Ptr` preserves
the verified data layout without pretending that ordinary Salicin aggregate
calling convention matches C.

## Verification

Native regression tests compile C and Salicin separately with the same host
Clang and link them together. They verify:

- every signed, unsigned, pointer-sized, and 128-bit integer width in both
  parameter and return position;
- raw mutable pointers through `memset`;
- `sizeof` and alignment agreement for a nested `struct(c)`;
- C reads from a Salicin-created record containing integers, a raw pointer,
  a fixed array, and another `struct(c)`;
- C writes through a record pointer followed by Salicin field reads;
- source diagnostics for every unsupported signature and field category.
