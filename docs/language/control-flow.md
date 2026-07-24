# Control-flow design

Status: target design. The compiler may implement individual parts incrementally, but new control
features must converge on this model rather than add unrelated statement-only syntax.

## 1. One model

Salicin control flow has three layers:

1. `return`, `break`, and `continue` are ordinary source-declared effect operations. Their familiar
   spellings are compiler fast paths to the validated `core.control` identities.
2. `if`, `loop`, `while`, `for`, and `match` have source-visible callable contracts. The compiler
   recognizes only the validated core identities and lowers them without allocating closures or
   case values.
3. Blocks after a control call are trailing values: total closures for lazy computation and
   partial closures for pattern selection.

The parser may retain contextual productions to resolve braces, but name resolution and type
checking must happen as if the corresponding core callable had been invoked. An unrelated
same-named declaration never acquires control authority.

## 2. Lazy control calls

`if` takes an eager condition and lazy branches:

```sc
pub let if(T: type, E: effect)
  (condition: bool)
  (move then: (): T with(E))
  (move else: (): T with(E)): T with(E)

pub let if(E: effect)
  (condition: bool)
  (move then: (): () with(E)): () with(E)
```

The ordinary source forms are therefore:

```sc
if condition {
  then_value
} else {
  else_value
}

if first {
  a
} else if second {
  b
} else {
  c
}
```

`else if` is not a combined grammar production. `else` is a named lazy argument whose expression is
another `if` call, implicitly wrapped in a zero-parameter closure by the general trailing-call rule.
The condition remains eager; making it a closure would obscure evaluation order without adding
expressive power.

`loop` and `while` establish the loop control handlers:

```sc
pub let loop(T: type, E: effect)
  (move body: (): () with(Break(T), Continue, E)): T with(E)

pub let while(E: effect)
  (move condition: (): bool with(Break(()), Continue, E))
  (move body: (): () with(Break(()), Continue, E)): () with(E)
```

They are called as:

```sc
let answer = loop {
  if ready() {
    break 42
  }
}

while {
  queue.has_items()
} {
  queue.process_one()
}
```

The two `while` closures are intentionally symmetric. The former `while condition { body }` form is
not part of the target language.

## 3. Control effects

The source contracts are:

```sc
pub let Break(T: type) = effect {
  let exit(move value: T): Never
}

pub let Continue = effect {
  let next(): Never
}

pub let Return(T: type) = effect {
  let exit(move value: T): Never
}

pub let break(T: type)
  (move value: T): Never with(Break(T))
pub let break(): Never with(Break(()))

pub let continue(): Never with(Continue)

pub let return(T: type)
  (move value: T): Never with(Return(T))
pub let return(): Never with(Return(()))
```

The compiler accepts the familiar fast paths:

```sc
break value
break
continue
return value
return
```

The parenthesized calls remain valid. A control function may be stored or captured; doing so is
safe because its effect remains in its type. Calling a captured `break` outside a compatible loop
handler, or a captured `return` outside the corresponding function boundary, fails effect
checking. No lexical ban on escaping control callables is required.

`loop` handles the nearest `Break(T)` and `Continue`. A function boundary handles its own
`Return(T)`. The handled effect is removed from the result row while all unrelated effects are
forwarded.

## 4. Pattern cases are partial functions

An irrefutable parameter closure is a total function. A refutable pattern defines a partial
function: applying `{ Some(value) -> value }` to `None` does not enter its body. This partiality is
neither an error nor an algebraic effect. It is the dispatch result consumed by `match`.

Pattern closures have anonymous concrete types and implement the same call-capability hierarchy as
ordinary closures, but through partial-call traits:

```sc
pub let Attempt(Input: type, Output: type) = enum {
  Miss(Input),
  Hit(Output),
}

pub let PartialFnOnce(
  Input: type,
  Output: type,
  E: effect,
) = trait {
  let attempt(move self)
    (move input: Input): Attempt(Input, Output) with(E)
}

pub let PartialFnMut(Input: type, Output: type, E: effect) = trait
where Self: PartialFnOnce(Input, Output, E) {
  let attempt(self: borrow(mut)(Self))
    (move input: Input): Attempt(Input, Output) with(E)
}

pub let PartialFn(Input: type, Output: type, E: effect) = trait
where Self: PartialFnMut(Input, Output, E) {
  let attempt(self: borrow(Self))
    (move input: Input): Attempt(Input, Output) with(E)
}
```

As with `FnOnce`/`FnMut`/`Fn`, the compiler derives the strongest valid partial-call capability from
capture use. A matcher that consumes temporary arms needs only `PartialFnOnce`. When the compiler
proves the parameter pattern irrefutable, the same anonymous closure also implements the
corresponding total `FnOnce`/`FnMut`/`Fn` capability; its `attempt` can only produce `Hit`.

A case literal uses an unparenthesized pattern before `->`:

```sc
{ Some(value) -> value }
{ _ -> 0 }
{ Point(x, y) -> x + y }
```

This stays unambiguous with ordinary closures:

```sc
{ (value: i32) -> value + 1 } // ordinary callable
{ value -> value + 1 }        // pattern closure; irrefutable, so also a total callable
```

