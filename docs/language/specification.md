# Salicin Language Specification

Status: evolving language specification

This document defines the meaning of Salicin source programs. It describes the current language,
not compiler internals, historical designs, or planned features. The precise parser grammar is in
[Grammar](grammar.md). Implemented coverage and remaining work are tracked in
[Implementation status](../project/status.md) and the [roadmap](../project/roadmap.md). Dated
research comparisons and design gates are recorded in the
[programming-language research ledger](research-ledger.md).

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

Every expression has a type. `()` is the sole unit type and unit value. `never` is the prelude's
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
`root`, `super`, and `package`. Words such as `type`, `sort`, `access`, `region`, `effect`, `effects`,
`parameters`, `borrow`, `copy`, `move`, `shared`, `mut`, and control-operation names are
contextual: they retain their special meaning only in the corresponding grammatical position.

Region binders are ordinary identifiers declared by `comptime r: region`. Diagnostics may display inferred
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

Top-level `let` declarations introduce values, functions, types, type aliases, sorts, effects,
traits, or modules according to their annotation and initializer.

Types, traits, functions, values, parameters, variants, modules, effects, and
ordinary sort names all use `snake_case`. `usize` and `string`
retain their type spelling in compile-parameter positions because the same names also classify
compile-time size and metadata values; the parameter context selects their sort semantics.

```sc fragment
let scalar = i32
let point = struct { x: i32, y: i32 }
let add(x: i32)(y: i32): i32 = { x + y }
```

Named functions have a callable overload namespace distinct from nominal type names. This permits
an explicit factory function to share a name with its result type. Other top-level declarations
must not conflict.

### 3.1 Visibility

Declarations are private to their module and descendant modules by default. `pub(package)` exposes
a declaration throughout its package. `pub` also exposes it to dependants.

```sc fragment
pub let point = struct {
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
not introduce a user binding. The form is authorized by the private edition
contract `pub let test(comptime name: string)(move body: (): bool): () = builtin()`; the string remains
compile-time runner metadata rather than a runtime argument. The name must be a non-empty
string literal and is used in diagnostics. Registrations are private to their
source package and cannot have visibility or attributes.

The body is evaluated as a parameterless function returning `bool`. `true`
passes and `false` fails. Its effects must be discharged within the body under
the ordinary effect rules. Test bodies are excluded from ordinary program and
library builds. `salic test` collects registrations from the selected package,
links one native runner, executes tests in source order, and reports the first
failing name. A target with no registrations is an error. The current runner
supports at most 254 registrations per target.

### 3.3 Sorts

A sort classifies compile-time values. An abstract sort has no source-enumerable set of values and
is compiler-owned:

```sc fragment
let type: sort
```

A defined sort lists its complete members:

```sc fragment
let optimization = sort {
  debug
  release
}

let empty = sort {}

