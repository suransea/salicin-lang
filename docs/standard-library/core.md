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
  let add(move self)(move rhs: Number): Number = ...
}
```

`Eq(Rhs)` borrows both operands and returns `bool`; `!=` invokes the same method exactly once and
negates its result:

```sali
use core.ops.Eq

extend Number: Eq(Number) {
  let eq(borrow self)(borrow rhs: Number): bool = self.value == rhs.value
}
```

`PartialOrd(Rhs)` also borrows both operands. Its `partial_cmp` method returns `PartialOrdering`,
whose variants are `Less`, `Equal`, `Greater`, and `Unordered`. All four ordering operators invoke
the method once; an `Unordered` result makes each operator false:

```sali
use core.ops.{PartialOrd, PartialOrdering}

extend Number: PartialOrd(Number) {
  let partial_cmp(borrow self)(borrow rhs: Number): PartialOrdering = ...
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

`core.control` contains the error-control protocols `ControlFlow`, `Try`, `FromResidual`, and
`FromError`. They are ordinary, explicitly imported names:

```sali
use core.control.{Try, FromResidual}
```

The compiler validates their complete edition-defined declaration shapes before compiling a
package. `Option` and `Result` implement these protocols in ordinary generic core extensions, so
libraries may use them for explicit container algorithms and normal function completion may use
`Try.from_output` as value-construction sugar. They no longer control language error propagation:
`throws(E)`, `throw`, and `try { ... }` are built-in effect semantics, and user-defined `Try`
implementations cannot intercept them. `do` itself has no error-specific semantics.

Primitive implementations and the unit spelling `void` remain compiler-defined. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import policy and
[the language specification](../language/specification.md) for semantic rules.
