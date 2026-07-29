# Core library

`library/core` contains edition-matched declarations that do not require heap allocation or host
services. The compiler embeds these `.sc` files, parses them through the ordinary frontend, and
validates declarations that have language-defined roles.

Compiler-owned definitions are explicit. The private root declaration
`let builtin() = builtin()` bootstraps a declaration marker that is
unavailable to user packages. Primitive types, compiler-defined type
constructors, intrinsic functions, and intrinsic extension methods use
complete `= builtin()` initializers. Edition validation rejects missing,
unknown, malformed, or public markers. Trait requirements and effect
operations remain bodyless because they are abstract contracts, not
compiler-provided default implementations. Operations derivable from those
primitives remain ordinary Salicin definitions: the core implementation does
not use `builtin()` merely as an optimization annotation.

The same private root module declares
`pub let foreign(comptime abi: abi): never = builtin()`,
`pub let foreign(comptime abi: abi, comptime symbol: string): never = builtin()`, and
`pub let test(move body: (): bool): () = builtin()`. These are canonical syntax
contracts for foreign initializers and test registrations. `c` is the member of the finite
`abi` sort selected by `foreign(c)`; `test("name")` consumes its ordinary string
literal in syntax before lowering the boolean action.

## Modules

`core.lib` is the root facade. It only re-exports the public root surface: `never`, `movable`, `copyable`,
`droppable`, `option`, and `result`.

`core.prelude` is also only a facade and contains the deliberately small implicit surface:

- the uninhabited `never` type
- the `movable`, `copyable`, and `droppable` traits

The definitions live in focused modules. `core.never` owns `never`, `core.marker` owns `movable`,
`copyable`, and `droppable`, and `core.option` and `core.result` own fundamental ordinary data types that are
intentionally not prelude names:

```sc fragment
pub let option(comptime t: type) = enum {
  some(t),
  none,
}

pub let result(comptime e: type)
  (comptime t: type) = enum {
  ok(t),
  err(e),
}
```

Naming `option` or `result` requires an ordinary root alias such as
`let option = core.option` or `let result = core.result`.

`core.numeric` extends every primitive integer with `min`, `max`, `clamp`,
and `sign`. Signed integers return a same-width unsigned value from
`magnitude`, including at the signed minimum; unsigned magnitude is the
identity. `value.checked_into(output: target)()` returns
`core.option(target)` and accepts only another integer type. A value outside
the target range produces `none`; there is no implicit, wrapping, or
truncating fallback. Invalid `clamp` bounds trap.

These methods have one canonical owner and are not mirrored through `std`.
Their compiler intrinsics preserve the source contract in CTFE and native
code, including `isize`/`usize` at the compiler's explicit target width.

Their inherent helper surface is allocation-free:

| Type | Inspection/view | Transform | Fallback/conversion |
| --- | --- | --- | --- |
| `option(t)` | `is_some`, `is_none`, `as_ref` | `map`, `and_then` | `unwrap_or`, `unwrap_or_else`, `ok_or`, `ok_or_else` |
| `result(error)(t)` | `is_ok`, `is_err`, `as_ref` | `map`, `map_error`, `and_then` | `unwrap_or`, `unwrap_or_else`, `ok`, `err` |

`as_ref()` preserves the receiver region and defaults to shared access;
`as_ref(mut)()` requires an exclusive receiver and produces exclusive payload
borrows. Matching a borrowed enum inspects its discriminant and aliases its
payload storage instead of moving it. The returned view therefore cannot
outlive the source, and an exclusive view blocks overlapping access.
Transform callbacks and lazy fallbacks forward their declared effect row and
run only in the selected variant. `unwrap_or` and `ok_or` take eagerly
evaluated values; the `_else` forms evaluate their callback only on `none` or
`err`. All consuming helpers evaluate and move each payload at most once.

`movable` is an automatically satisfied structural marker for relocatable values. `copyable` has the
supertrait constraint `trait(requires: self is movable)`, while `droppable` remains independent: an owning resource may
be movable without being copyable. Source code does not need handwritten `movable` implementations
for ordinary aggregates.
Operators and syntax that lower through these identities use the validated standard-library
declarations directly; aliasing is only required when source code writes the short names.

