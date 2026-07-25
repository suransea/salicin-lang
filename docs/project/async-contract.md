# Initial Async Contract

Status: accepted design for `ASYNC-CONTRACT-1`; implementation progress is recorded in
[`status.md`](status.md)

This document fixes the first implementable async boundary. Source sketches marked `sc future`
remain design syntax until the corresponding implementation tasks complete.

## Goals

The first async slice provides:

- cold anonymous futures;
- explicit polling with deterministic cancellation;
- `await` inside an async computation;
- residual effect forwarding;
- one allocation-free spin executor;
- no implicit allocation, thread, timer, reactor, or executor choice.

It does not introduce a general runtime trait-object model.

## Source Contracts

`core.marker` owns the mobility marker:

```sc future
pub let Move = trait {}
```

`core.async` owns the allocation-free async contracts:

```sc future
pub let Poll(T: type) = enum {
  Pending
  Ready(T)
}

pub let Future(E: effect) = trait
where Self: Move {
  let Output: type
  let poll(R: region)
    (self: borrow(mut)(R)(Self))(): Poll(Output) with(E)
}

pub let Executor = trait {
  let run(E: effect, F: type)
    (self: borrow(mut)(Self))
    (move future: F): F.Output with(E)
  where F: Future(E)
}
```

The compiler validates these declarations as language items before privileged async lowering.
Names alone have no authority.

`Move` is a source-backed auto marker for types whose values may be relocated without invalidating
their internal state. `Copy` requires `Move`. Scalars, borrows, raw pointers, and nominal values
whose fields are all `Move` satisfy it structurally. A compiler-generated value with an internal
self-reference does not.

`Future(E)` is parameterized by the residual effect row of `poll`. The internal suspension effect
is discharged by the generated state machine and is not part of `E`.

## Async Expressions

```sc future
let future = async {
  let first = compute()
  let second = await next(first)
  first + second
}
```

Evaluating `async { body }`:

1. evaluates and transfers its captures from left to right;
2. creates a cold anonymous value implementing `Future(E)`;
3. does not execute `body`.

The body starts on the first `poll`. Each `await operand` evaluates `operand` once, stores the
resulting future, and polls it. `Ready(value)` resumes the body with `value`; `Pending` stores the
current state and returns `Pending` from the outer future.

`await` is contextual and valid only within an async body. It cannot cross a named function,
closure, handler clause, or nested async boundary.

The type and residual effects of the body determine `Future(E).Output` and `E`. Handling an effect
inside the body removes it normally. Unhandled `Throws(Error)`, `Unsafe`, and custom effects remain
requirements of `poll`.

## State Machines

The compiler lowers each async expression to a private nominal state machine containing:

- a discriminant for not-started, suspended, and completed states;
- transferred captures;
- locals live across each suspension;
- the currently awaited child future;
- initialization flags required for partial construction and cleanup.

Only values live across a suspension become fields. Evaluation before a suspension remains ordinary
straight-line code. A completed future cannot be polled again; repeated polling is a contract trap,
not a second execution.

Generated state-machine names, fields, and states are not source entities and never appear in
diagnostics.

### Suspension In Loops

A suspension inside `while` or `loop` is lowered as a reusable iteration state, not as recursive
nesting of anonymous futures. Conceptually, one suspended iteration completes with one of two
compiler-internal outcomes:

```sc future
let AsyncLoopStep(Carry: type, Output: type) = enum {
  Continue(Carry)
  Break(Output)
}
```

This is a lowering model, not a public standard-library declaration. `Carry` contains exactly the
values live across the loop backedge. Each iteration takes those values by value. A `continue` or
fallthrough transfers them to `Continue`; a value-producing `break` transfers its value to
`Break`. A `while` condition that becomes false is the unit-valued break path.

The parent future stores one active iteration child and reuses that storage after the child
completes:

1. `Pending` leaves the active child and carried values initialized and returns `Pending`;
2. `Ready(Continue(carry))` destroys the completed child, constructs the next iteration in the same
   child slot from `carry`, and polls it immediately;
3. `Ready(Break(output))` destroys the completed child, marks the parent completed, and returns
   `Ready(output)`.

Immediate iterations are consumed in an ordinary poll-local loop. They do not add observable
suspension points or recurse in either the generated type or the host call stack. The implementation
may impose a documented fairness budget later, but the initial allocation-free contract runs until
a child returns `Pending` or the source loop exits.

A pre-test `while` evaluates its condition before constructing the first iteration and after each
`Continue`; a post-test loop skips only the first condition check. A false condition is
`Break(())`, and no condition is evaluated while an active iteration is Pending. The current
implementation requires a recurring condition to be pure; residual handler specialization is a
separate milestone.

When one source iteration contains multiple sequential suspension points, it is lowered to a
finite, non-recursive iteration future. That child owns only the currently active nested segment
and eventually produces the same step outcome. Its `Break(Output)` type is inferred after binding
each awaited `Future.Output` in source order. Cancelling the parent delegates cleanup through this
finite child chain.

