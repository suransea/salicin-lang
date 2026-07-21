# Changelog

Salicin follows semantic versioning while the compiler is experimental. During 0.x, minor releases
may extend or tighten source semantics; patch releases preserve semantics within the implemented
subset.

## Unreleased

## 0.52.0 - 2026-07-21

- Added type-qualified inherent and trait method calls such as `Number.read(number)()` while
  reusing the same borrow, mutable-borrow, move, partial-application, and temporary-receiver rules
  as instance syntax.
- Allowed `self:` as a receiver argument label to select a method when an associated function has
  the same name; unlabelled type calls continue to prefer associated functions and enum variants.
- Inferred omitted generic nominal arguments from a concrete receiver, allowing
  `Cell.read(cell)()` to resolve a `Cell(i32)` method without `_` syntax.

## 0.51.0 - 2026-07-21

- Extended `match` to boolean and integer scrutinees with literal, binding, wildcard, and guarded
  arms.
- Staged each scalar scrutinee exactly once and gave every guard and arm body an independent
  lexical scope while preserving source-order candidate fallback.
- Recognized unguarded `true` plus `false` as an exhaustive boolean match; integer matches and
  guarded boolean coverage still require an unguarded wildcard or binding fallback.

## 0.50.0 - 2026-07-21

- Added integer and boolean literal patterns inside enum payloads and nested struct patterns.
- Lowered literal tests into short-circuit candidate guards, preserving source-order fallback and
  evaluating explicit guards only after every literal test succeeds.
- Kept resource pattern bindings speculative until literal tests pass, then committed their moves
  exactly once; added type and integer-range diagnostics for invalid literal payload patterns.

## 0.49.0 - 2026-07-21

- Allowed a constant index to move a resource element directly out of an array temporary,
  including array literals and arrays returned from calls.
- Transferred the selected element from its cleanup constant-index path while dropping every
  unselected resource element exactly once.
- Continued to require `Copy` elements for dynamic indexing because a runtime-selected element
  does not yet have a finite static move-path identity.

## 0.48.0 - 2026-07-21

- Allowed fixed arrays to contain move-only nominal and resource values instead of requiring every
  element to implement `Copy`.
- Completed resource-array cleanup by discovering local array drop glue, expanding runtime drop
  slots per element, and preserving exactly-once drop across element moves, reinitialization, and
  overwrites.
- Kept dynamic and temporary-array indexing limited to `Copy` elements; resource elements must be
  accessed through a bound array and a constant-index place so ownership has a finite identity.

## 0.47.0 - 2026-07-21

- Promoted fixed-array constant indices to full places, allowing reads, shared and mutable
  borrows, assignment, explicit moves followed by reinitialization, and raw-pointer construction.
- Tracked every fixed-array element as a distinct ownership and loan leaf, so disjoint constant
  indices can be accessed independently while conflicting accesses are rejected.
- Mapped typed nested struct/array projections into cleanup constant-index paths and retained
  runtime-checked dynamic indexing as a read-only expression for now.

## 0.46.0 - 2026-07-21

- Allowed complete calls to pass temporary values directly to shared and mutable borrow
  parameters, matching the temporary receiver behavior from v0.42-v0.43.
- Staged every value argument in source order whenever a call contains a borrowed temporary,
  preserving left-to-right effects, stable addresses, early-exit cleanup, and exactly-once drop.
- Applied the same lowering to named and inferred generic functions, bound-method argument groups,
  and local partial calls while continuing to reject partial applications that capture borrows.

## 0.45.0 - 2026-07-21

- Added safe source-backed `box_write(boxed)(value)` and `boxed.write(value)` APIs for `Box(T)`
  when `T: Copy`, complementing v0.44's safe Copy reads.
- Required a mutable Box borrow while copying the replacement into heap storage; non-Copy boxes
  continue to use ownership-aware `replace` and never materialize the write member.
- Extended alloc bootstrap validation and taught raw Copy stores to preserve the zero-sized unit
  representation.

## 0.44.0 - 2026-07-21

- Added safe source-backed `box_read(boxed)` and `boxed.read()` APIs for `Box(T)` when `T: Copy`,
  without exposing a raw pointer or transferring pointee ownership.
- Defined `read` in a separate constrained generic inherent extension, so resource boxes retain all
  owning operations but do not acquire an unsound copying method.
- Extended strict alloc bootstrap validation for the new function, Copy proof, and extension shape,
  and taught raw Copy loads to preserve the zero-sized unit representation.

## 0.43.0 - 2026-07-21

- Allowed temporary nominal values to receive inherent and trait methods through
  `mut borrow self`.
- Marked compiler-generated receiver bindings mutable only when the selected method requires it,
  preserving a stable address and running resource destruction exactly once after the call.
