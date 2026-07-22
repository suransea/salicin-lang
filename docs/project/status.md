# Implementation status

Salicin is an experimental compiler and language design. The repository currently includes a native
compiler pipeline, project manifests and local dependencies, ownership and borrow analysis,
source-backed core traits and containers, cleanup lowering, raw allocation primitives, and a growing
`Box`/`Vec` allocation library.

A current design goal is to keep compiler-backed language features source-shaped wherever possible:
standard control capabilities should be declared as ordinary core effects, traits, or protocols,
then validated as lang items and lowered by the compiler. `Throws(Error)` already follows this
model as a normal effect with `raise`; future work should extend the same rule to customizable
operators such as `throw` rather than adding closed compiler-only cases for each type.

The unit type has one source spelling, `()`; the former `void` alias is removed before 1.0. The
uninhabited prelude enum is spelled `Never`; the former lowercase `never` spelling has no
compatibility alias.

Transparent type aliases and type-constructor aliases are implemented. `let Scalar = i32`,
`let Family(T: type): type = Box(T)`, and `let Constructor: (T: type): type = Box` all expand before
runtime lowering, preserve the target nominal identity, support forward references and constructor
inference, and reject alias cycles and arity mismatches. Type positions also accept labeled
constructor arguments such as `Pair(V: bool, K: i32)`; labels are matched against the constructor's
compile-time parameter names, normalized to declaration order, and erased before semantic lowering.

Compiler-lowered capabilities are now source-backed by validated declarations in ordinary core
modules: `core.effects` owns `Unsafe`, `Throws(Error)` with `raise(move error: Error): Never`, and
an ordinary `Async` effect with a minimal `suspend(): ()` operation;
`core.access` owns `Shared` and `Mutable`; `core.control` owns the bodyless intrinsic signatures for
`do`, `try`, `throw`, `unsafe`, and `loop`; `core.ops` owns the standard `Chain` and `Coalesce`
protocol declarations for `?.` and `??`. These exports remain outside the prelude. `await` is
intentionally absent until the async/Future lowering slice is implemented, at which point its
executable standard-library contract must land with the implementation.
`Never`-returning algebraic operations are handled as abort operations whose clauses omit `resume`,
so `Throws(Error).raise` can now be exercised through the same handler path as user-defined effects.
`throw error` reads the validated `core.control.throw` effect declaration and then desugars to that
ordinary operation when the active row has a unique standard `Throws(Error)` effect.
`core.control.try` now declares its action requirement as `Throws(E)`, and contextual `try { ... }`
with an expected `Result(T, Error)` materializes ordinary `Throws(Error)` as `Result` through a
generated `Throws(Error).handle`. Context-free ordinary `Throws` inference now
covers direct standard-effect function calls, local function values, explicitly instantiated generic
calls such as `fail(bool)(...)`, and `do` return boundaries forwarding standard `Throws(Error)` into
a contextual `try { ... }`, provided the success type is probeable and the escaping error type is
unique. Mixed `Throws(Error)` plus `Unsafe` rows now run through the standard effect spelling while
preserving `unsafe { ... }` authorization across generated CPS frames. Concrete residual-handler
paths also compose through ordinary `Throws(Error)` rows. `Never`-only actions, enum-variant error
construction inside generated standard handler frames, and fully generic residual-row cases remain
future work.
`core.control` also defines `Continuation(Input, Output)` and
`EffectCallable(Input, Output, Answer)` as validated empty source contracts. The latter has a
distinct owned semantic type plus a four-pointer LLVM call/drop/environment/flag layout and guarded
drop glue. Compiler-internal HIR can now erase an owned CPS closure into that layout and invoke it
with an input plus `Continuation(Output, Answer)`; target-specific adapters preserve captured
environments, and cleanup planning treats both operations as ownership transfers. Reusable handlers
now accept directly passed, explicitly typed effectful closure bindings even when the complete
handler call occurs later or is nested inside a larger expression. The callable environment is
created at the original declaration, then its shared and mutable `Copy` captures are lifted
as `borrow` and `borrow(mut)`, while consuming owned roots are lifted as `move`; the closure is then
injected before selective CPS. Native regressions cover `FnMut` resumption plus `FnOnce` cleanup on
both resumption and abandonment, including state/drop observations in following evaluation.
Local callable alias moves now carry the original action metadata and relocate borrowed pointer
slots without confusing them with owned pointee values. A direct trailing-closure action is also
materialized automatically. Earlier `copy` and `move` arguments across the complete call are staged
as typed locals in source order before that action, preserving side effects and ownership. Earlier
borrowed arguments remain pending loan-aware staging; conditional values, cross-function transport,
and fully general erased action construction remain the next implementation stages.

