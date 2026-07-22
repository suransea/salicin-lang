# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sc` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- `Option(T)` and `Result(T, E)`
- the uninhabited `Never` type
- the `Copy` and `Drop` traits

`core.ops` contains the arithmetic protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`, the equality
protocol `Eq`, the ordering protocol `PartialOrd`, the unary protocols `Neg` and `Not`, the bitwise
protocols `BitAnd`, `BitOr`, `BitXor`, `Shl`, and `Shr`, the assignment protocols, and the
nullish-control protocols `Chain` and `Coalesce`. They are not in the prelude.
Arithmetic and bitwise protocols consume their operands and use an associated `Output` type:

```sc
use core.ops.Add

extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { ... }
}
```

`Eq(Rhs)` borrows both operands and returns `bool`; `!=` invokes the same method exactly once and
negates its result:

```sc
use core.ops.Eq

extend Number: Eq(Number) {
  let eq(borrow self)(borrow rhs: Number): bool = { self.value == rhs.value }
}
```

`PartialOrd(Rhs)` also borrows both operands. Its `partial_cmp` method returns `PartialOrdering`,
whose variants are `Less`, `Equal`, `Greater`, and `Unordered`. All four ordering operators invoke
the method once; an `Unordered` result makes each operator false:

```sc
use core.ops.{PartialOrd, PartialOrdering}

extend Number: PartialOrd(Number) {
  let partial_cmp(borrow self)(borrow rhs: Number): PartialOrdering = { ... }
}
```

`Neg` and `Not` consume their operand and define an associated `Output` type. Consequently an
overloaded `!` may return a non-boolean result; only the built-in boolean operation is fixed to
`bool`. Generic code can state the same output relationship in a normal where predicate.

`BitAnd(Rhs)`, `BitOr(Rhs)`, `BitXor(Rhs)`, `Shl(Rhs)`, and `Shr(Rhs)` have the same two consuming
parameter groups and associated `Output` shape as arithmetic protocols. Built-in integer shifts use
arithmetic right shift for signed integers and logical right shift for unsigned integers. Negative
or out-of-width shift counts trap instead of exposing backend undefined behavior.

`AddAssign(Rhs)`, `SubAssign(Rhs)`, `MulAssign(Rhs)`, `DivAssign(Rhs)`, `RemAssign(Rhs)`,
`BitAndAssign(Rhs)`, `BitOrAssign(Rhs)`, `BitXorAssign(Rhs)`, `ShlAssign(Rhs)`, and
`ShrAssign(Rhs)` are separate mutation protocols. Each mutably borrows `self`, consumes `rhs`, and
returns `()`:

```sc
pub let AddAssign(Rhs: type) = trait {
  let add_assign(borrow(mut) self)(move rhs: Rhs): ()
}
```

The corresponding `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, and `>>=` syntax binds to
these validated identities for nominal values. Built-in integers use the same fixed operator
semantics, including division, remainder, and shift traps. The left place is resolved once; an
inherent or unrelated trait method with the same member spelling cannot intercept compound
assignment.

Writing `left + right`, `left & right`, `left == right`, or `left < right` does not itself require an
import. An import is required when source names the protocol in an implementation, bound, type, or
direct member access.

`Chain` and `Coalesce` are the standard protocols for `?.` and `??`:

```sc
pub let Chain = trait {
  let Item: type
  let Rebind(Value: type): type
  let chain(E: effect, U: type)(move self)(move transform: (Item): U with(E)): Rebind(U) with(E)
}

pub let Coalesce = trait {
  let Item: type
  let coalesce(E: effect)(move self)(move fallback: (): Item with(E)): Item with(E)
}
```

The protocols use the same trait and generic-associated-constructor syntax as user declarations.
The current compiler validates these standard contracts, accepts GAT references in trait method
signatures, and supports direct constructor implementations such as `let Rebind = Maybe` in both
concrete and generic nominal trait implementations. `??` dispatches non-`Option`/`Result` nominal
values through `Coalesce` when the fallback can be represented as a no-capture lifted function. `?.`
dispatches non-`Option`/`Result` nominal values through `Chain` when the synthesized transform
closure can be represented in the same way; simple field access is supported, while transforms that
capture outer method-call arguments still require the general callable-to-function bridge. The
built-in `Option`/`Result` paths remain available.

`core.effects` owns standard effect identities. It is not part of the prelude:

```sc
pub let Unsafe = effect {}

pub let Throws(Error: type) = effect {
  let raise(move error: Error): Never
}

pub let Async = effect {
  let suspend(): ()
}
```

`Unsafe`, `Throws(Error)`, and `Async` are validated lang-item identities, but their declarations use
the same source-level effect forms as user code. `Throws.raise` is an ordinary `Never`-returning
effect operation and can be handled with a normal abort clause such as `raise: { (error) -> ... }`.
Source `throw error` targets this ordinary operation when the current effect row has exactly one
active `Throws(Error)`. Contextual `try { ... }` with an expected `Result(T, Error)` handles
ordinary `Throws(Error)` through the same algebraic handler path, using `done -> Ok` and
`raise -> Err`. Without an explicit `Result` context, direct calls and local function-value calls
to ordinary `Throws(Error)` functions now infer the same handler result when the success type is
probeable and the escaping error type is unique. `Async` currently exposes only a minimal
`suspend(): ()` operation; executable
async/Future lowering will add its handler contracts in the same implementation slice rather than
pretending `await` already works.

