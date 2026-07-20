# Changelog

Salicin follows semantic versioning while the compiler is experimental. During 0.x, minor releases
may extend or tighten source semantics; patch releases preserve semantics within the implemented
subset.

## Unreleased

## 0.18.0 - 2026-07-21

- Lowered ownership transfer from enum match payloads into pattern bindings. Moving a payload clears
  whole-enum cleanup, gives each moved drop-bearing binding its own runtime flag, and materializes
  cleanup slots for active-variant siblings left behind by wildcards.
- Preserved cleanup across normal arm completion and early return, while isolating compiler cleanup
  state between candidates. Native trap coverage proves that unmatched resource siblings are
  destroyed and moved bindings are not destroyed twice.
- Kept values with custom `Drop` indivisible and rejected nested drop-bearing extraction and guarded
  payload moves until downcast projection trees and guard rollback are implemented.

## 0.17.0 - 2026-07-21

- Materialized recursive runtime flag trees for fields of structs without custom `Drop`. A field
  move clears the root, its projection ancestors, and the moved subtree while preserving initialized
  sibling flags.
- Lowered `children_when_clear` behavior: a complete root invokes whole-value glue once, while an
  incomplete root recursively drops only live child projections, including nested structs.
- Restored projection flags on field initialization and re-enabled the root when semantic flow proves
  every field complete. Conditional field overwrite now consults its own flag before cleaning a
  possibly present old value.
- Added native coverage for direct, nested, and conditional field moves, sibling fallback cleanup,
  field reconstruction, and conditional reconstruction. Types with custom `Drop` remain indivisible;
  drop-bearing enum patterns and closure environments are still rejected pending their lowering.

## 0.16.0 - 2026-07-21

- Connected each LLVM function emitter to its verified `CleanupPlan` and materialized drop flags
  for owned parameters and locals whose typed root move paths need destruction.
- Updated flags on root moves and reinitialization, conditionally dropped overwritten values before
  stores, and emitted reverse-order cleanup on normal block exit, explicit and implicit returns,
  loop breaks, match scrutinees, discarded expressions, and callee-owned parameters.
- Staged owned aggregate fields and call arguments until construction or invocation commits. If a
  later operand returns early, already evaluated resources are cleaned; successful completion clears
  the staging flags and transfers ownership exactly once.
- Added native observable cleanup tests using runtime traps, covering once-only destruction,
  conditional moves, overwrite, return, break, match, discarded values, and partial-construction
  early exits.
- Explicitly rejected projection moves/rebuilds, drop-bearing pattern bindings and temporary field
  extraction, and resource-owning closure captures until projection-level and closure-environment
  cleanup lowering can execute those cases correctly.

## 0.15.0 - 2026-07-21

- Added the canonical `Drop` trait to the edition 2026 core source and validated its exact
  `drop(mut borrow self)(): ()` contract through the normal parser and trait schema pipeline.
- Restricted `Drop` implementations to the package defining the nominal type, rejected types that
  also implement `Copy`, and prohibited direct source calls to `Drop.drop` so automatic cleanup
  cannot be preceded by a manual double-drop.
- Replaced conservative nominal `needs_drop` with recursive classification based on custom drop and
  field layouts. Containing structs and active enum variants inherit cleanup only from fields that
  actually need it.
- Emitted deterministic recursive LLVM drop glue for custom-drop types, structs, enums, and arrays,
  including discriminant dispatch for enums, and added native link/run coverage. Scope-exit calls
  and LLVM materialization of the v0.14 flags remain pending, so destructor side effects are not yet
  observable.

## 0.14.0 - 2026-07-21

- Added a semantic `needs_drop` classification to every static cleanup move path. Built-in `Copy`
  values remain trivial; non-`Copy` nominal aggregates and callables conservatively retain cleanup
  obligations until source-backed `Drop` provides exact recursive glue.
- Derived tree-shaped drop obligations from the cached `may_init`/`must_init` fixed point at every
  `StorageDead`. Definitely initialized values use static obligations, conditionally complete values
  use stable drop flags, and partial aggregates recurse into live children without double-dropping a
  parent and its fields.
- Added deterministic flag set/clear actions for storage lifetime changes, initialization,
  overwrite, move, transfer, and discriminant updates. Cleanup verification recomputes this analysis
  and rejects stale caches.