let select(comptime mode: optimization)(value: i32): i32 = { value }
let answer = select(optimization.release)(42)
```

An abstract sort and an empty defined sort are different. `let name = sort` is invalid:
compiler-owned abstract sorts use `: sort`, while user-defined finite sorts use
`= sort { ... }`. User packages cannot introduce a new abstract sort.
Finite members are named through their Sort, as in `optimization.release`.

`type`, `region`, `effect`, `effects`, `parameters`, and metadata-only `string` are
compiler-owned abstract compile-time sorts. `access` is the finite sort
`sort { shared mut }`. `bool` remains an ordinary
closed runtime enum whose values can also classify compile-time parameters. Any other closed enum
or defined finite sort can be used the same way.

`abi` is the compiler-owned finite sort `sort { c }`. Its `c` member is the calling-convention
argument accepted by the current foreign initializer.

### 3.4 Compiler Definitions

The embedded `core` package declares compiler-provided definitions with the
complete initializer `builtin()`. The bootstrap declaration is private and
has the unique exact shape:

```sc fragment
let builtin() = builtin()
```

This unique self-recursive spelling bootstraps the compiler-definition marker;
it is not an ordinary call and edition validation assigns its uninhabited
`never` result. Semantic analysis obtains each other definition's sort or type
from its annotation, validates the complete
edition-owned signature, and resolves the marker before code generation.
Compiler-owned types and type constructors use the same form:

```sc fragment
pub let i32: type = builtin()
pub let array(comptime t: type)(comptime l: usize): type = builtin()
pub let size_of(comptime t: type): u64 = builtin()
```

`builtin()` is private to `core`. User functions, types, extension methods,
and globals cannot use it. Unknown core markers are invalid. Trait
requirements, effect operations, and user opaque types are genuinely
abstract and remain bodyless; they do not receive builtin default
implementations.

The same root module publicly declares the other syntax-owned contracts:

```sc fragment
pub let foreign(comptime abi: abi): never = builtin()
pub let test(comptime name: string)(move body: (): bool): () = builtin()
```

`foreign(c, ...)` passes the finite `abi.c` value (using the contextual short spelling `c`) as
statically validated metadata to its containing function declaration; `test("name") { ... }`
passes a compile-time `string` and supplies a pure boolean runner body. Neither metadata payload
is a runtime value.

## 4. Types and Compile-Time Parameters

The primitive integer families are:

```text
i8 i16 i32 i64 i128 isize
u8 u16 u32 u64 u128 usize
```

`isize` and `usize` have the target pointer width. Integer arithmetic is checked; overflow,
division by zero, and invalid shifts trap rather than producing an unspecified value.

`bool` is an ordinary closed enum with `false` and `true`. `()` is unit. `never` is uninhabited.
Arrays, borrows, raw pointers, tuples, function types, structs, and enums are type constructors.

Compile-time parameters occur in their own parameter groups:

```sc fragment
let identity(comptime t: type)(value: t): t = { value }
let first(comptime t: type, comptime l: usize)(values: array(t)(l)): t = { values[0] }
```

Supported compile-time parameter sorts include:

- `comptime t: type`;
- `comptime l: usize`;
- `comptime s: string` for compiler-owned UTF-8 metadata;
- `comptime r: region`;
- `comptime x: effect` for one nominal effect identity;
- `comptime e: effects`;
- `comptime p: parameters`;
- `comptime a: access`;
- values of another closed compile-time type;
- bounded type and effect constructor sorts.

Compile-time arguments participate in overload selection and monomorphization, then are erased
from runtime calling conventions. A rejected explicit or inferred argument is diagnosed against
its source binder and compile-time sort. Group arity, unknown labels, sort mismatches, and
underconstrained inference are distinct errors.

### 4.1 Type Constructors and Aliases

Each parenthesized compile-time group is a distinct constructor layer:

```sc fragment
pub let array(comptime t: type)(comptime l: usize): type = core.memory.array(t)(l)
let result(comptime error: type)(comptime value: type) = enum {
  ok(value)
  err(error)
}
```

`array(i32)(4)` applies two groups. It is not equivalent to `array(i32, 4)`.

A type alias is transparent and preserves the identity of its target:

```sc fragment
let scalar = i32
let family(comptime t: type): type = core.option(t)
let constructor: (comptime t: type): type = core.option
```

Alias expansion must terminate. Cyclic aliases and arity or sort mismatches are rejected.

### 4.2 Compile-Time Evaluation, Dependent Array Lengths, and Globals

Salicin does not require a second spelling such as `const fn` for compile-time functions. An
ordinary function may be evaluated in a static context when its body is available, its effects are
empty, and its inputs and result belong to the supported static subset. The current subset contains
unit, `bool`, every integer width, tuples, fixed arrays, fully instantiated structs, and closed
enums. It admits immutable local bindings, checked operators, `if`, exhaustive supported `match`
forms and guards, field or tuple projection, bounds-checked `usize` array indexing, and calls to
other eligible source functions. Calls may retain multiple runtime groups and
labels, explicitly select or infer generic arguments, cross module boundaries,
and target a statically resolved inherent or unique trait method or associated
function. `return` exits the interpreted function through nested immutable
blocks, `if`, and `match`. Value-changing recursion is admitted within the
fixed 16,384-step and 128-active-call limits; an equal repeated call is an
immediate cycle error.

```sc fragment
let next(comptime value: usize): usize = { value + 1 }

