# Algebraic effects design

This document fixes the implementation model for user-defined resumable effects. It complements the
normative language specification while the compiler slice is being implemented.

## Goals

- Effect declarations, operations, and handlers are ordinary source-visible declarations rather
  than attributes or compiler-only names.
- Operation calls participate in the existing `with(...)` effect row and propagate automatically.
- Handling removes exactly one nominal effect identity while forwarding every other row member.
- Handlers receive a real delimited continuation. They may resume once or decline to resume; a
  mandatory automatic callback return is not called an algebraic effect.
- The surface reuses associated members, labeled arguments, closures, and trailing closures. It does
  not add `perform`, `handle`, or `resume` keywords.

## Declaration and operation calls

```sc
let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let increment(): i32 with(State(i32)) = {
  let value = State(i32).get()
  State(i32).put(value + 1)
  value
}
```

An operation requirement has no body. It is a member of one nominal effect declaration, and a full
application performs that operation. Partial application is pure, just as partial application of an
ordinary effectful function is pure. An operation may use ordinary parameter groups and passing
modes. Its result type must be explicit. Effect declarations may initially parameterize over types;
the general kind system can later admit other compile-time parameters without changing operation
syntax.

Every operation has the declared effect in its row. An operation may additionally declare
`with(...)`; those additional requirements are forwarded and are not removed merely because its
own effect is handled.

Operations are called through the effect identity, so no hidden global functions enter scope.
Effects and operations obey normal module visibility. An operation name is unique within one effect
in the first implementation; named overloads can be added after handler labels can represent an
overload shape without ambiguity.

## Derived `handle` member

Every effect declaration derives a compiler-lowered associated member named `handle`:

```sc
let answer = State(i32).handle(
  get: { (resume) -> resume(41) },
  put: { (value, resume) -> resume(()) },
) {
  increment() + 1
}
```

The final trailing closure is the handled action. Every operation is supplied exactly once as a
labeled closure. Labels make handler selection independent of source order and fit the language's
name-only overload policy. `handle` is reserved in the effect member namespace.

For an operation with parameter groups `(P1)...(Pn): O`, its handler closure has the contextual
shape `(P1)...(Pn)(resume): R`, where `R` is the result of the complete handler. For a zero-argument
operation, the callback has only the `resume` group. Handler closure parameter types may be omitted
because the effect declaration provides a unique contextual signature; this contextual omission is
limited to handler clauses until general closure-parameter inference is specified.

By default the action result and handler result are the same type. A `done:` clause permits an
answer-type transformation:

```sc
let text = State(i32).handle(
  done: { (value) -> format(value) },
  get: { (resume) -> resume(41) },
  put: { (value, resume) -> resume(()) },
) {
  increment()
}
```

If the action returns `A` and the complete handler returns `R`, `done` has contextual type `(A): R`.
Omitting it is an identity clause and therefore requires `A = R`.

## Continuation semantics

`resume` is a compiler-created, region-bound `FnOnce`-like callable. Calling it with the operation's
result continues immediately after the operation and produces the complete handler result `R`.
The handler is deep: the same handler remains active while the continuation runs, so another
operation of the same nominal effect selects the same clauses.

The first implementation is one-shot:

- moving or invoking `resume` consumes it;
- using it again is an ownership error;
- not invoking it aborts the suspended continuation and runs cleanup for every owned value in the
  abandoned frames;
- it cannot escape the handler's compiler-created region;
- multi-shot continuations require an explicit future protocol and cannot be obtained by copying a
  one-shot continuation.

This restriction preserves ordinary move and borrow guarantees while still supporting early exit,
retry through a newly invoked action, state interpretation, parser choice, and other affine
algebraic handlers.

## Effect rows

For an action `(): A with(State(S), E)` and clauses requiring `C`, handling `State(S)` produces a
result with row `E + C`. It removes the exact instantiated identity: handling `State(i32)` does not
handle `State(i64)` or another module's `State(i32)`.

The nearest active matching handler receives an operation. Nested handlers for different effects
compose in lexical order. Existing `try`, `unsafe`, and future `async` handlers retain their specific
carrier/lowering rules but share row propagation and nesting with user handlers.

An effect-row parameter represents the complete remaining row. Handler inference may solve a
single equation of the form `ActionRow = HandledEffect + Remainder`; it does not perform general row
unification or silently discard duplicate nominal identities.

## Lowering contract

Effect operations and derived handlers are justified by the source `effect { ... }` declaration in
the same way enum constructors are justified by an `enum` declaration. Shared continuation behavior
has the edition-pinned `Continuation(Input, Output)` declaration in `core.control`; the compiler may
not recognize a resumable runtime protocol that has no matching standard-library source contract.

Lowering uses typed, one-shot continuation-passing IR for functions whose row contains a resumable
user effect. Continuation environments contain values live across an operation. Cleanup planning
must cover both resumption and abandonment before native code generation is considered complete.
Marker-only `let UI = effect` remains valid and has no operations or derived handler clauses.

The current native slice performs selective CPS transformation for lexically visible operations,
including one-shot resumption and abandonment. Operation propagation through separately compiled
functions, loop backedges, and the final typed continuation ABI remain implementation work; those
cases are rejected rather than compiled with callback-only semantics.
