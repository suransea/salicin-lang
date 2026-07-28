# Runtime Text Contract

Status: accepted implementation contract  
Accepted: 2026-07-28

This contract defines the first UTF-8 runtime text model for the 2026 edition.
It refines TEXT-1 through TEXT-3 in the executable TODO. The standard-library
surface remains the authority for module placement and minimum API names.

## Values and invariants

`unicode_scalar` is a copyable nominal value containing exactly one Unicode
scalar value. Safe construction accepts `0..U+D7FF` and `U+E000..U+10FFFF`;
it rejects the surrogate range and values above `U+10FFFF`. Noncharacters and
currently unassigned code points remain valid scalar values. Its canonical
UTF-8 encoding is one to four bytes.

`str` is an immutable dynamically sized UTF-8 view. A value is usable only
through `borrow(str)`. Its representation is a data address and a byte length,
but safe source cannot construct either field, detach the view from its source
loan, mutate its bytes, or observe a trailing sentinel. The empty view may use
any aligned non-null dangling address because no byte is accessed.

`string` is the existing owning UTF-8 value. Its data address and length cover
exactly the initialized UTF-8 bytes. A zero capacity denotes immutable static
literal storage; non-zero capacity denotes allocator-owned storage and is at
least the length. Empty values may use the allocator's accepted zero-size
address. Safe mutation preserves UTF-8 and initialized-storage invariants.

## Literals and escapes

A source string literal is decoded by the lexer, encoded as UTF-8, and lowered
to a private, unnamed, mergeable, immutable global byte array. Equal decoded
byte sequences within one compilation share storage. Literal storage has
program lifetime and no destructor. Its address is never used for language
identity or equality.

The 2026 literal escape set is `\\`, `\"`, `\n`, `\r`, and `\t`. Raw source
newlines and unknown escapes are diagnostics. The literal length passed to
`core.literal.string_literal.from_string_literal` is the decoded UTF-8 byte
length, not source-token length or scalar count.

A literal defaulting to `string` contains the global address, decoded byte
length, and zero capacity without allocation. A target-typed byte array
receives the exact bytes. A target-typed slice or later `str` view borrows
compiler-owned literal storage for the enclosing use and cannot expose mutable
access. Compiler metadata strings such as test names and foreign symbols do
not instantiate a runtime value or emit storage unless also used as ordinary
runtime literals.

## Borrowed views and regions

`string.as_str` returns a shared `borrow(r)(str)` tied to the receiver borrow
region `r`. Checked UTF-8 conversion from `borrow(r)(slice(u8))` returns a
view with the same region. A successful subview is tied to its source view's
region. No safe conversion from a mutable byte slice yields a mutable text
view; callers retain the byte loan while the shared view exists.

The implemented `str.from_utf8` conversion validates before entering the
unsafe representation boundary. The internal `raw_str`/`raw_str_bytes` casts
change only the view invariant and pointee type: they preserve the original
fat pointer, region, reference origin, and source loan. Composite values such
as `option(borrow(r)(str))` retain that origin through construction and match
payload binding.

`str.as_bytes` returns a shared byte slice with the same region. It never
returns mutable bytes. Borrow checking, rather than runtime ownership flags,
prevents mutation or deallocation while a view or iterator is live.

## UTF-8 validation and boundaries

Validation accepts only the well-formed sequences in Unicode 17 Table 3-7. It
rejects isolated continuation bytes, invalid leading bytes, truncated
sequences, overlong encodings, surrogate encodings, and values above
`U+10FFFF`. Errors report the valid-prefix length, or equivalently the leading
byte of the first ill-formed subsequence. Validation is linear in byte length
and performs no allocation.

Byte offsets `0` and `len` are boundaries. An interior offset is a boundary
exactly when its byte is not a UTF-8 continuation byte. An offset above `len`
is never a boundary. Checked slicing validates both bounds and both boundaries
before forming a view; failure returns `none` and forms no borrow.

## Equality, conversion, and failure

Text equality compares byte lengths and then bytes. This is exact Unicode
scalar-sequence equality; it does not normalize, case-fold, collate, or compare
grapheme clusters. `string` and `str` comparisons use the same rule.

Borrowed byte-to-text conversion returns `option(borrow(r)(str))`. Owned
byte-to-string conversion returns a result whose error retains the original
byte vector and the valid-prefix length, which is also the leading-byte offset
of the first ill-formed subsequence. Allocation failure remains the allocator
trap defined by the standard-library surface. Recoverable invalid UTF-8 never
discards owned bytes.

The implemented owned conversion reports `valid_up_to`, the byte length of
the valid prefix and the leading-byte offset of an ill-formed or truncated
subsequence. Success transfers non-empty vector storage into `string`; failure
stores the unchanged vector in `from_utf8_error`. Consuming conversion back to
bytes transfers heap storage and copies immutable static literal storage.

## Construction and mutation

`string.new` and zero-capacity construction reuse the immutable empty literal;
positive capacity allocates owned uninitialized tail storage. Construction
from `str` copies exactly its validated bytes, while construction and `push`
from `unicode_scalar` write only the scalar's canonical UTF-8 encoding.
`push_str` accepts only a validated shared view. A static literal detaches into
uniquely owned storage before its first append.

