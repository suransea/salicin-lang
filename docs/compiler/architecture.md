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
  - `cleanup_plan.rs` adapts HIR into verified cleanup plans before emission.
  - `compile_time.rs` encodes compiler-visible compile-time domain values, source effect
    identities, and compile-parameter shape helpers.
  - `emitter.rs` evaluates global constants and emits textual LLVM IR.
  - `hir.rs` defines typed IR structs, semantic types, places, signatures, and helper predicates.
  - `names.rs` centralizes stable symbol, monomorphization instance, trait-method, and canonical
    type encodings.
  - `operators.rs` centralizes operator-syntax bindings to validated lang-item protocols.
  - `source_rewrite.rs` owns source-level rewrites before semantic lowering, including labeled
    type-argument normalization, type-alias expansion, region-parameter erasure, and generic
    type substitution, plus AST hygiene helpers used by handler and static-function specialization.
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

Useful future `codegen/` modules are:

- `registry.rs` for item collection, generic nominal/function instantiation, and trait impl maps;
- `lower.rs` for expression and statement lowering into HIR;
- `borrow.rs` for move/borrow flow checks that currently live inside `Analyzer`;
- `effects.rs` for algebraic-effect operation/handler CPS rewriting and residual-row logic.

The practical rule is: first move code behind a small `pub(super)` boundary with no behavior
changes, then make data ownership cleaner. Large semantic rewrites should come after the module
shape is visible.

The compiler embeds edition-matched sources from `library/core`, `library/alloc`, and the C allocator
from `runtime`. Embedded Salicin declarations still pass through the normal parser and semantic
pipeline; bootstrap validation additionally checks the exact declarations needed by the compiler.

The crate currently keeps the compiler in one Rust package while giving it a repository-level
`compiler/` boundary. If independent compiler crates become useful, they can be introduced below
that boundary without moving language or library documentation again.