`core.access` owns standard access identities, also outside the prelude:

```sc
pub let Shared = access
pub let Mutable = access
```

The language still writes borrow types as `borrow T` and `borrow(mut) T`; naming these declarations
directly requires an ordinary `use core.access...`.

`core.control` owns the edition-pinned contracts for compiler-lowered control functions. It is not
part of the prelude. The module declares the compiler-provided `do`, `try`, `throw`, `unsafe`, and
`loop` control functions. Their bodyless signatures are permitted only for validated core lang
items; ordinary package functions still require `= { ... }` bodies.

It also declares two compiler-owned erased runtime contracts:

```sc
pub let Continuation(Input: type, Output: type) = struct()
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct()
```

`Continuation` is a one-shot suspended computation. `EffectCallable` is an owned action awaiting a
handler-supplied continuation from `Output` to `Answer`; `Input` is the action's packed runtime input.
Both native values carry call and drop entries, an environment pointer, and an ownership flag. They
are module exports rather than prelude names and cannot be replaced by same-named user declarations.
The compiler-internal action entry has the logical signature
`(environment, Input, Continuation(Output, Answer)): Answer`. Erasing or invoking an action consumes
its owner; a dropped, uninvoked action releases its captured environment through the stored drop
entry. These low-level operations are not source-level standard-library functions.

```sc
pub let do(E: effect, T: type)(move action: (): T with(E)): T with(E)
pub let try(F: effect, T: type, E: type)
  (move action: (): T with(core.effects.Throws(E), F)): Result(T, E) with(F)
pub let throw(Error: type)(move error: Error): Never with(core.effects.Throws(Error))
pub let unsafe(E: effect, T: type)
  (move action: (): T with(core.effects.Unsafe, E)): T with(E)
pub let loop(E: effect, T: type)(move body: (): () with(E)): T with(E)
```

Here `try` removes only `Throws(E)`, `unsafe` removes only the `Unsafe` requirement, and both forward
the remainder row. `throw` introduces the standard `Throws(Error)` requirement, while `do` and
`loop` forward the whole row.

`core.iter` owns iteration rather than the prelude:

```sc
pub let Iterator = trait {
  let Item: type
  let next(borrow(mut) self)(): Option(Item)
}

pub let IntoIterator = trait {
  let IntoIter: type
  let into_iter(move self)(): IntoIter
}
```

Implementing or naming either trait requires `use core.iter.{Iterator, IntoIterator}`. The `for`
syntax itself needs no import and dispatches only through these validated identities. It evaluates
the iterable once, moves it into `IntoIterator.into_iter`, repeatedly mutably borrows the resulting
iterator for `Iterator.next`, and stops on `None`. An inherent or unrelated trait method named
`into_iter` or `next` cannot intercept this lowering.

The control spellings bind to these validated identities without importing ordinary names. Standard
effect identities such as `Throws` remain normal `core.effects` exports when named in source. An
ordinary same-named declaration cannot acquire lang-item lowering behavior. Future control features
follow the same rule: for example, async lowering must add `Future`, `async`, and handler contracts
to the matching core release when it becomes executable, rather than reserving undocumented compiler
magic in advance.

`core.algebra` contains first-order algebra protocols rather than putting them in the prelude:

```sc
pub let Semigroup(T: type) = trait {
  let combine(move left: T, move right: T): T
}

pub let Monoid(T: type) = trait {
  let empty(): T
  let combine(move left: T, move right: T): T
}
```

The compiler does not prove algebraic laws.

`core.functional` contains higher-kinded protocols over compile-time type constructors. It is not
part of the prelude:

```sc
pub let Functor(F: (Value: type): type) = trait {
  let map(E: effect, A: type, B: type)(
    move value: F(A),
    move transform: (A): B with(E),
  ): F(B) with(E)
}

pub let Applicative(F: (Value: type): type) = trait {
  let pure(A: type)(move value: A): F(A)

  let apply(E: effect, A: type, B: type)(
    move transform: F((A): B with(E)),
    move value: F(A),
  ): F(B) with(E)
}

pub let Monad(M: (Value: type): type) = trait {
  let flat_map(E: effect, A: type, B: type)(
    move value: M(A),
    move next: (A): M(B) with(E),
  ): M(B) with(E)
}
```

These declarations use constructor kinds such as `(Value: type): type`. Traits with a matching
constructor subject can already be implemented for generic nominal constructors. Method
implementations are registered as generic function templates and validated, for example
`extend Carrier: Functor { let map(E, A, B)... }`. Constructor trait associated functions without a
receiver can be called from the bare constructor, for example `Carrier.map(...)`; dispatch then uses
the existing generic function instance pipeline. Trait inheritance constraints such as
`Applicative where F: Functor`, associated-type lowering, receiver-style HKT methods, and full HKT
equation solving remain future semantic work.

`ControlFlow`, the old propagation `Try`, `FromResidual`, and `FromError` were removed together with postfix `.try`. `Option` and
`Result` are ordinary enum values and require explicit constructors. Language error propagation is
defined by the standard `Throws(E)` effect, `throw`, and `try { ... }`; `do` has no error-specific
semantics.

Primitive implementations remain compiler-defined. The unit type has the single spelling `()`. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import policy and
[the language specification](../language/specification.md) for semantic rules.
