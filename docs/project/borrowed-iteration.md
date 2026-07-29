# Borrowed Iteration Contract

Status: implemented for the 2026 edition<br>
Accepted: 2026-07-29

This contract defines shared and mutable borrowed traversal for `array(t)(n)`
and `slice(t)`.

## Construction and source ownership

`array.iter()` and `slice.iter()` produce `slice_iter(shared)(t)`.
`array.iter(mut)()` and `slice.iter(mut)()` produce
`slice_iter(mut)(t)`. Array iteration first forms an access-preserving slice
view and then uses the same iterator representation and advancement contract
as a slice.

An iterator stores the source view for its complete lifetime. Creating a
shared iterator prevents mutation of the source; creating a mutable iterator
retains exclusive source access. Construction neither allocates nor copies,
moves, or drops elements. Consequently borrowed array traversal has no
`copyable` bound and works for resource elements.

Consuming `for array` remains the separate owning array iterator contract.
COLL-2 removes the copy limitation from borrowed traversal through
`array.iter`; it does not change ownership transfer by the consuming form.

## Yield and advancement

`iterator.item(r)` is `borrow(a)(r)(t)`, where `a` is the source access and
`r` is the region of the mutable borrow used for one `next` call. A yielded
element therefore:

- retains the original array or slice loan;
- cannot outlive the iterator borrow that produced it;
- prevents another `next` call until that yield is no longer live;
- is mutable only when the iterator owns mutable source access.

`next` checks exhaustion before indexing. An exhausted call returns `none`
without forming an element borrow. Dropping or breaking from an iterator
releases its source view; it does not drop elements because the source owner
still owns them.

## Research basis

Pure Borrow models split borrows with their source history retained, which
supports deriving each element loan from the iterator-held view rather than
creating an independent capability
([PLDI 2026, DOI 10.1145/3808259](https://doi.org/10.1145/3808259)).
Persistent Iterators identifies invalidation and aliasing as the central
hazards of mutable iterator designs
([PLDI 2026, DOI 10.1145/3808324](https://doi.org/10.1145/3808324)).
Salicin does not snapshot mutable containers as that work does; instead, its
affine source loan forbids concurrent invalidation and its GAT-like
`item(r)` family makes advancement exclusive.

The annotation-free call surface also follows the direction of
[Fully-Automatic Type Inference for Borrows with Lifetimes](https://2026.splashcon.org/details/oopsla-2026/22/Fully-Automatic-Type-Inference-for-Borrows-with-Lifetimes):
ordinary shared iteration needs no lifetime or access annotation, while
mutable access is the only explicit choice.
