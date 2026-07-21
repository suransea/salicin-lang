# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sali` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- `Option(T)` and `Result(T, E)`
- the uninhabited `never` type
- the `Copy` and `Drop` traits

`core.ops` contains the arithmetic protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`, plus the equality
protocol `Eq`. They are not in the prelude. Arithmetic protocols consume their operands and use an
associated `Output` type:

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

Writing `left + right` or `left == right` does not itself require an import. An import is required when source names the
protocol in an implementation, bound, type, or direct member access.

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