- Added cleanup and HIR regression coverage for `Copy` versus resource paths, static and conditional
  drops, partial aggregate cleanup, flag transitions, malformed drop forests, and cache consistency.
  This release plans destruction but does not yet expose `Drop` or emit LLVM flag storage and calls.

## 0.13.0 - 2026-07-20

- Removed `_` type and expression inference nodes from the AST and parser. Generic calls now infer
  omitted compile-time parameter groups from runtime arguments, constructor fields, variant payloads,
  fallible-container context, and expected result types.
- Kept ordinary parentheses for both compile-time and runtime application. Positional groups made only
  of recognizable type expressions remain explicit compile-time application; other groups begin runtime
  application, so `identity(20)`, `Cell(20)`, and `Option.Some(20)` infer without placeholder syntax.
- Added named compile-time arguments, including partial groups such as
  `Result(E: bool).Err(false)`, and named runtime arguments such as `make(value: 10)`.
  Named runtime arguments must follow declaration order, preserving left-to-right evaluation.
- Preserved `_` exclusively where it already means an ignored name, including wildcard patterns and
  anonymous callable signature slots. Type annotations such as `Cell(_)` and expressions such as
  `identity(_)(20)` now receive targeted parse diagnostics.
- Migrated the core regression suite and CLI fixtures to omission-based inference, and added coverage
  for contextual inference combined with named compile-time and runtime arguments.

## 0.12.0 - 2026-07-20

- Extended the cached cleanup fixed point with `may_live` and `must_live` state for every local.
  `StorageLive` now requires definitely dead storage, value initialization, movement, overwrite,
  transfer, discriminant writes, branch conditions, and return places require definitely live
  storage, and operation-position queries expose dead, maybe-live, or live state.
- Made structural `StorageDead` idempotent so one scope-exit summary safely closes storage that was
  entered on all, some, or none of the incoming paths. Scope-exit edges also end storage in the
  dataflow state, giving conditional temporaries a definite restart point without treating a
  cleanup marker as an executed destructor.
- Gave every `while` condition and loop body its own per-iteration temporary scope. Condition edges
  end the condition lifetime after its branch value is consumed, and normal body backedges end the
  body lifetime before re-entering it; scope verification now accepts that explicit exit followed
  by entry into a descendant evaluation scope.
- Removed `TemporaryStorageLiveness` from pending capabilities and added fixed-point, conditional
  join, idempotent-end, `while`, and `loop` regression coverage. Destruction remains disabled:
  `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, and LLVM cleanup emission are
  still future work.

## 0.11.0 - 2026-07-20

- Pre-registered a complete static move-path forest for every owned argument, return place, user or
  pattern binding, and planner temporary. Struct fields, every enum downcast and payload field,
  every constant array index, `Copy` values, and empty or zero-sized aggregates keep explicit
  paths; borrow aliases keep none. A checked per-function limit of 65,536 paths prevents oversized
  aggregate layouts from exhausting the compiler.
- Made both constant and dynamic array indexing explicit `Copy` extraction. The base (and runtime
  index when present) is still evaluated and staged exactly once, but indexing initializes the
  result without consuming an array element or inventing a non-finite dynamic move path.
- Added a cached control-flow fixed point over all move-path nodes. `may_init` joins by union,
  `must_init` by intersection, unreachable predecessors are ignored, scope-exit edges and
  `StorageLive`/`StorageDead` clear local state, and operation-position replay validates each
  `MoveOut`, `Overwrite`, and atomic `Transfer` against the stable state.
- Tracked enum discriminants alongside initialization. Active-downcast checks, field-to-root
  recomposition, overwrite invalidation, compatible transfer forests, initialized branch
  conditions, and complete return places are now verifier invariants in reachable control flow;
  malformed enum topology is rejected even in unreachable blocks.
- Removed `MovePathStateDataflow` from the pending capabilities. `Init` remains an idempotent
  initialization summary rather than an underlying write, and callable environment forests remain
  expression-backed because function types do not yet carry capture layouts.
- Kept destruction deliberately disabled. Temporary-storage liveness, conditional cleanup for
  maybe-overwrite, mutation through borrowed places, match/pattern transfer, and partial or local
  closure captures remain pending. There is still no `needs_drop`, runtime drop flag,
  source-backed `Drop`, drop glue, resource-bearing global cleanup, or LLVM cleanup emission.

## 0.10.0 - 2026-07-20

- Replaced the cleanup planner's result-presence boolean with concrete `CleanupDestination` places
  and explicit store-versus-discard uses. Resource-valued bindings, discarded expressions,
  assignments, function bodies, explicit returns, and every value-bearing `break` now have stable
  storage before ownership can cross a control-flow edge.
- Added atomic `Transfer` operations with initialize, overwrite, and maybe-overwrite destination
  states. Every transfer names distinct, non-overlapping source and destination move paths and
  consumes its source; verifier-enforced pending state dataflow keeps that not-yet-executable part
  visible in both directions.
- Represented aggregate construction through field, constant-index, enum-downcast, and closure-
  capture projections. Enum construction records its discriminant first, while a struct, array,
  enum, partial application, or closure root becomes initialized only after all children complete.
- Staged call arguments and field or index bases, and made uninhabited parameter entries and
  expressions terminate the cleanup CFG, including calls whose uninhabited result was hidden by
  contextual coercion. A later return, `break`, or diverging argument cannot commit an incomplete
  value to its final destination; nested `break` values abandon partial outer staging while
  transferring only the successful inner value.
- Removed the pending capabilities for unmaterialized resource results and loop-break value
  transfer. Temporary-storage liveness, move-path state dataflow, maybe-overwrite state,
  borrowed-place mutation, match dispatch and pattern transfer, and partial or closure capture
  details remain explicit pending capabilities.
- Kept LLVM emission unchanged: `CleanupPlan` still verifies ownership structure but does not emit
  destruction. `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, resource-bearing
  global semantics, and LLVM cleanup edges remain future work.