For a general unit-valued iteration body, `break` and `continue` at the current loop depth become
early returns from that iteration future. Normal exits from nested `if` and `match` branches receive
the fallthrough `Continue(())` outcome. Rewriting does not cross a nested loop, closure, or async
boundary.

Values declared inside an iteration are owned by that iteration. On `continue`, `break`, or
fallthrough, values not transferred into the step outcome are dropped before the control transfer.
Dropping the parent while suspended drops only the active iteration and then the parent fields;
completed iterations are never retained or dropped again. Loop-carried borrows remain subject to
the same `Move` rule as every other value stored across `await`; in particular, an iteration cannot
return a borrow into its own storage as `Carry`.

Move-only parent values referenced by the post-await continuation are explicit fields of `Carry`.
The continuation moves them into every reachable `Continue`, and the parent reinitializes their
state fields before constructing the next child. A `Break` path instead consumes or drops them in
that continuation. A source loop with no reachable `Break` uses the standard uninhabited `Never`
type as `Output`; the internal break variant cannot be constructed.

## Ownership And Cancellation

An anonymous future is an owned resource unless all of its stored state is structurally `Copy` and
the compiler can prove that copying cannot duplicate an active computation. The initial
implementation does not make active futures `Copy`.

Dropping a not-started or suspended future cancels it:

- the body is not resumed;
- each initialized field is dropped exactly once;
- the active child future is dropped before earlier stored locals in reverse initialization order;
- moved-out and never-initialized fields are skipped;
- cancellation performs no implicit effect handling or unwind.

After `Ready(output)`, ownership of `output` leaves the state machine and remaining state is cleaned
exactly once.

## Move And Borrowing

The first version rejects a borrow whose referent is stored in the same generated state machine
when that borrow is live across `await`. This includes references to captured fields, earlier local
fields, and projections of either. Such a state machine cannot implement `Move`, which is required
by the initial `Future(E)` contract. Diagnostics identify the source borrow, suspension point, and
failed `Move` requirement.

A future may retain a borrow of an external source when the future's lifetime is proven not to
outlive that source. The loan remains active for the lifetime of the future and ordinary shared or
mutable alias rules continue to apply.

Explicit `move` parameter passing, returning an owned value, relocation assignment, and moving a
value into reallocating storage require `Move`. Initializing a value directly in its final storage
does not. Polling requires an exclusive borrow, so a future cannot move while a poll is active.

No public `Pin` type is introduced. If Salicin later admits non-`Move` futures, they must be
constructed and polled in stable storage through explicit in-place APIs. That change requires a
separate design for construction, projection, drop, and unsafe escape.

## Executor

The initial executor is an ordinary library value implementing `Executor`. Its `run` method polls
one future repeatedly until `Ready` and returns the output.

`Pending` grants permission to poll again but does not imply a wake notification. This bounded spin
executor is sufficient to validate state transitions, nested awaits, cancellation, and effects. A
later host executor may add an explicit wake contract without changing `Future(E)` only if polling
without a context remains sound; otherwise that addition is a new contract revision.

Creating or polling a future never selects an executor. Heap erasure, when needed for recursive or
heterogeneous storage, uses a dedicated allocation-layer `BoxFuture(E)(T)` adapter and is always an
explicit operation.

## Recursion And Erasure

Non-recursive private functions may infer an anonymous future result. Public APIs must expose a
named concrete future or an explicit `BoxFuture(E)(T)`.

Direct async recursion is rejected because it creates an infinitely sized state machine. Recursion
requires an explicit allocation and erasure boundary such as `BoxFuture`. This adapter is a
dedicated linear future representation, not general dynamic trait dispatch.

## Rejection Boundaries

The compiler rejects:

- `await` outside an async body;
- a generated future that cannot satisfy its `Move` requirement, including a self-reference live
  across suspension;
- a future escaping an external borrow region;
- moving or polling a future while it is borrowed;
- polling a completed future when statically evident;
- recursive anonymous future layouts without explicit indirection;
- effect rows that cannot be determined for the generated `Future(E)` implementation;
- public anonymous future results;
- generated state whose size or cleanup plan exceeds compiler limits.

## Acceptance Evidence

Implementation tasks must cover:

- cold creation and first poll;
- immediate `Ready` and one or more `Pending` transitions;
- nested await and residual `Throws`, `Unsafe`, and custom effects;
- move captures and locals live across suspension;
- suspended `while` and `loop` iterations, including `continue`, fallthrough, false conditions, and
  value-producing `break`;
- loop-carried owned values, consecutive immediately-ready iterations, and reuse of one child slot;
- cancellation before start and at every suspension point;
- cancellation in a suspended iteration without retaining or redropping completed iterations;
- external shared and mutable borrow retention;
- structural `Move`, self-reference, escape, double-poll, recursion, and public-API rejection;
- deterministic IR, source-level diagnostics, and exactly-once native cleanup.
