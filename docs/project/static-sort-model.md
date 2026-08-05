# Extensible Static Sort Model

Status: META-1 implemented

Salicin's compile-time language has ordinary value classifiers such as
`type`, `usize`, `effects`, and source-declared finite sorts. A smaller class
of compiler-owned static fragment sorts represents syntax-derived data that
must be consumed by a compiler phase and never becomes a runtime value. This
contract defines how that class can grow without making arbitrary syntax or
declarations first-class by accident.

## Complete Registration

Every compiler-owned fragment sort is identified by an edition-owned stable
ASCII `snake_case` key. A registration is valid only when it declares all of:

- its universe level;
- whether source expressions may construct it;
- whether it has any runtime representation;
- the compiler phase that owns it;
- its scope discipline;
- its equality discipline;
- its unique normal form and consumer;
- every permitted compiler producer;
- fixed node, nesting-depth, and referenced-binding limits.

There are no permissive defaults. Duplicate keys, zero budgets, missing
producers, source-constructible compiler fragments, runtime-lowerable
fragments, and malformed names are rejected by the model. Parser recognition
comes from the edition registry rather than a closed Rust enum, so a future
sort is added by a complete descriptor rather than another unrelated set of
matches.

Edition 2026 currently registers exactly one fragment sort: `constraint`.
`declaration`, `syntax`, `expression`, and `pattern` are not registered.

## Constraint Contract

`constraint: sort(2)` is produced only by syntax elaboration and immediately
normalizes into solver-owned `Constraint`/`Goal` IR. Its contract is:

- phase: elaboration;
- scope: immediate producing lexical context;
- equality: opaque to source programs;
- producer: syntax elaborator only;
- normal form: solver constraint goal;
- limits: 4,096 structural type nodes, depth 128, and 512 associated bindings.

The fragment may refer to binders in the declaration currently being
elaborated because it cannot escape, be returned, stored, defaulted,
explicitly supplied, or substituted into another context. Normalization
consumes the fragment before solver use. A future fragment that crosses this
boundary must use an explicit environment classifier; it cannot weaken the
current immediate-context rule.

Constraint structure is counted before conversion to solver IR. Limit errors
are deterministic and source-independent; truncation, partial normalization,
and unbounded recursion are forbidden.

## Phase and Runtime Separation

Fragment values are erased compiler facts, not CTFE values and not runtime
types. The static model rejects runtime lowering requests and wrong-phase
consumption. Ordinary runtime values accepted by CTFE, including `string`,
closed enums, and composite values, remain outside this fragment registry.
Likewise `type`, `region`, `effect`, `effects`, and `parameters` retain their
existing dedicated normalization rules.

## Equality and Diagnostics

Opaque fragments cannot participate in source `==`, hashing, ordering,
defaults, or generic-value comparison. A future normalized-structural
equality policy must name its canonical normal form; raw AST identity, source
range, pointer identity, and traversal order are never equality rules.

Validation distinguishes unknown or duplicate sorts, forbidden producers,
wrong phases, scope escape, runtime lowering, opaque comparison, and each
resource limit. User-facing normalization failures identify the source where
predicate instantiation occurs and do not expose generated symbols or
internal fragment storage.

## Research Basis

Contextual modal typing and environment classifiers show that open code must
record the context that closes it. Recent work also demonstrates that scope
extrusion remains a problem with effects and mutation, and that staged
let-insertion can change evaluation semantics. Salicin consequently exposes
no general quotation or code-value sort in META-1; the registry makes the
obligations enforceable before such a producer exists.

- [Contextual MetaML: Syntax and Full Abstraction (LICS 2026)](https://doi.org/10.4230/LIPIcs.LICS.2026.83)
- [Taming Scope Extrusion in Gradual Imperative Metaprogramming (2026)](https://arxiv.org/abs/2602.19951)
- [Handling Scope Checks (POPL 2026)](https://arxiv.org/abs/2601.18793)
- [Contextual Metaprogramming for Session Types (ESOP 2026)](https://arxiv.org/abs/2601.15180)
- [When Do Staging Annotations Preserve Semantics? (2026)](https://arxiv.org/abs/2606.30854)
- [Let It Be Optimized: Building Multi-Stage Evaluators with Let-Insertion and Optimizations in Small Pieces (ICFP 2026)](https://icfp26.sigplan.org/details/icfp-2026-icfp-papers/4/Let-It-Be-Optimized-Building-Multi-Stage-Evaluators-with-Let-Insertion-and-Optimizat)

## Non-goals

META-1 does not add quotations, macros, generated declarations, AST
inspection, runtime reflection, arbitrary compiler plugins, cross-stage
persistence, or a source API for registering abstract sorts. Concrete future
reflection features must enter the roadmap with named producers and consumers
and extend this registry without weakening existing contracts.