## 0.9.0 - 2026-07-20

- Replaced the single moved-place set with normalized alternatives of uninitialized move-path
  leaves. Root and projected moves can now be rebuilt by assigning the root or every field, while
  branch joins preserve correlated alternatives and loop backedges validate the resulting state.
- Bounded exact initialization alternatives at 64. Larger state spaces conservatively widen to
  fully initialized versus the union of possibly uninitialized leaves, preventing exponential
  growth without accepting a use that the exact analysis would reject.
- Distinguished initialized, uninitialized, and maybe-uninitialized uses and assignment kinds, so
  root/field reinitialization and conditional overwrites receive stable ownership diagnostics.
- Prevented a non-`Copy` pattern binding from being moved in a `match` guard: a failed guard may try
  a later candidate, so only a `Copy` binding may be explicitly consumed there.
- Added and verified one type-independent `CleanupPlan` per lowered HIR function. The plan records
  lexical, loop, and match-arm scopes, owned and borrowed locals, move paths, storage/init/move/
  overwrite events, and real branch, loop, guard, break, and return edges. Both checking and code
  generation build and verify this ownership CFG before continuing.
- Kept cleanup lowering deliberately non-executable. Pending capabilities explicitly cover
  unmaterialized resource results, move-path state dataflow, temporary-storage liveness, loop-break
  value transfer, mutation through borrowed places, maybe-overwrite state, match dispatch and
  pattern-binding transfer, and partial-application or closure captures.