let buffer(comptime element: type)(comptime length: usize) = struct {
  values: array(element)(next(length))
}
```

Static expressions preserve call-group boundaries and labels and are
evaluated after generic static arguments are substituted and before runtime
type lowering. The result therefore participates in type identity:
`buffer(i32)(2)` contains an `array(i32)(3)`. Global initializers use the same
evaluator and retain the same exact typed normalized values before LLVM
encoding; they may call the same eligible ordinary source functions.
Mutation, borrowing, handlers, closures, runtime effects, foreign calls,
builtins without a specified CTFE rule, and bodyless functions are rejected
in static evaluation. Checked overflow, division by zero, invalid
shifts, or exhaustion of the implementation's evaluation budget are compile errors. Struct values
retain canonical nominal identity and declaration-order fields. Unsized, address-dependent,
allocating, recursively laid out, or `droppable` fields are rejected before construction. Enum
values retain canonical identity, source variant identity, and only the active declaration-order
payload; matching never exposes or depends on a backend discriminant. Resource exclusion checks
every possible variant of an enum before it becomes a CTFE value. `size_of`
and `align_of` preserve the fully substituted queried type and are encoded
against the compilation target rather than the compiler host.

### 4.3 Borrow, Pointer, and Array Types

The safe reference constructor is:

```sc fragment
borrow(comptime a: access = shared)(comptime r: region)(comptime t: type)
```

When omitted, access is `shared` and the region is inferred. The common forms are `borrow(t)`,
`borrow(mut)(t)`, and `borrow(r)(t)`.

The raw pointer family is:

```sc fragment
ptr(comptime a: access = shared)(comptime t: type)
```

Raw pointer dereference, arithmetic that can leave an allocation, initialization, and ownership
reconstruction require an `unsafe` boundary.

The fixed-size array family is:

```sc fragment
array(comptime t: type)(comptime l: usize)
```

Array length is part of the type. Array indexing requires `usize`, evaluates
its base and index once, and performs a bounds check.

## 5. Functions and Application

A function declaration may contain multiple compile-time and runtime parameter groups:

```sc fragment
let map(comptime t: type, comptime u: type)(value: t)(transform: (t): u): u = {
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
let apply(comptime t: type, comptime u: type)(value: t)(function: (t): u): u = {
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

- `copy` duplicates a `copyable` value;
- `move` transfers ownership of a `movable` value;
- `borrow` creates a shared loan;
- `borrow(mut)` creates an exclusive loan.

```sc fragment
let consume(comptime t: type)(move value: t): () = { ... }
let inspect(comptime t: type)(value: borrow(t)): () = { ... }
let update(comptime t: type)(value: borrow(mut)(t)): () = { ... }
```

An omitted mode uses the type's default: `copyable` values are copied and resource values are moved.
An explicit mode always takes precedence.

`movable` is a source-backed structural auto marker. Scalars, borrows, raw pointers, and aggregates
whose owned members are all `movable` may be relocated. `copyable` inherits `movable`; duplicating a value
therefore always implies that either resulting value may also be relocated. Parameter transfer,
return, assignment from an existing place, and movement into reallocating storage require `movable`.
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

An `comptime a: access` parameter selects shared or mutable borrowing without defining two APIs:

```sc fragment
let view(comptime a: access)(comptime t: type)(value: borrow(a)(t)): borrow(a)(t) = {
  value
}
```

Access is a compile-time value. A mutable instantiation preserves exclusivity; it is not a runtime
flag or a subtype of shared access.

## 7. Structs, Enums, and Patterns

Structs are nominal product types:

```sc fragment
let point = struct {
  x: i32,
  y: i32,
}

let origin = point { x: 0, y: 0 }
```

Fields are initialized left to right. Every required field must appear exactly once. Field access
preserves the ownership and borrow state of the base.

`struct(c)` selects the target C aggregate representation as part of the type
constructor:

```sc fragment
let timespec = struct(c) {
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
let option(comptime t: type) = enum {
  none
  some(t)
}
```

Patterns may match literals, bindings, tuples, structs, and enum variants. `_` discards a matched
value. A refutable pattern is accepted only where failure has an explicit control path.

`match` evaluates its scrutinee once and tests arms in source order. Guards run only after their
pattern succeeds. Arms must agree on a result type, except that `never` coerces to the other arm
type. A match over a closed type must be exhaustive.

```sc fragment
match value {
  option(i32).some(number) -> number
} {
  option(i32).none -> 0
}
```

## 8. Traits, Extensions, and Static Dispatch

A trait is neither a runtime type nor a Sort. Semantically, it declares a relation over a subject
and any compile-time arguments. A bound such as `t: iterator` is a logical constraint (a solver
goal); an applicable `extend(t, iterator)` supplies implementation evidence. Associated-type
bindings add projection-equality constraints to the same goal. Trait declarations and evidence are
erased after static dispatch.

A trait declares associated types and required or default methods:

```sc fragment
let iterator = trait {
  let item(comptime r: region): type
  let next(comptime r: region)(self: borrow(mut)(r)(self)): core.option(item(r))
}
```

An `extend` block adds inherent members or implements a trait:

```sc fragment
extend(point) {
  let translated(self: borrow(self))(dx: i32, dy: i32): point = {
    point { x: self.x + dx, y: self.y + dy }
  }
}
```

`extend` has the call-shaped compiler contract
`extend(comptime t: type, impl: (self): ())` for inherent members and
`extend(comptime t: type, tt: trait, impl: (self): ())` for trait implementations. The final implementation
argument is written as a trailing declaration block. Its target is a type pattern: constructor
parameters are bound by destructuring and their sorts are inferred from the constructor signature.
For example, `extend(result(error)(t), core.flow.chain) { ... }` binds `error` and `t` as `type`
values without a separate compile-time parameter header.

Trait dispatch is static. Implementations are selected by the concrete subject and trait
arguments; no runtime dictionary or implicit open-world dispatch is introduced.

Where predicates constrain generic declarations:

```sc fragment
let duplicate(comptime t: type)(value: t): (t, t)
where t: core.marker.copyable = {
  (value, value)
}
```

Associated type equalities refine a bound:

```sc fragment
let produce(comptime t: type)(value: t): i32
where t: produce(item = i32) = {
  value.produce()
}
```

Generic associated constructors retain their parameter groups and sorts. Their receiver region can
determine a yielded type, as in `iterator.item(r)`.

Where predicates can equate a generic associated constructor with a type expression by declaring
alpha-renamable binders on the left:

```sc fragment
let borrow_item(comptime t: type)(value: t): ()
where t: iterator(item(comptime r: region) = borrow(r)(i32)) = { ... }
```

The binder groups and sorts must exactly match the associated declaration. The right side may use
outer compile-time parameters and its own binders. Transparent aliases are expanded before
comparison. Equation rewriting is direct, uses at most 32 nested expansions, and does not infer
missing binders or reorder groups.

An implementation is legal only in the package that owns the trait or the nominal target. Two
applicable implementations with the same static key are rejected.

Operators are syntax for methods of validated source traits. Ordinary methods with the same name
cannot intercept operator dispatch. In particular, prefix `!value` invokes the validated
`core.ops.bit.not.not` contract; this is distinct from the postfix propagation operator described
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
pub let if(comptime e: effects, comptime t: type)
  (condition: bool)
  (move then: (): t with(e))
  (move else: (): t with(e)): t with(e)

pub let while(comptime e: effects)
  (move condition: (): bool with(e))
  (move do: (): () with(e)): () with(e)
```

The surface forms supply their branch, condition, and body blocks as lazy callable groups. The
canonical declarations for `do`, `loop`, `match`, and `for` are validated in the same way.
`break`, `continue`, and `return` resolve to the canonical `core.control` functions, which introduce
the corresponding `loop_exit(t)`, `iteration_skip`, or `function_exit(t)` effect before the enclosing construct
handles it. A same-named user declaration cannot redirect any of these forms. The complete
contracts and their lowering obligations are specified in [Control-flow contracts](control-flow.md).

`loop { ... }` repeats until `break(value)`. All reachable breaks from one loop agree on the result
type. `while`, `do ... while`, and `for` have unit result. `for` obtains an iterator through
the validated source traits `core.iter.into_iterator` and `core.iter.iterator`, then repeatedly calls
`iterator.next`.

`return(value)` exits the nearest named function or closure. `break(value)` exits the nearest
loop. `continue()` starts its next iteration. These exits have type `never`.

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
let counter = effect {
  let next(): i32
}
```

A function's `with(...)` clause is part of its type:

```sc fragment
let read(): i32 with(counter) = {
  counter.next()
}
```

An operation transfers control to the nearest matching handler. A resumable clause receives a
single-use continuation. Resuming supplies the operation result and eventually returns the
handler's answer type. Abandoning the continuation cleans its captured state exactly once.

`throwing(error)` is the standard abortive error effect. `throw(error)` invokes its `raise`
operation. `try { ... }` handles that effect and materializes `core.result(error)(value)`.

```sc fragment
let parse(): i32 with(throwing(parse_error)) = { ... }

let result = try {
  parse()
}
```

`effect` and `effects` are deliberately distinct sorts. A value of `effect` is exactly one nominal
identity, such as `counter` or `throwing(parse_error)`; this is the sort used by
`handle(comptime self: effect)`. A value of `effects` is a normalized zero-or-more row: `pure` is the empty
row, and `with(...)` combines identities and row variables without order or duplicates.

Handling one identity removes it from the row and preserves every other requirement. Parameters
such as `comptime e: effects` are compile-time row variables and are instantiated before runtime lowering.
Once instantiated, a capturing closure passed to a parameter with that row follows the same
ownership, materialization, and handling rules as a closure whose concrete effects were written
directly.

An `async { ... }` expression is cold: creating it does not execute its body. The compiler
materializes private nominal state containing a state word and captured fields. That state is
structurally `movable`; relocation transfers its initialized captures, and cancellation drops them
exactly once. `core.async.async` is the intrinsic that materializes this
anonymous state. `core.async.await` is a source polling loop: `pending`
performs `suspension.suspend()`, while `ready(value)` returns the value.
Syntax-directed lowering may specialize `await` into the generated state
machine without changing its source contract.
A compiler-generated future implements `future((), output = t)`. Polling a body with
no suspension point transfers its captures, executes the body once, and returns `poll.ready(t)`;
polling that completed future again traps. The completed state no longer drops transferred
captures. An unhandled `unsafety` requirement is inferred from the body and attached to the
generated future's `poll` contract; creating the future remains pure, while polling requires an
unsafe handler. A body without suspension may retain a custom residual effect,
including standard `throwing(error)`, with by-value `copyable`, move-only,
shared-borrow, or mutable-borrow captures.
Borrow captures store the reference value in future state and retain their
ordinary loan until that state is consumed or dropped. Polling inside the
corresponding handler specializes the generated poll and resume source before
runtime lowering. Move-only capture fields transfer once and completed future
cleanup does not drop them again. A residual `throwing(error)` poll may be
handled by `try { future.poll() }`; both successful ready and thrown paths
preserve capture cleanup. A suspended body may also retain a custom residual
effect, including `throwing(error)`, when its first segment ends in one `await`.
It may either return that value directly or run a finite linear sequence of
continuations and awaits after ready. Every segment may capture by-value
`copyable` or move-only values, or retain a region-checked shared or mutable
reference to external storage. Pre-await locals used by a continuation may be
retained when the resulting state remains structural `movable`. Only the first
segment may retain a custom effect or `throwing`; each later child poll row must
be pure apart from `unsafety`.
Polling through the enclosing handler specializes the cold transition before
runtime lowering. That transition marks transferred captures unavailable
before evaluating the await operand, so an abort cannot drop them twice. A
starting state continues to own move-only continuation captures until the
factory returns; pre-await locals remain ordinary factory locals and are
cleaned there if evaluation aborts. A successful factory transition stores
the child and retained locals together. The operand and its residual effects
run only while creating the child on the first poll; returning `pending` and
polling the stored child again do not replay them. `poll.ready(value)` runs the
next continuation exactly once, destroys the completed child, and either
completes or stores and polls the next child. Completion, error, and
cancellation drop every initialized state field exactly once. Other suspended
residual shapes remain unsupported. Outside residual
specialization, one linear non-tail form, `let value = await child`, may
execute ordinary continuation code after ready; the continuation's captures
remain owned by the parent while suspended. Multiple sequential bindings compose recursively and preserve earlier ready values
across later pending states. Ordinary preceding locals used by the continuation are retained in
generated state and follow normal copyable, movable, and drop rules. A borrow of another retained local,
including through a borrow alias chain, is rejected because it would make the generated future
self-referential and therefore non-`movable`. Borrows of external storage remain subject to their
ordinary region and alias constraints. An `if` or `match` may place one tail await in every branch
when every child future has the same output; concrete child types may differ. The condition,
scrutinee, and guards run once before suspension, and cancellation drops only the selected child.
Branch-local linear statements may surround await, and a non-suspending branch completes
immediately when selected. Under residual specialization, a one-shot `if` or
`match` may select direct-tail children of the same concrete future type. The
selected child factory may use the first segment's residual row; selection and
factory evaluation occur once, and pending, ready, or cancellation retains
only the selected child. Direct `if` and `match` selection may also choose
heterogeneous concrete child types through a private active-variant future,
including pattern payload bindings, a move-only selector, and retained
continuation locals. After a pure child becomes ready, a final continuation
that does not suspend again may retain a custom effect or `throwing`; it executes
once under the poll caller's handler after the completed child and its output
have been transferred. Residual construction or polling of a later child and
residual recurring loops are not implemented yet.

`unsafe` is an authority effect. `unsafe { ... }` authorizes operations whose contracts cannot be
verified by the safe type and ownership rules; it does not disable type checking or cleanup.

The contextual forms `try { ... }`, `throw(error)`, and `unsafe { ... }` target validated source
declarations in `core.error` and `core.unsafe`. Their declarations expose the effect introduced or
handled and the remainder row that is forwarded. User declarations with the same spelling remain
ordinary declarations.

## 11. Propagation Operators

Postfix `value!` invokes the validated source trait `core.flow.raise`:

```sc fragment
pub let raise = trait {
  let output: type
  let error: type
  let raise(move self): output with(core.error.throwing(error))
}
```

It propagates the stored error through the active `throwing(error)` effect. Postfix `value!!` invokes
the separately validated `core.flow.unwrap` contract:

```sc fragment
pub let unwrap = trait {
  let output: type
  let unwrap(move self): output
}
```

It forcefully unwraps a supported optional or result value and traps when no success value exists.
Neither postfix operator is name-based, and neither can be intercepted by a same-named inherent
method or user trait. Prefix `!value` is instead the `not` operator described in section 8.

`?.` performs conditional chaining through the source-declared `chain` protocol. `??` performs
fallback selection through `coalesce`. Both protocols are validated `core.flow` traits. Their
right-hand transforms or fallback bodies are lazy and run only on the corresponding path.

The root `core.option` and `core.result` types follow the same ownership rule as user protocols:
payloads are moved, copied, or borrowed according to the surrounding expression and expected type.

## 12. Modules and Packages

A source file defines a module. A directory module is rooted at its `mod.sc`; sibling `.sc` files
define child modules. Paths use `.` in source. There is no `use` declaration: code either keeps a
qualified path or introduces a transparent alias with ordinary `let`:

```sc fragment
let point = package.geometry.point
let support = root.support
let shared = super.shared
```

`self`, `super`, and `root` begin lookup at the current module, parent module, and package root.
A dependency package name may begin an absolute dependency path. An alias has the visibility of its
`let` declaration; `pub let` and `pub(package) let` therefore provide explicit facade exports.

Project metadata lives in `salicin.toml`. A library target starts at
`src/lib.sc`; a binary target starts at `src/main.sc`. A root `[workspace]`
contains explicit relative member paths. Membership does not create a
dependency: each package still declares its direct dependencies. Workspace
resolution is recorded in one root `salicin.lock`.
`--locked` requires that exact graph; `--frozen` also prohibits dependency
network access.

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

Recoverable failures use `throwing(error)` or another declared effect. Contract violations without a
recoverable API, including bounds failures and forced unwrap failures, trap and terminate the
process. Cleanup is deterministic for ordinary and handled exits; a process trap is not a
recoverable unwind mechanism.

## 14. Unsafe and Foreign Calls

`foreign(c)` is a complete declaration initializer for a C-owned function.
Calling the declaration implicitly requires `unsafety`; the declaration does
not spell an explicit effect row. The optional second argument is a validated
ASCII linker symbol. When omitted, it defaults to the Salicin declaration
name. The foreign subset accepts every signed, unsigned, pointer-sized, and
128-bit integer plus raw pointers as parameters and results; `()` is accepted
only as a result. It rejects `bool`, Unit parameters, arrays, aggregates,
borrows, slices, and callable values. A C array or `struct(c)` therefore
crosses this function boundary behind `ptr` rather than by value. The complete
target mapping and cross-language evidence are specified by the
[C interoperability contract](../project/c-interoperability.md).

```sc fragment
let read(
  fd: i32,
  buffer: ptr(mut)(u8),
  comptime count: usize,
): isize = foreign(c)

let c_read(
  fd: i32,
  buffer: ptr(mut)(u8),
  comptime count: usize,
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
