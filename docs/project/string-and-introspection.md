# Unified String and Introspection Contract

Status: proposed implementation contract

This document defines an implementation path for ordinary compile-time
reflection and one `string` type shared by compile-time evaluation and native
execution. It preserves Salicin's source-backed language model: syntax may
receive compiler-directed lowering only after the edition-matched `core`
bundle declares and validates the corresponding type or function. Every
irreducible operation has a complete `= builtin()` declaration. Compiler-only
Rust enums, magic source names, and undeclared callable operations are not
language surface.

## Outcomes

The completed surface has these properties:

```salicin
let runtime_text: string = "hello"
let register(comptime name: string): () = {}

let value_type: type = type_of(runtime_text)
let string_sort = sort_of(string)
```

- `"hello"` has the ordinary type `string` in every phase.
- `comptime name: string` carries a typed CTFE value, not a member of a
  compiler-only `string` sort.
- `type_of(expression)` reports the checked runtime type without evaluating,
  moving, borrowing, or capturing `expression`.
- `sort_of(value)` reports the immediate sort of a compile-time value.
- bare `sort` is invalid in every declaration and expression; every universe
  is written `sort(n)` and satisfies `sort(n) : sort(n + 1)`.
- all compiler-recognized identities and signatures are declared in the
  edition-pinned `core` sources.

General macros, source generation, compile-time IO, open reflection over
declarations, arbitrary user-defined CTFE allocation, and an unbounded
universe-polymorphic runtime data model are not included.

## Source-Backed Rule

A feature in this contract is source-backed only when all of the following
hold:

1. its public type, function, trait, effect, or sort identity is declared in
   an edition-pinned Salicin source module;
2. an operation not expressible as ordinary Salicin has a complete function
   or type declaration using `= builtin()`;
3. the core bundle validates the declaration's module, visibility, generic
   groups, runtime groups, passing modes, result, effects, and builtin body;
4. semantic analysis and lowering resolve the validated declaration identity
   instead of matching an unqualified source spelling;
5. deleting or changing the source declaration makes core-bundle validation
   fail before user code is checked.

Public source-defined wrappers should own bounds checks, UTF-8 validation,
looping, and derived behavior. Builtins should be restricted to
representation access, literal materialization, non-evaluation, and
operations that cannot be expressed without exposing private storage.

## Sort Levels

Sort levels are explicit positive compile-time integers:

```text
sort(n) : sort(n + 1), for n >= 1
```

`sort` without a level is never valid. `sort(0)` is reserved and rejected:
ordinary runtime values are classified by types, not by a universal
level-zero sort.

The universe former has an edition-pinned bootstrap declaration:

```salicin
/// Constructs the universe at positive level `level`.
pub let sort(
  comptime level: usize,
): sort(level + 1) = builtin()
```

The compiler necessarily recognizes enough syntax to parse this bootstrap,
as it already does for `builtin`, but the core bundle validates its exact
module, name, visibility, parameter, dependent result, and builtin
initializer before accepting user code. Universe comparison and lowering
resolve this validated identity rather than an unqualified spelling.

First-order sorts classify ordinary compile-time values. They inhabit
`sort(2)`:

The core declarations become:

```salicin
/// Sort of compile-time type values.
pub let type: sort(2)
/// Sort of compile-time lifetime regions.
pub let region: sort(2)
/// Sort of individual compile-time effect identities.
pub let effect: sort(2)
/// Sort of normalized compile-time effect rows.
pub let effects: sort(2)
/// Sort of runtime parameter schemas.
pub let parameters: sort(2)
```

There are two declaration forms at every positive level.

An abstract sort at level `n` is declared by annotating it with its
classifier:

```salicin
let entity: sort(n + 1)
```

Abstract sort declarations remain compiler-owned in this milestone. User
packages define closed finite sorts rather than introducing open compiler
metadata domains.

A closed finite sort at level `n` is defined with an explicitly levelled
constructor:

```salicin
pub let access = sort(1) {
  shared,
  mut,
}
```

The general declaration rules are:

```text
let S: sort(n + 1)       declares an abstract sort S at level n
let S = sort(n) { ... }  defines a closed finite sort S at level n
member : S
S : sort(n + 1)
```

Top-level sort declarations initially require `n` to normalize to a positive
integer literal. Generic signatures such as `sort_of` may use a symbolic
level parameter and normalized level expressions such as `level + 1`.