`core.ops` is a compatibility facade over smaller protocol modules. `core.ops.arith` defines
`add`, `sub`, `mul`, `div`, `rem`, and `neg`; `core.ops.bit` defines `not`, `bit_and`, `bit_or`,
`bit_xor`, `shl`, and `shr`; `core.ops.assign` defines the compound-assignment protocols; and
`core.cmp` defines `eq`, `partial_ordering`, and `partial_ord`. The `core.ops` facade re-exports the
operator-facing names for ordinary aliases. They are not in the prelude.
Arithmetic and bitwise protocols accept their operands with automatic passing and use an associated
`output` type. copyable operands remain usable; resource operands move:

```sc fragment
let add = core.ops.add

extend(number, add(number)) {
  let output = number
  let add(self)
    (rhs: number): number = { ... }
}
```

`eq(rhs)` borrows both operands and returns `bool`; `!=` invokes the same method exactly once and
negates its result:

```sc fragment
let eq = core.ops.eq

extend(number, eq(number)) {
  let eq(self: borrow(self))
    (rhs: borrow(number)): bool = { self.value == rhs.value }
}
```

`partial_ord(rhs)` also borrows both operands. Its `partial_cmp` method returns `partial_ordering`,
whose variants are `less`, `equal`, `greater`, and `unordered`. All four ordering operators invoke
the method once; an `unordered` result makes each operator false:

```sc fragment
let partial_ord = core.ops.partial_ord
let partial_ordering = core.ops.partial_ordering

extend(number, partial_ord(number)) {
  let partial_cmp(self: borrow(self))
    (rhs: borrow(number)): partial_ordering = { ... }
}
```

`neg` and `not` use automatic passing for their operand and define an associated `output` type. Consequently an
overloaded `!` may return a non-boolean result; only the built-in boolean operation is fixed to
`bool`. The boolean implementation is ordinary source control flow, and signed
integer negation is defined as subtraction from zero. Generic code can state
the same output relationship in a normal where predicate.

`bit_and(rhs)`, `bit_or(rhs)`, `bit_xor(rhs)`, `shl(rhs)`, and `shr(rhs)` have the same two automatic
parameter groups and associated `output` shape as arithmetic protocols. Built-in integer shifts use
arithmetic right shift for signed integers and logical right shift for unsigned integers. Negative
or out-of-width shift counts trap instead of exposing backend undefined behavior.

`add_assign(rhs)`, `sub_assign(rhs)`, `mul_assign(rhs)`, `div_assign(rhs)`, `rem_assign(rhs)`,
`bit_and_assign(rhs)`, `bit_or_assign(rhs)`, `bit_xor_assign(rhs)`, `shl_assign(rhs)`, and
`shr_assign(rhs)` are separate mutation protocols. Each mutably borrows `self`, accepts `rhs` with
automatic passing, and
returns `()`:

```sc fragment
pub let add_assign(comptime rhs: type) = trait {
  let add_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}
```

The corresponding `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, and `>>=` syntax binds to
these validated identities for nominal values. Built-in integers use the same fixed operator
semantics, including division, remainder, and shift traps. The left place is resolved once; an
inherent or unrelated trait method with the same member spelling cannot intercept compound
assignment. Their standard implementations are source definitions of the form
`self = self + rhs`; only the underlying scalar operation is intrinsic.

Writing `left + right`, `left & right`, `left == right`, or `left < right` does not itself require an
alias. An alias is required when source names the protocol in an implementation, bound, type, or
direct member access.

`core.flow` contains the standard protocols for `?.` and `??`. They are not in the prelude:

```sc fragment
pub let chain = trait {
  let item: type
  let rebind(comptime value: type): type

  let chain(comptime e: effects, comptime u: type): with(e)
    (self)
    (transform: with(e)((item): u)): rebind(u)
}

pub let coalesce = trait {
  let item: type

  let coalesce(comptime e: effects): with(e)
    (self)
    (fallback: with(e)((): item)): item
}
```

The protocols use the same trait and generic-associated-constructor syntax as user declarations.
The compiler lowers GAT references in trait method signatures and supports direct constructor
implementations such as `let rebind = maybe` plus partially applied type aliases. GAT parameters
may carry `type`, `access`, `region`, `usize`, and closed-value sorts; implementation constructors
must match those sorts and parameter-group boundaries, not merely their arity. `??` dispatches non-`option`/`result` nominal
values through `coalesce` when the fallback can be represented as a no-capture lifted function. `?.`
dispatches non-`option`/`result` nominal values through `chain` when the synthesized transform
closure can be represented in the same way; simple field access is supported, while transforms that
capture outer method-call arguments still require the general callable-to-function bridge. The
facade `core.option`/`core.result` paths remain available as standard-library specializations.

`core.effect` owns standard effect identities. It is not part of the prelude; ordinary source
should alias these identities through `core.effect`:

```sc fragment
pub let unsafety = effect {}

