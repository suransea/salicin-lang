# Compiler architecture

`salic` is implemented in Rust and emits textual LLVM IR, which `clang` can link with the minimal
runtime for native builds.

```text
.sc source
  -> lexer and parser
  -> module and package resolution
  -> type, ownership, borrow, and cleanup analysis
  -> LLVM IR generation
  -> clang and runtime linkage
```

The top-level phase order is encoded in `codegen/pipeline.rs`. Private `AnalyzedProgram` and
`PreparedProgram` wrappers prevent LLVM emission before semantic analysis, cleanup-plan
verification, and global constant evaluation have succeeded. These wrappers are phase markers,
not a stable compiler API.

The implementation lives under `compiler/src`:

- `lexer.rs`, `parser.rs`, and `ast.rs` define the source frontend.
- `editor.rs` exposes token ranges and phased frontend diagnostics in UTF-8
  bytes and zero-based UTF-16 coordinates. It analyzes either one document or
  a complete source graph without coupling the compiler to an LSP transport.
- `incremental.rs` defines the versioned path-independent source-to-LLVM input
  fingerprint. It does not define or store cache artifacts.
- `manifest.rs`, `lockfile.rs`, and `modules.rs` load package/workspace graphs,
  preserve resolved provider identities, and resolve names.
- `core.rs` and `alloc.rs` load and validate compiler-matched standard-library declarations.
- `cleanup.rs` models resource storage and destruction across control flow.
- `codegen/` owns typed lowering and LLVM emission:
  - `mod.rs` keeps the public compile/check entry points and the current `Analyzer` implementation.
  - `access.rs` owns visibility boundary checks, effective member access, and public API leak
    validation over lowered types.
  - `arrays.rs` lowers fixed-size array literals and static/dynamic array indexing.
  - `assignment.rs` lowers compound assignments through user-defined operator traits or builtin
    integer assignment paths.
  - `calls.rs` lowers call dispatch, internal callable adapters, named overloads, and labeled or
    positional call argument ordering.
  - `chain.rs` owns `?.` and custom `Chain` protocol type probing, access typing, and
    handler-aware lowering.
  - `coalesce.rs` owns `??` and custom `Coalesce` protocol type probing and lowering.
  - `cleanup_plan.rs` adapts HIR into verified cleanup plans before emission.
  - `compile_time.rs` encodes compiler-visible compile-time sort values, source effect identities,
    the compatibility adapter for typed `StaticValue`s, and compile-parameter shape helpers.
  - `control.rs` lowers loops, `break`, and `continue`, including loop backedge flow checks.
  - `constructors.rs` lowers struct literals, struct and enum construction, field argument
    validation, and context-sensitive short enum variant resolution.
  - `ctfe_value.rs` defines the recursive runtime-typed value shared by dependent-expression and
    global-constant evaluation, plus exact checked integer operations, while keeping erased
    metadata in `StaticValue`.
  - `emitter.rs` normalizes globals into typed CTFE values and encodes them as textual LLVM IR.
  - `effects.rs` owns source-level support state, effect identity helpers, call-site effect
    requirements and diagnostics, effect-forwarding `do` lowering, effect operation lowering,
    and handler entry lowering.
  - `fallible.rs` defines standard `Option`/`Result` short-circuit container metadata, inference
    helpers, and throws-result return-boundary lowering shared by `??`, `?.`, `try`, and `throw`.
  - `flow.rs` tracks local scopes, move initialization alternatives, lexical loans, and lowering
    context state used by ownership and borrow checks.
  - `functions.rs` lowers function and global bodies, materializes generic function instances,
    resolves function/global value types, and validates binary entry-point shape.
  - `handlers.rs` owns algebraic-handler state, CPS source transformation, and handler-specific
    AST rewrite helpers.
  - `hir.rs` defines typed IR structs, semantic types, places, signatures, and helper predicates.
  - `inference.rs` owns generic function-instance inference, type-argument seeding, unification,
    template resolution, and expression-constraint inference helpers.
  - `layouts.rs` builds struct/enum field layouts, validates recursive value layout cycles, and
    reports missing nominal layout diagnostics.
  - `lower.rs` defines shared expression-lowering data, type-probe helpers, and HIR construction
    helpers used by multiple lowering paths.
  - `matches.rs` lowers scalar and enum `match` expressions and owns pattern binding validation.
  - `members.rs` lowers value and type member access, including associated constants, unit enum
    variants, and field diagnostics.
  - `names.rs` centralizes stable symbol, monomorphization instance, trait-method, canonical type,
    and source-level diagnostic function-name encodings.
  - `nominals.rs` owns generic nominal snapshots, struct/enum constructor inference, type-head and
    instance resolution, recursive validation, materialization, and nominal complexity guards.
  - `operators.rs` centralizes operator-syntax bindings, candidate selection, type probes, and HIR
    lowering for validated lang-item protocols.
  - `ownership.rs` centralizes Copy/drop type predicates, custom Drop crossing checks, and inferred
    pass-mode selection used by ownership-sensitive lowering.
  - `source_rewrite.rs` normalizes validated control-call groups, including static expansion of
    `if` and heterogeneous partial-function `match` cases, before semantic control-flow passes.
  - `places.rs` lowers local place expressions and owns move initialization plus lexical loan
    bookkeeping over HIR places.
  - `pipeline.rs` sequences semantic analysis, cleanup preparation, constant evaluation, and LLVM
    emission behind explicit phase-marker types.
  - `raw.rs` lowers layout queries, raw pointer constructors, raw allocation primitives, raw
    borrow/take/offset/trap operations, and `forget`.
  - `references.rs` lowers contextual reference values and reference call arguments, promotes
    returned-reference loans, and validates explicit reference-return escape sources and regions.
  - `registry.rs` defines item, trait, overload, and generic-instance registry keys, schemas,
    candidate lookup, and generic implementation pattern matching helpers.
  - `source_rewrite.rs` owns source-level rewrites before semantic lowering, including labeled
    type-argument normalization, type-alias expansion, region-parameter erasure, and generic
    type substitution, plus AST hygiene helpers used by handler and static-function specialization.
  - `static_eval.rs` evaluates scalar, tuple, fixed-array, and concrete struct values through
    ordinary pure source functions before dependent types are lowered to runtime layouts,
    including recursive resource exclusion, aggregate limits, projection, indexing, and nested
    tuple or struct patterns.
  - `target.rs` defines the explicit native target width used by CTFE, literal validation,
    runtime guards, and LLVM scalar lowering instead of inheriting Rust host integer widths.
  - `throws.rs` probes custom-effect call rows to identify dedicated and standard throws sources,
    infers context-free `try { ... }` `Result(E)(T)` types, and lowers `try { ... }`, `throw`, and
    automatic throws propagation return-boundary wrappers.
  - `types.rs` lowers and renders source-level type syntax, enforces type compatibility and
    unification, recognizes uninhabited types, and owns compile-time type arguments plus
    source/nominal type probes used by inference and expression lowering.
  - `tests.rs` contains the large codegen regression suite.
