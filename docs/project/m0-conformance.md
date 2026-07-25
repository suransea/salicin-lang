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
| Unicode source and identifiers | Lexer enforces Unicode XID and NFC normalization; dedicated unit and source fixtures cover composed/decomposed and non-ASCII identifiers | Non-XID zero-width source spelling and cross-script lookalike file-module diagnostics | `pass/unicode_identifiers.sc` | **Covered** |
| Logical newlines, declarations, and lexical scopes | Lexer/parser newline tests; `pass/logical_newlines.sc`; `pass/block_mutation.sc`; `local_bindings_shadow_imports_without_hiding_them_from_outer_scopes` | Parser separator tests; scope and import fail fixtures | `pass/logical_newlines.sc`, `run_supports_grouped_calls_and_unit_main`, `pass/block_mutation.sc` | **Covered** |
| Modules, packages, local dependencies, and explicit visibility | File-module, facade-use, package-target, and dependency CLI tests | Module path, import, package boundary, manifest, and dependency diagnostics | `local_path_dependency_runs_only_its_library_and_writes_a_stable_lockfile`, module/package native tests | **Covered** |
| Immutable/mutable bindings and implemented primitive scalars | `pass/block_mutation.sc`, `pass/primitive_scalar_widths.sc`; scalar operator fixture families | `fail/primitive_*` covers literal ranges and forbidden implicit widening, alongside invalid operator, division/remainder, and shift diagnostics | `primitive_scalar_widths_and_boundaries_run_natively` and scalar/operator CLI test families | **Covered**: all twelve declared integer identities lower at their specified widths; `isize`/`usize` follow the native target pointer width |
| Tuples and unit | `pass/tuple_basics.sc` covers structural types, literals, singleton syntax, projection, and patterns | `fail/tuple_*` covers pattern arity, projection shape/bounds, and move errors with source-level diagnostics | `tuple_types_literals_projection_patterns_and_cleanup_run_natively`, including guarded non-`Copy` fallback and partial aggregate cleanup | **Covered** |
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
| Bounded C FFI imports | `extern "C"` blocks with optional `@link_name`; `pass/ffi_c_abs.sc`, `pass/ffi_c_memset.sc` | `fail/ffi_*` covers unsafe authority, ABI/type/group restrictions, duplicate/reserved symbols, and unsupported ABIs | `c_ffi_scalars_and_raw_pointers_link_and_run_natively` links libc scalar and raw-pointer calls | **Covered** for M0 imports; stable exports and `@repr(C)` aggregates remain in post-M0 `ABI-1` |
| Binary/library targets and native LLVM emission | check/emit/build/run, explicit targets, default targets, and library dependency tests | output collision, target selection, manifest, dependency, and cycle diagnostics | shorthand native build, package binaries, dependencies, and ledger | **Covered** |
| Diagnostics suitable for source-level debugging | Stable source-name fragments, defining declaration positions, local initializer positions, and end-exclusive expression-root ranges, including trailing source-closure calls, are asserted across parser, CLI, and codegen tests | Broad fail fixture corpus checks semantic rejection and rejects generated `$...` names | N/A | **Covered** |

## Audit tasks

The matrix creates these release-blocking tasks without changing the frozen scope:

The implementation gaps identified by this audit are closed. `M0-QUALITY-1` now owns the clean
release gate rather than redefining any missing capability.
