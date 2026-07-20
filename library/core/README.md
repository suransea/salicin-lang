# Salicin core bootstrap

This directory contains the compiler-matched Salicin sources that begin the `core` standard-library
layer. The compiler embeds `src/prelude.sali`, parses it with the ordinary frontend, validates every
lang-item declaration, and shares the resulting identities across the complete package graph.

The bootstrap bundle currently defines `Option`, `Result`, `never`, the canonical empty marker trait
`pub let Copy = trait {}`, the canonical `Drop` trait, and the five arithmetic operator traits `Add`,
`Sub`, `Mul`, `Div`, and `Rem`. Each operator trait takes `Rhs`, defines an `Output` associated type, and moves both `self`
and `rhs`. It deliberately has no package manifest yet:
mounting `core` as an explicitly addressable virtual package, compiling it without its own prelude,
and selecting per-package edition preludes are later bootstrap steps.

The v0.6 compiler closes field and exported-signature visibility before this source surface grows:
core types may now expose selected fields without making representation details constructible, and
public declarations cannot accidentally name narrower implementation types.

The v0.7 compiler validates the identity and exact shape of all five source-backed arithmetic traits.
Nominal left operands use unique static trait dispatch with expected-output and integer-literal range
filtering; built-in integers keep direct lowering, and same-named user traits cannot spoof lang-item
semantics.

The v0.8 compiler likewise validates `Copy` by canonical identity and exact shape. Primitive types,
unit, `never`, the internal error-recovery type, and arrays of `Copy` elements are intrinsic. A
nominal struct or enum opts in with `extend T: Copy {}` only in its defining package, and every field
or variant payload must recursively be `Copy`. An implementation for a concrete generic instance
does not generalize; blanket/generic implementations and `where` proofs are not supported yet.
Validated nominal `Copy` participates in inferred parameter passing, ordinary reads, closure
captures, and partial application, while explicit `move` still consumes. Function and closure types
remain non-`Copy`, and same-named user traits cannot spoof the lang item.

The v0.9 compiler normalizes move-path initialization alternatives, permits sound root and field
reinitialization, conservatively widens after 64 exact alternatives, and prevents non-`Copy`
pattern bindings from being moved in match guards. It also builds and verifies a type-independent
`CleanupPlan` from each function's real HIR scopes and control-flow edges.

The v0.10 compiler gives resource expressions concrete cleanup destinations. Bindings, discarded
values, assignments, returns, loop breaks, call arguments, and projected reads use stable staging;
atomic transfers consume one move path and initialize or overwrite another. Structs, arrays, enums,
partial applications, and closures initialize projected children before their root, so an early
exit leaves a representable partial value instead of committing an invalid destination.

The v0.11 compiler pre-registers complete static move-path forests for owned storage, including all
struct fields, enum variants and payloads, constant array indexes, `Copy` values, and empty
aggregates. Borrow aliases have no owned paths, and a checked 65,536-path per-function limit bounds
aggregate expansion. Constant and dynamic indexes are modeled as `Copy` extraction rather than
consuming or runtime-indexed move paths.

`CleanupPlan` now caches a control-flow fixed point over `may_init` and `must_init`, clears state on
storage and scope exits, replays operations at their exact position, and tracks enum discriminants
for active-downcast, reconstruction, overwrite, transfer-shape, branch, and return validation.
`MovePathStateDataflow` is no longer pending. Function types still lack an environment layout, so
concrete callable captures remain expression-backed and explicitly pending.

The v0.12 compiler extends that fixed point with `may_live` and `must_live` for every local.
Operations can only use definitely live storage, `StorageLive` can only start definitely dead
storage, and idempotent structural `StorageDead` summaries close conditional lifetimes. Per-
iteration temporary scopes end `while` conditions and loop bodies before their next evaluation, so
`TemporaryStorageLiveness` is no longer pending.

The v0.14 compiler marks every static move path with a type-driven `needs_drop` classification and
derives tree-shaped obligations at each storage end. Definitely complete values need no runtime
flag; conditionally complete values receive stable flags with set/clear actions; partially
initialized aggregates fall back to their initialized child obligations without double-destruction.
The verifier recomputes and checks the cached analysis.

The v0.15 compiler validates `Drop.drop(mut borrow self)(): ()` from this ordinary core source.
Implementations are restricted to the nominal's defining package, conflict with `Copy`, and cannot
be invoked directly. Exact recursive `needs_drop` now emits LLVM glue for custom drops, containing
structs, and active enum variants; the glue passes native parsing and linking tests.