Universes are invariant in the initial implementation. The inhabitance rule
`sort(n) : sort(n + 1)` does not imply that an arbitrary value of `sort(n)`
may be silently used where `sort(n + 1)` is expected. Universe lifting or
cumulativity requires a later explicit contract.

Canonical declarations are:

```salicin
pub let type: sort(2)
pub let access = sort(1) { shared, mut }
pub let abi = sort(1) { c }
let optimization = sort(1) { size, speed }
```

The result level of a finite declaration is inferred from its explicit
constructor. An optional redundant annotation, when supported, must agree:

```salicin
pub let access: sort(2) = sort(1) { shared, mut }
```

Bare legacy forms receive direct diagnostics:

```text
`let name: sort` requires an explicit classifier level; write `sort(2)`
`let name = sort { ... }` requires an explicit declared level; write `sort(1) { ... }`
```

## Source-Backed Introspection

Runtime type inspection and compile-time sort inspection remain separate.
This matches the language's distinct runtime `Type` and static `Sort`
domains.

Examples:

```salicin
type_of(42)           // i32
type_of(runtime_text) // string

sort_of(i32)          // type
sort_of('static)      // region
sort_of(shared)       // access
sort_of(type)         // sort(2)
sort_of(sort(2))      // sort(3)
```

### `sort_of`

Once compile-time binders may be classified by an inferred sort at any
universe level, `sort_of` is an ordinary universe-polymorphic source
definition:

```salicin
/// Returns the inferred immediate sort of `value`.
pub let sort_of(
  comptime level: usize,
  comptime classifier: sort(level),
  comptime value: classifier,
): sort(level) = {
  classifier
}
```

The compiler infers `level` and `classifier` through ordinary static-argument
inference; it does not special-case the body. For `sort_of(i32)`,
`classifier = type` and `level = 2`. For `sort_of(type)`,
`classifier = sort(2)` and `level = 3`.

### `type_of`

`type_of` is an unevaluated syntax contract.
Representing it as an ordinary eager parameter would incorrectly execute
effects and move values.
Its source declaration therefore describes the expression as a lazy callable:

```salicin
/// Returns the inferred type of `expression` without executing it.
pub let type_of(
  comptime e: effects,
  comptime t: type,
)
  (move expression: (): t with(e)): type = builtin()
```

The parser rewrites:

```salicin
type_of(expression)
```

to the validated syntax contract without constructing a runtime closure.
Semantic checking checks `expression` in an unevaluated child context,
infers `e` and `t`, and returns `t`. It must not:

- emit HIR for `expression`;
- consume or borrow locals referenced by `expression`;
- require the inferred effect row in the surrounding function;
- instantiate cleanup or closure-capture state;
- run CTFE.

The callable-shaped declaration makes laziness and the accepted expression
shape visible in source, following the existing source-backed control syntax.
Malformed or missing declarations fail core validation. A runtime expression
that happens to be CTFE-evaluable still belongs to `type_of`; `sort_of`
accepts compile-time entities rather than reclassifying source expressions by
whether optimization can evaluate them.

## The `string` Type

`string` becomes an opaque ordinary type owned by `core`, because literals,
test registration, and compile-time parameters must have one identity even
when the `alloc` package is absent:

```salicin
// core/string.sc
/// Owning, growable, well-formed UTF-8 text.
pub let string: type = builtin()

extend(string, core.marker.movable) {}

extend(string, core.marker.droppable) {
  let drop(self: borrow(mut)(self))(): () = builtin()
}
```

It does not implement `copyable`. Static and inline instances obey the same
move-only ownership contract as heap instances; storage mode never changes
source semantics.

`library/core/src/lib.sc` and the prelude re-export this identity:

```salicin
pub let string = core.string.string
```

`library/alloc/src/lib.sc` re-exports the same identity and does not declare
another `string` struct.

### Logical Value

In both phases a string is a finite sequence of Unicode scalar values whose
canonical storage encoding is well-formed UTF-8. Equality, ordering,
incremental identity, and CTFE hashing operate on the UTF-8 bytes.

Unicode normalization is not implicit. Canonically equivalent but
byte-distinct scalar sequences remain distinct until an explicit Unicode
library operation normalizes them.

### Runtime Representation

The native ABI uses a target-sized three-word opaque value. On a 64-bit target
it is 24 bytes and has unobservable storage modes:

- `inline`: short UTF-8 content is stored inside the value;
- `static`: content points into immutable program data and owns no allocation;
- `heap`: content points to a uniquely owned allocation and records capacity.

The representation stores byte length. Scalar count and grapheme count are
not cached in the initial ABI. Static storage detaches to heap storage before
mutation; heap storage mutates in place under `borrow(mut)`; inline storage
promotes to heap only when capacity is exceeded.

Tag layout, inline capacity, growth factor, and empty-string encoding are
private implementation details. Tests assert behavior and target size, not
specific tag bits.

### Primitive Builtin Kernel

Private top-level declarations give the compiler a small, validated lowering
surface. Names below are descriptive; their canonical module identities and
exact signatures form the contract.

```salicin
// Creates the empty inline string.
let string_new(): string = builtin()

// Creates an empty string with space for at least `capacity` UTF-8 bytes.
let string_with_capacity(capacity: u64): string = builtin()

let string_len_bytes(value: borrow(string)): u64 = builtin()
let string_capacity(value: borrow(string)): u64 = builtin()

// The source wrapper performs the bounds check.
let string_byte_at_unchecked(
  value: borrow(string),
  index: u64,
): u8 = builtin()

// Returns a shared byte view tied to the source borrow.
let string_as_bytes(
  comptime r: region,
)
  (value: borrow(r)(string)): borrow(r)(slice(u8)) = builtin()

let string_reserve(
  value: borrow(mut)(string),
  additional: u64,
): () = builtin()

// Callers preserve the UTF-8 invariant.
let string_push_byte_unchecked(
  value: borrow(mut)(string),
  byte: u8,
): () with(core.unsafe.unsafety) = builtin()

// `new_length` has already been checked as a UTF-8 boundary.
let string_truncate_unchecked(
  value: borrow(mut)(string),
  new_length: u64,
): () with(core.unsafe.unsafety) = builtin()

// Transfers ownership between the opaque string and allocation adapters.
pub let string_from_raw_parts(
  pointer: ptr(mut)(u8),
  length: u64,
  capacity: u64,
): string with(core.unsafe.unsafety) = builtin()

pub let string_into_raw_parts(
  move value: string,
): (ptr(mut)(u8), u64, u64) with(core.unsafe.unsafety) = builtin()
```

The raw-parts operations always return heap-owned storage. Converting an
inline or static string to raw parts first allocates and copies, so ownership
does not depend on a hidden storage mode.

The two raw-parts operations are public so that the separate `alloc` package
can implement zero-copy adapters. They remain outside the prelude and require
`unsafe`; their pointer, length, capacity, allocation, and single-owner
preconditions are part of their declaration documentation. All other
primitive functions remain private to `core.string`.

The compiler must validate every declaration above. An undeclared Rust
lowering or string operation selected by spelling is a contract violation.

### Public Source Definitions

Public methods are ordinary source wrappers wherever possible:

```salicin
extend(string) {
  let new(): string = {
    string_new()
  }

  let with_capacity(capacity: u64): string = {
    string_with_capacity(capacity)
  }

  let len_bytes(self: borrow(self))(): u64 = {
    string_len_bytes(self)
  }

  let capacity(self: borrow(self))(): u64 = {
    string_capacity(self)
  }

  let is_empty(self: borrow(self))(): bool = {
    self.len_bytes() == 0
  }

  let as_bytes(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(slice(u8)) = {
    string_as_bytes(self)
  }

  let byte_at(self: borrow(self))(index: u64): u8 = {
    if index >= self.len_bytes() {
      unsafe { raw_trap() }
    }
    string_byte_at_unchecked(self, index)
  }

  let reserve(self: borrow(mut)(self))(additional: u64): () = {
    string_reserve(self, additional)
  }
}
```

UTF-8 validation, boundary checks, `is_empty`, prefix/suffix operations,
search, scalar iteration, and derived comparison stay in Salicin source.
Only byte access and private-storage mutation cross the builtin boundary.

An initial `unicode_scalar` ordinary type should precede safe single-scalar
append. Until then, source may expose `push_str` and keep byte append private
and unsafe.

### String Literals

Literal materialization also has a source contract. After typed composite
compile-time parameters are available, `core/string.sc` declares:

```salicin
/// Materializes compiler-validated UTF-8 literal bytes as `string`.
let string_literal(
  comptime n: usize,
  comptime bytes: array(u8)(n),
): string = builtin()
```

