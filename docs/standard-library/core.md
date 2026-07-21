# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sali` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

## Modules

`core.prelude` contains the deliberately small implicit surface:

- `Option(T)` and `Result(T, E)`
- the uninhabited `never` type
- the `Copy` and `Drop` traits

`core.ops` contains the operator protocols `Add`, `Sub`, `Mul`, `Div`, and `Rem`. Each protocol uses
an associated `Output` type and a method corresponding to its operator.

Primitive implementations and a few bootstrap aliases remain compiler-defined. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/import migration and
[the language specification](../language/specification.md) for semantic rules.