pub let throwing(comptime error: type) = effect {
  let raise(move error: error): never
}

pub let suspension = effect {
  let suspend(): ()
}
```

`unsafety`, `throwing(error)`, and `suspension` are validated lang-item identities, but their declarations use
the same source-level effect forms as user code. `failure.raise` is an ordinary `never`-returning
effect operation and can be handled with a normal abort clause such as `raise: { (error) -> ... }`.
Standard and user effect identities follow the universal `snake_case` naming rule, including the
final segment of a `with(...)` effect path. Effect
row parameters such as `comptime e: effects` are resolved as parameters rather than nominal effects.
Source `throw(error)` targets this ordinary operation when the current effect row has exactly one
active `throwing(error)`. Contextual `try { ... }` with an expected `result(error)(t)` handles
ordinary `throwing(error)` through the same algebraic handler path, using `done -> ok` and
`raise -> err`. Without an explicit `result` context, direct calls and local function-value calls
to ordinary `throwing(error)` functions infer the same handler result when the success type is
probeable and the escaping error type is unique. `suspension` currently exposes only a minimal
`suspend(): ()` operation; executable
async/future lowering will add its handler contracts in the same implementation slice rather than
pretending `await` already works.

`core.sorts` owns standard compile-time sorts, also outside the prelude:

```sc fragment
pub let type: sort(2)
pub let region: sort(2)
pub let effect: sort(2)
pub let effects: sort(2)
pub let parameters: sort(2)
pub let constraint: sort(2)
pub let abi = sort(1) {
  c
}
```

Inside a compiler-owned `requires(...)` guard, `left is right` selects the `is`
relation between the classifiers of its operands. `type` implements
`is(constraint)`, allowing function guards such as
`requires(t is copyable)` and extension requirement groups such as
`(requires: t is copyable)`.

`effect` classifies one nominal effect identity; `effects` classifies a normalized zero-or-more
effect row. `string` currently classifies compiler-consumed UTF-8 metadata, and `abi` is a finite
calling-convention sort whose first supported value is `c`.

`core.borrow` owns the finite access sort and its unqualified aliases:

```sc fragment
pub let access = sort(1) {
  shared
  mut
}
pub let mut = access.mut
pub let shared = access.shared
```

`core.passing` owns the parameter modifier functions:

```sc fragment
pub let copy(comptime p: parameters): parameters
pub let move(comptime p: parameters): parameters
```

Borrow types and values are written with the declared `borrow` form: `borrow(t)`,
`borrow(mut)(t)`, and `borrow(a)(r)(t)`. `borrow(a)` refers to the finite access sort; generic
passing modifiers use the `(comptime p: parameters): parameters` function sort.

`core.memory` declares the fixed-size `array(t)(l)`, unsized `slice(t)`, and
`ptr(comptime a: access = shared)(t)` raw-pointer family. `slice(t)` is never a first-class stored value:
programs use `borrow(a)(r)(slice(t))`, represented as a pointer and length while retaining the
source loan and region. array borrows unsize contextually, and `vec(t).as_slice(a)()` borrows its
initialized prefix without transferring ownership.

The source-backed slice extension provides `len()` and bounds-checked `at(index)`. Shared access is
the default; `at(mut)(index)` returns a mutable element borrow when the slice borrow is mutable.
Out-of-bounds access traps. The pointer extension provides `offset(index)` for either access and
`init(value)` / `take()` only for `ptr(mut)(t)`. Pointer methods retain the `unsafety` requirement of
their underlying raw intrinsics; `init` expects uninitialized storage and `take` leaves storage
uninitialized.

`core.ops.index.index(key)` is the single bracket protocol. Its `index(comptime a: access)` method returns
`borrow(a)(output)`, so shared reads, explicit element borrows, and mutable assignment use one
implementation without a separate `index_mut`. Arrays implement `index(usize)` through a validated
core intrinsic; slice implements `index(u64)` in source by forwarding to `at`.

Capability modules are separated by semantics:

- `core.effect` owns generic handler infrastructure.
- `core.error` owns `throwing`, `throw`, and the `try` interpreter into `result`.
- `core.async` owns `suspension`, `poll`, `future`, `executor`, `async`, and `await`.
- `core.unsafe` owns the `unsafety` authority effect and its lexical interpreter.
- `core.result` owns only the `result` data type and its ordinary protocols.
- `core.control` owns structural control flow: `break`, `continue`, `return`, `do`, `loop`,
  `while`, `if`, `match`, `for`, and lexical `defer`.

`throw` and `throwing` are not result-specific. `throwing(error)` is an independent effect, while
`try` is one interpreter that chooses `result(error)(t)` as its output. Other handlers may
interpret the same effect differently.

`core.effect` declares the protocol and erased runtime contracts used by algebraic handler lowering:

```sc fragment
pub let continuation(comptime input: type, comptime output: type): type
pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type): type
pub let handle = trait(comptime self: effect) {
  let clauses(comptime value: type, comptime answer: type): parameters
  let handle(comptime value: type, comptime answer: type, comptime rest: effects): with(rest)
    ...clauses(value, answer)
    (move action: with(self, rest)((): value)): answer
}
```

`continuation` is a one-shot suspended computation. `effect_callable` is an owned action awaiting a
handler-supplied continuation from `output` to `answer`; `input` is the action's packed runtime input.
Both native values carry call and drop entries, an environment pointer, and an ownership flag. They
are `core.effect` exports rather than prelude names and cannot be replaced by same-named user
declarations.
The compiler-internal action entry has the logical signature
`(environment, input, continuation(output, answer)): answer`. Erasing or invoking an action consumes
its owner; a dropped, uninvoked action releases its captured environment through the stored drop
entry. Within an active handler, compatible open runtime action parameters use this representation
when crossing named effectful frames or another reusable handler. The source closure may have
shared, mutable, or moved captures, but the erased owner itself is always one-shot and cannot escape
with a borrow-capturing environment. `handle` is an effect-kinded lang trait automatically satisfied by every source
`effect` declaration. Its `clauses` associated parameter schema names the compiler-derived labeled
clause groups used by `.handle`; `...` expands that schema into an ordered sequence of runtime
parameter groups. Consequently source calls use named trailing closures directly, for example
`state(i32).handle get { ... } put { ... } action { ... }`, while the generated implementation has exactly the
shape declared by the trait. These low-level operations and generated handler implementations are
not ordinary source-level standard-library functions.

`core.async` makes the asynchronous model explicit in source. `future(e)` is a `movable` trait with an
associated `output` and a mutable-borrowing `poll` method returning `poll`. `executor.run` is
an allocation-free protocol. The ordinary zero-field `std.async.spin`
implementation repeatedly polls one owned future until `ready`; the concrete
polling policy is intentionally above the freestanding protocol.
Constructing a cold future does not select or run an executor.
`async` remains the direct intrinsic that materializes the anonymous future
state selected for its action, while `await` is source-defined. Their
signatures expose their effect rows and `future(e, output = t)` relationship.
`await` repeatedly calls `poll`; `pending` invokes
`suspension.suspend()`, and `ready(value)` exits the source loop. The compiler may
take an equivalent syntax-directed state-machine path for `await`.
Compiler-generated futures without suspension already
implement the inferred `future(e)` instance and transition from cold state to `poll.ready` exactly
once. `e` may be empty, `unsafety`, or a custom residual effect. A body without suspension can poll
under the corresponding algebraic handler through generated poll/resume source specialization
when its captures are by-value `copyable` or move-only values. Move-only fields transfer exactly once
and are not dropped again with completed future state. Borrowed, suspended, and `throwing`-residual
bodies remain compiler work. Polling
enforces `e` while construction remains pure. A single tail-position `await` creates its child on the first parent poll,
stores it across `pending`, and completes the parent from `ready`; cancellation drops a stored child
exactly once. One non-tail `let value = await child` may continue with a linear suffix whose captures
are retained in parent state. Sequential awaits compose through nested continuation futures and
preserve earlier results across later pending states. Suspension nested in control flow remains
compiler work. Locals live across a sequential suspension are state fields with ordinary ownership
and cleanup. A borrow cannot cross suspension together with a local referent stored in that same
Future state cannot retain such a borrow because `future` requires `movable`;
external region-checked borrows remain permitted.
`if` and `match` branches consisting of one tail await select their child before using this same
polling contract. Different concrete child types use a private active-variant future when their
output agrees. Each branch retains its own linear locals across suspension; a branch without await
is an immediate ready future. Loop suspension remains compiler work.

```sc fragment
pub let do(comptime e: effects, comptime t: type): with(e)
  (move action: with(e)((): t)): t