The lexer decodes escapes and validates source UTF-8, then expression
lowering resolves this declaration. CTFE returns the canonical string value;
native lowering chooses inline or static storage. The compiler does not
expose `string_literal` as a user-facing constructor.

Before the generalized compile-time binder milestone lands, the same
declaration may temporarily be represented by a dedicated AST literal node,
but the core bundle must already contain and validate the declaration. The
temporary lowering is removed when the source signature becomes callable.

## Compile-Time Value Model

Compile-time parameters currently store only `ast::Sort`. They must be
generalized to a classifier that can be a named static sort, an explicit sort
universe, an inferred classifier, or an ordinary runtime type:

```text
CompileClassifier
  = StaticSort(Sort)
  | RuntimeType(Type)
  | SortUniverse(StaticExpr level)
  | InferredClassifier(name)
```

Consequently:

```salicin
comptime t: type       // a value classified by the static sort `type`
comptime n: usize      // existing compiler integer value
comptime name: string  // a typed CTFE value of ordinary runtime type `string`
comptime s: sort(l)    // a classifier inhabiting an explicit universe
comptime value: s      // a value classified by inferred classifier `s`
```

`usize` retains its dependent-constant behavior and is the scalar case of
typed CTFE rather than a permanent string-like special sort.

The normalized evaluator gains:

```text
CtfeValue::String {
    utf8: Vec<u8>,
}
```

This value contains no host address, LLVM value, capacity, storage tag, or
allocator state. Its exact type is the resolved core `string` identity.

The existing blanket rejection of CTFE values whose runtime types implement
`droppable` is refined: the validated core `string` type has a compiler-owned
resource-free normalized representation and is admitted explicitly.
User-defined droppable values remain rejected.

## `alloc` Integration

The current `alloc.string.string` struct is removed. `alloc.string` becomes an
adapter module over the core identity:

```salicin
let string = core.string.string
let vec = alloc.vec.vec
let result = core.result

pub let from_utf8_error = struct {
  bytes: vec(u8),
  valid_prefix: u64,
}

/// Validates and consumes `bytes`, transferring its allocation on success.
pub let string_from_utf8(
  move bytes: vec(u8),
): result(from_utf8_error)(string) = {
  // UTF-8 validation remains ordinary Salicin source.
  // On success, take the vector raw parts and call the validated core
  // `string_from_raw_parts` contract.
}

/// Consumes a string and returns owned bytes.
pub let string_into_bytes(move value: string): vec(u8) = {
  // Call `string_into_raw_parts`, then construct `vec(u8)`.
}
```

Because inherent extensions must be declared by the package owning the type,
the common inherent `string` API lives in `core.string`. Allocation-specific
zero-copy adapters are free functions in `alloc.string`; they do not create a
second type or rely on an orphan inherent extension.

An eventual `str` or `string_slice(r)` may be a non-owning, UTF-8-boundary
checked view. It is not a second compiler/runtime string identity: literals,
compile-time parameters, owned values, equality, and reflection continue to
use `string`. A borrowed view must carry or be constrained by its source
region and cannot outlive the owning string.

## Test Registration

The source-backed syntax contract changes from the removed string sort to the
ordinary type:

```salicin
pub let test(
  comptime name: string,
)
  (move body: (): bool): () = builtin()
```

Core validation checks that `name` uses
`CompileClassifier::RuntimeType(core.string.string)`. Registration names are
read from `CtfeValue::String`, encoded deterministically for symbols when
needed, and decoded only at the CLI boundary. The symbol encoding is not the
semantic identity of the string.

## Implementation Work Packages

### S1: Source identities and opaque runtime type

- add `library/core/src/string.sc`;
- add the `string` type and primitive builtin declarations;
- add exact core-bundle validation and resolved lang-item identities;
- re-export the type from `core` and the prelude;
- lower the three-word opaque native representation;
- implement move, drop, inline, static, heap, detach, and raw-parts behavior;
- add native ownership and cleanup tests.

### S2: Ordinary runtime string literals

- add string literals to the expression AST and type them as core `string`;
- validate and resolve the literal materialization declaration;
- emit inline/static literals without heap allocation;
- implement the public source wrappers and UTF-8 boundary tests;
- verify literal move, borrow, return, global, comparison, and cleanup paths.

### S3: Typed compile-time string values

