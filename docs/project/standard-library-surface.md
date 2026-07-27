# Initial Standard-Library Surface

Status: accepted for the 2026 edition  
Accepted: 2026-07-27

This contract fixes the first usable `core`, `alloc`, and host `std` surface.
It is an implementation target, not a claim that every declaration below
already exists. The [TODO](todo.md) owns the remaining implementation order.

## Design rules

The surface follows six rules.

1. Every source identifier uses `snake_case`, including types, traits,
   variants, parameters, functions, values, modules, effects, and sorts.
2. The prelude contains only names needed pervasively by ordinary syntax.
   Allocation, failure, formatting, collections, and host access stay
   qualified or use explicit local aliases.
3. `core` needs neither allocation nor a host. `alloc` may allocate but may
   not access a host. `std` is the only layer that may expose host services.
4. Safe APIs preserve ownership, initialization, UTF-8, and borrow
   invariants. An unchecked operation requires `unsafe_effect`; it is not
   made safe merely by living in the standard library.
5. `io` is visible host authority, not an error-transport mechanism. Host
   failures are values returned in `result(io_error)(t)`.
6. Each ordinary operation has one canonical name. Overloads may share a name
   only when they have the same semantics and are unambiguous from their
   inputs; return-type-only overloads are forbidden.

Names prioritize clarity at the call site. Boolean queries use `is_` or
`has_`; mutating operations use imperative verbs; consuming transformations
use `into_`; borrowed projections use `as_`; checked conversions use
`to_` or `try_`; unchecked operations end in `_unchecked`.

## Library layers and modules

### `core`

`core` is available to every target accepted by the compiler. It contains no
allocator or host symbol.

| Public module | Responsibility |
| --- | --- |
| `core.primitives` | `bool`, fixed-width integers, `isize`, and `usize` |
| `core.never` | `never` |
| `core.marker` | `movable`, `copyable`, and `droppable` |
| `core.sorts` | compiler-owned static classifiers |
| `core.passing` | `copy`, `move`, and `comptime` parameter modifiers |
| `core.borrow` | `access`, `shared`, `mut`, and `borrow` |
| `core.memory` | `array`, `slice`, `ptr`, layout queries, and safe contiguous access |
| `core.option` | `option` and its source-backed operations |
| `core.result` | `result` and its source-backed operations |
| `core.cmp` | equality and partial-ordering protocols |
| `core.ops` | arithmetic, bit, assignment, and indexing protocols |
| `core.flow` | chaining, fallback, unwrap, and typed raising protocols |
| `core.iter` | iterator protocols and allocation-free algorithms |
| `core.convert` | checked scalar and numeric conversion protocols |
| `core.text` | borrowed UTF-8 `str`, `unicode_scalar`, validation, and iteration |
| `core.fmt` | allocation-free parse, display, debug, and writer protocols |
| `core.effect` | effect-handler machinery |
| `core.error` | typed failure effect machinery |
| `core.control` | compiler-lowered structured control contracts |
| `core.unsafe` | `unsafe_effect` and its handler boundary |
| `core.async` | the accepted cold-future surface |
| `core.foreign` | `abi` and the `foreign` initializer contract |
| `core.algebra` | algebraic protocols |
| `core.functional` | higher-kinded functional protocols |

`core.lib` is a small root facade, not a second home for every declaration.
Definition modules own canonical identities. Compatibility aliases are not
added when a declaration moves.

### `alloc`

`alloc` depends only on `core` and the replaceable allocator ABI.

| Public module | Responsibility |
| --- | --- |
| `alloc.boxed` | the owning `box(t)` allocation |
| `alloc.vec` | `vec(t)` and consuming vector iteration |
| `alloc.string` | owning UTF-8 `string` and ownership-preserving conversion errors |
| `alloc.fmt` | `string_writer` and allocation-backed formatting helpers |

`alloc.raw` remains package-private. Safe source cannot call the allocator or
forge container metadata. Allocation failure and invalid allocation layout
remain process traps until the allocator ABI can represent recoverable
failure without weakening existing ownership guarantees.

### `std`

