# Collection Mutation Contract

Status: implemented for the 2026 edition<br>
Accepted: 2026-07-29

This contract defines in-place mutation shared by `array(t)(n)` and
`slice(t)`.

## Operations and element bounds

Arrays and mutable slices expose:

- `swap(left, right)`, which exchanges two elements;
- `reverse()`, which reverses the complete sequence;
- `fill(value)` for `t: copyable`;
- `copy_from(source)` for `t: copyable`;
- `copy_within(source_start, source_end, destination_start)` for
  `t: copyable`.

`swap` and `reverse` work for every element with a concrete sized
representation. They move resource elements in place without copying,
allocating, or dropping them. `fill`, `copy_from`, and `copy_within` require
`copyable`; copyable and droppable are mutually exclusive, so these operations
have no resource-cleanup path.

`copy_from` requires the source and destination lengths to be equal. Safe
borrowing prevents its shared source from aliasing its mutable destination.
Copying within one sequence is expressed by `copy_within`, not by constructing
overlapping borrows.

## Bounds and overlap

`swap` validates both indices before moving either element. `copy_from`
validates the complete length equality before its first write.
`copy_within` treats its source as the half-open range
`[source_start, source_end)` and validates, in order:

- `source_start <= source_end`;
- `source_end <= len`;
- `destination_start <= len`;
- `source_end - source_start <= len - destination_start`.

The subtraction form prevents arithmetic overflow. An empty source range is
valid at any destination through `len`.

After validation, `copy_within` copies backward when
`destination_start > source_start` and forward otherwise. The result is
therefore the same as copying through a temporary sequence, including when
source and destination overlap.

## Progress, effects, and unsafe isolation

Invalid bounds or lengths trap before mutation, so failure leaves no
partially changed sequence. Once validation succeeds, every operation uses
only fixed primitive moves or copies. There are no callbacks, allocations,
user effects, or recoverable failures between writes; no partial progress is
observable from safe code.

The implementation obtains a slice data pointer through the compiler-owned
`raw_slice_ptr` intrinsic. The intrinsic requires `unsafety` and preserves the
view's shared or mutable access in the pointer type. Public safe methods retain
their source borrow for the whole call and confine all raw-pointer operations
to the core implementation.

## Research basis

[VerusBelt](https://doi.org/10.1145/3808325) gives a semantic foundation for
building verified safe APIs over unsafe Rust mechanisms, including mutable
borrows. Salicin follows the same abstraction boundary: range and ownership
obligations are discharged before entering a small raw-pointer kernel.

[Pincer](https://2026.splashcon.org/details/oopsla-2026/66/From-Raw-Pointers-to-Memory-Safety-A-Modular-Demand-Driven-Typestate-Analysis-for-Ru)
focuses analysis on small unsafe regions and uses aliasing XOR mutability to
prune safe code. That supports keeping `raw_slice_ptr` explicit and
authority-gated while ordinary callers see only exclusive safe methods.

[Pure Borrow](https://doi.org/10.1145/3808259) models derived borrows with
their source history retained. Salicin's array-to-slice delegation and
mutation methods retain that source loan instead of treating a view or its
raw implementation pointer as an independent capability.
