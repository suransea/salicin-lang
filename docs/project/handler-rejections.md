# Handler rejection boundaries

Status: EH1 closure inventory

This document records the source-expressible boundaries intentionally retained by algebraic-handler
lowering. A boundary is complete only when its diagnostic uses source names and a CLI negative
fixture fixes the message fragment. Parser errors and ordinary type, arity, visibility, and borrow
errors are covered by their subsystem tests and are not duplicated here.

## Clause and operation contracts

| ID | Lowering location | Stable diagnostic fragment | Negative fixture | Enforced boundary |
|---|---|---|---|---|
| `EFF-R-CLAUSE-COMPLETE` | `effects.rs::lower_effect_handler` | `missing handler clause` | `algebraic_effect_missing_clause.sc` | Every operation overload has one clause. |
| `EFF-R-NEVER-RESUME` | `effects.rs::lower_effect_handler` | `Never-returning operation ... without resume` | `algebraic_effect_never_abort_resume.sc` | A diverging operation has no continuation value. |
| `EFF-R-OVERLOAD-CALL` | `effects.rs::lower_effect_operation_call` | `overloaded effect operation ... requires named arguments` | `algebraic_effect_overload_positional.sc` | Runtime parameter labels select operation overloads. |
| `EFF-R-OVERLOAD-CLAUSE` | `effects.rs::lower_effect_handler` | `overloaded handler clause ... declaration order before resume` | `algebraic_effect_overload_clause_labels.sc` | Clause parameter names select the same overload identity. |
| `EFF-R-UNHANDLED` | `mod.rs::require_function_effects` | `call to State(i32).get requires custom effect State(i32)` | `algebraic_effect_unhandled.sc` | An operation must be handled or propagated in the current row. |

The complete structural validator remains in `effects.rs::lower_effect_handler`. Its diagnostics for
missing labels, non-closure arguments, duplicate clauses, unknown clauses, action parameters, and
wrong clause arity reject malformed uses of the source-backed `Handle` contract, not unsupported
handler capabilities. The parser and codegen tests cover those contract checks; the two semantic
boundaries above retain dedicated CLI fixtures.

## Continuation and ownership boundaries

| ID | Lowering location | Stable diagnostic fragment | Negative fixture | Enforced boundary |
|---|---|---|---|---|
| `EFF-R-CONT-ONCE` | `handlers.rs::transform_handler_expr` | `continuation resume is one-shot` | `algebraic_effect_resume_twice.sc` | EH1 continuations are one-shot, not multi-shot. |
| `EFF-R-CONT-ESCAPE` | `handlers.rs::transform_handler_expr` | `continuation resume cannot escape its handler clause` | `algebraic_effect_continuation_escape.sc` | A source continuation cannot outlive its clause frame. |
| `EFF-R-BORROW-OVERLAP` | `handlers.rs::transform_effectful_named_call` | `overlapping borrowed arguments` | `algebraic_effect_identical_field_borrows.sc`, `algebraic_effect_parent_child_borrows.sc`, `algebraic_effect_dynamic_index_alias.sc` | Frame fusion accepts only statically disjoint same-root projections. |
| `EFF-R-ACTION-BORROW` | ownership lowering reached from `handlers.rs` | `already borrowed` | `algebraic_effect_reusable_borrowed_action_overlap.sc` | A reusable action cannot overlap a staged mutable argument loan. |

These rows enforce the status contract that resume and abandonment own one continuation and that
borrowed roots may cross a generated frame only when ownership and aliasing remain statically
deterministic.

## Callable transport boundaries

| ID | Lowering location | Stable diagnostic fragment | Negative fixture | Enforced boundary |
|---|---|---|---|---|
| `EFF-R-ALIAS-BINDING` | `handlers.rs::transform_handler_block` | `must be an inferred immutable binding` | `algebraic_effect_mutable_function_alias.sc` | Static aliases are compile-time targets, not mutable runtime slots. |
| `EFF-R-ALIAS-ESCAPE` | `handlers.rs::transform_handler_expr` | `effectful function alias ... cannot escape` | `algebraic_effect_function_alias_escape.sc` | A static target alias exists only inside its active handler. |
| `EFF-R-DYNAMIC-ESCAPE` | `handlers.rs::transform_handler_expr` | `dynamic effectful callable ... cannot escape` | `algebraic_effect_dynamic_callable_escape.sc` | A finite target tag has meaning only inside its active handler. |
| `EFF-R-DYNAMIC-ASSIGN` | `handlers.rs::transform_handler_expr` | `incompatible target set` | `algebraic_effect_dynamic_callable_assignment.sc` | Mutable finite selections require identical signatures and target sets. |
| `EFF-R-ERASED-SHAPE` | `handlers.rs::transform_effectful_named_call` | `requires one optional input group, move passing, and exactly the handled effect` | `algebraic_effect_erased_callable_shape.sc` | The EH1 erased ABI supports a move-only zero- or one-input action with exactly one handled effect. |
| `EFF-R-ERASED-ONCE` | `handlers.rs::transform_erased_effect_callable_call` | `one-shot and cannot be invoked more than once` | `algebraic_effect_erased_callable_twice.sc` | An erased callable owns one call-or-drop opportunity. |
| `EFF-R-ERASED-BORROW` | closure escape checking before handler transport | `cannot escape while it captures a borrow` | `algebraic_effect_erased_callable_borrow_escape.sc` | An erased environment cannot carry a borrow beyond its lexical handler. |

These are the callable limits stated in [implementation status](status.md): known targets use static
specialization, finite target sets use handler-local tags, and open targets use the bounded owned
`EffectCallable` ABI.

## Defensive invariants

Diagnostics beginning with `internal handler` and guards such as a missing generated continuation
input, an empty dispatch set, or a generated closure losing its runtime group describe compiler
invariants. They are not accepted source rejection boundaries and therefore have no negative
fixtures. Reaching one from a source program is a compiler bug.

The unresolved residual-effect guard in `handlers.rs::transform_effectful_named_call` remains a
conservative ownership check. Ordinary generic instantiation resolves effect arguments before this
path, so there is currently no independent source form that reaches it. It must become either a
source-level boundary with a fixture or an internal assertion if later generic-row work makes that
distinction observable.

## Diagnostic rules

- User-facing handler diagnostics must name source operations, functions, parameters, and locals.
- `$handler$`, `$effect$operation$`, and other generated symbol prefixes must not appear.
- Message fragments in the tables are compatibility assertions for the CLI fixtures.
- Source spans are tracked separately by `M0-DIAG-1`.
