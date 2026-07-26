# Composite Compile-Time Evaluation Contract

Status: accepted P0 contract

This document defines the semantic boundary for composite compile-time
evaluation (CTFE). It is an implementation contract, not a promise that every
runtime expression can execute during compilation.

Salicin has one runtime object language and one erased static language.
Ordinary pure functions belong to the runtime language, but an eligible call
may be interpreted at a static use site. Runtime `struct` and `enum`
declarations remain types; they do not become `sort`s merely because one of
their values is normalized during compilation.

## Evaluation Sites and Phases

The composite evaluator is required at dependent fixed-array lengths, global
constant initializers, and later compiler-owned consumers that explicitly
request a runtime-typed normalized value. Evaluation is mandatory at these
sites. Failure is a compile-time diagnostic, not permission to defer the
expression to runtime. Pure expressions outside a required site may still be
optimized, but optimization is not CTFE and cannot change accepted source
behavior.

The evaluator receives resolved declarations, fully substituted compile-time
arguments, a target description, and an expected runtime type. It returns
either one typed normalized value or one source-backed failure. It never emits
LLVM IR, allocates runtime storage, observes a host address, or invokes
runtime cleanup.

## Typed Value Domain

One recursive typed value representation owns all runtime-typed CTFE results:

- unit;
- `bool`;
- `i8`, `i16`, `i32`, `i64`, `i128`, and `isize`;
- `u8`, `u16`, `u32`, `u64`, `u128`, and `usize`;
- tuples with their exact ordered field types;
- fixed arrays with exact element type and length;
- concrete non-generic or fully instantiated generic structs, identified by
  canonical nominal identity and ordered source fields;
- concrete closed enums, identified by canonical nominal identity, source
  variant identity, and typed payload fields.

Every integer value carries its exact signedness and width. `isize` and
`usize` use the compilation target's pointer width, never the compiler host's
width. A value is well formed only when every child value has exactly the type
required by its tuple position, array element, nominal field, or enum payload.

Compiler metadata remains represented by `StaticValue`: `type`, `string`,
`region`, `effect`, `effects`, finite-sort members, parameter schemas, and
constructor values. `StaticValue` is erased before runtime lowering and is
not a recursive container for runtime tuples, arrays, structs, or enums. A
narrow adapter may translate existing marker-shaped monomorphization inputs,
but composite values must never be encoded as invented source `Type` names.

## Admitted Source Constructs

Evaluation is strict and left to right. The admitted initial surface is:

- scalar, tuple, array, struct, and enum literals or constructors;
- immutable local `let` bindings and expression blocks;
- tuple and nominal field projection;
- bounds-checked fixed-array indexing;
- unary, arithmetic, comparison, logical, and bitwise operations defined for
  the exact operand type;
- short-circuit `and` and `or`;
- `if`;
- exhaustive `match` with literals, wildcards, immutable bindings, tuple,
  struct, and enum patterns, nested patterns, and pure guards;
- direct calls to eligible pure source functions, including statically
  resolved methods and associated functions;
- `return` within an interpreted function.

Calls preserve source runtime-group boundaries, labels, generic substitution,
and declaration identity. Arguments are evaluated once in source order.
Pattern tests and guards are evaluated in arm order. Only the selected branch
or arm is evaluated.

The first composite milestone does not admit mutation, mutable locals,
assignment, loops, borrowing, raw pointers, slices, function values, closures,
async, algebraic handlers, effects, foreign or builtin bodies, allocation, or
runtime `string`, `vec`, or `box` values. Unselected control-flow branches may
contain rejected constructs because CTFE validates executed behavior at the
required use site; declaration checking still validates their ordinary
runtime types.

## Function Eligibility

A source function may execute during CTFE when:

- its selected declaration and body are available after module resolution;
- all compile-time arguments and runtime arguments are concrete;
- the call is fully applied at the point where its result is required;
- its effect row is `pure`;
- every executed parameter, local, and result type is in the typed value
  domain;
- no executed operation borrows storage, performs mutation, allocates,
  invokes a handler, calls foreign code, or calls a compiler builtin without a
  separately specified CTFE rule.

Eligibility is checked at the static call site. Ordinary pure functions do
not receive a second `const` or `comptime` declaration modifier, and rejecting
one static call does not make the function invalid for runtime use.