- Continued to reject partial applications that would capture either shared or mutable receiver
  borrows beyond the complete call.

## 0.42.0 - 2026-07-21

- Allowed temporary nominal values to receive methods through shared `borrow self`, including both
  inherent and trait-dispatched methods.
- Materialized each borrowed temporary receiver as a compiler-generated lexical binding so it
  remains addressable through the complete call and is destroyed exactly once afterward.
- Kept mutable borrowing of temporary receivers explicit: `mut borrow self` still requires a
  mutable local binding and now reports a focused diagnostic.

## 0.41.0 - 2026-07-21

- Enabled source-backed default trait methods for concrete and blanket implementations; omitted
  methods now use the trait body while explicitly supplied methods override it.
- Substituted `Self`, trait parameters, and associated types into each concrete default method and
  retained the trait definition's source provenance for name and visibility resolution.
- Added abstract definition-time checking for default bodies using an assumed `Self: Trait` proof,
  including all associated-type equalities, even when the trait has no implementations.
- Verified default methods that call required methods, consume `self`, return associated types, and
  dispatch through nested blanket implementations.

## 0.40.0 - 2026-07-21

- Added definition-time signature checking for every blanket trait method, including trait type
  arguments, `Self`, and associated-type substitutions.
- Reused generic-function abstract validation for blanket method bodies through hidden templates,
  so unused implementations are checked under their declared where proofs.
- Allowed abstract trait arguments only inside guarded template validation and rolled every
  temporary nominal, method, signature, and assumed implementation back afterward.
- Added eager diagnostics for invalid associated type expressions, signature mismatches, missing
  names, and incomplete blanket `Drop` implementations without requiring concrete instantiation.

## 0.39.0 - 2026-07-21

- Replaced conservative same-trait blanket rejection with first-order unification of trait argument
  patterns, including nested nominal and array types with occurs checking.
- Allowed provably disjoint blanket implementations such as `Convert(i32)` and `Convert(i64)` on
  the same generic target.
- Added source-order-independent coherence checks between blanket and concrete implementations,
  grounding blanket parameters from each concrete target before deciding overlap.
- Kept where predicates out of disjointness proofs, so potentially compatible implementations
  remain rejected without specialization or a proof of mutually exclusive constraints.

## 0.38.0 - 2026-07-21

- Enabled blanket generic `Copy` implementations with `where` proofs, including nested generic
  structs and enums.
- Added definition-time abstract structural validation, initial fixed-point participation, and
  immediate validation for nominal instances materialized after the fixed point.
- Enabled blanket generic `Drop` implementations and integrated their concrete methods into the
  existing recursive drop glue and structured cleanup paths.
- Preserved defining-package ownership for both language traits and rejected every concrete
  instance that would acquire both `Copy` and `Drop`.

## 0.37.0 - 2026-07-21

- Implemented blanket generic trait extensions such as
  `extend(T: type) Cell(T): Read where T: Read`, selected lazily for each concrete nominal
  instance.
- Substituted target parameters through trait arguments, method signatures and bodies, where
  predicates, and associated-type definitions including `let Item = T`.
- Added conditional nested selection, deterministic blanket-overlap rejection, and the general
  package orphan rule requiring either the trait or target type to be local.
- Kept blanket `Copy` and `Drop` implementations reserved for their structural fixed-point and
  destruction-coherence work.

## 0.36.0 - 2026-07-21

- Added `where` predicates to blanket generic inherent extensions with the documented multiline
  syntax.
- Constrained methods are materialized only for nominal instances whose concrete type arguments
  satisfy every predicate; unsatisfied instances do not acquire the member at all.
- Propagated extension predicates onto generic associated-function templates, producing precise
  call-site constraint errors, and made assumed generic proofs participate in conditional member
  selection.
- Restricted extension member API boundaries by predicate traits and associated equality types, and
  added module-level leak checks for public constrained extensions.

## 0.35.0 - 2026-07-21

- Added associated-type equality bindings in where trait references, including
  `where T: Add(T, Output = T)` and user traits such as `Produce(Item = i32)`.
- Validated equality names and duplicates at template definition time, substituted equality types
  into assumed method signatures, and checked every concrete impl's associated types during
  monomorphization.
- Enabled generic arithmetic through the source-backed `Add`, `Sub`, `Mul`, `Div`, and `Rem` lang
  items when their `Output` is determined, including intrinsic integer implementations.
- Forwarded associated-type proofs through nested constrained generic calls while retaining full
  rollback of assumed selection state.

## 0.34.0 - 2026-07-21

- Enabled static method dispatch on abstract values through ordinary generic-function trait bounds,
  so a body constrained by `where T: Measure` may call `value.measure()`.
