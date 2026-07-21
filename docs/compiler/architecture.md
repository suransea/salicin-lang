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
- `codegen.rs` performs semantic analysis and LLVM lowering.
- `main.rs` implements the `salic` command-line interface.

The compiler embeds edition-matched sources from `library/core`, `library/alloc`, and the C allocator
from `runtime`. Embedded Salicin declarations still pass through the normal parser and semantic
pipeline; bootstrap validation additionally checks the exact declarations needed by the compiler.

The crate currently keeps the compiler in one Rust package while giving it a repository-level
`compiler/` boundary. If independent compiler crates become useful, they can be introduced below
that boundary without moving language or library documentation again.
