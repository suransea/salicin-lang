# Salicin Language Specification

Status: evolving language specification

This document defines the meaning of Salicin source programs. It describes the current language,
not compiler internals, historical designs, or planned features. The precise parser grammar is in
[Grammar](grammar.md). Implemented coverage and remaining work are tracked in
[Implementation status](../project/status.md) and the [roadmap](../project/roadmap.md).

The source file extension is `.sc`. Source text is UTF-8.

## 1. Language Model

Salicin is a statically typed, statically compiled language with:

- uniform `let` declarations;
- curried compile-time and runtime parameter groups;
- deterministic left-to-right evaluation;
- move, copy, shared-borrow, and mutable-borrow parameter passing;
- lexical ownership and deterministic cleanup;
- nominal structs and enums;
- monomorphized generics and statically dispatched traits;
- algebraic effects with source-declared handlers.

Every expression has a type. `()` is the sole unit type and unit value. `Never` is the prelude's
ordinary uninhabited enum and coerces to any expected type.

Language syntax is source-backed unless this specification explicitly calls it primitive. A
source-backed construct resolves to a canonical declaration in the edition's `core` library. The
implementation validates that declaration's module identity and exact contract before granting
syntax-directed lowering; spelling the same name in user code grants no special behavior. Such
lowering may avoid ordinary call or closure allocation, but must remain observationally equivalent
to the validated declaration, including evaluation order, effects, ownership, and cleanup.

## 2. Lexical Rules

Identifiers are case-sensitive, follow Unicode XID rules, and are compared after NFC
normalization. Package names, file-module names, and foreign link names have narrower ASCII rules.

```sc fragment
let answer = 42
let café = answer
```

`//` introduces a line comment. `/* ... */` introduces a nestable block comment.

The lexer preserves logical newlines. A physical newline is ignored inside unmatched parentheses
or brackets, and after a token that necessarily continues an expression. Otherwise it separates
statements. A semicolon explicitly discards an expression's value.

```sc fragment
let value = add(
  20,
  22,
)

do {
  observe(value);
  value
}
```

Fixed structural keywords include `let`, `struct`, `enum`, `trait`, `extend`, `pub`, `where`,
`root`, `super`, and `package`. Words such as `type`, `domain`, `access`, `region`, `effect`,
`parameters`, `borrow`, `copy`, `move`, `shared`, `mut`, and control-operation names are
contextual: they retain their special meaning only in the corresponding grammatical position.

Region binders are ordinary identifiers declared by `R: region`. Diagnostics may display inferred
regions with a leading apostrophe; that rendering is not source binder syntax. `'static` is a
predefined region identity and cannot be redeclared.

## 3. Declarations and Names

`let` introduces an immutable binding. `let mut` introduces a value binding that may be reassigned
to another value of the same type.

```sc fragment
let width: i32 = 40
let mut height = 1
height = height + 1
```

A binding is not visible in its own initializer, except that a named function is visible in its
body for recursion. The same lexical scope cannot declare the same name twice. An inner scope may
shadow an outer binding.

Top-level `let` declarations introduce values, functions, types, type aliases, domains, effects,
traits, or modules according to their annotation and initializer.

```sc fragment
let Scalar = i32
let Point = struct { x: i32, y: i32 }
let add(x: i32)(y: i32): i32 = { x + y }
```

Named functions have a callable overload namespace distinct from nominal type names. This permits
an explicit factory function to share a name with its result type. Other top-level declarations
must not conflict.

### 3.1 Visibility

Declarations are private to their module and descendant modules by default. `pub(package)` exposes
a declaration throughout its package. `pub` also exposes it to dependants.

```sc fragment
pub let Point = struct {
  pub x: i32,
  pub y: i32,
}
```

The effective visibility of a field or implementation member cannot exceed that of its owner.
Public signatures must not expose less-visible entities. Visibility checks recursively inspect
generic arguments, fields, enum payloads, associated types, and inferred public result types.

