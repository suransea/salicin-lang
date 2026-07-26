# Language Roadmap

Status: living project direction

This roadmap records sequencing and exit conditions. It does not define language behavior; the
[language specification](../language/specification.md) and [grammar](../language/grammar.md) do
that. Current implementation facts belong in [status](status.md), actionable work in
[TODO](todo.md), and completed work in the [changelog](../../CHANGELOG.md).

## Direction

Salicin is moving toward a coherent native language built around deterministic ownership, explicit
effects, static abstraction, and source-backed library contracts. Work should complete one
end-to-end capability at a time and preserve:

- deterministic left-to-right evaluation and exactly-once cleanup;
- source-level diagnostics without generated names;
- static dispatch and bounded monomorphization;
- explicit authority for unsafe operations and effects;
- ordinary library declarations wherever compiler primitives are unnecessary.

## Current Milestone: Formatter Syntax Invariants

Async state machines now cover cold construction, explicit polling,
cancellation, finite sequential and branch suspension, recurring loop
suspension, and residual handler specialization for supported state shapes.
Unsupported recursive, self-referential, move-only backedge, effectful
condition, and nested residual iteration shapes receive source diagnostics.

The runtime representation audit, native calling convention, exported symbol
contract, and bounded C interoperability are complete. The current milestone
defines formatter-preserving syntax invariants before an LSP, workspace,
registry identity, or incremental compilation work begins.

## Test Throughput Foundation

The compiler provides contextual `test("name") { ... }` registrations and a
`salic test` command that collects the selected package into one native
runner. This removes per-case native linking from language-level regression
suites. Compatible repository execution fixtures are isolated as modules and
declare their own registrations for collection into one runner per semantic
group; the Rust harness does not synthesize tests. Fixtures that must terminate
the process remain independent. Test registration is intentionally narrower
than general compile-time execution.

## Confirmed ABI Direction

The completed representation and ABI milestone established three orthogonal source forms:

- C data representation belongs to the type constructor, written `struct(c) { ... }`; Salicin will
  not add a general `rep` modifier;
- each foreign-owned declaration uses a complete `foreign(c)` or
  `foreign(c, "external_symbol")` initializer, with an omitted symbol defaulting to the Salicin
  declaration name and calls requiring `Unsafe`;
- each compiler-owned core declaration uses a complete core-private `builtin()` initializer,
  including compiler-defined types and type constructors.

The bootstrap declaration is `let builtin(): Never = builtin()`. Semantic analysis treats its use
as a declaration-definition marker typed by the declaration annotation, not as ordinary runtime
`Never` coercion. Every marker except that bootstrap must be resolved before code generation.
Trait requirements remain bodyless; user opaque types are a separate design problem.

This direction replaces `rep c`, `@link_name`, and grouped `extern "C"` declarations. It does not
introduce `@` syntax, and `foreign` is not a variant of `builtin`.

## Completed Foundation: C Forms And Compiler Definitions

This milestone followed the direct-tail suspended residual async slice and
completed before the remaining async shapes, formatter, LSP, package, or
incremental-compilation work.

Exit conditions:

- C-compatible data uses only `struct(c)` and has verified layout diagnostics;
- every foreign declaration uses `foreign(c, ...)`, defaults its symbol
  predictably, and requires `Unsafe` at call sites;
- every compiler-owned core declaration uses the private complete
  `builtin()` initializer, while trait requirements remain bodyless;
- legacy `rep c`, `@link_name`, and grouped `extern "C"` forms have migration
  diagnostics and no accepted grammar path;
- source, contract, cross-module, LLVM, and native tests cover the three
  boundaries independently.

## Completed ABI Representation Review

The existing `struct(c)`, `foreign(c, ...)`, and core-private `builtin()`
source forms remain the foundation; they are not reopened as competing syntax.
The [ABI review](abi-review.md) records the implemented 64-bit host-target
representation for scalars, pointers, aggregates, enums, callables,
continuations, effect callables, and ownership modes. Low-level tests pin the
mapping, unsupported first-class values fail before LLVM emission, and
non-64-bit compiler hosts are rejected explicitly.

The review identified native call ownership, exported linkage, and C
cross-language verification as separate follow-up boundaries. The first two
are now complete; C verification remains active.

## Completed Native Calling Convention

The [native calling convention](native-calling-convention.md) defines runtime
group flattening, Unit and borrowed-Unit erasure, value and borrow passing,
owned cleanup transfer, direct returns, `Throws` result boundaries, algebraic
effect specialization, erased continuation records, and tail-call cleanup.
Caller and callee now share one parameter-erasure rule; source validation
rejects unsized parameters and returns before LLVM emission.

## Completed Native Linkage

The [native linkage contract](native-linkage.md) gives concrete primary
package `pub` functions and constants package-qualified symbols with ABI
fingerprints. Stable package identities replace graph-local numeric IDs in
nominal contracts, incompatible declarations cannot silently bind, generic
specializations belong to the consumer, and compiler-generated definitions
remain internal. Independent LLVM modules link and execute through this
contract. Precompiled package interfaces remain later distribution work.

## Completed C Interoperability

The [C interoperability contract](c-interoperability.md) defines the native
64-bit host-Clang boundary. Cross-language tests cover every integer width,
raw pointers, and bidirectional access to nested `struct(c)` storage
containing integer, pointer, array, and nested aggregate fields. By-value
arrays and aggregates, bool, borrows, and typed function pointers are rejected
until target ABI lowering explicitly supports them.

## Later Ecosystem Milestone

Tooling and package compatibility follow verified C interoperability:

- a formatter based on parser-preserving syntax invariants;
- an LSP over stable parser and semantic spans;
- workspaces and reproducible dependency resolution;
- incremental compilation keyed by stable semantic inputs.

No milestone may freeze a public ABI, package registry protocol, or compatibility promise while the
corresponding language representation is still changing.

## Deferred

The following require separate accepted designs and are not active work:

- multi-shot continuations;
- implicit ambient IO or allocation authority;
- garbage collection as a second ownership model;
- runtime trait objects and open-world dispatch;
- macros, reflection, and general compile-time execution;
- a public package registry;
- a stable 1.0 compatibility promise.

## Change Gate

A language change enters the roadmap only when it:

1. solves a concrete language or standard-library requirement;
2. states its interaction with ownership, effects, evaluation order, and cleanup;
3. defines rejection boundaries and source-level diagnostics;
4. has positive, negative, cross-module when relevant, and native evidence;
5. leaves the full formatting, lint, test, and documentation gates clean.