- introduce `CompileClassifier::RuntimeType`;
- admit `comptime name: string`;
- add `CtfeValue::String`;
- update substitution, inference, overload identity, incremental
  fingerprints, diagnostics, and module interfaces;
- update `test` registration and other metadata consumers;
- remove `Sort::String`, `StaticValue::String`, `StringSort`, and
  `pub let string: sort`.

### S4: `alloc` zero-copy adapters

- replace the `alloc.string.string` struct with the core alias;
- retain source UTF-8 validation and ownership-preserving error recovery;
- transfer successful `vec(u8)` allocations through the declared raw-parts
  contracts;
- add empty, inline, static, heap, invalid UTF-8, success, failure recovery,
  and allocator-leak tests;
- update the standard-library surface contract from two string identities to
  one string plus borrowed views.

### I1: Explicit Sort Universes

- add and validate the source-backed `sort(level)` universe former;
- parse, resolve, compare, substitute, print, and fingerprint `sort(n)`;
- implement `sort(n) : sort(n + 1)` for every positive compile-time level;
- normalize symbolic level expressions and diagnose unsolved or non-positive
  levels at the boundary that requires a concrete declaration level;
- keep universe matching invariant; do not add cumulative coercions;
- reject bare `sort` and `sort(0)` everywhere;
- require abstract declarations to use `: sort(2)`;
- require closed declarations to use `= sort(1) { ... }`;
- record and validate the declared level on every `SortDef`;
- change `SortDef` to retain its normalized declared level for both abstract
  and finite definitions;
- update constructor-sort comparison, diagnostics, interfaces, and
  incremental fingerprints.

### I2: Compile-Time `sort_of`

- support universe-level and classifier inference;
- admit `comptime classifier: sort(level)` and
  `comptime value: classifier`;
- add the ordinary universe-polymorphic source definition of `sort_of`;
- cover builtin, finite, symbolic, constructor, universe, and cross-module
  values;
- verify `sort_of(type) == sort(2)` and
  `sort_of(sort(n)) == sort(n + 1)`.

### I3: Runtime `type_of`

- add and validate the callable-shaped builtin syntax declaration;
- add the unevaluated expression-checking context;
- ensure no HIR, effects, moves, borrows, captures, cleanup, or CTFE escape
  from the inspected expression;
- cover literals, locals, generic values, calls, methods, branches,
  effectful expressions, move-only values, and ill-typed expressions.

String work packages S1-S4 may land before I1-I3. `sort_of` depends on I1;
typed compile-time string parameters depend on S3. `type_of` is independent
of the string representation once the core `string` type exists.

## Required Diagnostics

At minimum:

- malformed core declarations report the expected complete Salicin shape;
- `string` in a compile-time classifier position before S3 reports that typed
  compile-time values are not enabled, rather than treating it as a named
  finite sort;
- invalid UTF-8 conversion reports the byte length of the valid prefix;
- safe truncation reports a non-boundary byte offset distinctly from an
  out-of-range offset;
- bare `sort` reports that an explicit positive universe level is required;
- `sort(0)` reports that level zero is reserved for the non-universal runtime
  value domain;
- an abstract sort annotation reports the expected classifier level;
- a finite sort initializer reports the expected declared level;
- `type_of` still reports diagnostics produced while type-checking its
  expression, even though the expression is not evaluated;
- `type_of` rejects a compile-time sort value with a `sort_of` suggestion,
  while `sort_of` rejects a runtime expression with a `type_of` suggestion.

## Verification Gate

Completion requires:

- parser, core-bundle, semantic, CTFE, incremental, module, HIR, cleanup,
  LLVM IR, and native tests;
- cross-module and cross-package identity tests proving that core, prelude,
  and alloc aliases name the same `string` type;
- deterministic CTFE hashing and test discovery for ASCII, non-ASCII, NUL,
  escapes, and strings that differ only by Unicode normalization;
- target-size tests for 32-bit and 64-bit layouts without asserting private
  tag values;
- no allocation for native static literals and in-range inline literals;
- exactly one deallocation for heap strings across normal exit, return,
  failure, handlers, and early loop exit;
- detach-before-mutation for static storage;
- preservation of the original vector on failed consuming UTF-8 conversion;
- formatting, Clippy, unit, fixture, library, and native suites clean;
- documentation and standard-library contracts updated in the same release
  that changes the public identity.