pub let do(comptime e: effects): with(e)
  (move action: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): ()))
  (move while: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): bool)): () = {
  loop {
    core.control.iteration_skip.handle
      next { () }
      action { action() }
    if while() { continue() } else { break() }
  }
}
pub let try(comptime f: effects, comptime t: type, comptime e: type): with(f)
  (move action: with(core.error.throwing(e), f)((): t)): core.result(e)(t)
pub let throw(comptime error: type): with(core.error.throwing(error))
  (move error: error): never
pub let unsafe(comptime e: effects, comptime t: type): with(e)
  (move action: with(core.unsafe.unsafety, e)((): t)): t
pub let loop(comptime e: effects, comptime t: type): with(e)
  (move body: with(core.control.loop_exit(t), core.control.iteration_skip, e)((): ())): t
pub let while(comptime e: effects): with(e)
  (move condition: with(e)((): bool))
  (move do: with(e)((): ())): ()
pub let if(comptime e: effects, comptime t: type): with(e)
  (condition: bool)
  (move then: with(e)((): t))
  (move else: with(e)((): t)): t = {
  match condition
    { true -> then() }
    { false -> else() }
}
pub let match(comptime input: type, comptime output: type, comptime e: effects, comptime ...cases: parameters): with(e)
  (move input: input)
  ...cases: output
