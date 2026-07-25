# Salicin documentation

This directory is the single entry point for project documentation. Documents describe either the
language, the implementation, or the current project state; release history belongs only in the
top-level [changelog](../CHANGELOG.md).

Salicin code fences carry an executable-status label:

- `sc check` is a complete source unit compiled by the CLI regression suite.
- `sc fragment` is normative or explanatory source that is not independently compilable.
- `sc future` belongs to explicitly deferred or exploratory design.
- `sc fail` is intentionally rejected source.

Bare `sc` fences are forbidden. The documentation regression test recursively checks this
classification, rejects unterminated fences, and compiles every `sc check` block.

## Language

- [Language specification](language/specification.md): syntax and semantic rules.
- [Grammar](language/grammar.md): lexer and parser grammar.
- [Control-flow contracts](language/control-flow.md): source identity and lowering obligations.
- [Algebraic-effect contracts](language/algebraic-effects.md): rows, continuations, handlers, and
  lowering obligations.

## Implementation

- [Compiler architecture](compiler/architecture.md): frontend, semantic analysis, LLVM lowering,
  and package layout.
- [Standard library](standard-library/README.md): library layers, module policy, and prelude policy.
- [Core library](standard-library/core.md): compiler-owned, allocation-free declarations.
- [Allocation library](standard-library/alloc.md): owning heap containers.
- [Runtime](runtime.md): native allocator ABI.

## Project

- [M0 core scope](project/core-scope.md): frozen implementation target, maturity labels, exclusions,
  and the gate for expanding the language.
- [M0 conformance matrix](project/m0-conformance.md): positive, negative, diagnostic, and native
  evidence for every frozen M0 capability.
- [Implementation status](project/status.md): supported surface and known structural gaps.
- [Capturing callable bridge](project/callable-bridge-design.md): accepted static-specialization
  design for higher-order protocol calls.
- [Coherent generic trait methods](project/generic-trait-method-design.md): binder equivalence,
  contracts, coherence, and static instantiation.
- [Language roadmap](project/roadmap.md): milestone order, dependencies, exit conditions, and
  explicit deferrals.
- [Project TODO](project/todo.md): prioritized executable tasks with stable IDs and acceptance
  criteria.
- [Changelog](../CHANGELOG.md): release-by-release history.
