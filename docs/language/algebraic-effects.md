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
Effects and operations obey normal module visibility. Operations may overload only by their
runtime parameter names, never by parameter types. A call to an overloaded operation must use named
arguments in declaration order. Its handler clause uses those same names before `resume`, so two
clauses with the same operation label remain unambiguous:

```sc
let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let answer = Ask.handle(
  value: { (left, resume) -> resume(left) },
  value: { (right, resume) -> resume(right) },
) {
  Ask.value(left: 19) + Ask.value(right: 23)
}
```

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

The final trailing closure is the handled action. Every operation signature is supplied exactly
once as a labeled closure. A non-overloaded operation clause may choose local parameter names; an
overloaded clause must repeat the selected operation's parameter names in declaration order.
`handle` is reserved in the effect member namespace.

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
Owned actions crossing into reusable handlers use the adjacent
`EffectCallable(Input, Output, Answer)` contract. Its logical declaration is empty because the call
entry, drop entry, environment pointer, and ownership flag are compiler-private ABI fields.
The call entry receives the erased environment, the action input, and an owned
`Continuation(Output, Answer)`, then returns the handler answer. HIR represents action erasure and
invocation as explicit moves, and native lowering emits target-specific call/drop adapters for the
captured closure. The low-level conversion and invocation operations remain compiler-internal.

Lowering uses typed, one-shot continuation-passing IR for functions whose row contains a resumable
user effect. Continuation environments contain values live across an operation. Cleanup planning
must cover both resumption and abandonment before native code generation is considered complete.
Marker-only `let UI = effect` remains valid and has no operations or derived handler clauses.

The current native slice performs selective CPS transformation for lexically visible operations,
including one-shot resumption and abandonment. It propagates through fully applied ordinary named
functions by hygienically specializing their source bodies. Inferred immutable local aliases of a
statically known effectful function retain that identity through alias chains and enter the same
specialization path. A statically known named function or such an alias may also fill an effectful
callable parameter: the higher-order frame specializes that parameter to the source target, removes
it from the runtime frame, and transforms the resulting direct call normally. A finite
`if / else if / else` selection between named callable targets is represented by an integer tag
evaluated once at the binding, then dispatched at each call into the selected resumable entry with
the same caller continuation. Escaping closures and open-ended callable values still require the
general handler-aware runtime ABI and are rejected explicitly.

A function that defines a handler may itself accept an effectful callable parameter. When a call
supplies a named function or an immutable alias with no captured runtime environment, the
compiler creates a handler-function specialization, substitutes that target before selective CPS,
and removes the callable from the specialization's runtime parameter groups. Other arguments and
supplied or remaining curried groups stay runtime values, so the specialized call may still be
partially applied. A genuinely unknown callable parameter is not silently invoked
through the direct function-pointer ABI; it remains pending the erased handler-aware callable ABI.
For a complete call whose first parameter is that callable, a nested conditional selection between
known targets is distributed into one specialized call per leaf. The selector is evaluated before
later parameter groups, only the selected branch evaluates its remaining arguments, and every leaf
enters the same ordinary handler CPS and cleanup path.

Selective CPS removes only the handled nominal identity. Residual `unsafe`, `throws(Error)`, and
other nominal requirements remain on generated resumable frames. Intercepted operations also retain
their explicitly declared residual row through a compiler-created capability gate, so replacing an
operation with its clause cannot accidentally authorize or erase unrelated effects. For
`throws(Error)`, resume accepts the operation's logical success type while generated closures retain
the `Result(Success, Error)` ABI internally.
Lexically active handler identities are carried separately through compiler-generated CPS closures,
so an outer handler can satisfy a residual requirement that an inner handler exposes only after
named-frame specialization. This internal capability is not added to the frame's public callable
row and is not inherited by an ordinary user closure. A throwing tail continuation that must wrap a
success value returns through its `Result` boundary instead of using the direct tail-call fast path.

