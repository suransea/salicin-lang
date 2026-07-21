# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sc` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- `Option(T)` and `Result(T, E)`
- the uninhabited `never` type
- the `Copy` and `Drop` traits

`core.ops` contains the arithmetic protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`, the equality
protocol `Eq`, the ordering protocol `PartialOrd`, the unary protocols `Neg` and `Not`, and the
bitwise protocols `BitAnd`, `BitOr`, `BitXor`, `Shl`, and `Shr`. They are not in the prelude.
Arithmetic and bitwise protocols consume their operands and use an associated `Output` type:

```sali
use core.ops.Add

extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { ... }
}
```

`Eq(Rhs)` borrows both operands and returns `bool`; `!=` invokes the same method exactly once and
negates its result:

```sali
use core.ops.Eq

extend Number: Eq(Number) {
  let eq(borrow self)(borrow rhs: Number): bool = { self.value == rhs.value }
}
```

`PartialOrd(Rhs)` also borrows both operands. Its `partial_cmp` method returns `PartialOrdering`,
whose variants are `Less`, `Equal`, `Greater`, and `Unordered`. All four ordering operators invoke
the method once; an `Unordered` result makes each operator false:

```sali
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

Writing `left + right`, `left & right`, `left == right`, or `left < right` does not itself require an import. An
import is required when source names the protocol in an implementation, bound, type, or direct
member access.

`core.control` owns the edition-pinned contracts for compiler-lowered control constructs. It is not
part of the prelude. The module declares the `Unsafe` and parameterized `Throws(E)` effect identities,
the `Shared` and `Mutable` access identities, and the compiler-provided `do`, `try`, `unsafe`, and
`loop` trailing-closure functions. Their bodyless signatures are permitted only for validated core
lang items; ordinary package functions still require `= { ... }` bodies.

```sali
pub let do(E: effect, T: type)(move action: (): T with(E)): T with(E)
pub let try(F: effect, T: type, E: type)
  (move action: (): T with(throws(E), F)): Result(T, E) with(F)
pub let unsafe(E: effect, T: type)
  (move action: (): T with(unsafe, E)): T with(E)
pub let loop(E: effect, T: type)(move body: (): () with(E)): T with(E)
```

Here `try` removes only `throws(E)`, `unsafe` removes only the unsafe requirement, and both forward
the remainder row. `do` and `loop` forward the whole row.

The lowercase syntax spellings bind to these validated identities without an import. An ordinary
same-named declaration cannot acquire their lowering behavior. Future control features follow the
same rule: for example, async lowering must add its effect, `Future`, `async`, and `await` contracts
to the matching core release when it becomes executable, rather than reserving undocumented compiler
magic in advance.

`ControlFlow`, the old propagation `Try`, `FromResidual`, and `FromError` were removed together with postfix `.try`. `Option` and
`Result` are ordinary enum values and require explicit constructors. Language error propagation is
defined solely by `throws(E)`, `throw`, and `try { ... }`; `do` has no error-specific semantics.

Primitive implementations remain compiler-defined. The unit type has the single spelling `()`. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import policy and
[the language specification](../language/specification.md) for semantic rules.