### 3.2 Test Registrations

A top-level test registration has call-like syntax with one compile-time name
and one trailing body:

```sc fragment
test("arithmetic") {
  20 + 22 == 42
}
```

`test` is contextual in this position. The form registers the body with the
test target during compilation; it is not an ordinary runtime call and does
not introduce a source-visible declaration. The name must be a non-empty
string literal and is used in diagnostics. Registrations are private to their
source package and cannot have visibility or attributes.

The body is evaluated as a parameterless function returning `bool`. `true`
passes and `false` fails. Its effects must be discharged within the body under
the ordinary effect rules. Test bodies are excluded from ordinary program and
library builds. `salic test` collects registrations from the selected package,
links one native runner, executes tests in source order, and reports the first
failing name. A target with no registrations is an error. The current runner
supports at most 254 registrations per target.

### 3.3 Domains

A domain classifies compile-time values. An abstract domain has no source-enumerable set of values:

```sc fragment
let type: domain
```

A defined domain lists its complete members:

```sc fragment
let optimization = domain {
  debug
  release
}

let Empty = domain {}
```

An abstract domain and an empty defined domain are different. `let Name = domain` is invalid:
abstract domains use `: domain`, while defined domains use `= domain { ... }`.

`type`, `region`, `effect`, and `parameters` are compile-time domains. `access` and `bool` are
ordinary closed enums whose values can also classify compile-time parameters. Any other closed enum
or defined domain can be used the same way.

### 3.4 Compiler Definitions

The embedded `core` package declares compiler-provided definitions with the
complete initializer `builtin()`. The bootstrap declaration is private and
has the unique exact shape:

```sc fragment
let builtin(): Never = builtin()
```

The initializer marks the containing declaration; it is not an ordinary call
whose `Never` result is coerced to the annotation. Semantic analysis obtains
the definition's kind or type from that annotation, validates the complete
edition-owned signature, and resolves the marker before code generation.
Compiler-owned types and type constructors use the same form:

```sc fragment
pub let i32: type = builtin()
pub let Array(T: type)(L: usize): type = builtin()
pub let size_of(T: type): u64 = builtin()
```

`builtin()` is private to `core`. User functions, types, extension methods,
and globals cannot use it. Unknown core markers are invalid. Trait
requirements, effect operations, and user opaque types are genuinely
abstract and remain bodyless; they do not receive builtin default
implementations.

## 4. Types and Compile-Time Parameters

The primitive integer families are:

```text
i8 i16 i32 i64 i128 isize
u8 u16 u32 u64 u128 usize
```

`isize` and `usize` have the target pointer width. Integer arithmetic is checked; overflow,
division by zero, and invalid shifts trap rather than producing an unspecified value.

`bool` is an ordinary closed enum with `false` and `true`. `()` is unit. `Never` is uninhabited.
Arrays, borrows, raw pointers, tuples, function types, structs, and enums are type constructors.

Compile-time parameters occur in their own parameter groups:

```sc fragment
let identity(T: type)(value: T): T = { value }
let first(T: type, L: usize)(values: Array(T)(L)): T = { values[0] }
```

Supported compile-time parameter kinds include:

- `T: type`;
- `L: usize`;
- `R: region`;
- `E: effect`;
- `P: parameters`;
- `A: access`;
- values of another closed compile-time type;
- bounded type and effect constructor kinds.

Compile-time arguments participate in overload selection and monomorphization, then are erased
from runtime calling conventions. A rejected explicit or inferred argument is diagnosed against
its source binder and compile-time kind. Group arity, unknown labels, kind mismatches, and
underconstrained inference are distinct errors.

### 4.1 Type Constructors and Aliases

Each parenthesized compile-time group is a distinct constructor layer:

```sc fragment
pub let Array(T: type)(L: usize): type = core.memory.Array(T)(L)
let Result(Error: type)(Value: type) = enum {
  Ok(Value)
  Err(Error)
}
```