## Resource Exclusion

Before constructing a composite value, the evaluator recursively rejects
unsized fields; references, pointers, slices, callable values, continuations,
or address-dependent layout values; types with an applicable `droppable`
implementation; fields whose type recursively requires destruction;
allocation-backed values; and recursive nominal layouts without a finite
value representation.

Unit and empty aggregates are allowed. Exclusion is based on resolved type and
trait identity, never on a declaration's spelling.

## Arithmetic, Indexing, and Conversion

Integer operations use checked mathematical intermediates followed by an
exact range check for the result type. Signed division overflow, division or
remainder by zero, invalid negation, and any out-of-range result fail CTFE.
Shift counts must be non-negative where applicable and strictly smaller than
the operand width. There is no LLVM poison, wrapping fallback, or
host-language overflow.

Conversions are explicit and fallible unless the destination can represent
every source value. Array indexes are `usize`; an index greater than or equal
to the array length fails before reading an element.

## Equality and Normalization

Equality is defined only for two well-formed values of the same exact runtime
type. Unit values are equal; scalars compare by typed mathematical value;
tuples, arrays, and struct fields compare recursively in source order; enum
values compare nominal identity, source variant identity, then payload.

Nominal identity includes resolved provider, package, module, declaration,
and concrete generic arguments. It excludes checkout paths, graph-local IDs,
source traversal order, LLVM names, padding, discriminants, and field offsets.
Enum normalization records the source variant identity rather than the
backend tag value.

Equivalent resolved programs must produce the same typed value and stable
diagnostic regardless of hash-map order, module traversal order, output path,
or host architecture.

## Complexity Budgets

Every required evaluation uses fixed compiler-owned limits:

- 16,384 semantic evaluation steps;
- 128 active source calls;
- 64 levels of recursive value nesting;
- 65,536 total value nodes;
- 65,536 elements in any one materialized aggregate.

A literal, operation, binding, projection, index, pattern test, guard
transition, function entry, and function return each consume at least one
step. Constructing an aggregate additionally consumes one node per contained
value. Repeated calls with the same canonical function and equal argument
values form an immediate cycle diagnostic; value-changing recursion may
continue only within the budgets.

These limits are part of deterministic compilation and cannot be raised by
source code. A future compiler version may revise them as an explicit
language-version change. Partial values are discarded when a budget is
exhausted.

## Diagnostics

Failures identify the required CTFE site and the executed source construct.
Nested calls add a bounded source call trace. Diagnostics distinguish
unsupported types or constructs; unavailable, builtin, foreign, effectful, or
incompletely applied calls; exact type mismatches; arithmetic and indexing
failures; invalid or non-exhaustive patterns; resource-bearing values;
repeated-call cycles; and each complexity limit.

Messages use source names and canonical public module paths where needed.
They do not expose generated specializations, marker types, LLVM layout, or
hash traversal order. The same resolved input and target must select the same
primary failure.

## Consumers

Dependent array lengths accept only a normalized non-negative `usize`.
Global constants retain their exact normalized runtime type and are converted
to LLVM constants only after CTFE succeeds. LLVM emission is a total encoding
of an already typed value; it does not re-evaluate source expressions.

Consumer identity uses the normalized typed value. Declaration order, module
walk order, temporary evaluator IDs, and output paths are not part of array
type identity, nominal instantiation identity, incremental fingerprints, or
emitted constants.

## Non-Goals

This contract does not add general staged code generation, quotation,
splicing, macros, reflection, compile-time IO, source-adjustable evaluation
limits, runtime nominal values as `sort` members, compile-time allocation, or
resource destruction. Mutation, loops, allocation, and resource-bearing
values require a later contract rather than an implementation shortcut.

## Verification Matrix

Completion requires positive and rejection coverage for every scalar and
composite family, exact-width arithmetic, target pointer widths, construction,
projection, indexing, patterns, control flow, generic and cross-module calls,
nominal identity, `option`/`result`, resource exclusion, cycles, every budget,
stable diagnostics, deterministic IR, and native global values. Dependent
arrays and globals must share the evaluator in tests rather than merely
produce equal-looking results through separate implementations.
