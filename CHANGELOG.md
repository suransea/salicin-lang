# Changelog

Salicin follows semantic versioning while the compiler is experimental. During 0.x, minor releases
may extend or tighten source semantics; patch releases preserve semantics within the implemented
subset.

## Unreleased

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
