# Experimental Native Linkage

Status: implemented native export contract

This document defines how separately emitted Salicin LLVM modules identify
public definitions. It extends the [native calling convention](native-calling-convention.md);
it is an experimental compiler contract, not a stable 1.0 ABI or a precompiled
package format.

## Export Boundary

Code generation gives external LLVM linkage only to runtime definitions that
are all of the following:

- declared `pub`;
- owned by the selected primary package;
- concrete after semantic analysis;
- representable by the native calling convention.

Private and `pub(package)` definitions remain internal. Public definitions
from source dependencies also remain internal when the compiler emits the
selected package, so one unified package graph cannot accidentally provide a
dependency's exports twice. Generated closures, adapters, drop glue, async
state helpers, and other compiler-created definitions are always internal.

A non-Unit public global is exported as a constant definition. Unit globals
have no runtime storage and therefore no native symbol. Nominal types define
ABI identity and layout metadata but do not create standalone linker symbols.

## Stable Identity

Every package graph carries a resolved `(source, name, exact version)`
identity independently of its temporary numeric package ID. Workspace and
path sources use portable lock-root-relative paths; registry sources use
their normalized registry name. Dependency canonical names use that full
identity, so graph traversal order and absolute checkout paths cannot change
an exported contract. Duplicate full providers are rejected before semantic
analysis and identify both package IDs; equal names and versions from
different providers remain distinct.

An exported symbol contains three logical components:

1. the defining package identity;
2. the canonical source definition name, including its module and overload;
3. an ABI fingerprint.

The concrete textual mangling remains an implementation detail and may change
before 1.0.

## ABI Fingerprint

A function fingerprint records the flattened runtime parameters in source
order, their resolved copy, move, shared-borrow, or mutable-borrow modes, the
normalized type identity of every parameter and result, and the source
`Unsafe`, `Throws`, and custom-effect contract. Unit positions remain in the
fingerprint even though native lowering erases them. Borrow regions do not:
they constrain source checking but have no runtime representation.

A global fingerprint records its normalized value type. Nominal identities
use the defining package's stable identity rather than a graph-local package
number, including when nested inside tuples, arrays, pointers, callables,
continuations, or effect rows.

Matching declarations therefore select the same symbol. A source or effect
signature change selects a different symbol and cannot silently bind to an
incompatible definition.

## Generics

Generic templates do not own native symbols. The consuming compilation owns
each concrete specialization and emits it with internal linkage. This keeps
one specialization policy inside the compilation that selected its concrete
types and avoids provider/consumer disagreement over instantiation.

Exporting precompiled generic implementations would require a separate
interface and distribution design; it is not inferred from linker symbols.

## Collision And Failure Rules

The source resolver rejects duplicate declarations, duplicate overload label
shapes, duplicate numeric package IDs, and duplicate stable package
identities before code generation. Symbol components use injective encoding,
so distinct accepted names do not collide through escaping.

Two objects that provide the same package, definition, and ABI fingerprint
are duplicate definitions and the native linker rejects them. A caller built
against an incompatible fingerprint has an unresolved symbol instead of
binding to the wrong implementation. Salicin package builds avoid both cases
by accepting one source provider for each stable package identity.

The current project driver compiles a complete source dependency graph into
one LLVM module. Independent library modules can already link through this
native contract, as covered by the native regression suite, but the compiler
does not yet serialize or consume precompiled Salicin package interfaces.
That artifact belongs to the later package-distribution milestone.

## Verification

Regression coverage proves that:

- public primary-package functions and constants receive external linkage;
- private, package-visible, dependency-owned, generic, and generated
  definitions remain internal;
- package identity separates same-named exports from different providers;
- changing a function ABI changes its export identity;
- changing only graph-local numeric package IDs does not;
- independently emitted LLVM modules link and execute through their exported
  symbols.