`Array(i32)(4)` applies two groups. It is not equivalent to `Array(i32, 4)`.

A type alias is transparent and preserves the identity of its target:

```sc fragment
let Scalar = i32
let Family(T: type): type = core.Option(T)
let Constructor: (T: type): type = core.Option
```

Alias expansion must terminate. Cyclic aliases and arity or kind mismatches are rejected.

### 4.2 Borrow, Pointer, and Array Types

The safe reference constructor is:

```sc fragment
borrow(A: access = shared)(R: region)(T: type)
```

When omitted, access is `shared` and the region is inferred. The common forms are `borrow(T)`,
`borrow(mut)(T)`, and `borrow(R)(T)`.

The raw pointer family is:

```sc fragment
Ptr(A: access = shared)(T: type)
```

Raw pointer dereference, arithmetic that can leave an allocation, initialization, and ownership
reconstruction require an `unsafe` boundary.

The fixed-size array family is:

```sc fragment
Array(T: type)(L: usize)
```

Array length is part of the type. Array indexing evaluates its base and index once and performs a
bounds check.

## 5. Functions and Application

A function declaration may contain multiple compile-time and runtime parameter groups:

```sc fragment
let map(T: type, U: type)(value: T)(transform: (T): U): U = {
  transform(value)
}
```

Application consumes one group at a time:

```sc fragment
let add(x: i32)(y: i32): i32 = { x + y }
let add_two = add(2)
let answer = add_two(40)
```

Supplying a group creates or invokes the next function layer. Arguments for a supplied group are
evaluated left to right. A partial application performs the passing actions for supplied arguments
but does not execute the final body until all runtime groups are supplied.

### 5.1 Unary Groups, Labels, and Trailing Closures

Runtime parameters may be labeled. Positional arguments must precede labeled arguments, and each
parameter is supplied exactly once.

```sc fragment
let clamp(value: i32, min lower: i32, max upper: i32): i32 = { ... }
let bounded = clamp(42, min: 0, max: 100)
```

One positional argument may omit its parentheses when it supplies a runtime
group containing exactly one parameter:

```sc fragment
let increment(value: i32): i32 = { value + 1 }
let apply(value: i32)(move action: (i32): i32): i32 = { action(value) }

let answer = apply 40 { (value: i32) -> increment value }
```

Each bare argument supplies a separate group, so `f x y` means `f(x)(y)`.
Application binds more tightly than infix operators. Parentheses remain
required for empty groups, groups with multiple parameters, labeled
arguments, and a compound expression intended as one bare argument.

A trailing closure supplies the next unapplied function group. It may supply the first group
directly (`run { action() }`) without a preceding parenthesized group. Multiple trailing closures
supply successive groups. A label may precede a trailing closure.

```sc fragment
if condition then {
  on_true()
} else {
  on_false()
}
```

### 5.2 Function Types and Closures

A function type records its runtime groups, result, and effect row:

```sc fragment
let apply(T: type, U: type)(value: T)(function: (T): U): U = {
  function(value)
}
```

Closure literals use block syntax when an expected function type determines their parameters, or
an explicit parameter list when needed:

```sc fragment
let increment: (i32): i32 = { value -> value + 1 }
```

Closures capture referenced outer bindings. Shared captures can be copied when their complete
environment is copyable. Mutable and owning captures obey the same exclusivity and move rules as
explicit parameters.

## 6. Ownership and Borrowing

Every runtime parameter has a passing mode:

- `copy` duplicates a `Copy` value;
- `move` transfers ownership of a `Move` value;
- `borrow` creates a shared loan;
- `borrow(mut)` creates an exclusive loan.

```sc fragment
let consume(T: type)(move value: T): () = { ... }
let inspect(T: type)(value: borrow(T)): () = { ... }
let update(T: type)(value: borrow(mut)(T)): () = { ... }
```

An omitted mode uses the type's default: `Copy` values are copied and resource values are moved.
An explicit mode always takes precedence.