`std` is an edition-matched source bundle above `core` and `alloc`. It
re-exports lower-layer declarations through focused modules and adds host
facilities.

| Public module | Responsibility |
| --- | --- |
| `std.option`, `std.result` | fundamental value helpers |
| `std.cmp`, `std.ops`, `std.flow`, `std.iter` | protocols and common algorithms |
| `std.convert` | numeric and text conversions |
| `std.text` | borrowed text and Unicode scalars |
| `std.string` | owning strings and string formatting |
| `std.array`, `std.slice`, `std.vec`, `std.boxed` | collection and owner surfaces |
| `std.fmt` | parsing, formatting, writers, and builders |
| `std.error`, `std.effect`, `std.unsafe`, `std.async` | explicit effect surfaces |
| `std.io` | byte readers/writers, standard streams, and `io_error` |
| `std.process` | process arguments and exit information |
| `std.fs` | paths, file options, owned files, and bounded convenience operations |
| `std.test` | failure values and assertion helpers |

The compiler embeds the edition-matched `library/std` sources and derives the
mounted facade from their public aliases. The initial bundle is deliberately
alias-only: each target must resolve to an already validated `core` or `alloc`
identity, so the alias gains no compiler or runtime authority of its own.
Definitions, private aliases, foreign targets, duplicate exports, and
unsupported native targets are rejected. A user module or dependency cannot
claim `core`, `alloc`, or `std`.

## Prelude

The 2026 prelude contains exactly:

- `never`, `movable`, `copyable`, and `droppable`;
- `bool`, the fixed-width integers, `isize`, and `usize`;
- `array`, `ptr`, `size_of`, and `align_of`;
- `copy`, `move`, and `comptime`;
- `shared` and `mut`.

`borrow` remains contextual syntax and its qualified declaration remains
available. `slice`, `str`, `unicode_scalar`, `option`, `result`, `box`, `vec`,
`string`, operator traits, iterator traits, formatting traits, error types,
effects, I/O, and assertions are excluded.

Compiler-recognized syntax may resolve a validated language item without
making its spelling an unqualified user name. In particular, operator tokens,
`?.`, `??`, structured control, `foreign(c)`, and `test("name")` do not expand
the prelude.

## Ownership and borrowing

Public APIs use these modes consistently:

| Intent | Receiver or parameter | Result |
| --- | --- | --- |
| inspect a value | `borrow(t)` | copied scalar or a borrow tied to the receiver |
| mutate in place | `borrow(mut)(t)` | `()` or a borrow tied to the exclusive receiver |
| transfer ownership | `move value: t` | a new owner or ownership-preserving error |
| accept cheap reusable input | automatic passing, with an explicit `copyable` bound when required | never silently consumes a non-copy value |
| expose immutable contiguous data | `slice(t)` or `str` | shared borrow only |
| expose mutable contiguous data | `slice(mut)(t)` | exclusive borrow; never for UTF-8 bytes |
| create a resource | host operation `with(io)` | `result(io_error)(owner)` |
| operate on a resource | borrow the owner `with(io)` | result value; no hidden ownership transfer |
| close a resource | `move` the owner `with(io)` | `result(io_error)(())` |

Borrowed views retain the source loan. An iterator yielding borrowed elements
cannot outlive that loan. Mutable iteration keeps one exclusive source loan
and cannot yield overlapping live element loans. Safe text APIs never expose
mutable UTF-8 bytes.

A consuming conversion that can fail returns the original owner in its error
when doing so is necessary to avoid data loss. `string.from_utf8(move bytes)`
therefore returns a `from_utf8_error` that owns the rejected `vec(u8)`.

Resource destruction is deterministic. `file.close(move self)` attempts one
close, consumes the logical handle even on error, and reports the error.
`droppable.drop` also attempts close exactly once but cannot report failure;
programs that need the error must call `close` explicitly.

## Absence, failure, effects, and traps

The return form is selected by who can reasonably prevent or recover from the
condition.

