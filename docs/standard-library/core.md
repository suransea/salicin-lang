# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sc` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- the uninhabited `Never` type
- the `Copy` and `Drop` traits

The `core` root contains fundamental ordinary data types that are intentionally not prelude names:

```sc
pub let Option(T: type) = enum {
  Some(T),
  None,
}

pub let Result(E: type)
  (T: type) = enum {
  Ok(T),
  Err(E),
}
```

Naming them requires an ordinary root import such as `use std.Option` or `use std.Result`.
Operators and syntax that lower through these identities use the validated standard-library
declarations directly; importing is only required when source code writes the names.

`core.ops` contains the arithmetic protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`, the equality
protocol `Eq`, the ordering protocol `PartialOrd`, the unary protocols `Neg` and `Not`, the bitwise
protocols `BitAnd`, `BitOr`, `BitXor`, `Shl`, and `Shr`, and the assignment protocols. They are not
in the prelude.
Arithmetic and bitwise protocols consume their operands and use an associated `Output` type:

```sc
use std.ops.Add

extend Number: Add(Number) {
  let Output = Number
  let add(move self)
    (move rhs: Number): Number = { ... }
}
```

`Eq(Rhs)` borrows both operands and returns `bool`; `!=` invokes the same method exactly once and
negates its result:

```sc
use std.ops.Eq

extend Number: Eq(Number) {
  let eq(self: borrow(Self))
    (rhs: borrow(Number)): bool = { self.value == rhs.value }
}
```

`PartialOrd(Rhs)` also borrows both operands. Its `partial_cmp` method returns `PartialOrdering`,
whose variants are `Less`, `Equal`, `Greater`, and `Unordered`. All four ordering operators invoke
the method once; an `Unordered` result makes each operator false:

```sc
use std.ops.{PartialOrd, PartialOrdering}