`Move` is a source-backed structural auto marker. Scalars, borrows, raw pointers, and aggregates
whose owned members are all `Move` may be relocated. `Copy` inherits `Move`; duplicating a value
therefore always implies that either resulting value may also be relocated. Parameter transfer,
return, assignment from an existing place, and movement into reallocating storage require `Move`.
Direct in-place initialization does not relocate an existing value and does not require it.

A moved binding cannot be read, moved, or borrowed again. Moving one field leaves other fields
usable, but the aggregate cannot be used as a whole until reinitialized. Assignment drops the old
initialized value before installing the new value.

Shared loans may overlap. A mutable loan excludes all other loans to the same place. A loan cannot
outlive its source, escape through a longer result region, or remain live across an invalidating
mutation. Reborrowing may shorten a loan but never lengthen it.

Lexical scope exit drops each still-initialized owned value exactly once, in reverse initialization
order. The same rule applies to normal completion, `return`, `break`, handled effects, and partial
initialization cleanup.

### 6.1 Access Polymorphism

An `A: access` parameter selects shared or mutable borrowing without defining two APIs:

```sc fragment
let view(A: access)(T: type)(value: borrow(A)(T)): borrow(A)(T) = {
  value
}
```

Access is a compile-time value. A mutable instantiation preserves exclusivity; it is not a runtime
flag or a subtype of shared access.

## 7. Structs, Enums, and Patterns

Structs are nominal product types:

```sc fragment
let Point = struct {
  x: i32,
  y: i32,
}

let origin = Point { x: 0, y: 0 }
```

Fields are initialized left to right. Every required field must appear exactly once. Field access
preserves the ownership and borrow state of the base.

`struct(c)` selects the target C aggregate representation as part of the type
constructor:

```sc fragment
let Timespec = struct(c) {
  seconds: i64,
  nanoseconds: i64,
}
```

It is not a general representation modifier. A C-representation struct must
be non-empty, and each concrete field must be an integer, a raw pointer, a
non-zero fixed array of another valid C field type, or another `struct(c)`.
In particular, `bool`, borrows, tuples, enums, and ordinary Salicin structs
are rejected because their layout is not part of this C data contract.
Generic `struct(c)` constructors are validated after their compile-time
arguments are instantiated. The target's ordinary non-packed C alignment and
padding rules determine `size_of` and `align_of`.

Enums are nominal closed sums:

```sc fragment
let Option(T: type) = enum {
  None
  Some(T)
}
```

Patterns may match literals, bindings, tuples, structs, and enum variants. `_` discards a matched
value. A refutable pattern is accepted only where failure has an explicit control path.

`match` evaluates its scrutinee once and tests arms in source order. Guards run only after their
pattern succeeds. Arms must agree on a result type, except that `Never` coerces to the other arm
type. A match over a closed type must be exhaustive.

```sc fragment
match value {
  Option(i32).Some(number) -> number
} {
  Option(i32).None -> 0
}
```

## 8. Traits, Extensions, and Static Dispatch

A trait declares associated types and required or default methods:

```sc fragment
let Iterator = trait {
  let Item(R: region): type
  let next(R: region)(self: borrow(mut)(R)(Self)): core.Option(Item(R))
}
```

An `extend` block adds inherent members or implements a trait:

```sc fragment
extend Point {
  let translated(self: borrow(Self))(dx: i32, dy: i32): Point = {
    Point { x: self.x + dx, y: self.y + dy }
  }
}
```

Trait dispatch is static. Implementations are selected by the concrete subject and trait
arguments; no runtime dictionary or implicit open-world dispatch is introduced.

Where predicates constrain generic declarations:

```sc fragment
let duplicate(T: type)(value: T): (T, T)
where T: core.marker.Copy = {
  (value, value)
}
```

Associated type equalities refine a bound:

```sc fragment
let produce(T: type)(value: T): i32
where T: Produce(Item = i32) = {
  value.produce()
}
```