| Form | Use |
| --- | --- |
| plain value | total operation for all valid inputs |
| `option(t)` | ordinary absence with no useful error detail, such as `get`, `first`, `last`, `find`, or `pop` |
| `result(e)(t)` | malformed external data, checked conversion, allocation-independent parsing, or recoverable host failure |
| `with(effects)` | observable capability or control effect; never a substitute for a recoverable error value |
| trap | violated checked precondition, impossible safe invariant, fixed arithmetic trap, invalid allocation layout, or allocation failure |

Every trapping collection operation has a nearby checked alternative:
`get(index)` returns `option(borrow(t))`, while `at(index)` and indexing trap
when out of bounds. A range operation validates the complete range before
forming a borrow or mutating storage.

UTF-8 validation, integer parsing, narrowing conversion, file open, read,
write, flush, seek, and explicit close do not trap for input or host errors.
`unwrap` and `at` deliberately trap and must document that fact. The standard
library does not add a general catchable panic mechanism in this milestone.

Callback-taking operations forward the callback's effect row exactly and
evaluate each input once. `io` and `unsafe_effect` are distinct: safe host
operations require `io` but do not silently acquire `unsafe_effect`; raw
pointer or unchecked representation operations require `unsafe_effect`
whether or not they also perform I/O.

## Error families

Errors are small, inspectable values with no mandatory allocation.

| Error | Minimum information |
| --- | --- |
| `utf8_error` | first invalid byte index and, when known, expected sequence length |
| `parse_int_error` | `empty`, `invalid_digit`, `invalid_sign`, or `overflow`, plus the failing byte index when applicable |
| `int_conversion_error` | source value was outside the destination range |
| `io_error` | portable `io_error_kind` and optional signed raw host code |
| `test_failure` | optional owned message plus source registration identity supplied by the runner |

`io_error_kind` initially includes `not_found`, `permission_denied`,
`already_exists`, `invalid_input`, `invalid_data`, `interrupted`,
`would_block`, `write_zero`, `unexpected_eof`, `broken_pipe`, `unsupported`,
`out_of_memory`, and `other`. Platform-specific codes remain observable but
must not change portable control flow.

Low-level `read` and `write` expose partial progress. A successful zero-byte
read means EOF when the requested buffer is non-empty. A successful
zero-byte write for non-empty input becomes `write_zero` in `write_all`.
`read_exact` reports `unexpected_eof`. High-level retrying helpers retry
`interrupted`; primitive operations preserve it. Text readers validate UTF-8
and return `invalid_data` rather than replacement text.

## Host authority

`io` is a compiler-validated standard effect identity. Only the native entry
boundary may install its host handler. Naming a user effect `io`, declaring a
same-shaped operation, or forging a file value grants no authority.

The entry function may be pure or declare `with(io)`. The native launcher
handles only the validated standard `io` identity and converts an unhandled
entry failure into a deterministic nonzero exit. Library functions remain
effect-polymorphic where possible and acquire no host authority by import.

Standard streams and process arguments are link-time capabilities of the
entry environment. Open files are owned runtime capabilities: opening
requires `io`, and the resulting unforgeable `file` limits subsequent
operations to that resource. Paths never imply ambient access by themselves.

The first host implementation supports:

- `x86_64-unknown-linux-gnu`, continuously tested in CI;
- `aarch64-apple-darwin`, verified by native release testing.

The compiler is currently 64-bit-native-only. `library/std` must be rejected
with a target-specific diagnostic on Windows, WASI, 32-bit, and other targets
until that target has a runtime implementation and conformance suite.
`core` checking remains independent of host availability; `alloc` additionally
requires its allocator ABI.

## Minimum API matrix

Names below are the minimum acceptance surface. Inherent operations keep
canonical identity in their definition module even when reached through a
`std` facade.

