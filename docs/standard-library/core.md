# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sali` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- `Option(T)` and `Result(T, E)`
- the uninhabited `never` type
- the `Copy` and `Drop` traits

`core.ops` contains the arithmetic protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`, the equality
protocol `Eq`, the ordering protocol `PartialOrd`, and the unary protocols `Neg` and `Not`. They are
not in the prelude. Arithmetic protocols consume their operands and use an associated `Output` type:

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

Writing `left + right`, `left == right`, or `left < right` does not itself require an import. An
import is required when source names the protocol in an implementation, bound, type, or direct
member access.

`core.control` contains the error-control protocols `ControlFlow`, `Try`, `FromResidual`, and
`FromError`. They are ordinary, explicitly imported names:

```sali
use core.control.{Try, FromResidual}
```

The compiler validates their complete edition-defined declaration shapes before compiling a
package. `Option` and `Result` implement these protocols in ordinary generic core extensions.
Postfix `.try` and `throw` validate those edition identities. User-defined nominal types may
implement `Try` and serve as either operands or explicit function propagation boundaries. Normal
function completion and `return` use `from_output`, `.try` uses `FromResidual`, and `throw` uses
`FromError`. Explicit `try do` expressions use the same protocols while establishing a nearer
propagation boundary.

Primitive implementations and the unit spelling `void` remain compiler-defined. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import policy and
[the language specification](../language/specification.md) for semantic rules.
