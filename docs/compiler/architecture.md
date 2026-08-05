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

- `lexer.rs`, `parser.rs`, and `ast.rs` define the source frontend. `parser/post_parse.rs`
  performs extend-parameter inference plus compile-parameter scope normalization and validation
  after syntax parsing; `parser/tests.rs` keeps parser regressions out of the implementation file.
- `editor.rs` exposes token ranges, phased frontend diagnostics, and a
  source-backed semantic occurrence index in UTF-8 bytes and zero-based UTF-16
  coordinates. Dense symbol IDs are scoped to one immutable snapshot;
  ambiguous references retain ordered candidate sets and generated
  specializations never enter the index. Typed definition, reference, and
  hover queries select only unique identities and carry dependency editability;
  resolved package graphs remain intact in snapshots. Diagnostics preserve document
  identity, phase, severity, stable code, and optional exact range.
  `modules.rs` produces structured resolver diagnostics and renders strings
  only for legacy CLI/compiler entry points; the editor never parses rendered
  messages or manufactures fallback locations. It analyzes either one
  document or a complete source graph without coupling the compiler to an LSP
  transport. `WorkspaceSession` layers versioned full-text open buffers over
  caller-supplied baseline sources and produces immutable, thread-safe
  snapshots; completed analyses pass one session/revision gate before
  publication, so superseded results are dropped without any source-file I/O.
- `lsp.rs` owns bounded `Content-Length` JSON-RPC framing, LSP lifecycle
  enforcement, UTF-8 file-URI conversion, and full-document synchronization
  into `WorkspaceSession`. The CLI owns package and target discovery before
  constructing the transport; the transport neither discovers nor writes
  workspace files. A reader thread, coalescing analysis worker, and
  publication loop communicate through bounded responsibilities: input stays
  responsive during checking, every immutable snapshot crosses the
  session/revision acceptance gate, cancelled or superseded requests complete
  exactly once, and only current results publish diagnostics or answer full
  UTF-16 semantic-token, definition, reference, and hover requests.
- `incremental.rs` defines the versioned path-independent source-to-LLVM input
  fingerprint and artifact-schema key mapping. `incremental_cache.rs` resolves
  the private user-cache root and implements strict local lookup, validation,
  corruption replacement, and atomic concurrent publication. Command
  pipelines reuse validated entries for `emit-ir`, `build`, `run`, and
  `test`; `check` remains uncached. The driver exposes explicit bypass and
  stderr tracing, while cache cleanup delegates its ownership checks and
  atomic namespace detachment to the storage module. Unit and multi-process
  CLI tests maintain the complete identity, corruption, failure, relocation,
  and concurrency acceptance matrix.
- `manifest.rs`, `lockfile.rs`, and `modules.rs` load package/workspace graphs,
  preserve resolved provider identities, and resolve names.
- `core.rs`, `alloc.rs`, and `standard.rs` load edition-matched library
  sources. `standard.rs` admits ordinary unprivileged `std` definitions,
  rejects mirror aliases so each declaration retains one canonical module
  identity, and enforces the native target boundary.