- Added temporary assumed trait implementations during template checking. Their signatures and
  selection entries are fully rolled back, while concrete monomorphizations select the real impl.
- Propagated bounds through nested generic calls: a constrained generic function can satisfy the
  equivalent predicate required by another generic function without prematurely choosing a concrete
  type.
- Kept methods involving associated types disabled until associated-type equality predicates can
  determine their signatures.

## 0.33.0 - 2026-07-21

- Added source-level `where T: Trait` predicates to generic functions, including multiple predicates
  and trailing commas without introducing a new argument delimiter.
- Generic template checking now treats `where T: Copy` as proof that abstract `T` values may be
  copied, while every concrete monomorphization verifies all predicates against collected impls.
- Added module rewriting and public-API visibility checks for predicate subjects, traits, and trait
  arguments; unknown traits, arity mismatches, duplicate predicates, and unsatisfied bounds are
  deterministic errors.
- Kept associated-type equalities, bounded abstract method dispatch, constrained extensions, and
  generic trait implementations reserved for subsequent constraint-system milestones.

## 0.32.0 - 2026-07-21

- Implemented blanket generic inherent extensions with the specified
  `extend(T: type) Cell(T) { ... }` syntax. Header parameters survive module qualification and are
  substituted when each concrete struct or enum instance is materialized.
- Added inferred generic associated-function dispatch, including runtime arguments, expected result
  types, named type arguments, reordered target parameters, and qualified file-module types. Methods
  use the existing concrete receiver ABI and preserve move, mutable-borrow, and recursive cleanup.
- Added `Box.new`, `Box(T).new`, `boxed.as_mut_ptr()`, `boxed.into_inner()`, and `boxed.replace(value)`
  in ordinary alloc source while retaining the free-function compatibility surface.
- Enforced defining-package ownership, determined extension parameters, duplicate-member rejection,
  and first-slice diagnostics for generic members, associated constants, specialization, and generic
  trait implementations that still await `where`-clause selection.

## 0.31.0 - 2026-07-21

- Added safe source-backed `box_into_inner` and `box_replace` operations. The former consumes the
  unique Box and transfers its pointee to the caller; the latter mutably borrows a Box, returns its
  old pointee, and installs a new owner without an intermediate drop.
- Added unsafe `raw_take(MutPtr(T)): T` for move-initialized storage and safe `forget(value)` for
  explicitly abandoning an owner. Both participate in move analysis and cleanup verification;
  use-after-forget and safe-context raw takes are rejected.
- Added native custom-`Drop` coverage proving into-inner destruction exactly once, replacement of
  two resources exactly twice, and intentional forget without destructor execution.

## 0.30.0 - 2026-07-21

- Embedded an edition-pinned ordinary Salicin `alloc` bundle with validated `Box(T)`, `box_new`, and
  `box_ptr` declarations.
- Added move-initializing `raw_init`, recursive Box pointee drop glue, and target-layout-matched
  deallocation. Native tests cover nested and recursive Box layouts, ZSTs, custom Drop, and unique
  ownership relocation.

## 0.29.0 - 2026-07-21

- Added target-aware `size_of(T)` and `align_of(T)` intrinsics lowered through LLVM layout constant
  expressions, including aggregates and concrete generic instances.

## 0.28.0 - 2026-07-21

- Defined replaceable `salicin_alloc` and `salicin_dealloc` ABI symbols with a weak default C
  runtime and strong-symbol override coverage.

## 0.27.0 - 2026-07-21

- Added `Ptr(T)` and `MutPtr(T)`, explicit `unsafe do`, raw load/store, and reserved allocator
  intrinsics as the first audited unsafe boundary.

## 0.26.0 - 2026-07-21

- Gave every closure and partial application a concrete compiler-generated environment type. Its
  identity includes the static call target, remaining call shape, capability, capture modes, and
  capture types; different anonymous callable types therefore do not silently erase into one ABI.
- Allowed owning partial applications and `FnOnce` closures to return across function boundaries.
  LLVM uses named environment structs passed by value, with no allocator, hidden code pointer, or
  dynamic dispatch. Callers can move, invoke, or abandon the returned value.
- Extended cleanup forests, runtime flag trees, and generated drop glue over concrete environment
  fields. Native tests cover Copy captures, resource captures, post-return relocation, successful
  consumption, and abandoned returned environments with observable destruction.
- Continued to reject escaping shared or mutable borrow captures. Callable parameters remain gated
  on generic `Fn`/`FnMut`/`FnOnce` constraints so anonymous concrete types do not need source names.

## 0.25.0 - 2026-07-21