An explicitly typed local closure whose row contains the handled effect is lowered to a resumable
closure while it remains lexically visible to that handler. Its ordinary result becomes the input
of a hidden `Continuation(Input, HandlerAnswer)` parameter appended to the final runtime group, and
its body tail-invokes that continuation. The normal closure environment remains intact: shared
captures retain `Fn` behavior, mutable captures retain `FnMut` state across resumptions and later
calls, and move captures retain `FnOnce` ownership. Abandoning an operation inside the closure drops
the moved capture through the erased continuation exactly once. Such a closure may also specialize
an effectful higher-order callable parameter or participate in a finite handler-local selection.
The tag dispatches directly to the chosen closure environment, preserving
`Fn`/`FnMut` state and `FnOnce` flow consumption; it does not create an escaping erased closure
value. Selected and unselected owned environments remain covered by normal scope and abandonment
cleanup. The finite selection tag may be copied into immutable local aliases under the same lexical
handler; those aliases retain the target set, may be called directly, and may specialize a
higher-order resumable frame. Mutable aliases may be reassigned between values with the same
signature and finite target set; assignment remaps tags when target order differs. Values escaping
that lexical handler and open-ended target sets remain dynamic-ABI work. Existing dynamic values may
also form branches of another finite selection: their target sets are merged and their runtime tags
are remapped into the union. The outer selector may suspend and transfer capturing branch
environments into its one-shot continuation. Borrowed callable environments are materialized for
the duration of the continuation call, while moved nested environments disarm their complete drop
subtrees before ownership is transferred.
Operation and ordinary call arguments are traversed in source order, `done:` may change the answer
type, and nested handlers of the same identity select the nearest matching boundary. Arguments of
an effect-propagating named call enter CPS before its callee frame is built, so multiple suspended
arguments resume left to right and the eventual call receives their produced values.
Each named-call specialization is a real local closure frame, so shared and mutable borrow
parameters retain their capabilities, explicit returns target the callee frame, and its locals are
cleaned before the caller continuation resumes. Named frames now complete through typed one-shot
continuation closures and an explicit tail-continuation terminator. Consequently, omitting
`resume` abandons the complete suspended cross-function computation, while a clause may continue
computing after `resume` returns. Shared and mutable captures forwarded through a moved continuation
are rebased to pointers in its new callable environment. Direct recursion and resumable loop
backedges use these CPS frames. Concrete continuation closures erase to a uniform internal value
containing call and drop entries, an environment pointer, and a one-shot flag. Named frames receive
that value as an explicit hidden parameter; each direct or mutually recursive call site creates a
fresh node for its remaining computation, so recursive function result and handler answer types may
differ. Invoking a node transfers its environment to the call entry and disarms the erased value;
abandoning an armed node calls its drop entry. Thus either terminal path destroys every move-captured
value exactly once. A named recursive frame is visible only while transforming its own callee body;
a later sequential call in the caller continuation creates an independent frame instead of a false
recursive backedge. Selective CPS preserves source order through operation and ordinary-call
arguments, arrays, indexes, members, `match` scrutinees and arm
bodies, and immediate `do`, `unsafe`, and `try` wrappers. Effectful `&&` and `||` operands retain
their lazy branch semantics. Effectful `??` evaluates its fallback only on `None` or `Err`, and both
the scrutinee and fallback may suspend independently. Match guards may suspend when the complete
match input implements `Copy`, or when the candidate can retain read-only projections into a
non-`Copy` enum owned by the continuation; false guards continue with the next candidate. Fully applied optional
method calls evaluate the owned receiver first, enter argument CPS only on `Some` or `Ok`, and rewrap
the method result before continuing. Retaining a non-Copy scrutinee across suspended candidate
selection first tests a binding-erased, non-owning pattern and then transfers the sole owner into the
guard continuation. If the guard resumes `true`, an ordinary rematch commits the original payload
bindings before entering the body; if it resumes `false`, the untouched owner continues through the
remaining candidates. Referenced `Copy` bindings are copied into generated continuations. Referenced
non-`Copy` bindings are instead rebuilt in each continuation as read-only projected aliases rooted in
that continuation's owned enum capture. The guard may inspect or borrow such a binding but cannot
move it; ownership is still transferred only by the successful commit rematch.
Capturing indirect calls remain implementation work rather than falling back to callback-only
semantics.

Lexically nested handlers of different effect identities compose in source order. The outer
selective-CPS pass traverses the inner handler's action and clause closures, including generated
named-call frame and continuation closures, while the inner clause's `resume` parameter shadows any
same-spelled outer continuation. Nested handlers of the same identity continue to select the nearest
boundary instead of being fused.