The v0.16 emitter consumes the verified cleanup plan's typed root classification and materializes
LLVM flags and glue calls for owned parameters, locals, reverse lexical scope exit, return, break,
conditional root moves, overwrite, discarded values, and match scrutinees. Aggregate fields and
call arguments are staged so a later early return cleans already evaluated owned values before
their transfer commits. Native trap tests make execution and double-drop failures observable.

The v0.17 emitter materializes a recursive flag tree for fields of structural structs. Moving a
field clears its root, ancestors, and target subtree while preserving sibling flags; a cleared root
falls back through child obligations. Reinitialization restores the subtree and re-enables whole-
value glue once semantic flow proves the root complete. Conditional field overwrite consults the
projection flag before dropping the old value. Native tests cover nested movement, sibling cleanup,
and conditional reconstruction. Fields cannot be moved through a type that itself has custom
`Drop`, and enum payload, pattern, and closure-environment projections remain pending.

The v0.18 emitter transfers direct enum payload ownership into unguarded match bindings. It clears
whole-enum ownership, registers moved resource bindings independently, and preserves active-variant
resource siblings as fallback cleanup slots across normal and early-return exits. Custom-`Drop`
enums remain indivisible; nested payload moves and guarded resource moves await downcast trees and
guard rollback. Closure-environment projections remain pending.

The v0.20 emitter makes guarded payload bindings speculative. A guard can inspect non-owning
binding storage; only its successful body edge commits enum decomposition and activates binding and
remainder flags. Failure and early return retain the intact enum root for the next candidate or
scope cleanup. Non-`Copy` bindings still cannot be consumed inside the guard itself. Whole-value
guarded bindings preserve custom-`Drop` values, while their payloads remain indivisible.

The v0.19 emitter recursively partitions nested structural payload patterns. A deep moved binding
owns its selected subtree, while resource siblings at every enclosing struct and active-variant
level retain independent cleanup slots on normal and early-return exits. Traversal through a type
with custom `Drop` remains forbidden because its destructor requires an intact value. Guarded
resource transfer still awaits rollback-aware lowering.

The v0.21 emitter gives local `FnOnce` move captures stable environment storage and recursive drop
slots. Abandoned and conditionally invoked closures clean retained captures; invocation transfers
them through early-exit argument staging to the lifted function without double drop. The cleanup
plan no longer reports `LocalClosureCapture` as pending. General partial applications remain
Copy-only, and first-class or escaping callable environments still need an explicit ABI layout.

The v0.22 emitter permits owning move captures in local partial applications. Such a partial is
`FnOnce`; capture flags transfer through further currying, final invocation, conditional use, and
early-return staging. Abandoned partials clean their retained captures. The separate
`PartialApplicationCapture` pending capability has been removed, leaving no callable-capture
pending marker. Borrowed captures and first-class or escaping callables still require a public ABI.

The v0.23 emitter performs drop-aware overwrite through mutable-borrow parameters. It evaluates the
replacement first, calls exact glue on the old root or projected field, and then transfers the new
value into borrowed storage. No owned flag is fabricated for the referent. The
`BorrowedPlaceMutation` pending capability has been removed; native traps verify both root and field
cleanup.

The v0.24 cleanup planner makes match refinement and binding ownership executable IR rather than
side-channel promises. `AssumeDiscriminant` refines a fully initialized enum at arm entry, and
ordinary atomic transfers commit pattern ownership immediately for unguarded arms or only on the
success edge for guarded arms. The verifier checks enum topology, liveness, and initialization.
`MatchDispatch`, `PatternBindingTransfer`, `MaybeOverwrite`, and the entire `PendingCapability`
infrastructure have been removed.

The v0.25 compiler lets named functions, closures, and partial applications move between local
bindings. Owning captures relocate to fresh environment storage while old flags are cleared;
borrowed captures retain their lexical loans. `FnOnce` invocation now consumes the callable root in
cleanup IR after argument staging. This is concrete, statically known relocation rather than
implicit boxing or dynamic erasure; cross-function callable return and parameter ABI remains open.

The v0.26 compiler assigns a named LLVM environment struct to every anonymous concrete closure or
partial type. Owning environments return by value across function boundaries, retain a statically
known call target, and participate in move forests, flag trees, and recursive drop glue. Copy and
resource captures, relocation after return, consumption, and abandonment have native coverage.
Borrow-capturing environments still cannot escape; generic callable parameters await the source-
level `Fn`/`FnMut`/`FnOnce` constraint surface.

Compile-time globals are independently materialized at each use and are
outside the cleanup plan; resource-bearing global semantics must be settled before `Drop` is
allowed on globals.

The adjacent standard-library route is therefore: finish generic callable parameters, then define
raw pointers and the allocator ABI. Only after those
boundaries are real will `alloc` be added, followed
by platform `std` over the C ABI and minimal runtime.