Generic associated constructors retain their parameter groups and kinds. Their receiver region can
determine a yielded type, as in `Iterator.Item(R)`.

Where predicates can equate a generic associated constructor with a type expression by declaring
alpha-renamable binders on the left:

```sc fragment
let borrow_item(T: type)(value: T): ()
where T: Iterator(Item(R: region) = borrow(R)(i32)) = { ... }
```

The binder groups and kinds must exactly match the associated declaration. The right side may use
outer compile-time parameters and its own binders. Transparent aliases are expanded before
comparison. Equation rewriting is direct, uses at most 32 nested expansions, and does not infer
missing binders or reorder groups.

An implementation is legal only in the package that owns the trait or the nominal target. Two
applicable implementations with the same static key are rejected.

Operators are syntax for methods of validated source traits. Ordinary methods with the same name
cannot intercept operator dispatch. In particular, prefix `!value` invokes the validated
`core.ops.bit.Not.not` contract; this is distinct from the postfix propagation operator described
in section 11.

## 9. Blocks and Control Flow

A block evaluates statements in order. Its final expression is the block value. An explicit
semicolon turns the preceding expression into `()`.

`if`, `match`, loops, and exits are expression forms supplied through validated control contracts.
Conditions have type `bool`.

```sc fragment
let absolute = if value < 0 {
  -value
} else {
  value
}
```

The principal source contracts in `core.control` are:

```sc fragment
pub let if(E: effect, T: type)
  (condition: bool)
  (move then: (): T with(E))
  (move else: (): T with(E)): T with(E)

pub let while(E: effect)
  (move condition: (): bool with(E))
  (move do: (): () with(E)): () with(E)
```

The surface forms supply their branch, condition, and body blocks as lazy callable groups. The
canonical declarations for `do`, `loop`, `match`, and `for` are validated in the same way.
`break`, `continue`, and `return` resolve to the canonical `core.control` functions, which introduce
the corresponding `Break(T)`, `Continue`, or `Return(T)` effect before the enclosing construct
handles it. A same-named user declaration cannot redirect any of these forms. The complete
contracts and their lowering obligations are specified in [Control-flow contracts](control-flow.md).

`loop { ... }` repeats until `break(value)`. All reachable breaks from one loop agree on the result
type. `while`, `do ... while`, and `for` have unit result. `for` obtains an iterator through
the validated source traits `core.iter.IntoIterator` and `core.iter.Iterator`, then repeatedly calls
`Iterator.next`.

`return(value)` exits the nearest named function or closure. `break(value)` exits the nearest
loop. `continue()` starts its next iteration. These exits have type `Never`.

`defer { action }` registers a zero-argument trailing closure for the current lexical block. Registration
evaluates and captures the action immediately. Registered actions run in reverse registration
order after the block result or exit value is evaluated and before control leaves the block.
They run on normal completion, `return`, `break`, `continue`, and `throw`. `defer` is a statement,
not a value-producing expression.

Assignment evaluates the target place once, then evaluates the new value. Compound assignment also
resolves the target only once.

## 10. Effects and Handlers

An effect declares operations:

```sc fragment
let Counter = effect {
  let next(): i32
}
```

A function's `with(...)` clause is part of its type:

```sc fragment
let read(): i32 with(Counter) = {
  Counter.next()
}
```

An operation transfers control to the nearest matching handler. A resumable clause receives a
single-use continuation. Resuming supplies the operation result and eventually returns the
handler's answer type. Abandoning the continuation cleans its captured state exactly once.

`Throws(Error)` is the standard abortive error effect. `throw(error)` invokes its `raise`
operation. `try { ... }` handles that effect and materializes `core.Result(Error)(Value)`.

```sc fragment
let parse(): i32 with(Throws(ParseError)) = { ... }

let result = try {
  parse()
}
```

