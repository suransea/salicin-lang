# Algebraic-Effect Contracts

This document defines the implementation contract for source-declared algebraic effects. The
[language specification](specification.md) defines their observable semantics.

## Effect Declarations

An effect is a nominal compile-time identity with one or more operations:

```sc fragment
let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}
```

An operation has explicit runtime parameter groups, passing modes, result type, and optional
forwarded effects. A complete operation call performs the effect. Partial application is pure until
the final group is supplied.

Operations are selected through their effect identity. They obey ordinary visibility and overload
rules. A declaration with the same operation name in another effect is unrelated.

## Effect Rows

`with(...)` is part of a function type:

```sc fragment
let increment(): i32 with(State(i32)) = {
  let value = State(i32).get()
  State(i32).put(value + 1)
  value
}
```

Rows are unordered sets of nominal effect identities. Handling one identity removes exactly that
identity and forwards every other requirement. A compile-time `E: effect` parameter may represent
an abstract residual row and is instantiated before runtime lowering.

Effect rows do not encode ordinary allocation, I/O, or mutation. Those capabilities are represented
by library types and APIs unless they explicitly declare an effect.

## Handler Shape

Every source effect is validated against `core.effect.Handle`. Its derived `handle` member
accepts:

- one labeled clause for each operation;
- an `action` closure containing the handled computation;
- an optional completion transform when the contract permits answer-type conversion.

Conceptually:

```sc fragment
let answer = State(i32).handle
  get { resume -> resume(41) }
  put { (value, resume) -> resume(()) }
  action {
    increment() + 1
  }
```

Clause parameter and result types come from the effect declaration. Overloaded operations retain
their declared labels so each clause remains unambiguous.

An operation returning `Never` is abortive. Its clause has no continuation and directly produces
the handler answer.

## Continuations

A resumable clause receives a delimited, single-use continuation. Calling it:

1. supplies the operation result;
2. resumes the suspended action;
3. eventually produces the complete handler answer.

The continuation owns its suspended frames and cleanup state. It cannot be copied, invoked twice,
or outlive captured borrows. Dropping it abandons the suspended computation and cleans every
initialized captured value exactly once.

Resumption and abandonment are both ordinary ownership paths. Neither path may leak captures,
duplicate drops, or silently skip destructors.

## Standard Effects

`core.error.Throws(Error)` is the standard abortive error effect. Its `raise` operation returns
`Never`. `throw(error)` invokes that operation. `try { action }` is one standard interpreter that
handles it into `core.Result(Error)(Value)`; the effect itself is independent of `Result`.

`core.unsafe.Unsafe` is an authority effect. Its handler is the lexical `unsafe { ... }` boundary.
Authorization does not weaken type checking, ownership, region checking, or cleanup.

Standard effects use the same nominal row and handler machinery as user effects. Their source
declarations are validated lang items, not name-based exceptions.

## Selective CPS Lowering

The compiler may lower only effectful call paths into continuation-passing form. Pure functions and
unaffected paths retain their ordinary ABI.

For a handled path, lowering must preserve:

- source-order argument evaluation;
- parameter passing modes;
- the exact residual effect row;
- operation identity and overload labels;
- one-shot continuation ownership;
- lexical borrow bounds;
- cleanup flags for initialized captures;
- the handler answer type;
- source locations in diagnostics.

Known named effectful functions, their aliases, and statically selected callable alternatives may
specialize into CPS frames. An unknown callable must not be silently treated as pure.

## Runtime Contracts

`Continuation(Input, Output)` and `EffectCallable(Input, Output, Answer)` are
source-declared type forms with complete core-private `= builtin()`
initializers and compiler-owned representations. They are not empty
structures, and their values are linear resources.

The runtime representation may use generated frames and adapters, but those details are not
observable language entities. Generated names must not appear in user diagnostics or participate in
source lookup.

## Rejection Boundaries

The compiler rejects a handler when it cannot prove:

- a unique effect and operation identity;
- a complete, nonduplicated clause set;
- compatible clause and answer types;
- safe continuation ownership;
- valid capture lifetimes;
- preservation of residual effects;
- deterministic cleanup.

Unsupported dynamic effect dispatch is rejected rather than approximated with an incorrect pure
call or an erased effect row.
