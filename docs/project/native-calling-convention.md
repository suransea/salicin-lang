# Experimental Native Calling Convention

Status: implemented native calling contract

This document defines calls between Salicin functions. It builds on the
[ABI representation review](abi-review.md); the
[native linkage contract](native-linkage.md) defines how separately emitted
modules agree on this convention. Neither contract is a stable 1.0 ABI.

## Signature Lowering

Compile-time parameter groups select a specialization and have no runtime
position. All runtime parameter groups are flattened from left to right into
one native parameter list. Argument labels participate in source overload
selection only.

Each runtime parameter lowers as follows:

| Source parameter | Native parameter | Ownership |
| --- | --- | --- |
| `value: ()` | erased | none |
| `value: borrow(())` | erased | none |
| `value: borrow(T)` | `ptr` | caller |
| `value: borrow(mut)(T)` | `ptr` | caller, exclusive for the loan |
| inferred or `copy value: T` | value representation of `T` | callee copy; caller retains source |
| `move value: T` | value representation of `T` | transferred to callee |

A callable source value may refine an inferred mode to copy, move, or borrow
during static call resolution. The selected mode, not its spelling alone,
determines the HIR argument and cleanup behavior.

Arguments are evaluated left to right and staged before the call. If a later
argument exits control flow, every earlier staged owned argument is cleaned.
Once the call begins, a moved argument is initialized as a callee local and
the caller no longer cleans it. A callee destroys each still-owned parameter
on every ordinary, return, error, and handled-effect exit.

## Returns

`()` returns as native `void`; a completed call synthesizes the source unit
value. Other sized values return directly in their audited LLVM
representation. Return construction transfers owned result responsibility to
the caller. `Never` has no return value and terminates its path with
`unreachable`.

Borrow returns are one pointer, or the audited slice reference record, and
remain tied to a source parameter region. Semantic analysis rejects a returned
borrow whose region cannot be traced to the function's borrow parameters.

Unsized `Slice(T)` is not a first-class parameter or return. It must cross a
call behind `borrow`, `borrow(mut)`, or `Ptr`. Struct, enum, global, parameter,
and return validation runs before LLVM emission and reports the source
declaration.

## Effects And Errors

`Unsafe` is compile-time authority and adds no runtime parameter.

Direct calls with algebraic effects are specialized into compiler-generated
continuation control flow. The source effect row is not passed as a dictionary
or hidden variadic argument. When a runtime action must be erased, it uses the
audited owned `EffectCallable(Input, Output, Answer)` record; invoking it
consumes the active flag exactly once.

`Throws(Error)` requires the function's runtime result to be the matching
`Result(Error)(Output)` boundary. Ordinary completion constructs `Ok(Output)`;
`throw` and propagated failure construct `Err(Error)`. Callers either forward
that same boundary or destructure it under `try`. Error exits follow the same
owned-parameter cleanup rule as ordinary returns.

## Callables And Continuations

Noncapturing function values use one function pointer. Concrete closures and
partial applications retain a statically known entry function and pass their
capture environment according to its concrete compiler-private type. Borrowed
captures remain caller-owned; copied or moved captures follow their selected
mode.

An erased `Continuation(Input, Output)` or
`EffectCallable(Input, Output, Answer)` is an owned four-pointer record:
entry, drop entry, environment, and active flag. Invocation clears the flag
before transferring the environment; abandonment invokes the drop entry.
These records are compiler-private native values and cannot cross `foreign(c)`.

## Tail Calls

A source tail call evaluates and stages arguments normally, releases caller
temporaries, performs all remaining caller cleanup, calls the callee, and
returns its result immediately. The current LLVM does not promise `musttail`;
tail position is an ownership and control-flow contract, not a stack-usage
guarantee.

## Boundary Summary

- Runtime groups flatten deterministically and compile-time groups disappear.
- Caller and callee share one parameter-erasure and representation mapping.
- Ownership transfer occurs exactly at call entry.
- Return ownership transfers exactly at successful return construction.
- Effects add no undocumented direct-call parameters.
- `Throws` uses one explicit `Result` runtime return.
- Unsupported unsized positions fail before LLVM emission.
- Separate objects select this agreement through the ABI fingerprint defined
  by the native linkage contract.