- Allowed named functions, local closures, and partial applications to move into another local
  binding. Callable identity and `Fn`/`FnMut`/`FnOnce` behavior follow the destination binding, while
  every use of the moved source is rejected by ordinary ownership flow.
- Relocated concrete callable environments at LLVM emission. Owning captures move to fresh stable
  storage, old flags are cleared, and the destination environment assumes exact recursive cleanup;
  borrowed closure captures retain their existing lexical loans.
- Made consuming a `FnOnce` closure or partial application explicit in cleanup IR by moving out its
  callable root after argument staging. This covers later-argument early exits without duplicating
  environment ownership.
- Kept cross-function callable escape rejected. Bare function signatures still describe call shape,
  not an implicitly boxed or dynamically erased environment; concrete return/parameter ABI remains
  the next callable boundary.

## 0.24.0 - 2026-07-21

- Made enum match refinement explicit in cleanup IR with `AssumeDiscriminant`. The verifier rejects
  invalid variants, non-enum paths, dead storage, and enum roots that are not fully initialized.
- Lowered pattern ownership into ordinary atomic `Transfer` operations. Unguarded arms commit at arm
  entry; guarded resource bindings commit only on the guard-success edge, leaving the scrutinee whole
  on guard failure and early return.
- Removed `MatchDispatch`, `PatternBindingTransfer`, `MaybeOverwrite`, and the complete
  `PendingCapability` infrastructure. Executable cleanup plans are now self-contained inputs to
  move-state and drop-flag verification rather than carrying parallel promises about later lowering.

## 0.23.0 - 2026-07-21

- Lowered definite overwrite through `mut borrow` parameters for values that need drop. Root and
  projected field assignments now invoke the old referent's exact drop glue before storing the new
  owned value, without inventing an owned flag for borrowed storage.
- Preserved evaluation order: the replacement is evaluated first, so an early return leaves the old
  referent intact; once available, old cleanup precedes the ownership-transferring store. Caller-side
  cleanup subsequently owns only the replacement.
- Removed `BorrowedPlaceMutation` from pending capabilities and added native success plus observable
  root/field trap tests. Conditional initialization remains an owned-storage concern; mutable-borrow
  assignment is verified as a definite overwrite.

## 0.22.0 - 2026-07-21

- Allowed local partial applications to capture owning `move` arguments, including values with
  custom or recursive `Drop`, instead of restricting every captured argument to `Copy`.
- Classified any partial with a move capture as `FnOnce`, rejected repeated or maybe-repeated use,
  and transferred captures through chained currying, final invocation, conditional calls, and
  later-argument early exits using stable environment flags.
- Removed `PartialApplicationCapture`, the final callable-capture pending capability. Native tests
  cover invocation, continued currying, abandonment, conditional use, early return, and observable
  sibling cleanup. Borrowed captures and escaping/first-class callables remain outside this ABI.

## 0.21.0 - 2026-07-21

- Allowed local `FnOnce` closures to own nominal values that need drop. Each move capture now has
  stable environment storage and a recursive runtime drop slot from closure construction until
  abandonment or invocation.
- Transferred capture ownership through the existing early-exit argument staging before calling
  the lifted function. Successful calls clear environment flags; later-argument return paths clean
  staged captures; uncalled and conditionally called closures clean exactly the captures they retain.
- Removed the `LocalClosureCapture` pending capability and added cleanup-plan plus native coverage
  for single, multiple, conditional, abandoned, custom-`Drop`, and early-return capture paths.
  General partial applications remain Copy-only and still retain their separate pending marker.

## 0.20.0 - 2026-07-21

- Enabled drop-bearing pattern bindings in guarded match arms through speculative binding storage.
  Guards may inspect bindings without owning them; ownership flags and active-variant remainder
  slots are committed only on the successful edge into the arm body.
- Preserved the intact enum root on guard failure and on early return from guard evaluation, so a
  later candidate or enclosing cleanup retains exactly one owner. Non-`Copy` bindings remain
  non-movable while their guard is evaluating.
- Added native true, false, early-return, and sibling-trap coverage for guarded payload transfer.
  Whole-value guarded bindings also work for custom-`Drop` enums, while extracting a payload from
  such an enum remains prohibited.

## 0.19.0 - 2026-07-21

- Extended enum match ownership transfer through nested structural payload patterns. Recursive
  remainder lowering now partitions each moved path from still-owned siblings at every struct
  level and assigns cleanup slots only to the surviving subtrees.
- Preserved nested remainder cleanup across normal arm completion and early return, with native
  success and trap tests covering siblings both inside the destructured struct and beside it in the
  active enum variant.
- Rejected nested movement through a type with custom `Drop`, whose destructor requires an intact
  `self`. Guarded resource moves remain pending rollback-aware lowering.

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