- Did not add `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, or LLVM destructor
  emission. Compile-time globals are still constants materialized independently at each use and do
  not participate in `CleanupPlan`; resource-bearing globals and their `Drop` semantics must be
  fixed before `Drop` becomes observable.

## 0.8.0 - 2026-07-20

- Added the canonical source-backed `pub let Copy = trait {}` marker to the edition `core` bundle,
  with strict declaration-shape and lang-item identity validation; same-named user traits cannot
  acquire compiler `Copy` semantics.
- Made primitive types, `()`, `never`, the compiler's internal error-recovery type, and arrays whose
  elements are `Copy` intrinsically copyable.
- Added explicit nominal `Copy` implementations with `extend T: Copy {}`. Struct fields and every
  enum variant payload are checked recursively, including private representation fields and arrays,
  and only the package defining the nominal type may provide the implementation.
- Kept concrete generic implementations local to the exact instance: for example,
  `extend Cell(i32): Copy {}` does not make `Cell(bool)` or the generic template `Copy`; blanket and
  generic `Copy` implementations and `where`-based proofs remain unsupported.
- Routed validated nominal `Copy` through ordinary reads, inferred parameter modes, closure captures,
  and function or bound-method partial application. Unannotated parameters copy `Copy` values and
  move other values, while an explicit `move` still consumes even a `Copy` value.
- Kept function and closure types non-`Copy` in this implementation. `Drop` is not public yet: the
  next ownership step is internal scope cleanup and drop flags, before `Drop`, raw pointers, and the
  allocator ABI can unlock `alloc`; platform-facing `std` follows later.

## 0.7.0 - 2026-07-20

- Added source-backed `Sub(Rhs)`, `Mul(Rhs)`, `Div(Rhs)`, and `Rem(Rhs)` core lang-item traits
  alongside `Add(Rhs)`. Each strictly validated contract has an `Output` associated type and a
  matching method that receives both `self` and `rhs` by move.
- Extended `+`, `-`, `*`, `/`, and `%` to statically dispatch through their compiler-matched core
  trait for nominal left operands, while preserving direct built-in integer lowering.
- Generalized arithmetic-trait candidate selection to use expected `Output` constraints and integer
  literal range filtering, including type probing through local bindings in nonempty blocks, with
  deterministic mismatch and ambiguity diagnostics and single evaluation of both operands.
- Kept lang-item semantics identity-based: user traits with the same names cannot spoof an arithmetic
  operator contract or intercept its lowering.
- Guarded built-in integer division and remainder before LLVM lowering: a zero divisor and signed
  `MIN / -1` or `MIN % -1` trap at runtime, while the equivalent invalid constant expressions are
  rejected during compilation.

## 0.6.0 - 2026-07-20

- **Breaking:** struct and named enum payload fields are now private by default. Existing
  cross-module access must mark fields `pub(package)`, and cross-package access must mark them `pub`.
- Added private-by-default, `pub(package)`, and `pub` visibility to struct fields and named enum
  payload fields; effective field visibility is capped by the enclosing nominal declaration, while
  positional enum payloads inherit their enum's visibility.
- Enforced field boundaries across reads, writes, borrows, positional and labeled construction,
  generic inference, optional chaining, and struct/enum pattern destructuring, while preserving
  public core `Option` and `Result` payload access.
- Preserved nominal visibility and source provenance through generic instance construction and
  validation snapshots so monomorphization cannot erase or widen a template's access boundary.
- Rejected API signatures and exposed fields whose recursively nested nominal types have a narrower
  audience, covering functions, annotated globals, structs, enums, trait methods, and associated
  type defaults after module canonicalization.
- Added post-inference API checks for omitted function result and global annotations, including
  private types nested in generic containers such as `Option(Hidden)`.
- Made inherent extension members inherit their target type's API boundary, and prevented public
  trait implementations from selecting associated types narrower than the effective trait/target
  audience; private generic trait arguments also narrow method-candidate visibility.
- Prevented unqualified unit variants from discovering private enums outside their module boundary,
  and made single-source compiler entry points run the same canonical API validation as projects.
- Added cross-module and cross-package regression coverage for opaque private fields, package fields,
  public construction and named payload matching, and source-backed core positional variants.

## 0.5.0 - 2026-07-20

- Started the standard-library bootstrap with an edition-pinned `library/core` source bundle that is
  embedded into the compiler and parsed through the ordinary Salicin frontend.
- Moved `Option`, `Result`, and `never` out of hand-built Rust AST, promoted `Add` from per-program
  declarations, and placed all four in ordinary `.sali` source with deterministic shape validation.
- Added a structured lang-item registry that records the validated core declaration identities used
  by optional chaining, coalescing, propagation, throwing, and overloaded addition.
- Made `Add` part of the edition prelude, so implementations use the compiler-matched core trait
  without redeclaring it in each program.
- Ensured one core identity is shared across package graphs; same-spelled declarations, module
  aliases, child modules, and shadowing type parameters cannot acquire compiler language semantics.

## 0.4.0 - 2026-07-20

- Added strict `salicin.toml` package loading, default and explicit library/binary targets, target
  selection, project discovery, protected inputs, and project-local build outputs.
- Defined prelude `void` as the ordinary alias of `()` and `never` as an empty enum, including
  uninhabited control-flow coercion and empty-match elimination.
- Added automatically discovered file modules with deterministic two-pass package name resolution,
  nested qualified value/type/constructor paths, and canonical lowering across functions, nominal
  types, traits and extensions.
- Added private, `pub(package)` and `pub` top-level visibility, descendant access to private names,
  lexical shadowing during module resolution, and diagnostics for private, duplicate, conflicting,
  invalid and unknown qualified module names.
- Added declaration and module imports with aliases and grouped syntax, explicit `root` / `self` /
  `super` anchors, visibility-preserving `pub use` facades, cycle diagnostics, and access-chain
  checks that prevent private aliases from becoming visibility backdoors.
- Added strict local path dependencies, canonical dependency-cycle detection, library-only dependency
  source discovery, and deterministic atomically updated `salicin.lock` files.
- Added stable package identities and package-local dependency aliases, preserving nominal identity
  through shared diamond dependencies, preventing accidental transitive-dependency access, and
  enforcing private and `pub(package)` boundaries across packages.
- Preserved package and module provenance through semantic analysis so private trait methods cannot
  leak into method lookup across visibility boundaries, including optional chains and generic bodies.
- Required portable relative dependency paths and extended protected-output checks to hardlinks on
  both Unix and Windows.

## 0.3.0 - 2026-07-20

M2 development follows the v0.2 language foundation.

- Added compile-time `type` parameter groups and explicit generic named-function application.
- Added deterministic, cached, on-demand monomorphization while preserving runtime parameter groups
  and local partial application.
- Added definition-time abstract checking for generic function bodies, including return types and
  ownership, so invalid unused templates are diagnosed before instantiation.
- Added explicit generic struct and enum application, including nested data instances, constructors,
  variants, pattern matching, stable instance metadata and recursive-layout validation.
- Added side-effect-free `_` type-argument inference for generic functions, structs and enum
  variants, using expected and runtime argument constraints before lowering each argument once.
- Added concrete trait implementations with required associated types, exact grouped-signature and
  pass-mode validation, coherence checks, stable static method dispatch, inherent-method precedence,
  and support for concrete generic nominal instances such as `Cell(i32)`.
- Added name-based `Add(Rhs)` language-trait validation and statically dispatched `+` for concrete
  nominal left operands, including `Output`-guided inference, deterministic ambiguity diagnostics,
  literal range filtering and single-evaluation ownership semantics; integer addition remains built in.
- Added reserved prelude `Option(T)` and `Result(T, E)` generic enums as ordinary nominal templates,
  reusing existing constructors, type-argument inference, matching, monomorphization and LLVM layouts.
- Added `?.` optional chaining over owned prelude `Option` and `Result` values for struct fields and
  fully applied methods, preserving lazy arguments, single evaluation, residuals and nested outputs.
- Added right-associative `??` for prelude `Option` and `Result`, with single evaluation of the
  consumed container, lazy fallback control flow, payload-aware inference and ownership-flow joins.
- Added postfix `.try` propagation for explicitly annotated named functions returning prelude
  `Option` or `Result`, including single-evaluation residual returns, exact `Result` error matching,
  inferred operands, and automatic `Some` / `Ok` wrapping of normal tails and `return` values.
- Added `throw error` for explicitly annotated `Result` functions, lowering the error exactly once
  into the enclosing result's `Err` variant with precise error-type checking and terminating flow.

## 0.2.0 - 2026-07-20

- Added nominal structs and enums with positional or labeled construction.
- Added field access and mutable field assignment.
- Added exhaustive enum matching, payload bindings, guards and nested matches.
- Added local non-escaping partial application for multi-group named functions.
- Added place-aware `copy` and `move` checking, including use-after-move diagnostics.
- Added shared and mutable borrows, explicit borrow values, and pointer-based borrow parameters.
- Added path-sensitive move-state joins for conditionals, short-circuit expressions and matches.
- Added lambda lifting for local, non-escaping closures with shared/mutable scalar captures and
  single-use nominal move captures.
- Added complete grouped calls for curried capturing closures, with explicit diagnostics for
  unsupported partial application.
- Added `while`, value-yielding `loop`, and value-carrying `break` expressions.
- Added conservative loop back-edge checking that rejects moving bindings declared outside the
  loop body.
- Added inline fixed `Array(T, N)` values, array literals, and runtime-checked `i32` indexing;
  the initial subset is limited to `Copy` elements and read-only indexing.
- Added inherent `extend` blocks with statically dispatched `borrow self`, `mut borrow self`, and
  `move self` methods, plus type-namespace associated functions and constants.
- Added separate instance/type member lookup and conservative diagnostics for temporary receivers
  and non-`Copy` bound-method partial application; trait-backed extensions remain reserved for M2.

## 0.1.0 - 2026-07-20

- Added the `salic` single-file compiler and `.sali` source format.
- Added scalar types, named functions, parameter groups, local bindings, control flow and operators.
- Added textual LLVM IR generation and native linking through Clang.
- Added `build`, `check`, `emit-ir` and `run` commands.