On `Miss`, `attempt` returns the exact unmatched input so another partial function can try it. This
is essential for non-`Copy` input: pattern inspection may read the discriminant and borrow fields,
but it cannot commit moves until the pattern and guard have both succeeded. On `Hit`, ownership
transfers into the selected bindings and body.

A partial closure retains its body's latent effect row and may be stored, moved, or passed to another
matcher. Calling `attempt` directly exposes `Attempt`; only an exhaustive matcher may erase the
`Miss` possibility and return `Output` directly. In an effect-kind argument position,
`with(A, B, E)` denotes row union.

Case guards use contextual `if`:

```sc
{ Some(value) if value > 0 -> value }
```

A guarded case never contributes to exhaustiveness because its guard may reject the input.

## 5. Heterogeneous repeated runtime groups

Every capturing partial closure has a different anonymous concrete type. Control matching therefore
needs a statically known pack of case types and the same number of curried runtime groups:

```sc
pub let match(
  Input: type,
  Output: type,
  E: effect,
  ...Cases: type,
)
  (move input: Input)
  ...(move case: Cases): Output with(E)
where ...Cases: PartialFnOnce(Input, Output, E)
```

`...Cases: type` is a compile-time type pack. In `...(move case: Cases)`, every pack element expands
to one complete runtime parameter group, preserving heterogeneous closure types. A repeated
`where` predicate constrains every element independently. Calls remain statically expanded; there
is no runtime array, iterator, allocation, or dynamic arity.

This is deliberately different from:

```sc
(...move parameters: P)
```

which expands one compile-time `P: parameters` schema inside one runtime group. The two constructs
have different delimiters and cannot be confused:

- `...(group)` repeats groups;
- `(...name: P)` expands parameters inside one group.

The earlier candidate

```sc
...Cases: parameter
...
...Cases: Output with(E)
```

is not used. The insight that a case is a partial function lets the ordinary type system carry its
anonymous type, ownership capability, input, output, and effect. A `parameter` pack would make one
metavariable simultaneously describe parameter declarations, patterns, runtime groups, and branch
result constraints.

## 6. `match`

Prefix `match` is the control call:

```sc
match option
  { Some(value) -> value }
  { None -> 0 }
```

It evaluates the input exactly once and tests cases from top to bottom. Every reachable body must
produce the common `Output`; their effect rows are widened to the inferred `E`. The compiler checks
exhaustiveness and reports unreachable unguarded cases.

Because `match` is the consumer of a case sequence, putting it before the input keeps every case
adjacent to the operation that consumes it. The target language does not retain postfix
`value match { ... }`.

`if let` is unnecessary:

```sc
match option
  { Some(value) -> use(value) }
  { _ -> () }
```

`while let` is the explicit composition of `loop` and `match`:

```sc
loop {
  match next()
    { Some(value) -> consume(value) }
    { None -> break }
}
```

There is no separate `if let` or `while let` grammar in the target language.

## 7. `for`

`for` takes the iterable eagerly and an irrefutable pattern case lazily:

```sc
pub let for(
  Iterable: type,
  Iter: type,
  Item: type,
  E: effect,
  Body: type,
)
  (move iterable: Iterable)
  (move body: Body): () with(E)
where Iterable: core.iter.IntoIterator(IntoIter = Iter),
      Iter: core.iter.Iterator(Item = Item),
      Body: PartialFnOnce(Item, (), with(Break(()), Continue, E))
```

The surface form is:

```sc
for collection {
  item -> consume(item)
}

for points {
  Point(x, y) -> draw(x, y)
}
```

The body pattern must be irrefutable for `Item`. A refutable enum case is rejected rather than
silently skipping elements:

```sc
// Rejected when Item is Option(T).
for values {
  Some(value) -> consume(value)
}
```

Explicit filtering uses `match` inside the body:

```sc
for values {
  item -> match item
    { Some(value) -> consume(value) }
    { None -> () }
}
```

`for pattern in iterable` is not retained in the target grammar. Moving the iterable immediately
after `for` aligns it with every other eager-first control call, while the trailing case owns all
per-iteration binding.

`for` evaluates the iterable once, lowers through the validated `IntoIterator` and `Iterator`
identities, and handles `Break(())` plus `Continue` around both iterator advancement and the body.

## 8. Surface summary

```sc
if condition { then } else { otherwise }
if condition { then } else if other { second } else { otherwise }

loop { body }
while { condition } { body }
for iterable { pattern -> body }

match value
  { pattern -> result }
  { _ -> fallback }

break value
break
continue
return value
return
```

Only braces following `match` or `for` may begin with a bare pattern followed by `->`. Ordinary
trailing braces remain closure literals. This contextual distinction lets the parser remain
deterministic without reserving constructor names or introducing a second pattern namespace.

## 9. Compiler lowering obligations

The fast path must preserve the callable contracts:

- resolve and validate the core lang-item identity before granting special lowering;
- evaluate eager inputs exactly once and from left to right;
- evaluate only the selected lazy branch or case body;
- preserve `Case` and closure capture ownership;
- infer and subtract only the control effects handled by the construct;
- run lexical cleanup on `return`, `break`, and `continue`;
- check match exhaustiveness, unreachable cases, guards, and irrefutability for `for`;
- produce no runtime `Case` objects or closure allocations when the complete call is statically
  visible.

Partial application or aliasing may fall back to the ordinary callable representation. It must have
the same observable behavior as the optimized complete call.