pub let for(comptime e: effects, comptime iterable: type, comptime iter: type, comptime item: type): with(e)
  (move iterable: iterable)
  (move body: with(core.control.loop_exit(()), core.control.iteration_skip, e)((item): ())): () =
requires(
  iterable is core.iter.into_iterator &&
  iterable.iter == iter &&
  iter is core.iter.iterator &&
  iter.item == item
)
```

Here `try` removes only `throwing(e)`, `unsafe` removes only the `unsafety` requirement, and both forward
the remainder row. `throw` introduces the standard `throwing(error)` requirement. `loop` and `for`
handle their declared `loop_exit`/`iteration_skip` effects while forwarding `e`; `if` and `match` evaluate
only the selected lazy branch or case. The source definitions that do not require intrinsic
lowering remain intentionally simple:

```sc fragment
pub let do(comptime e: effects, comptime t: type): with(e)
  (move action: with(e)((): t)): t = {
  action()
}

pub let try(comptime f: effects, comptime t: type, comptime e: type): with(f)
  (move action: with(core.error.throwing(e), f)((): t)): core.result(e)(t) = {
  core.error.throwing(e).handle raise { (error) -> core.result.err(error) } done { (value) -> core.result.ok(value) } action {
    action()
  }
}

pub let throw(comptime error: type): with(core.error.throwing(error))
  (move error: error): never = {
  core.error.throwing(error).raise(error)
}
```

`core.iter` owns iteration rather than the prelude:

```sc fragment
pub let iterator = trait {
  let item(comptime r: region): type
  let next(comptime r: region)(self: borrow(mut)(r)(self))
    (): core.option(item(r))
}

pub let into_iterator = trait {
  let into_iter: type
  let into_iter(move self)
    (): into_iter
}

pub let array_into_iter(comptime t: type)
  (comptime l: usize) = struct { ... }

pub let owned_item(comptime t: type)(comptime r: region): type = t
pub let borrowed_item(comptime a: access, comptime t: type)(comptime r: region): type =
  borrow(a)(r)(t)

