# Programming-Language Research Ledger

Last reviewed: 2026-07-26

This ledger records research that materially changes Salicin's language model. It is a repeatable
review gate, not a claim that any language can remain “advanced” without continuing comparison,
implementation, and measurement.

## Current Decisions

### Two Levels, One Function Syntax

Salicin has a runtime object language and a normalization-oriented static language, but ordinary
pure function definitions are reusable by compile-time evaluation. Phase comes from the use site
and available effects rather than a second `const fn` spelling. Static evaluation uses a restricted
expression IR, never the unrestricted runtime AST; every static use must terminate within the
implementation's guarded evaluation budget.

This follows the separation advocated by two-level type theory while keeping source ergonomics
close to staged compilation:

- [Closure-Free Functional Programming in a Two-Level Type Theory (ICFP 2024)](https://doi.org/10.1145/3674648)
- [Staged Compilation with Two-Level Type Theory (2022)](https://arxiv.org/abs/2209.09729)
- [When Do Staging Annotations Preserve Semantics? (2026)](https://arxiv.org/abs/2606.30854)

The implementation consequence is strict: effects, mutation, borrowing, handlers, foreign calls,
and runtime closures do not enter type-level evaluation. Compile-time arithmetic is checked and
evaluation failure is a diagnostic rather than undefined behavior.

### Sorts Classify Static Values

`struct` and `enum` define runtime types; `sort` classifies erased static values. Constructor sorts
retain every parameter's sort and every curried group boundary. For example,
`(type)(usize): type` is distinct from `(type, usize): type` and `(type)(type): type`.

User code may define finite sorts. Open-ended abstract sorts are compiler-owned because adding an
uninterpreted classifier without elimination, equality, normalization, and ABI rules would create
an unsound extension point rather than useful abstraction.

Syntax metadata uses the same static-language boundary. `abi = sort { c }` is finite, with
decidable member equality; `string` is compiler-owned and currently introduced only by syntax
positions such as test registration names. Both are erased before runtime lowering. A metadata
string is the decoded UTF-8 literal payload and is never implicitly converted to the future
runtime text representation.

### Dependent Information Is Staged and Controlled

The first dependent feature is computed array length: ordinary pure functions can normalize a
`usize` expression after generic arguments are substituted. This is intentionally narrower than
allowing arbitrary runtime terms in types. It keeps equality decidable in the implemented fragment
and gives the compiler one normalization boundary.

The design is compared against recent staged shape-dependent work:

- [Compile-Time Tensor Shape Checking via Staged Shape-Dependent Types (2026)](https://arxiv.org/abs/2604.23807)

Future shape inference should be best-effort and should request explicit static arguments when
inversion is ambiguous; the compiler must not guess equations it cannot justify.

### Trait Requirements Are Logical Goals

Parser `where` predicates are lowered to an independent `Constraint`/`Goal` vocabulary, including
associated-type projection equations. Existing implementation lookup is being migrated behind this
boundary. The intended solver model is canonical goals under assumptions, with explicit ambiguity
and coherence diagnostics rather than ad-hoc recursive lookup.

References:

- [The Chalk book: clauses and goals](https://rust-lang.github.io/chalk/book/)
- [A Survey of Trait and Type Class Coherence (2025)](https://arxiv.org/abs/2502.20546)

### Effect Identities and Effect Rows Have Separate Sorts

Nominal effect identities are static values classified by `effect`; normalized zero-or-more effect
rows are static values classified by `effects`. Named effects contribute identities to a row; they
are not themselves sorts. `pure` inhabits `effects`, not `effect`. Row normalization must be
order-insensitive and duplicate-free.
This direction is checked against Koka's row-polymorphic effect model:

- [Programming with Row-polymorphic Effect Types](https://koka-lang.github.io/koka/doc/book.html)

## Review Gate

Before extending the static language:

1. Record the new construct's Sort and normalization rule.
2. State whether equality remains decidable and how ambiguity is diagnosed.
3. Prove or test phase separation: runtime effects and storage cannot leak into static evaluation.
4. Add positive, rejection, substitution, and nontermination/complexity tests.
5. Compare the change with primary literature or current production-language specifications and
   date this ledger.

Research review does not replace implementation evidence. A proposal is only part of Salicin once
the grammar, semantic IR, diagnostics, tests, and specification agree.