Structured control flow includes `while`, value-producing `loop`, `break`, and `continue`.
`continue` targets the nearest loop, participates in loop-backedge ownership validation, and runs
all lexical cleanup required when leaving nested scopes before starting the next iteration.
`for name in value { ... }` and `for _ in value { ... }` lower through validated, source-backed
`core.iter.IntoIterator` and `core.iter.Iterator` identities. The iterable is evaluated once,
`into_iter` consumes it, and each iteration mutably borrows the iterator for `next`; unrelated
same-named methods cannot intercept the lowering. Break, continue, ownership flow, and cleanup reuse
the ordinary loop machinery.
`if let pattern = value { ... }` supports conditional enum destructuring with optional `else` or
`else if`. It evaluates the scrutinee once and lowers through ordinary `match`, so successful-arm
bindings stay scoped to that arm and share the same ownership and cleanup analysis.
`while let pattern = value { ... }` reevaluates the scrutinee each iteration and exits when the
pattern fails. It lowers to the same `match` and unit-loop machinery, including normal `break`,
`continue`, ownership backedges, and lexical cleanup.
Arithmetic, bitwise, and shift compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`,
`^=`, `<<=`, `>>=`) resolves its left place once.
Built-in integers retain checked trap boundaries, while nominal values dispatch through the
source-backed `core.ops` `*Assign` traits with a mutable receiver borrow. Same-named ordinary methods
cannot intercept operator lowering.
`core.ops.Chain` uses a `Rebind(Value: type): type` generic associated constructor and `Coalesce`
uses an effect-forwarding fallback closure. The compiler now accepts such GAT declarations and
method-signature references, but still rejects GAT implementations and GAT where-predicate
equalities. Concrete nominal trait implementation methods can now carry matching compile-time
parameter groups and are registered as generic templates, which unblocks source-level protocol
methods such as `coalesce(E)`. `??` now dispatches non-`Option`/`Result` nominal values through
`core.ops.Coalesce` when its fallback can be represented as a no-capture lifted function. Capturing
fallback closures still require the general callable-to-function argument bridge. `?.` dispatch
through `Chain` remains future work; the built-in `Option`/`Result` paths remain available.

`core.algebra` currently provides first-order `Semigroup(T)` and `Monoid(T)` protocols outside the
prelude. `core.functional` now provides higher-kinded `Functor`, `Applicative`, and `Monad`
protocol declarations over constructor kinds such as `(Value: type): type`. Constructor-valued
implementations currently register matching generic nominal constructors and validate method bodies
as generic function templates. Constructor trait associated functions without a receiver can dispatch
from the bare constructor, so implementations such as `extend Option: Functor` can expose
`Option.map(...)` through the ordinary generic function instance pipeline. The remaining HKT work is
associated-type lowering, receiver-style HKT methods, trait inheritance constraints, and broader
constructor equation solving.

Access keyword generics are implemented for functions and generic inherent members: `A: access` accepts `shared` or `mut`,
defaults to shared when omitted, participates in monomorphization, and can drive parameter modes,
borrow types, borrow expressions, and raw pointer borrows. The alloc free functions and methods use
this path. Mutable borrowing has one source spelling, `borrow(mut)`; separately named mutable alloc
aliases and the former prefix spelling are intentionally absent before 1.0.
Passing keyword generics are also implemented for functions and generic inherent members:
`P: passing` accepts `auto`, `copy`, or `move` and can be referenced directly in parameter keyword
position. Functions and trait methods place a contextual `with(...)` clause after the result type:
`: T with(Unsafe)` adds the checked unsafe call requirement, while `: T with(Throws(E))` declares the
standard recoverable-error effect. `try { ... }` handles that effect and produces an explicit
`Result`. Without a contextual result type, direct ordinary `Throws(E)` calls and local function
values can infer `Result(T, E)` when the success type and unique error type are probeable; postfix
`.try`, lowercase `with(throws...)`, lowercase `with(unsafe)`, and `with(try...)` are removed with
no compatibility aliases or dedicated parser migration paths.
Callable source types use the same shape, such as
`(i32): i32 with(Unsafe)`; the clause is not a runtime or currying group. Complete direct, method,
aliased, and partially applied unsafe calls require an enclosing `with(Unsafe)` function or
`unsafe { ... }` handler. `do` forwards the implemented `Unsafe` effect into nested immediate
calls. `let UI = effect` declares a nominal, module-visible marker effect.
Parameterized user effects may declare typed operation requirements. Operation calls use an exact
instantiated identity such as `State(i32)`, propagate through the existing row machinery, and are
checked for parameter modes, result types, arity, visibility, and missing row requirements.
Operations share the language's name-only overload rule: runtime label shapes must differ, calls
use named arguments, and repeated handler labels select signatures through clause parameter names.
Handling removes only the selected nominal identity: operation gates and generated resumable frames
retain residual `Unsafe`, `Throws(E)`, and other nominal requirements. Concrete residual
`Throws(E)` rows now compose through nested handlers using the ordinary standard effect path; fully
effect-parameterized residual rows remain implementation work.
Derived handlers support typed one-shot resumption, abandonment, `done:` answer conversion, named-call
propagation, direct recursion, and resumable loop backedges. Cross-function abandonment and
computation after `resume` use explicit CPS continuation closures. Direct and mutually recursive
frames share an erased call/drop-entry plus environment ABI with a runtime one-shot flag.
Reusable handler functions may accept an algebraic-effect callable parameter. Calls with a known
named function or immutable function alias create a deduplicated static specialization, erase that
parameter from the runtime groups, and run the substituted action through the handler's ordinary CPS
pass. A complete call may select that leading action through a nested conditional tree; the call is
distributed into target-specific specializations after evaluating the selector and before later
curried arguments. Truly unknown runtime callable parameters still require the general handler-aware
callable ABI.
Inferred immutable local aliases of statically known effectful functions are resolved through the same CPS
path, including chained aliases. Statically known function arguments also specialize higher-order
effectful frames and are erased from those frames' runtime parameter lists. Explicitly typed
capturing local closures use a hidden erased continuation argument while lexically enclosed by the
handler. Their ordinary capture environments preserve `Fn`, `FnMut`, and `FnOnce` behavior,
including repeated mutable calls and exactly-once abandonment cleanup, and they may specialize a
higher-order frame. Finite conditional trees between named targets use a binding-site integer tag
and call-time resumable branch dispatch, including forwarding through a higher-order frame.
Finite selections may target lexically registered capturing resumable closures while preserving
`FnMut` state, `FnOnce` consumption, and exactly-once cleanup. Escaping callables and open-ended
dynamic targets remain implementation work and receive dedicated diagnostics. A finite selection tag
may be copied through immutable handler-local aliases and forwarded into a specialized higher-order
frame. Mutable aliases accept assignments with the same signature and finite target set, remapping
runtime tags across different target orders; incompatible sets are rejected before ordinary value
lowering. Nested selections may union existing dynamic values and remap their tags. Effectful nested
selectors forward capturing branch environments through their continuation, preserving shared and
mutable borrows, `FnMut` state, `FnOnce` transfer, and exactly-once nested cleanup.
Compiler-generated CPS closures carry a separate lexical handler-capability set, allowing an inner
handler's residual algebraic row to compose through an already specialized outer named frame without
publishing that capability on the closure type. Throwing handler tails return through their physical
`Result` boundary when wrapping prevents a direct tail call.
Recursive-frame visibility is limited to callee-body
transformation, so sequential calls to the same effectful named function remain independent.
Abandonment invokes the armed environment's drop entry,
whereas resumption transfers and disarms it; native resource regressions cover exactly-once cleanup
on both paths. CPS traversal
currently covers ordinary arguments, arrays, indexes, members, match bodies, immediate effect
wrappers, lazy boolean branches, lazy `Option`/`Result` coalescing, and match guards over `Copy`
inputs. Arguments of an effect-propagating named call are transformed before its resumable callee
frame, including multiple left-to-right suspensions. Fully applied optional method calls preserve receiver-before-argument order and skip
effectful arguments on residual paths for both builtin fallible families. Suspended guards over
non-Copy match inputs use a binding-erased inspection pattern before moving the owned value into the
continuation. Payload bindings are rematched and committed only after a `true` resumption, while
`false` resumes into the remaining ordinary match candidates. Referenced Copy bindings cross by
value; referenced non-Copy bindings are reconstructed as read-only projections from each
continuation's owned enum capture. Those views may be inspected or borrowed but not moved before the
guard commits.
Different user-defined handlers compose lexically through action, clause, and generated-frame
closure boundaries; nested handlers of the same identity retain nearest-boundary selection.
Function and generic inherent-member `E: effect` parameters represent complete rows, default to pure,
participate in monomorphization, forward through ordinary compile-time calls such as
`callee(E)(value)`, and infer pure, Unsafe, custom, or standard `Throws(Error)` rows from
higher-order callable arguments. A selected `Throws(Error)` row preserves its error type through
forwarding and specialization.
Named non-capturing functions can be passed and invoked through the native function-pointer ABI.
Concrete and generic top-level functions, concrete-nominal inherent members, and trait requirements may form label-directed
overload sets. Their runtime parameter-label shapes must differ, and at least one explicit named
call argument must select a unique candidate; a method's implicit receiver is not disambiguating
evidence. Trait conformance, default and blanket implementations, where-bound assumptions, curried
groups, module resolution, imports, type and optional-chain probing, closure lowering, effects, and
native mangling preserve that choice. Generic templates are selected by runtime labels before their
compile-time groups are inferred or consumed. Blanket generic inherent extensions preserve the same
overload set across every applicable concrete nominal instance.
Callable effect rows support requirement subtyping: a pure function value can fill an unsafe or
custom-effect slot, while a value requiring additional effects cannot fill a narrower slot. The
slot's widened requirements remain checked at indirect calls, and generic row inference retains the
callable's exact source row.
Fixed ordinary `Throws(E)` direct calls, contextual `try` handling, `do` return-boundary
forwarding, mixed `Throws(E)`/`Unsafe` rows, and concrete residual-handler composition are
implemented for the public direct, explicitly instantiated generic, mixed-effect, and residual
fixtures.
Effect-parameterized residual-handler lowering still needs full standard-row coverage. Ordinary
`Option` and `Result` functions require
explicit variant construction; the removed `Try`, `FromResidual`, `FromError`, and `ControlFlow`
language protocols no
longer participate in return completion or propagation. `do` transparently forwards the complete
active row through its immediate closure boundary, including recoverable-error, `Unsafe`,
and nominal marker effects. Capturing closure values, generic trait methods, the remaining general
algebraic-continuation ABI, and async color lowering remain design or implementation work.

`core` and `alloc` are mounted in ordinary module resolution. `core.ops` traits and alloc containers
are not part of the prelude. `Box`, `Vec`, and their free functions require
`use alloc.boxed...` / `use alloc.vec...` (or a qualified path), while operator traits require
`use core.ops...` when named. Their internal identities remain isolated from same-named user
declarations; operator syntax continues to dispatch through validated lang items.

The implementation is broad but not stable. Important incomplete boundaries include:

- `core` provides the initial prelude plus arithmetic, bitwise, unary, equality, partial-ordering,
  control, and iteration protocols. Language error propagation is the standard `Throws(E)` effect.
  Slices, trait-based indexing, standard array/container iterator implementations, and `Future`
  remain to be implemented;
- `std` host APIs have not been started;
- registry dependencies, workspaces, stable ABI guarantees, and a package distribution format are
  not defined;
- asynchronous syntax is designed but Future lowering and an executor interface are not complete;
- diagnostics, source locations, tooling, and incremental compilation need substantial work.

The [language specification](../language/specification.md) states intended language rules. This file
records implementation state. Release-specific additions and fixes are recorded only in the
[changelog](../../CHANGELOG.md).
