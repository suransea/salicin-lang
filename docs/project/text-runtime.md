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
`U+10FFFF`. The first invalid byte offset is reported. Validation is linear
in byte length and performs no allocation.

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
byte vector and the first invalid offset. Allocation failure remains the
allocator trap defined by the standard-library surface. Recoverable invalid
UTF-8 never discards owned bytes.

## Iteration

Byte iteration yields copied `u8` values. Scalar iteration decodes forward and
yields copied `unicode_scalar` values. Both iterators retain a shared source
loan until they are destroyed. Safe iteration over a valid `str` has no
invalid-input branch; internal decode failure is an invariant violation and
traps. Early exit releases the loan exactly once.

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

- [Unicode 17, Chapter 3](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-3/)
- [Rust `str` boundary contract](https://doc.rust-lang.org/std/primitive.str.html)
- [Swift strings and characters](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/stringsandcharacters/)
- [Place Capability Graphs (OOPSLA 2025)](https://pm.inf.ethz.ch/publications/GrannanBilaFialaGeerMedeirosMuellerSummers25.pdf)
- [From Linearity to Borrowing (OOPSLA 2025)](https://johnm.li/from-linearity-to-borrowing.pdf)
- [SafeFFI (2025 preprint)](https://arxiv.org/abs/2510.20688)