- `main.rs` implements the `salic` command-line interface.

`static_semantics.rs` defines phase-independent `StaticValue`, `Constraint`, projection-equation,
and `Goal` IR. The current monomorphizer still has a compatibility encoding for some static values
inside source `Type` nodes; all new static evaluation and trait-goal work crosses that encoding
through explicit adapters rather than adding new marker conventions.

The current `Analyzer` is still intentionally oversized. Its next split should preserve the same
pipeline boundaries rather than carve by syntax shape:

```text
resolved AST
  -> source rewrites and alias expansion
  -> item collection and lang-item validation
  -> generic/trait instance registry
  -> expression typing and HIR construction
  -> ownership/borrow flow analysis
  -> algebraic-effect and control lowering
  -> cleanup-plan construction
  -> LLVM emission
```

The remaining splits should move method bodies out of `Analyzer` along the same boundaries,
especially expression and statement lowering that now depends on `lower.rs` helpers. The practical
rule is: first move code behind a small `pub(super)` boundary with no behavior changes, then make
data ownership cleaner. Large semantic rewrites should come after the module shape is visible.

The compiler embeds edition-matched sources from `library/core`, `library/alloc`, and the C allocator
from `runtime`. Embedded Salicin declarations still pass through the normal parser and semantic
pipeline. Compiler-provided core definitions carry complete `= builtin()`
initializers. Bootstrap validation checks the unique private marker, exact
known declarations, and the abstract boundary for trait requirements and
effect operations. The analyzer rejects marker use outside `core` and
consumes every non-bootstrap marker through a validated intrinsic path before
LLVM emission.

The crate currently keeps the compiler in one Rust package while giving it a repository-level
`compiler/` boundary. If independent compiler crates become useful, they can be introduced below
that boundary without moving language or library documentation again.
