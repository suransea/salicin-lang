# Collection Algorithms Contract

Status: implemented for the 2026 edition<br>
Accepted: 2026-07-30

This contract defines the common search, predicate, membership, and fold
surface for contiguous collections.

## Shared kernel and order

Borrowed `slice(t)` is the semantic kernel. Fixed arrays and `alloc.vec.vec(t)`
expose the same operations by creating a shared slice view and delegating to
that kernel:

- `find(predicate)` returns a borrow of the first accepted element;
- `position(predicate)` returns the first accepted zero-based index;
- `contains(needle)` reports whether an equal element exists;
- `any(predicate)` reports whether at least one element is accepted;
- `all(predicate)` reports whether every element is accepted;
- `fold(initial)(combine)` reduces from left to right.

Every operation observes elements in increasing index order. `find`,
`position`, and `any` stop after the first accepted element. `all` stops after
the first rejected element. `fold` invokes its callback exactly once for every
element unless the callback transfers control through an effect.

For an empty collection, `find` and `position` return `none`, `contains` and
`any` return `false`, `all` returns `true`, and `fold` returns its initial
accumulator without invoking the callback.

## Ownership and borrows

Predicate and fold callbacks receive `borrow(t)`. Searching therefore does not
copy, move, replace, or destroy collection elements, including move-only
resources. A borrow returned by `find` retains the source collection loan:
it cannot escape a local owner or overlap mutation of that owner.

`fold` consumes its initial accumulator and transfers the current accumulator
into each callback. Each successful callback returns the only accumulator for
the next step. Normal completion returns that owner to the caller; early
effect transfer destroys any in-flight owned state exactly once under the
language cleanup rules.

`contains` currently requires `t is copyable && t is eq(t)`. The slice kernel
copies each element value before equality dispatch, so membership does not
pretend that the present equality protocol accepts two source-tied borrows.
Move-only collections can express membership with `any` and a borrowing
predicate.

## Effects and authority

`find`, `position`, `any`, `all`, and `fold` infer and forward the callback's
exact effect row. A pure callback keeps the operation pure. A callback with
`throwing(error)`, a user effect, or `unsafety` requires the same handler or
authority at the collection call. The algorithms introduce no additional
effect or allocation.

The effect-generic signatures are checked with custom-effect callbacks.
Aborting rather than returning follows the ordinary lexical cleanup plan for
the callback, accumulator, delegated slice view, and collection owner.

## Non-goals

This surface does not add reverse search, range-limited search, mutable
predicates, sorting, iterator adapters, parallel evaluation, or collection
allocation. The current iterator protocol ties its item to each mutable
`next` borrow, so source-returning `find` lives on the slice kernel rather than
claiming a generic iterator result that the protocol cannot express.

## Research basis

[Linear Effects, Exceptions, and Resource Safety
(ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
shows why effect transfer, exceptions, linear ownership, and destructor order
must be designed together. Salicin therefore forwards callback effects
explicitly and keeps ownership in the ordinary cleanup plan.

[Handling Exceptions and Effects with Automatic Resource Analysis
(OOPSLA 2026)](https://2026.splashcon.org/details/oopsla-2026/8/Handling-Exceptions-and-Effects-with-Automatic-Resource-Analysis)
identifies non-local control as a central complication for resource reasoning.
The collection contracts do not hide that control behind an apparently pure
API: the callback row remains visible at every call.

[Persistent Iterators with Value Semantics
(2026)](https://arxiv.org/abs/2604.14072) examines invalidation and aliasing
hazards in mutable iterator designs. Salicin keeps source-returning search on
a borrow-retaining contiguous view and rejects mutation while a found element
borrow remains live.
