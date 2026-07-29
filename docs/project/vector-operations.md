# Vector Operations Contract

Status: implemented for the 2026 edition<br>
Accepted: 2026-07-29

This contract completes the common checked-access and slice-copy surface for
`alloc.vec.vec(t)`.

## Operations and bounds

Every vector exposes `len`, `is_empty`, `get`, `at`, `first`, `last`, and
shared or mutable `as_slice`. `get`, `first`, and `last` return `option`
without trapping. `at` and bracket indexing trap when the index is outside
the initialized prefix.

For `t is copyable`, vectors additionally expose:

- `extend_from_slice(source)`, which appends copies of the complete source;
- `fill(value)`, which replaces every initialized element;
- `copy_from(source)`, which requires source length to equal vector length;
- `copy_within(source_start, source_end, destination_start)`, which copies
  within the initialized prefix with memmove semantics.

`copy_from`, `copy_within`, and `fill` delegate to the same slice operations
used by arrays. Their bounds, length checks, empty-range behavior, and overlap
direction are therefore identical to the
[collection mutation contract](collection-mutation.md).

## Allocation and progress

`extend_from_slice` reads the source length and reserves the entire additional
capacity before initializing any new element. Length addition and allocation
layout overflow trap before allocation. The allocator ABI treats allocation
failure as process termination, not a recoverable error, so safe code cannot
observe a partially extended vector after an allocation failure.

After successful reservation, source elements are copied in order. The vector
length advances after each raw initialization, so the initialized-prefix
invariant remains exact even inside the unsafe kernel. Copying invokes no
callbacks or user effects and has no recoverable failure after reservation.
An empty source performs no allocation or raw access.

`copy_from` validates the complete length equality before its first write.
`copy_within` validates both range endpoints and the destination extent before
its first write, then copies backward when required by overlap. Consequently,
all recoverable observations are either the original vector or the complete
result; safe code never observes a partially copied vector.

## Aliasing and move-only elements

A shared source slice and a mutable destination vector must be disjoint under
the borrow checker. In particular, a slice borrowed from a vector cannot be
passed back to that vector's `extend_from_slice` or `copy_from` while its loan
is live. Self-overlap is expressed only by `copy_within`, whose implementation
uses one exclusive borrow.

Slice extension and copying are unavailable for move-only or droppable
elements because a borrowed slice cannot transfer ownership. Such elements
continue to use `push` for one owned value and `append` to move the complete
initialized prefix from another vector. `append` reserves first, then moves
each element and finally empties the source, so it neither copies nor
double-drops resources.

Convenience constructors from slices and copy-based `resize` remain outside
this surface. Callers can use `new` plus `extend_from_slice`, and can combine
`truncate` with owned `push` operations without adding a second initialization
contract.

## Research basis

[Verifying the Rust Standard Library (NFM 2026)](https://arxiv.org/abs/2606.17374)
identifies out-of-bounds access, dangling pointers, and use of uninitialized
memory as concrete proof obligations for unsafe standard-library code.
Salicin consequently makes the initialized prefix explicit, validates every
extent before writes, and advances length only after initialization.

[Place Capability Graphs (2025)](https://arxiv.org/abs/2503.21691) models
ownership and borrowing with precise capabilities through composite types,
function signatures, and loops. Salicin follows that direction by retaining
the source slice loan across calls and rejecting shared-source/mutable-vector
aliasing instead of encoding an unchecked dynamic exception.

[Lessons Learned From Verifying the Rust Standard Library
(2025)](https://arxiv.org/abs/2510.01072) emphasizes that unsafe library code
needs focused verification rather than relying only on the surrounding safe
type system. The vector implementation therefore confines pointer arithmetic
and initialization to the alloc kernel while the public surface remains safe
and constraint-checked.
