# Salicin documentation

This directory is the single entry point for project documentation. Documents describe either the
language, the implementation, or the current project state; release history belongs only in the
top-level [changelog](../CHANGELOG.md).

## Language

- [Language specification](language/specification.md): syntax and semantic rules.
- [Grammar](language/grammar.md): lexer and parser grammar.

## Implementation

- [Compiler architecture](compiler/architecture.md): frontend, semantic analysis, LLVM lowering,
  and package layout.
- [Standard library](standard-library/README.md): library layers, module policy, and prelude policy.
- [Core library](standard-library/core.md): compiler-owned, allocation-free declarations.
- [Allocation library](standard-library/alloc.md): owning heap containers.
- [Runtime](runtime.md): native allocator ABI.

## Project

- [Implementation status](project/status.md): supported surface and known structural gaps.
- [Changelog](../CHANGELOG.md): release-by-release history.