extend Number: PartialOrd(Number) {
  let partial_cmp(self: borrow(Self))
    (rhs: borrow(Number)): PartialOrdering = { ... }
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
  let add_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
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

`core.flow` contains the standard protocols for `?.` and `??`. They are not in the prelude:

```sc
pub let Chain = trait {
  let Item: type
  let Rebind(Value: type): type

  let chain(E: effect, U: type)
    (move self)
    (move transform: (Item): U with(E)): Rebind(U) with(E)
}

pub let Coalesce = trait {
  let Item: type

  let coalesce(E: effect)
    (move self)
    (move fallback: (): Item with(E)): Item with(E)
}
```

The protocols use the same trait and generic-associated-constructor syntax as user declarations.
The current compiler validates these standard contracts, accepts GAT references in trait method
signatures, and supports direct constructor implementations such as `let Rebind = Maybe` in both
concrete and generic nominal trait implementations. `??` dispatches non-`Option`/`Result` nominal
values through `Coalesce` when the fallback can be represented as a no-capture lifted function. `?.`
dispatches non-`Option`/`Result` nominal values through `Chain` when the synthesized transform
closure can be represented in the same way; simple field access is supported, while transforms that
capture outer method-call arguments still require the general callable-to-function bridge. The root
`core.Option`/`core.Result` paths remain available as standard-library specializations. The older
`std.ops.Chain` and `std.ops.Coalesce` paths are accepted as compatibility aliases, but new source
should import the protocols from `std.flow`.

`core.effects` owns standard effect identities. It is not part of the prelude; ordinary source
should import these identities through `std.effect`:

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
Standard and user effect identities use type-like nominal spelling: effect declarations and the
final segment of a `with(...)` effect path must start with an uppercase letter. Effect row parameters
such as `E: effect` are still resolved as parameters rather than nominal effects.
Source `throw(error)` targets this ordinary operation when the current effect row has exactly one
active `Throws(Error)`. Contextual `try { ... }` with an expected `Result(Error)(T)` handles
ordinary `Throws(Error)` through the same algebraic handler path, using `done -> Ok` and
`raise -> Err`. Without an explicit `Result` context, direct calls and local function-value calls
to ordinary `Throws(Error)` functions now infer the same handler result when the success type is
probeable and the escaping error type is unique. `Async` currently exposes only a minimal
`suspend(): ()` operation; executable
async/Future lowering will add its handler contracts in the same implementation slice rather than
pretending `await` already works.

`core.domains` owns standard compile-time domains, also outside the prelude:

```sc
pub let type = domain
pub let region = domain
pub let effect = domain

pub let access = domain {
  shared
  mut
}

pub let passing = domain {
  auto
  copy
  move
}
```

Borrow types and values are written with the declared `borrow` form: `borrow(T)`,
`borrow(mut)(T)`, and `borrow(A)(R)(T)`. `borrow(A)` and generic passing modes refer to these
domains in compile-time parameter positions.

`core.control` owns the edition-pinned contracts for compiler-lowered control functions. It is not
part of the prelude. `do`, `try`, `throw`, and `unsafe` are ordinary source-backed functions over
the standard effect declarations. The `unsafe` body removes the marker effect with
`Unsafe.handle()`; the compiler keeps only the lexical authority check for raw operations inside
`unsafe { ... }`. `loop` still needs primitive control-flow lowering, so its bodyless signature is
permitted only as a validated core lang item; ordinary package functions still require
`= { ... }` bodies.

It also declares the protocol and erased runtime contracts used by algebraic handler lowering:

```sc
pub let Continuation(Input: type, Output: type) = struct {}
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}
pub let Handle = trait(Self: effect) {
  let Clauses(Value: type, Answer: type): type
  let handle(Value: type, Answer: type, Rest: effect)
    (move clauses: Clauses(Value, Answer))
    (move action: (): Value with(Self, Rest)): Answer with(Rest)
}
```

`Continuation` is a one-shot suspended computation. `EffectCallable` is an owned action awaiting a
handler-supplied continuation from `Output` to `Answer`; `Input` is the action's packed runtime input.
Both native values carry call and drop entries, an environment pointer, and an ownership flag. They
are module exports rather than prelude names and cannot be replaced by same-named user declarations.
The compiler-internal action entry has the logical signature
`(environment, Input, Continuation(Output, Answer)): Answer`. Erasing or invoking an action consumes
its owner; a dropped, uninvoked action releases its captured environment through the stored drop
entry. `Handle` is an effect-kinded lang trait automatically satisfied by every source
`effect` declaration. Its `Clauses` associated constructor names the compiler-derived labeled
clause pack used by `.handle`, and its `handle` member records the public handler shape. The first
runtime group is a synthetic clause pack: source calls still write operation labels directly, for
example `State(i32).handle(get: ..., put: ...) { ... }`. These low-level operations and generated
handler implementations are not ordinary source-level standard-library functions.

```sc
pub let do(E: effect, T: type)
  (move action: (): T with(E)): T with(E)
pub let try(F: effect, T: type, E: type)
  (move action: (): T with(core.effects.Throws(E), F)): core.Result(E)(T) with(F)
pub let throw(Error: type)
  (move error: Error): Never with(core.effects.Throws(Error))
pub let unsafe(E: effect, T: type)
  (move action: (): T with(core.effects.Unsafe, E)): T with(E)
pub let loop(E: effect, T: type)
  (move body: (): () with(E)): T with(E)
```

Here `try` removes only `Throws(E)`, `unsafe` removes only the `Unsafe` requirement, and both forward
the remainder row. `throw` introduces the standard `Throws(Error)` requirement, while `do` and
`loop` forward the whole row. The source definitions are intentionally simple:

```sc
pub let do(E: effect, T: type)
  (move action: (): T with(E)): T with(E) = {
  action()
}

pub let try(F: effect, T: type, E: type)
  (move action: (): T with(core.effects.Throws(E), F)): core.Result(E)(T) with(F) = {
  core.effects.Throws(E).handle(
    raise: { (error) -> core.Result.Err(error) },
    done: { (value) -> core.Result.Ok(value) },
  ) {
    action()
  }
}

pub let throw(Error: type)
  (move error: Error): Never with(core.effects.Throws(Error)) = {
  core.effects.Throws(Error).raise(error)
}
```

`core.iter` owns iteration rather than the prelude:

```sc
pub let Iterator = trait {
  let Item: type
  let next(self: borrow(mut)(Self))
    (): core.Option(Item)
}

pub let IntoIterator = trait {
  let IntoIter: type
  let into_iter(move self)
    (): IntoIter
}
```

Implementing or naming either trait requires `use std.iter.{Iterator, IntoIterator}`. The `for`
syntax itself needs no import and dispatches only through these validated identities. It evaluates
the iterable once, moves it into `IntoIterator.into_iter`, repeatedly mutably borrows the resulting
iterator for `Iterator.next`, and stops on `None`. An inherent or unrelated trait method named
`into_iter` or `next` cannot intercept this lowering.

The control spellings bind to these validated identities without importing ordinary names. Standard
effect identities such as `Throws` remain normal `std.effect` exports when named in source, backed
by `core.effects` identities. An ordinary same-named declaration cannot acquire lang-item lowering
behavior. Future control features
follow the same rule: for example, async lowering must add `Future`, `async`, and handler contracts
to the matching core release when it becomes executable, rather than reserving undocumented compiler
magic in advance.

`core.algebra` contains first-order algebra protocols rather than putting them in the prelude:

```sc
pub let Semigroup = trait {
  let combine(move left: Self, move right: Self): Self
}

pub let Monoid = trait
where Self: Semigroup{let empty(): Self}
```

The compiler does not prove algebraic laws.

`core.functional` contains higher-kinded protocols over compile-time type constructors. It is not
part of the prelude:

```sc
pub let Functor = trait(Self: (Value: type): type) {
  let map(E: effect, A: type, B: type)
    (move self: Self(A))
    (move transform: (A): B with(E)): Self(B) with(E)
}

pub let Applicative = trait(Self: (Value: type): type)
where Self: Functor {
  let pure(A: type)
    (move value: A): Self(A)

  let apply(E: effect, A: type, B: type)
    (move self: Self((A): B with(E)))
    (move value: Self(A)): Self(B) with(E)
}

pub let Monad = trait(Self: (Value: type): type)
where Self: Applicative {
  let flat_map(E: effect, A: type, B: type)
    (move self: Self(A))
    (move next: (A): Self(B) with(E)): Self(B) with(E)
}
```

These declarations use constructor kinds such as `(Value: type): type` on the trait `Self` subject,
not as ordinary trait parameters. Traits with a matching constructor subject can be implemented for
generic nominal constructors. Method implementations are registered as generic function templates
and validated, for example `extend Carrier: Functor{let map(E, A, B)...}`. Receiver methods
dispatch from concrete nominal instances, so `Carrier(i32) { value: 41 }.map(add_one)` selects the
`Carrier: Functor` implementation and instantiates the generic method template. Constructor
associated functions without a receiver can still be called from the bare constructor; for example,
`Carrier.pure(...)` is available once `Carrier` implements `Applicative`. Trait-level `where`
constraints express protocol inheritance, so a
`Carrier: Applicative` implementation also requires `Carrier: Functor`, and `Carrier: Monad`
requires `Carrier: Applicative`.

The standard library implements `Functor`, `Applicative`, and `Monad` for `core.Option` and for
each partially applied `core.Result(Error)` constructor:

```sc
use std.Result
use std.functional.Monad

let next(value: i32): Result(bool)(i32) = {
  Result(bool)(i32).Ok(value + 1)
}

let value = Result(bool)(i32).Ok(41).flat_map(next)
```

Curried constructors may be used as constructor trait implementation targets, which is how
`Result(Error): Monad` is expressed without making `Result` special. Associated-type lowering and
broader constructor equation solving remain future semantic work.

`ControlFlow`, the old propagation `Try`, `FromResidual`, and `FromError` were removed together with postfix `.try`. `Option` and
`Result` are ordinary enum values and require explicit constructors. Language error propagation is
defined by the standard `Throws(E)` effect, `throw`, and `try { ... }`; `do` has no error-specific
semantics.

Primitive implementations remain compiler-defined. The unit type has the single spelling `()`. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import policy and
[the language specification](../language/specification.md) for semantic rules.
