# Salicin documentation

This directory is the entry point for current Salicin documentation. Each subject has one source
of truth:

- the specification defines language meaning;
- the grammar defines accepted source form;
- implementation documents explain how the compiler and libraries realize those contracts;
- status, roadmap, and TODO record facts, direction, and unfinished work respectively;
- release history belongs only in the top-level [changelog](../CHANGELOG.md).

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

- [Implementation status](project/status.md): supported behavior and known boundaries.
- [Language roadmap](project/roadmap.md): milestone order, exit conditions, and deferrals.
- [Project TODO](project/todo.md): unfinished tasks and their acceptance criteria.
- [Experimental ABI review](project/abi-review.md): current runtime representations and boundary
  gaps.
- [Native calling convention](project/native-calling-convention.md): parameter flattening,
  ownership transfer, cleanup, effects, and returns.
- [Native linkage](project/native-linkage.md): package-qualified exports, ABI fingerprints,
  generic ownership, and cross-module agreement.
- [C interoperability](project/c-interoperability.md): verified scalar calls, raw pointers,
  `struct(c)` layout, and rejected ABI categories.
- [Source formatter](project/formatter.md): token-preserving layout invariants, CLI behavior,
  idempotence, and deliberate limits.
- [Initial async contract](project/async-contract.md): accepted cold-future, polling, cancellation,
  borrowing, and executor boundary for the active async milestone.
- [Changelog](../CHANGELOG.md): release-by-release history.
