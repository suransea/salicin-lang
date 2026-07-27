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
pub let movable = trait {}
```

`core.async` owns the allocation-free async contracts:

```sc future
pub let poll(comptime t: type) = enum {
  pending
  ready(t)
}

pub let future(comptime e: effects) = trait
where self: movable {
  let output: type
  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))(): poll(output) with(e)
}

pub let executor = trait {
  let run(comptime e: effects, comptime f: type)
    (self: borrow(mut)(self))
    (move future: f): f.output with(e)
  where f: future(e)
}
```

The compiler validates these declarations as language items before privileged async lowering.
Names alone have no authority.

`movable` is a source-backed auto marker for types whose values may be relocated without invalidating
their internal state. `copyable` requires `movable`. Scalars, borrows, raw pointers, and nominal values
whose fields are all `movable` satisfy it structurally. A compiler-generated value with an internal
self-reference does not.

`future(e)` is parameterized by the residual effect row of `poll`. The internal suspension effect
is discharged by the generated state machine and is not part of `e`.

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
2. creates a cold anonymous value implementing `future(e)`;
3. does not execute `body`.

The body starts on the first `poll`. Each `await operand` evaluates `operand` once, stores the
resulting future, and polls it. `ready(value)` resumes the body with `value`; `pending` stores the
current state and returns `pending` from the outer future.

`await` is contextual and valid only within an async body. It cannot cross a named function,
closure, handler clause, or nested async boundary.

The type and residual effects of the body determine `future(e).output` and `e`. Handling an effect
inside the body removes it normally. Unhandled `throwing(error)`, `unsafety`, and custom effects remain
requirements of `poll`.

The implemented suspended residual slice accepts a first segment ending in
one `await`, optionally followed by a finite linear sequence of pure await
segments. The first segment may retain custom effects or `throwing`; later
child poll rows may not. Every segment may capture by-value `copyable` or
move-only values, or retain a region-checked shared or mutable reference to
external storage. Pre-await locals used by a continuation may likewise be
retained when the resulting state remains structural `movable`. The enclosing handler specializes the generated
poll source. On the cold transition, the parent marks transferred factory
captures unavailable before evaluating the child factory; an abort therefore
cannot clean the same capture again. A distinct starting state retains
move-only continuation captures while factory locals remain under ordinary
scope cleanup. After the factory returns, the child and retained locals enter
the suspended state together. A `pending` child remains stored, and later
polls invoke only the active child rather than replaying an earlier factory or
its residual effects. Each ready transition destroys its completed child
before constructing the next. Completion, error, and cancellation each
destroy every initialized child, retained local, and continuation capture
once. A borrow of storage retained in the same future remains rejected as
self-referential. A one-shot `if` or `match` may select between direct-tail
children of the same concrete future type while the selected child factory
retains the first-segment residual row. Direct `if` and `match` selection may
also produce heterogeneous concrete child types. The complete selection
expression remains source-typed through the residual handler, including
pattern payload bindings and a moved selector; each selected concrete child
is then transferred through a pure bridge into the private active-variant
state. When pre-await locals are live in the continuation, the same bridge
constructs the complete `(selected child, retained...)` bundle so the child
and retained fields enter the suspended state atomically. Selection and the
chosen factory execute once; pending, ready, and cancellation touch only the
selected child, while every initialized retained value is cleaned exactly
once. After a pure child becomes ready, a final continuation that does not
suspend again may retain custom effects or `throwing`. The pure state-machine
transition first destroys the completed child and transfers the await output,
continuation captures, and retained locals into one private tuple. The source
poll wrapper then executes that continuation under the enclosing handler.
Pending and cancellation never execute it; success, error, and handler
abandonment clean every transferred value once. A later sequential segment
may construct and poll another residual child. Handler ownership transfers to
that active child only after cold construction; pending returns it to the
parent, ready advances once, and cancellation or handler abandonment destroys
exactly the initialized child.

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
let async_loop_step(comptime carry: type, comptime output: type) = enum {
  iteration_skip(carry)
  loop_exit(output)
}
```

This is a lowering model, not a public standard-library declaration. `carry` contains exactly the
values live across the loop backedge. Each iteration takes those values by value. A `continue` or
fallthrough transfers them to `iteration_skip`; a value-producing `break` transfers its value to
`loop_exit`. A `while` condition that becomes false is the unit-valued break path.

The parent future stores one active iteration child and reuses that storage after the child
completes:

1. `pending` leaves the active child and carried values initialized and returns `pending`;
2. `ready(iteration_skip(carry))` destroys the completed child, constructs the next iteration in the same
   child slot from `carry`, and polls it immediately;
3. `ready(loop_exit(output))` destroys the completed child, marks the parent completed, and returns
   `ready(output)`.

Immediate iterations are consumed in an ordinary poll-local loop. They do not add observable
suspension points or recurse in either the generated type or the host call stack. The implementation
may impose a documented fairness budget later, but the initial allocation-free contract runs until
a child returns `pending` or the source loop exits.

A pre-test `while` evaluates its condition before constructing the first iteration and after each
`iteration_skip`; a post-test loop skips only the first condition check. A false condition is
`loop_exit(())`, and no condition is evaluated while an active iteration is pending. The current
implementation requires a recurring condition to be pure. Effectful conditions
require a distinct resumable condition state and are rejected before lowering.

