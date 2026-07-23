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

The implementation lives under `compiler/src`:

- `lexer.rs`, `parser.rs`, and `ast.rs` define the source frontend.
- `manifest.rs`, `lockfile.rs`, and `modules.rs` load project graphs and resolve names.
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
  - `compile_time.rs` encodes compiler-visible compile-time domain values, source effect
    identities, and compile-parameter shape helpers.
  - `control.rs` lowers loops, `break`, and `continue`, including loop backedge flow checks.
  - `emitter.rs` evaluates global constants and emits textual LLVM IR.
  - `effects.rs` owns source-level support state, effect identity helpers, effect-forwarding
    `do` lowering, effect operation lowering, and handler entry lowering.
  - `fallible.rs` defines standard `Option`/`Result` short-circuit container metadata, inference
    helpers, and throws-result return-boundary lowering shared by `??`, `?.`, `try`, and `throw`.
  - `flow.rs` tracks local scopes, move initialization alternatives, lexical loans, and lowering
    context state used by ownership and borrow checks.
  - `functions.rs` lowers function and global bodies, materializes generic function instances,
    resolves function/global value types, and validates binary entry-point shape.
  - `handlers.rs` owns algebraic-handler state, CPS source transformation, and handler-specific
    AST rewrite helpers.
  - `hir.rs` defines typed IR structs, semantic types, places, signatures, and helper predicates.
  - `inference.rs` owns generic type-argument seed, unification, template resolution, and
    expression-constraint inference helpers.
  - `layouts.rs` builds struct/enum field layouts, validates recursive value layout cycles, and
    reports missing nominal layout diagnostics.
  - `lower.rs` defines shared expression-lowering data, type-probe helpers, and HIR construction
    helpers used by multiple lowering paths.
  - `matches.rs` lowers scalar and enum `match` expressions and owns pattern binding validation.
  - `members.rs` lowers value and type member access, including associated constants, unit enum
    variants, and field diagnostics.
  - `names.rs` centralizes stable symbol, monomorphization instance, trait-method, and canonical
    type encodings.
  - `nominals.rs` owns generic nominal snapshots, recursive generic nominal validation, nominal
    instance materialization, and generic nominal type-complexity guards.
  - `operators.rs` centralizes operator-syntax bindings, candidate selection, type probes, and HIR
    lowering for validated lang-item protocols.
  - `ownership.rs` centralizes Copy/drop type predicates, custom Drop crossing checks, and inferred
    pass-mode selection used by ownership-sensitive lowering.
  - `places.rs` lowers local place expressions and owns move initialization plus lexical loan
    bookkeeping over HIR places.
  - `raw.rs` lowers layout queries, raw pointer constructors, raw allocation primitives, raw
    borrow/take/offset/trap operations, and `forget`.
  - `references.rs` lowers contextual reference values and validates explicit reference-return
    escape sources, regions, and loans.
  - `registry.rs` defines item, trait, overload, and generic-instance registry keys, schemas,
    candidate lookup, and generic implementation pattern matching helpers.
  - `source_rewrite.rs` owns source-level rewrites before semantic lowering, including labeled
    type-argument normalization, type-alias expansion, region-parameter erasure, and generic
    type substitution, plus AST hygiene helpers used by handler and static-function specialization.
  - `throws.rs` probes custom-effect call rows to identify dedicated and standard throws sources,
    infers context-free `try { ... }` `Result(E)(T)` types, and lowers `try { ... }`, `throw`, and
    automatic throws propagation return-boundary wrappers.
  - `types.rs` lowers and renders source-level type syntax, compile-time type arguments, and
    non-diagnostic source/nominal type probes used by inference and expression lowering.
  - `tests.rs` contains the large codegen regression suite.
- `main.rs` implements the `salic` command-line interface.

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
pipeline; bootstrap validation additionally checks the exact declarations needed by the compiler.

The crate currently keeps the compiler in one Rust package while giving it a repository-level
`compiler/` boundary. If independent compiler crates become useful, they can be introduced below
that boundary without moving language or library documentation again.