pub let slice_iter(comptime a: access)(comptime t: type) = struct { ... }
```

Implementing or naming either trait requires aliases such as
`let iterator = core.iter.iterator` and `let into_iterator = core.iter.into_iterator`. The `for`
syntax itself needs no alias and dispatches only through these validated identities. It evaluates
the iterable once, moves it into `into_iterator.into_iter`, repeatedly mutably borrows the resulting
iterator for `iterator.next`, and stops on `none`. An inherent or unrelated trait method named
`into_iter` or `next` cannot intercept this lowering.

`array(t)(l)` implements consuming value iteration when `t: copyable`. A borrowed `slice(t)` exposes
access-polymorphic `.iter(a)`: `slice_iter(a)(t)` stores the source loan and yields
`borrow(a)(r)(t)` for the region of each `next(r)` receiver borrow. Shared iteration therefore
works for non-`copyable` elements without moving them, while mutable iteration yields exclusive
element borrows. A yielded mutable borrow must end before the next call to `next`; the source
remains borrowed until the iterator is consumed or leaves scope. `vec(t)` implements consuming
iteration for all element types. Its iterator transfers the allocation, moves values in source
order, and on early exit drops exactly the unyielded suffix before releasing storage.

The control spellings bind to these validated identities without aliasing ordinary names. Standard
effect identities such as `throwing` remain normal `core.effect` exports when named in source, backed
by `core.effect` identities. An ordinary same-named declaration cannot acquire lang-item lowering
behavior. future control features
follow the same rule: for example, async lowering must add `future`, `async`, and handler contracts
to the matching core release when it becomes executable, rather than reserving undocumented compiler
magic in advance.

`std.algebra` contains opt-in first-order algebra protocols rather than putting them in `core` or the prelude:

```sc fragment
pub let semigroup = trait {
  let combine(left: self, right: self): self
}

pub let monoid = trait(requires: self is semigroup) {
  let empty(): self
}
```

The compiler does not prove algebraic laws.

`std.functional` contains higher-kinded protocols over compile-time type constructors. It is not
part of the prelude:

```sc fragment
pub let functor = trait(self: (comptime value: type): type) {
  let map(comptime e: effects, comptime a: type, comptime b: type): with(e)
    (self: self(a))
    (transform: with(e)((a): b)): self(b)
}

pub let applicative = trait(self: (comptime value: type): type)(requires: self is functor) {
  let pure(comptime a: type)
    (value: a): self(a)

  let apply(comptime e: effects, comptime a: type, comptime b: type): with(e)
    (self: self(with(e)((a): b)))
    (value: self(a)): self(b)
}

pub let monad = trait(self: (comptime value: type): type)(requires: self is applicative) {
  let flat_map(comptime e: effects, comptime a: type, comptime b: type): with(e)
    (self: self(a))
    (next: with(e)((a): self(b))): self(b)
}
```

These declarations use constructor sorts such as `(comptime value: type): type` on the trait `self` subject,
not as ordinary trait parameters. Traits with a matching constructor subject can be implemented for
generic nominal constructors. Method implementations are registered as generic function templates
and validated, for example
`extend(carrier, functor) { let map(comptime e: effects, comptime a: type, comptime b: type) ... }`.
Receiver methods
dispatch from concrete nominal instances, so `carrier(i32) { value: 41 }.map(add_one)` selects the
`carrier: functor` implementation and instantiates the generic method template. Constructor
associated functions without a receiver can still be called from the bare constructor; for example,
`carrier.pure(...)` is available once `carrier` implements `applicative`. Trait-level `where`
constraints express protocol inheritance, so a
`carrier: applicative` implementation also requires `carrier: functor`, and `carrier: monad`
requires `carrier: applicative`.

The standard library implements `functor`, `applicative`, and `monad` for `core.option` and for
each partially applied `core.result(error)` constructor:

```sc fragment
let result = core.result
let monad = std.functional.monad

let next(value: i32): result(bool)(i32) = {
  result(bool)(i32).ok(value + 1)
}

let value = result(bool)(i32).ok(41).flat_map(next)
```

Curried constructors may be used as constructor trait implementation targets, which is how
`result(error): monad` is expressed without making `result` special. `option` and `result` are
ordinary enum values and require explicit constructors. Language error propagation is defined by
the standard `throwing(e)` effect, `throw`, and `try { ... }`; `do` has no error-specific semantics.

Primitive implementations remain compiler-defined. The unit type has the single spelling `()`. A declaration only
receives language-item behavior when its validated identity comes from this edition's embedded core;
same-named user declarations do not gain special semantics.

See [standard-library organization](README.md) for the prelude/alias policy and
[the language specification](../language/specification.md) for semantic rules.