| Area | Required surface |
| --- | --- |
| `option(t)` | `is_some`, `is_none`, `as_ref`, `as_ref(mut)`, `map`, `and_then`, `unwrap_or`, `unwrap_or_else`, `ok_or` |
| `result(e)(t)` | `is_ok`, `is_err`, `as_ref`, `as_ref(mut)`, `map`, `map_error`, `and_then`, `unwrap_or`, `unwrap_or_else`, `ok`, `err` |
| integers | `min`, `max`, `clamp`, sign queries, checked width conversions, decimal parse, decimal display |
| `str` | byte `len`, `is_empty`, `as_bytes`, equality, boundary check, checked slice, prefix/suffix, find, byte iteration, scalar iteration |
| `unicode_scalar` | checked construction from `u32`, `to_u32`, UTF-8 encoded length, encode into caller storage |
| `string` | `new`, capacity construction, `from_str`, `from_utf8`, `as_str`, `push`, `push_str`, truncate at boundary, search, clear, byte recovery |
| `array(t)(n)` | `len`, `is_empty`, `get`, `at`, `first`, `last`, shared/mutable slice, shared/mutable iteration, swap, reverse, copy/fill where bounded |
| `slice(t)` | the same non-owning access and iteration vocabulary as arrays, plus checked subslicing |
| `vec(t)` | array/slice vocabulary where applicable, capacity, push/pop, insert/remove, append, truncate, extend from slice, consuming iteration |
| iteration | `find`, `position`, `contains`, `any`, `all`, and `fold`, with early-exit cleanup and forwarded effects |
| formatting | `parse`, `display`, `debug`, byte/text `writer`, `string_writer`, and allocation-backed `to_string` |
| byte I/O | `reader.read`, `reader.read_exact`, `writer.write`, `writer.write_all`, and `writer.flush` |
| console | stdin read/read_line, stdout/stderr write/print/println, and explicit flush |
| process | borrowed or owned argument iteration with defined invalid-host-text behavior |
| filesystem | `open_options`, `file.open`, `file.read`, `file.write`, `file.flush`, `file.seek`, `file.close`, and bounded whole-file helpers |
| tests | `fail`, `assert`, `assert_eq`, `assert_ne`, and common `option`/`result` expectations |

Whole-file and read-to-end helpers take an explicit maximum byte count. They
must not allocate without a caller-visible bound. `print` and assertion
formatting use source-backed static dispatch; this milestone adds neither
reflection nor formatting macros.

## Conformance

Each public operation needs:

- a declaration-level summary, complexity, ownership mode, effects, errors,
  and traps;
- positive native coverage and rejection coverage for invalid ownership,
  borrowing, effects, or target use;
- exact boundary tests for empty input, maximum integer widths, UTF-8
  boundaries, partial I/O, interruption, early exit, and cleanup;
- deterministic output and diagnostics independent of checkout path;
- no allocator leaks or double cleanup in success, error, and early-exit paths.

The milestone is complete only when the multi-module acceptance example in
`STD-3` uses this public surface without private helpers.

## Research basis

This contract combines Salicin's typed effect rows with capability-oriented
host resources. WASI distinguishes link-time function capabilities from
unforgeable runtime handles; Salicin similarly treats the entry environment
as the source of `io` authority and each opened file as a narrower owned
capability. Recent effect research compares row, capability, and modal
systems rather than requiring one representation to serve all three roles.

The byte-I/O rules follow the established separation between partial
`read`/`write` primitives and retrying exact/all helpers. Text follows the
Unicode definition of UTF-8 as one-to-four bytes per Unicode scalar value and
does not promise grapheme, normalization, locale, or collation behavior.
Naming follows clarity-at-use-site guidance while retaining Salicin's
language-wide `snake_case`.

Primary references reviewed on 2026-07-27:

- [WASI capabilities](https://github.com/WebAssembly/WASI/blob/main/docs/Capabilities.md)
- [WASI repository and Preview 2 status](https://github.com/WebAssembly/WASI)
- [Rust `std::io::Write`](https://doc.rust-lang.org/std/io/trait.Write.html)
- [Unicode 17, Chapter 3](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-3/)
- [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/)
- [Rows and Capabilities as Modal Effects (HOPE 2025)](https://conf.researchr.org/details/icfp-splash-2025/hope-2025-papers/5/Rows-and-Capabilities-as-Modal-Effects-Extended-Abstract)
- [Zero-Overhead Lexical Effect Handlers (OOPSLA 2025)](https://cs.uwaterloo.ca/~yizhou/papers/zero-oopsla2025.pdf)
