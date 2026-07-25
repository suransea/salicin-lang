# Minimum owning string design

Status: implemented by LIB-STRING-1

## Decision

The first owning text type is `alloc.string.String`. It is a nominal wrapper around `Vec(u8)` whose
initialized bytes are always well-formed UTF-8. The representation stays private. Length and
capacity are measured in bytes, matching the underlying allocation and making their cost explicit.

`String` is distinct from arbitrary bytes. Safe code may inspect its storage through a shared
`Slice(u8)` borrow and may consume it into `Vec(u8)`, but it cannot obtain a mutable byte slice.
Operations that mutate a `String` must preserve UTF-8 by construction.

The minimum public surface is:

```sc fragment
let String = std.string.String
let FromUtf8Error = std.string.FromUtf8Error

String.new()
String.with_capacity(byte_capacity)
String.from_utf8(bytes)
unsafe { String.from_utf8_unchecked(bytes) }

text.len_bytes()
text.capacity()
text.is_empty()
text.as_bytes()
text.reserve(additional_bytes)
text.clear()
text.append(other)
text.into_bytes()
```

`from_utf8` consumes a `Vec(u8)` and returns
`Result(FromUtf8Error)(String)`. On failure, `FromUtf8Error` owns the original vector and records the
first invalid byte position through `valid_up_to()`. `into_bytes()` recovers that vector. This
keeps validation recoverable without copying or losing the caller's allocation.
`from_utf8_unchecked` is the only initial unsafe escape hatch; violating its precondition is a
library contract violation.

`append` moves all bytes from another `String`, leaves the source empty, and preserves UTF-8 because
concatenating two well-formed UTF-8 sequences is well formed. `clear` and capacity-only operations
also preserve the invariant. Allocation failure, capacity overflow, and invalid allocator layouts
follow `Vec`: they trap and do not implicitly widen an effect row. Destruction delegates to the
owned vector and releases storage exactly once.

## Boundaries

- `Slice(u8)` is a byte view, not a borrowed UTF-8 type. A future `Str` design may add a
  validity-carrying unsized view when APIs demonstrate that pressure.
- There is no character scalar type yet. The minimum API therefore does not claim code-point,
  grapheme, case-conversion, or normalization operations.
- Indexing a `String` is not supported. Byte indexing would weaken the text abstraction and
  character indexing would have non-constant cost and unclear result semantics.
- General string literal expressions are not part of this task. The lexer currently accepts quoted
  strings only in ABI and attribute positions; literal syntax needs a separate frontend and static
  storage decision.
- Formatting, parsing, locale behavior, Unicode tables, and C-string conversion are separate
  library or ABI designs.

## Implementation evidence

`LIB-STRING-1` implements the representation and API above in source and exports it through
`alloc.lib` and `std.string`. Its tests cover:

1. empty construction, capacity, shared byte inspection, clearing, and consuming byte recovery;
2. valid one-, two-, three-, and four-byte UTF-8 sequences plus invalid leading, continuation,
   truncated, overlong, surrogate, and out-of-range sequences;
3. error ownership and exact first-invalid-byte reporting without duplicate cleanup;
4. append ordering, source invalidation/emptying, reallocation, and exactly-once cleanup;
5. rejection of private-field construction, mutable byte access, use after consuming conversion,
   and safe calls to the unchecked constructor.

String literals, `Str`, and character/Unicode APIs may enter the executable queue only after this
gate passes and a concrete consumer establishes their required semantics.
