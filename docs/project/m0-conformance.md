# M0 conformance matrix

Status: audited evidence baseline

This matrix maps every capability in the frozen [M0 core scope](core-scope.md) to current test
evidence. It records repository state after `7b9d277`; it is not a substitute for the executable
tests. A row is complete only when positive acceptance, negative rejection, stable source-level
diagnostics, and relevant native execution are all demonstrated.

Evidence labels:

- **Covered**: direct automated evidence exists.
- **Partial**: implementation or evidence covers only part of the frozen capability.
- **Missing**: the frozen capability is not implemented or has no direct evidence.
- **N/A**: native execution is not meaningful for the rejection-only part of a row.

Fixture names below are relative to `tests/fixtures`; Rust test names are in `tests/cli.rs` unless a
source file is named.

| M0 capability | Positive evidence | Negative and diagnostic evidence | Native evidence | Status / owner |
|---|---|---|---|---|
| Unicode source and identifiers | Lexer accepts alphabetic Unicode identifiers | No dedicated confusable, malformed, or diagnostic fixture | No Unicode native fixture | **Partial**, `M0-FRONTEND-EVIDENCE-1` |
| Logical newlines, declarations, and lexical scopes | Lexer/parser newline tests; `pass/block_mutation.sc`; `local_bindings_shadow_imports_without_hiding_them_from_outer_scopes` | Parser separator tests; scope and import fail fixtures | `run_supports_grouped_calls_and_unit_main`, `pass/block_mutation.sc` | **Covered** |
| Modules, packages, local dependencies, and explicit visibility | File-module, facade-use, package-target, and dependency CLI tests | Module path, import, package boundary, manifest, and dependency diagnostics | `local_path_dependency_runs_only_its_library_and_writes_a_stable_lockfile`, module/package native tests | **Covered** |
| Immutable/mutable bindings and implemented primitive scalars | `pass/block_mutation.sc`; scalar operator fixture families | type mismatch, invalid operator, division/remainder and shift diagnostics | scalar/operator CLI test families | **Partial**: runtime lowering currently covers `i32`, `i64`, `u32`, `u64`, and `bool`; `M0-SCALAR-1` owns the declared remaining widths |
| Tuples and unit | Unit parameters/results are covered throughout parser, ABI, and CLI tests | Unit misuse receives ordinary type diagnostics | `run_supports_grouped_calls_and_unit_main` | **Missing** for non-unit tuple types, literals, fields, and patterns; `M0-TUPLE-1` |
| Fixed arrays | `pass/array_*`; array type/length inference tests | `fail/array_*` with bounds, ownership, type, and alias diagnostics | `m1_loops_and_arrays_run_with_expected_result`, dynamic bounds trap and resource-drop tests | **Covered** |
| Nominal structs and enums | `pass/struct_*`, `pass/enum_match.sc`, generic nominal fixtures | struct, enum, layout, field, constructor, and recursive-layout fail fixtures | `m1_struct_programs_run_with_expected_result`, `m1_match_and_partial_programs_run_with_expected_result` | **Covered** |
| Exhaustive patterns and structured control flow | match/guard/if-let/while/loop/for fixture families | non-exhaustive, unreachable/invalid pattern, branch move, loop backedge, and scope fail fixtures | match, loop, array, iterator, cleanup, and ledger CLI tests | **Covered** |
| Named functions, grouped complete/partial application, and noncapturing function values | grouped call, partial application, overload, and function-value fixtures | arity, label, partial borrow, and ownership diagnostics | `run_supports_grouped_calls_and_unit_main`, function-value and partial-application native tests | **Covered** |
| Closures required by ordinary control APIs | closure capture, `Fn`/`FnMut`/`FnOnce`, and multistage partial fixtures | closure mutation, repeated consume, escaping borrow, and use-after-move fail fixtures | local closure and closure cleanup CLI tests | **Covered** |
| Static generic functions and nominal types over `type` | `m2_generic_*` suites; generic inference and module tests | generic arity, inference, invalid body, layout, and constraint diagnostics | generic function/nominal CLI suites | **Covered** for the frozen first-order slice |
| First-order traits, associated types, coherent static dispatch, and operators | concrete trait, associated output, where-bound, and operator protocol suites | coherence, missing member/type, ambiguity, mismatch, and orphan diagnostics | trait and operator CLI suites | **Covered** |
| Deterministic left-to-right evaluation | single-evaluation match/index tests and staged call-argument codegen tests | Ownership/borrow failures prevent invalid reordered access | `pass/match_scalar_single_evaluation.sc`, indexed handler ordering fixture | **Covered**; deterministic output across builds is separately `M0-DETERMINISM-1` |
| Explicit `copy`, `move`, `borrow`, and `borrow(mut)` passing | passing modifier, borrow value/type, mutable borrow, and generic forwarding suites | `fail/passing_*`, `fail/borrow_*`, and use-after-move fixture families | ownership, borrow overwrite, function call, and ledger native tests | **Covered** |
| Lexical borrow and move checking | borrow release, returned-borrow, projection, reinitialization, and move fixtures | borrow conflict, escape, overlap, use-after-move, and loop/branch flow diagnostics | ownership, mutable borrow, array, closure, and handler native suites | **Covered** |
| Deterministic cleanup | cleanup planner/verifier unit suite and `pass/drop_*` fixtures | planner invariant tests plus partial-move and invalid cleanup source diagnostics | structured exit, partial aggregate, match payload, closure, array, and ledger drop tests | **Covered** |
| `Option`, `Result`, `Throws(Error)`, `try`, and `throw` | standard container, throws, coalesce, chain, and result fixture suites | return/error mismatch, invalid try/throw context, and ambiguity diagnostics | result/throws/coalesce/chain native CLI tests | **Covered** |
| `Unsafe`, `unsafe`, and raw primitives behind authority | raw pointer, allocator, layout-query, and access-family fixtures | `fail/raw_*` validates authority, mutability, layout, type, and ownership boundaries | raw pointer, allocator, replacement ABI, invalid layout, and trap CLI tests | **Covered** |
| C FFI | No parser, AST, semantic, ABI, or emitter implementation | No rejection contract or diagnostic fixtures | None | **Missing**, `M0-FFI-1`; stable external ABI remains outside M0 |
| Binary/library targets and native LLVM emission | check/emit/build/run, explicit targets, default targets, and library dependency tests | output collision, target selection, manifest, dependency, and cycle diagnostics | shorthand native build, package binaries, dependencies, and ledger | **Covered** |
| Diagnostics suitable for source-level debugging | Stable source-name fragments, defining declaration positions, local initializer positions, and end-exclusive expression-root ranges, including trailing source-closure calls, are asserted across parser, CLI, and codegen tests | Broad fail fixture corpus checks semantic rejection and rejects generated `$...` names | N/A | **Covered** |

## Audit tasks

The matrix creates these release-blocking tasks without changing the frozen scope:

1. `M0-FRONTEND-EVIDENCE-1`: add Unicode identifier/confusable and logical-newline end-to-end
   fixtures.
2. `M0-TUPLE-1`: implement and test non-unit tuple types, values, projection, patterns, ownership,
   and cleanup.
3. `M0-SCALAR-1`: either lower every declared M0 primitive scalar width and target-sized integer,
   or narrow declarations and the frozen scope through the formal change gate.
4. `M0-FFI-1`: implement the bounded C FFI slice promised by M0, including ABI admissibility,
   unsafe calls, linking, diagnostics, and native round trips.
`M0-QUALITY-1` cannot close merely because the current suite is green; the implementation gaps
above must either be completed or pass the M0 change gate.
