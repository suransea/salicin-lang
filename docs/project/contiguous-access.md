# Contiguous Access Contract

Status: implemented for the 2026 edition  
Accepted: 2026-07-28

This contract defines the safe common access vocabulary for `array(t)(n)`,
`slice(t)`, and `alloc.vec.vec(t)`.

## Common operations

All three containers expose:

- `len(): u64` and `is_empty(): bool`;
- `get(index): option(borrow(t))`, with `get(mut)` returning a mutable borrow;
- `at(index): borrow(t)`, with `at(mut)` returning a mutable borrow;
- `first()` and `last()` as checked access, with mutable forms selected by
  explicit `mut`.

`array` and `vec` additionally expose `as_slice()`, with `as_slice(mut)`
preserving exclusive access. The resulting slice has the same source region
as its container borrow and does not allocate or copy elements.

## Bounds and loans

`get`, `first`, and `last` represent ordinary absence with `option`. They
validate the complete bound before forming an element borrow; a failed check
therefore neither creates an out-of-bounds pointer nor retains a source loan.
`at` and bracket indexing express a caller precondition and trap when it is
violated.

An empty container has length zero, reports `is_empty()`, and returns `none`
from `get`, `first`, and `last`. There are no negative indices, implicit
wrapping indices, or safe unchecked access operations.

A shared result prevents overlapping mutation for its lifetime. A mutable
result is exclusive and keeps the original array, slice, or vector borrowed
for its lifetime. Array-to-slice conversion is a compiler-validated
representation operation because a fixed array borrow must gain a runtime
length while retaining its source history.

## Research basis

The boundary-first rule follows the 2026 Safe Coding account of localizing
array, slice, and vector safety preconditions inside an abstraction before raw
access ([DOI 10.1145/3795888](https://doi.org/10.1145/3795888)). The
source-history rule follows Pure Borrow's treatment of split borrows as
retaining their origin rather than becoming independent capabilities
([DOI 10.1145/3808259](https://doi.org/10.1145/3808259)). Salicin's concrete
design is an inference from those principles: checked absence occurs before
loan creation, while successful projections and slice views keep the source
region.