When one source iteration contains multiple sequential suspension points, it is lowered to a
finite, non-recursive iteration future. That child owns only the currently active nested segment
and eventually produces the same step outcome. Its `loop_exit(output)` type is inferred after binding
each awaited `future.output` in source order. Cancelling the parent delegates cleanup through this
finite child chain. If that iteration child's own `poll` retains a residual
effect row, recurring handler specialization is not yet composed through the
nested poll and the source program is rejected.

A recurring loop with one residual child factory per iteration is specialized
under the enclosing handler. The child is constructed cold after a true
pre-test condition, and never before it. Each completed `iteration_skip` yields
`pending` at the source wrapper boundary, then constructs the next effectful
child on the following external poll. `ready`, child `pending`, cancellation,
`throwing`, and handler abandonment preserve one-shot construction and cleanup.
Post-test loops skip only the first condition check.

For a general unit-valued iteration body, `break` and `continue` at the current loop depth become
early returns from that iteration future. Normal exits from nested `if` and `match` branches receive
the fallthrough `iteration_skip(())` outcome. Rewriting does not cross a nested loop, closure, or async
boundary.

Values declared inside an iteration are owned by that iteration. On `continue`, `break`, or
fallthrough, values not transferred into the step outcome are dropped before the control transfer.
Dropping the parent while suspended drops only the active iteration and then the parent fields;
completed iterations are never retained or dropped again. Loop-carried borrows remain subject to
the same `movable` rule as every other value stored across `await`; in particular, an iteration cannot
return a borrow into its own storage as `carry`.

Move-only parent values referenced by the post-await continuation are explicit fields of `carry`.
The continuation moves them into every reachable `iteration_skip`, and the parent reinitializes their
state fields before constructing the next child. A `loop_exit` path instead consumes or drops them in
that continuation. A source loop with no reachable `loop_exit` uses the standard uninhabited `never`
type as `output`; the internal break variant cannot be constructed.

## Ownership And Cancellation

An anonymous future is an owned resource unless all of its stored state is structurally `copyable` and
the compiler can prove that copying cannot duplicate an active computation. The initial
implementation does not make active futures `copyable`.

Dropping a not-started or suspended future cancels it:

- the body is not resumed;
- each initialized field is dropped exactly once;
- the active child future is dropped before earlier stored locals in reverse initialization order;
- moved-out and never-initialized fields are skipped;
- cancellation performs no implicit effect handling or unwind.

After `ready(output)`, ownership of `output` leaves the state machine and remaining state is cleaned
exactly once.

## `movable` and Borrowing

The first version rejects a borrow whose referent is stored in the same generated state machine
when that borrow is live across `await`. This includes references to captured fields, earlier local
fields, and projections of either. Such a state machine cannot implement `movable`, which is required
by the initial `future(e)` contract. Diagnostics identify the source borrow, suspension point, and
failed `movable` requirement.

A future may retain a borrow of an external source when the future's lifetime is proven not to
outlive that source. The loan remains active for the lifetime of the future and ordinary shared or
mutable alias rules continue to apply.

Explicit `move` parameter passing, returning an owned value, relocation assignment, and moving a
value into reallocating storage require `movable`. Initializing a value directly in its final storage
does not. Polling requires an exclusive borrow, so a future cannot move while a poll is active.

No public `pin` type is introduced. If Salicin later admits non-`movable` futures, they must be
constructed and polled in stable storage through explicit in-place APIs. That change requires a
separate design for construction, projection, drop, and unsafe escape.

## executor

The initial executor is the ordinary zero-field library value `std.async.spin`. It implements
`executor`; its `run` method owns and polls one future repeatedly until `ready`, then returns the
output.

`pending` grants permission to poll again but does not imply a wake notification. This bounded spin
executor is sufficient to validate state transitions, nested awaits, cancellation, and effects. A
later host executor may add an explicit wake contract without changing `future(e)` only if polling
without a context remains sound; otherwise that addition is a new contract revision.

Creating or polling a future never selects an executor. Heap erasure, when needed for recursive or
heterogeneous storage, uses a dedicated allocation-layer `box_future(e)(t)` adapter and is always an
explicit operation.

## Recursion And Erasure

Non-recursive private functions may infer an anonymous future result. Public APIs must expose a
named concrete future or an explicit `box_future(e)(t)`.

Direct async recursion is rejected because it creates an infinitely sized state machine. Recursion
requires an explicit allocation and erasure boundary such as `box_future`. This adapter is a
dedicated linear future representation, not general dynamic trait dispatch.

## Rejection Boundaries

The compiler rejects:

- `await` outside an async body;
- a generated future that cannot satisfy its `movable` requirement, including a self-reference live
  across suspension;
- a future escaping an external borrow region;
- moving or polling a future while it is borrowed;
- polling a completed future when statically evident;
- recursive anonymous future layouts without explicit indirection;
- effect rows that cannot be determined for the generated `future(e)` implementation;
- public anonymous future results;
- generated state whose size or cleanup plan exceeds compiler limits.

## Acceptance Evidence

Implementation tasks must cover:

- cold creation and first poll;
- immediate `ready` and one or more `pending` transitions;
- nested await and residual `throwing`, `unsafety`, and custom effects;
- move captures and locals live across suspension;
- suspended `while` and `loop` iterations, including `continue`, fallthrough, false conditions, and
  value-producing `break`;
- loop-carried owned values, consecutive immediately-ready iterations, and reuse of one child slot;
- cancellation before start and at every suspension point;
- cancellation in a suspended iteration without retaining or redropping completed iterations;
- external shared and mutable borrow retention;
- structural `movable`, self-reference, escape, double-poll, recursion, and public-API rejection;
- deterministic IR, source-level diagnostics, and exactly-once native cleanup.