`reserve` checks byte-length overflow before allocation. `truncate` succeeds
only at an in-bounds UTF-8 boundary and otherwise returns `false` without
mutation. `clear` and successful truncation retain capacity. No safe operation
exposes the uninitialized tail or a mutable byte view.

## Ordering and search

`str` and `string` ordering is lexicographic over their canonical UTF-8 bytes,
which preserves Unicode scalar-value order. Prefix, suffix, containment, and
first-match search operate on exact bytes without normalization or locale
rules. Search reports the first UTF-8 byte offset, returns zero for an empty
needle, and only considers scalar boundaries. `string.substring` applies the
same checked endpoint contract as `str.get` before copying a new owner.

## Iteration

Byte iteration yields copied `u8` values. Scalar iteration decodes forward and
yields copied `unicode_scalar` values. Both iterators retain a shared source
loan until they are destroyed. Safe iteration over a valid `str` has no
invalid-input branch; internal decode failure is an invariant violation and
traps. Early exit releases the loan exactly once.
The implemented `str.bytes` and `str.scalars` iterators store the source
borrow directly. `scalar_count` and `scalar_at` consume the same scalar
iterator contract, so lookup is by scalar index while all slicing and search
positions remain byte offsets.

## Non-goals

This milestone does not add normalization, case conversion, collation,
grapheme segmentation, locale behavior, regex, a character literal syntax,
implicit allocation, formatting interpolation, or mutable UTF-8 byte access.

## Basis

The scalar domain and validation table follow Unicode 17 definitions D76,
D85, and D92. The checked byte-boundary rule matches the established UTF-8
slice rule that the start and end are boundaries and an offset beyond the end
is not. Swift's separate UTF-8 and Unicode-scalar views reinforce keeping byte
and scalar iteration explicit rather than treating user-perceived characters
as fixed-width values. The 2025 Place Capability Graph model makes stored
borrows and their place/capability relationships explicit; Salicin therefore
propagates the byte-source loan through the `slice`/`str` view cast and through
an `option` payload rather than treating validation as a new ownership origin.
The 2025 *From Linearity to Borrowing* calculus derives borrowing as a
temporary restriction of owner permissions; correspondingly, a live `str`
view blocks mutation of its underlying byte place and cannot escape a local
owner.
The 2025 SafeFFI work similarly identifies conversion from unchecked raw
pointers into safe typed pointers as the point where dynamic checks should be
concentrated, after which the type system propagates spatial and temporal
guarantees. Salicin therefore checks range and UTF-8 endpoints in `str.get`
before crossing one small unsafe `raw_subview` boundary; the resulting view
then carries the source loan without repeating those checks at each access.
The 2025 work on linear effects, exceptions, and destructors proves
resource-safety properties in which allocations are released exactly once
even on error paths. The owned conversion mirrors that discipline directly:
the input allocation moves into exactly one `result` payload, either the
successful `string` or the recoverable error, and normal drop glue handles
whichever branch the caller abandons.
The 2026 revision of *Typestate via Revocable Capabilities* shows how a
flow-sensitive capability may authorize a state transition while preventing
aliases from observing an invalid intermediate state. Salicin applies the
same boundary at a smaller scale: a unique mutable `string` borrow can reserve
and initialize storage, but the safe surface accepts only already-valid `str`
or `unicode_scalar` inputs and returns with the UTF-8 typestate restored.
The PLDI 2026 work on verification modulo tested library contracts treats
small modular method contracts, checked against tests, as the bridge for
reasoning about clients of complex libraries. Salicin therefore defines
ordering, matching, and search once over `str`; owning `string` methods borrow
that view, and fixtures test boundary, empty-needle, and delegation contracts
independently.
Pure Borrow (PLDI 2026) demonstrates that non-local borrowers may be split and
dropped without runtime communication back to the lender. Salicin's text
iterators follow that discipline: each iterator owns one shared source borrow,
yields only copied values, and releases the loan through ordinary lexical
lifetime tracking even on early exit.

- [Unicode 17, Chapter 3](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-3/)
- [Rust `str` boundary contract](https://doc.rust-lang.org/std/primitive.str.html)
- [Swift strings and characters](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/stringsandcharacters/)
- [Place Capability Graphs (OOPSLA 2025)](https://pm.inf.ethz.ch/publications/GrannanBilaFialaGeerMedeirosMuellerSummers25.pdf)
- [From Linearity to Borrowing (OOPSLA 2025)](https://johnm.li/from-linearity-to-borrowing.pdf)
- [SafeFFI (2025 preprint)](https://arxiv.org/abs/2510.20688)
- [Linear effects, exceptions, and resource safety (2025 preprint)](https://arxiv.org/abs/2510.23517)
- [Typestate via Revocable Capabilities (2026 revision)](https://arxiv.org/abs/2510.08889)
- [Verification Modulo Tested Library Contracts (PLDI 2026)](https://doi.org/10.1145/3808305)
- [Pure Borrow (PLDI 2026)](https://doi.org/10.1145/3808259)