Effects compose in one row. Handling one effect preserves all unhandled effects. Effect parameters
are compile-time row variables and are instantiated before runtime lowering. Once instantiated, a
capturing closure passed to a parameter with that row follows the same ownership, materialization,
and handling rules as a closure whose concrete effect was written directly.

An `async { ... }` expression is cold: creating it does not execute its body. The compiler
materializes private nominal state containing a state word and captured fields. That state is
structurally `Move`; relocation transfers its initialized captures, and cancellation drops them
exactly once. A compiler-generated future implements `Future((), Output = T)`. Polling a body with
no suspension point transfers its captures, executes the body once, and returns `Poll.Ready(T)`;
polling that completed future again traps. The completed state no longer drops transferred
captures. An unhandled `Unsafe` requirement is inferred from the body and attached to the
generated future's `poll` contract; creating the future remains pure, while polling requires an
unsafe handler. A body without suspension may retain a custom residual effect,
including standard `Throws(Error)`, with by-value `Copy`, move-only,
shared-borrow, or mutable-borrow captures.
Borrow captures store the reference value in future state and retain their
ordinary loan until that state is consumed or dropped. Polling inside the
corresponding handler specializes the generated poll and resume source before
runtime lowering. Move-only capture fields transfer once and completed future
cleanup does not drop them again. A residual `Throws(Error)` poll may be
handled by `try { future.poll() }`; both successful Ready and thrown paths
preserve capture cleanup. A suspended body may also retain a custom residual
effect, including `Throws(Error)`, when its first segment ends in one `await`
and has no retained local. It may either return that value directly or run one
pure linear continuation after Ready. Both segments may capture only by-value
`Copy` or move-only values, and the continuation may not suspend again.
Polling through the enclosing handler specializes the cold transition before
runtime lowering. That transition marks transferred captures unavailable
before evaluating the await operand, so an abort cannot drop them twice. The
operand and its residual effects run only while creating the child on the
first poll; returning `Pending` and polling the stored child again do not
replay them. `Poll.Ready(value)` runs the continuation exactly once, while
completion or cancellation drops the stored child exactly once. Other
suspended residual shapes remain unsupported. Outside residual
specialization, one linear non-tail form, `let value = await child`, may
execute ordinary continuation code after Ready; the continuation's captures
remain owned by the parent while suspended. Multiple sequential bindings compose recursively and preserve earlier Ready values
across later Pending states. Ordinary preceding locals used by the continuation are retained in
generated state and follow normal Copy, Move, and drop rules. A borrow of another retained local,
including through a borrow alias chain, is rejected because it would make the generated future
self-referential and therefore non-`Move`. Borrows of external storage remain subject to their
ordinary region and alias constraints. An `if` or `match` may place one tail await in every branch
when every child future has the same Output; concrete child types may differ. The condition,
scrutinee, and guards run once before suspension, and cancellation drops only the selected child.
Branch-local linear statements may surround await, and a non-suspending branch completes
immediately when selected. Residual effects with retained locals, nested suspension, branches,
loops, or borrowed suspended captures are not implemented yet.

`unsafe` is an authority effect. `unsafe { ... }` authorizes operations whose contracts cannot be
verified by the safe type and ownership rules; it does not disable type checking or cleanup.

The contextual forms `try { ... }`, `throw(error)`, and `unsafe { ... }` target validated source
declarations in `core.error` and `core.unsafe`. Their declarations expose the effect introduced or
handled and the remainder row that is forwarded. User declarations with the same spelling remain
ordinary declarations.

## 11. Propagation Operators

Postfix `value!` invokes the validated source trait `core.flow.Raise`:

```sc fragment
pub let Raise = trait {
  let Output: type
  let Error: type
  let raise(move self): Output with(core.error.Throws(Error))
}
```

It propagates the stored error through the active `Throws(Error)` effect. Postfix `value!!` invokes
the separately validated `core.flow.Unwrap` contract:

```sc fragment
pub let Unwrap = trait {
  let Output: type
  let unwrap(move self): Output
}
```

