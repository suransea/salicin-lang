# Capturing Callable Bridge Design

Status: accepted implementation design for `TYPE-CALLABLE-1`

## Problem

Salicin source function types such as `(T): U` have a plain function-pointer ABI. Local closures,
however, may own or borrow an environment. Passing a capturing closure directly to a function-typed
parameter cannot erase that environment without either changing the ABI or losing ownership
information.

`Chain`, `Coalesce`, and higher-order functional protocols already use source function types. Their
statically selected methods need a bridge for capturing transforms and fallback closures.

## Decision

The bridge is static callee specialization, not a universal fat function pointer.

When all of the following are known at a call site:

- the concrete callee;
- the function-typed parameter position;
- the closure body and complete capture set;
- capture modes and concrete capture types;
- the callable parameter and result effects;

the compiler creates a specialized source function:

1. hygienically rename the callee's parameters and locals;
2. remove the function-typed parameter;
3. insert one ordinary parameter per capture at the same argument position;
4. inject the original closure body as a local callable bound to the removed parameter name;
5. rewrite closure references to the lifted capture parameters;
6. lower and cache the specialized function through the normal function pipeline;
7. pass capture values, borrows, and remaining call arguments in original evaluation order.

The ordinary source signature and function-pointer ABI do not change.

## Ownership

Capture mode determines the lifted parameter mode:

| Closure capture | Lifted parameter | Callable class |
| --- | --- | --- |
| shared | `borrow` | `Fn` |
| mutable | `borrow(mut)` | `FnMut` |
| move | `move` | `FnOnce` unless the captured value is otherwise reusable |

The bridge must not copy a non-`Copy` capture, lengthen a loan, convert a mutable capture to shared,
or invoke an `FnOnce` closure more than once. The specialized callee is validated normally, so its
body determines whether the callable parameter is invoked zero, one, or multiple times. A moved
capture is accepted only when that usage is compatible with exactly-once ownership.

Unused branches still clean their lifted captures exactly once. A selected branch transfers or
borrows each capture according to the original closure creation.

## Evaluation Order

Specialization is semantically transparent:

1. evaluate arguments before the callable argument in source order;
2. evaluate each capture at the callable argument position;
3. evaluate later arguments;
4. enter the specialized callee;
5. execute the injected closure only when the original callee invokes its callable parameter.

The bridge must not evaluate the closure body during capture lifting. It must preserve lazy
`Coalesce` fallback and `Chain` transform behavior.

## Effects

The injected closure retains its declared:

- `Unsafe` requirement;
- `Throws(Error)` requirement;
- custom effect identities;
- abstract residual effect parameters after concrete instantiation.

The specialized callee must expose the same complete effect row as the unspecialized call. No
capture or bridge adapter may silently handle or discard an effect.

## Specialization Identity

The cache key includes:

- canonical callee identity;
- callable parameter group and index;
- closure parameter-group shape;
- closure result and effect types;
- ordered capture modes and types.

Capture runtime values are arguments, not part of the key. Equivalent closure shapes can share one
specialized function even when invoked with different captured values.

Generated names are internal and must not appear in source diagnostics.

## Initial Scope

The first implementation covers:

- direct calls to statically resolved named, inherent, and trait methods;
- one capturing closure argument per specialization step;
- `Chain` transforms synthesized by `?.`;
- `Coalesce` fallbacks synthesized by `??`;
- shared, mutable, and moved captures;
- pure and concrete effect rows.

Multiple capturing callable arguments are handled by repeated specialization. Nested closure values
inside an unknown runtime container are not statically bridged.

## Rejection Boundaries

The compiler rejects the bridge when:

- the callee or overload is ambiguous;
- the callable argument is only dynamically known;
- capture scanning cannot establish a complete lexical set;
- a capture mode conflicts with callee usage;
- a borrow would escape or overlap invalidly;
- effect-row substitution remains underconstrained;
- specialization exceeds the normal generic recursion limits.

Rejection must name the source callable parameter and the unsupported condition. It must not expose
generated specialization names.

## Acceptance Evidence

`TYPE-CALLABLE-1` is complete when native and negative tests cover:

- a custom `Coalesce` fallback capturing a shared scalar;
- a custom `Chain` method call capturing an outer argument;
- mutable capture with repeated invocation;
- moved resource capture with zero/one invocation and exactly-once cleanup;
- effectful capture forwarding;
- borrow overlap and escape rejection;
- deterministic specialization reuse and source-level diagnostics.