- `cleanup.rs` models resource storage and destruction across control flow.
- `codegen/` owns typed lowering and LLVM emission:
  - `mod.rs` keeps the public compile/check entry points and the current `Analyzer` implementation.
  - `analyzer_state.rs` separates collected source/registry state from mutable lowering artifacts;
    the `Analyzer` itself retains only session identity, those two phase states, and diagnostics.
  - `access.rs` owns visibility boundary checks, effective member access, and public API leak
    validation over lowered types.
  - `arrays.rs` lowers fixed-size literal backing storage, dispatches array and
    string literal construction through the source-backed core traits, performs
    array-to-slice unsizing, and lowers static/dynamic array indexing.
  - `assignment.rs` lowers compound assignments through user-defined operator traits or builtin
    integer assignment paths.
  - `async_source.rs` recognizes and rewrites source-level async control flow, recurring loops,
    retained bindings, and heterogeneous branch factories before future-state materialization.
  - `calls.rs` lowers call dispatch, owns callable-bridge specialization data and rewrites,
    internal callable adapters, named overloads, and labeled or positional call argument ordering.
  - `call_lowering.rs` owns resolved call lowering after dispatch, including bound methods,
    indirect calls, callable and handler specialization, partial application, closure arguments,
    and argument-temporary staging.
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
  - `diagnostic.rs` defines the public codegen diagnostic value while keeping construction
    internal to the codegen pipeline.
  - `emitter.rs` totally encodes already normalized typed CTFE globals as textual LLVM IR; it does
    not evaluate source expressions.
  - `expression_lowering.rs` lowers general expressions and local closures after type probing,
    including capture discovery, flow joins, and recursive-frame bookkeeping.
  - `extension_collection.rs` collects concrete and generic trait or inherent extensions,
    validates overlaps, and materializes pointer, slice, and array extension instances.
  - `effects.rs` owns source-level support state, effect identity helpers, call-site effect
    requirements and diagnostics, effect-forwarding `do` lowering, effect operation lowering,
    and handler entry lowering.
  - `fallible.rs` defines standard `option`/`result` short-circuit container metadata, inference
    helpers, and failure-result return-boundary lowering shared by `??`, `?.`, `try`, and `throw`.
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
  - `places.rs` lowers local place expressions and owns move initialization plus lexical loan
    bookkeeping over HIR places.
  - `pipeline.rs` sequences semantic analysis, cleanup preparation, constant evaluation, and LLVM
    emission behind explicit phase-marker types.
  - `probe.rs` performs non-mutating expression, place, call, and nominal-constructor type probes
    used to seed inference before full HIR lowering.
  - `raw.rs` lowers layout queries, raw pointer constructors, raw allocation primitives, raw
    borrow/take/offset/trap operations, and `forget`.
  - `references.rs` lowers contextual reference values and reference call arguments, promotes
    returned-reference loans, and validates explicit reference-return escape sources and regions.
  - `registry.rs` defines item, trait, overload, and generic-instance registry keys, schemas,
    candidate lookup, and generic implementation pattern matching helpers.
  - `source_rewrite.rs` owns source-level rewrites before semantic lowering, including validated
    control-call normalization, static `if` and heterogeneous partial-function `match` expansion,
    labeled type-argument normalization, type-alias expansion, region-parameter erasure, generic
    type substitution, and AST hygiene helpers used by handler and static-function specialization.
  - `static_eval.rs` evaluates scalar, tuple, fixed-array, concrete struct, and closed enum values
    through ordinary pure source functions before dependent types are lowered to runtime layouts,
    including on-demand nominal materialization, recursive resource exclusion, aggregate limits,
    projection, indexing, guards, nested tuple, struct, or variant patterns, grouped and generic
    calls, statically resolved members, returns, cross-module identity, and deterministic
    recursion budgets.
  - `target.rs` defines the explicit native target width used by CTFE, literal validation,
    runtime guards, and LLVM scalar lowering instead of inheriting Rust host integer widths.
  - `trait_collection.rs` collects top-level items and trait schemas, validates source trait
    contracts and Copy implementations, and normalizes trait implementation targets.
  - `failure.rs` probes custom-effect call rows to identify dedicated and standard failure sources,
    infers context-free `try { ... }` `result(e)(t)` types, and lowers `try { ... }`, `throw`, and
    automatic failure propagation return-boundary wrappers.
  - `types.rs` lowers and renders source-level type syntax, enforces type compatibility and
    unification, recognizes uninhabited types, and owns compile-time type arguments plus
    source/nominal type probes used by inference and expression lowering.
  - `tests.rs` contains the large codegen regression suite.
- `main.rs` implements the `salic` command-line interface.

`static_semantics.rs` defines phase-independent `StaticValue`, `Constraint`, projection-equation,
and `Goal` IR. The current monomorphizer still has a compatibility encoding for some static values
inside source `Type` nodes; all new static evaluation and trait-goal work crosses that encoding
through explicit adapters rather than adding new marker conventions.

`Analyzer` is a phase coordinator rather than a monolithic implementation. Its methods are split
along the following pipeline boundaries, and its data is divided between `CollectionState` and
`LoweringState`:

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

New work should extend the module that owns its phase instead of adding methods back to `mod.rs`.
Cross-phase state must be added deliberately to one of the two state structs; transient expression,
flow, and cleanup state belongs in their existing local contexts. Runtime future construction stays
in `async_lowering.rs` while source control-flow planning lives in `async_source.rs`.
`handlers.rs` remains a larger cohesive transformation because splitting its mutually recursive
continuation and handler state machine would create more coupling than it removes.

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
