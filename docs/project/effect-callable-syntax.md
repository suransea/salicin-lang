# Effect Callable Syntax

Status: accepted and implemented for Edition 2026

## Contract

`with(E)(F)` is a type constructor that adds the normalized effect row `E` to
the callable type `F`. Its operand must be callable:

```salicin
with(io)((str): string)
with(e)((i32): i32)
```

The row belongs to the complete callable, including every runtime parameter
group. `with()((A): B)` is equivalent to the pure callable `(A): B`.
A non-callable operand such as `with(io)(i32)` is rejected.

An effectful declaration places a callable-type/body boundary after its name
and compile-time parameter groups:

```salicin
let read: with(io)(path: str): string = { ... }

let apply(comptime e: effects): with(e)
  (action: with(e)((i32): i32))
  (value: i32): i32 = {
  action(value)
}
```

The first colon starts the runtime callable type; the final colon introduces
its result. A pure declaration stays compact:

```salicin
let identity(value: i32): i32 = { value }
```

`let f(...): with(e)(R)` is not an effect annotation: it attempts to use
`with` on a non-callable result and is rejected. This keeps the result
position available for future task or computation types.

## Migration

The Edition 2026 grammar and library sources use only the prefix form. The
parser temporarily accepts the former postfix form as migration input, but it
is not canonical syntax and new documentation, fixtures, and formatter tests
must not produce it. A later edition may remove that compatibility path.

The surface rewrite does not change the semantic representation:
`Type::Function` continues to carry one normalized row. It therefore does not
change handler lowering, cleanup, ownership, or calling convention.

## Research Basis

Recent modal-effect work shows that effect tracking can be separated cleanly
from the underlying function type, which motivates making `with(E)` an
explicit callable constructor rather than decorating a result. Recent work on
linear effects and automatic resource analysis also reinforces that exception
and handler syntax must preserve cleanup and resource-safety semantics; this
migration deliberately changes no such semantics.

- [Rows and Capabilities as Modal Effects (POPL 2026)](https://doi.org/10.1145/3776674)
- [Linear Effects, Exceptions, and Resource Safety (ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
- [Handling Exceptions and Effects with Automatic Resource Analysis (OOPSLA 2026)](https://arxiv.org/abs/2603.02260)