It forcefully unwraps a supported optional or result value and traps when no success value exists.
Neither postfix operator is name-based, and neither can be intercepted by a same-named inherent
method or user trait. Prefix `!value` is instead the `Not` operator described in section 8.

`?.` performs conditional chaining through the source-declared `Chain` protocol. `??` performs
fallback selection through `Coalesce`. Both protocols are validated `core.flow` traits. Their
right-hand transforms or fallback bodies are lazy and run only on the corresponding path.

The root `core.Option` and `core.Result` types follow the same ownership rule as user protocols:
payloads are moved, copied, or borrowed according to the surrounding expression and expected type.

## 12. Modules and Packages

A source file defines a module. A directory module is rooted at its `mod.sc`; sibling `.sc` files
define child modules. Paths use `.` in source. There is no `use` declaration: code either keeps a
qualified path or introduces a transparent alias with ordinary `let`:

```sc fragment
let Point = package.geometry.Point
let support = root.support
let shared = super.shared
```

`self`, `super`, and `root` begin lookup at the current module, parent module, and package root.
A dependency package name may begin an absolute dependency path. An alias has the visibility of its
`let` declaration; `pub let` and `pub(package) let` therefore provide explicit facade exports.

Project metadata lives in `salicin.toml`. A library target starts at `src/lib.sc`; a binary target
starts at `src/main.sc`. Local dependency resolution is recorded in `salicin.lock`.

The executable entry point is:

```sc fragment
let main(): i32 = { 0 }
```

It may instead accept the standard argument representation defined by the target library contract.
Returning `i32` selects the process exit status.

## 13. Evaluation and Failure

Salicin guarantees left-to-right evaluation for:

- function arguments;
- tuple and aggregate elements;
- struct fields;
- array elements;
- binary operands;
- assignment place components;
- match guards and selected arms.

An expression is evaluated at most once unless its source construct explicitly repeats it, such as
a loop condition.

Recoverable failures use `Throws(Error)` or another declared effect. Contract violations without a
recoverable API, including bounds failures and forced unwrap failures, trap and terminate the
process. Cleanup is deterministic for ordinary and handled exits; a process trap is not a
recoverable unwind mechanism.

## 14. Unsafe and Foreign Calls

`foreign(c)` is a complete declaration initializer for a C-owned function.
Calling the declaration implicitly requires `Unsafe`; the declaration does
not spell an explicit effect row. The optional second argument is a validated
ASCII linker symbol. When omitted, it defaults to the Salicin declaration
name. The stable foreign subset consists of explicitly supported scalar and
raw-pointer signatures.

```sc fragment
let read(
  fd: i32,
  buffer: Ptr(mut)(u8),
  count: usize,
): isize = foreign(c)

let c_read(
  fd: i32,
  buffer: Ptr(mut)(u8),
  count: usize,
): isize = foreign(c, "read")
```

Each foreign declaration has exactly one runtime parameter group, an
explicit result type, no compile-time parameters, `where` clause, explicit
effects, or Salicin body. `foreign` is separate from compiler-owned
`builtin()` definitions and from the `struct(c)` data representation.
Grouped `extern` declarations, `@link_name`, and all other `@` syntax are
rejected with migration diagnostics.

Foreign code must uphold every ownership, initialization, lifetime, alignment, and aliasing
precondition expressed by the Salicin declaration. `unsafe` makes that obligation explicit; it
does not infer or repair an invalid declaration.

## 15. Conformance

A conforming implementation must:

1. accept every program required by this specification and the grammar;
2. reject programs that violate static typing, ownership, borrowing, visibility, effect, or
   coherence rules;
3. preserve the specified evaluation order and cleanup behavior;
4. produce deterministic diagnostics and artifacts for identical inputs and configuration;
5. avoid assigning language meaning to implementation-only names or lowering details.

Where this document and the grammar differ, the specification governs semantics and the grammar
governs syntactic form. Repository tests provide implementation evidence, not additional language
rules.
