# Programming-Language Research Ledger

Last reviewed: 2026-07-30

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

The implementation consequence is comptime strict: effects, mutation, borrowing, handlers, foreign calls,
and runtime closures do not enter type-level evaluation. Compile-time arithmetic is checked and
evaluation failure is a diagnostic rather than undefined behavior.

### Sorts Classify Static Values

`struct` and `enum` define runtime types; `sort` classifies erased static values. Constructor sorts
retain every parameter's sort and every curried group boundary. For example,
`(type)(usize): type` is distinct from `(type, usize): type` and `(type)(type): type`.

User code may define finite sorts. Open-ended abstract sorts are compiler-owned because adding an
uninterpreted classifier without elimination, equality, normalization, and ABI rules would create
an unsound extension point rather than useful abstraction.

Syntax metadata uses the same static-language boundary. `abi = sort(1) { c }` is finite, with
decidable member equality; `string` is compiler-owned and currently introduced only by syntax
positions such as test registration names. Both are erased before runtime lowering. A metadata
The compile-time `string` value is the decoded UTF-8 literal payload and is never implicitly
converted to the future
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

### Composite CTFE Uses Typed Values and Fixed Budgets

The accepted [composite CTFE contract](../project/composite-ctfe.md) keeps
runtime-typed normalized values separate from erased `StaticValue` metadata.
It requires strict left-to-right evaluation, target-width `isize`/`usize`,
use-site diagnostics, structural nominal normalization, resource exclusion,
and compiler-owned step, call, nesting, and aggregate limits.

The July 2026 review retained Salicin's use-site staging model:

- Rust's current constant-evaluation reference makes required const contexts
  fail at compile time, interprets pointer-sized integers for the compilation
  target, and excludes executed destructor calls;
- Zig exposes a backwards-branch quota, confirming that bounded evaluation is
  operationally necessary, but Salicin deliberately keeps budgets fixed and
  unavailable to source so normalized identities remain reproducible;
- C++ work continues moving eligibility diagnostics from function
  declarations to required constant-evaluation uses, matching Salicin's reuse
  of ordinary pure functions;
- *When Do Staging Annotations Preserve Semantics?* (2026) makes evaluation
  order and let insertion part of semantics preservation. Salicin does not
  generate code during CTFE, but therefore specifies argument, binding,
  pattern, guard, and branch order rather than treating normalization as
  optimizer freedom.

Primary references:

- [Rust Reference: Constant evaluation](https://doc.rust-lang.org/reference/const_eval.html)
- [Zig language reference: `@setEvalBranchQuota`](https://ziglang.org/documentation/0.12.0/#setEvalBranchQuota)
- [P2448R0: Relaxing some `constexpr` restrictions](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2021/p2448r0.html)
- [When Do Staging Annotations Preserve Semantics? (2026)](https://arxiv.org/abs/2606.30854)

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

### Effect Rows Construct Callable Types

Reviewed on 2026-07-30. Salicin spells an effectful callable
`with(E)(F)`, where `F` must be callable and `E` is one normalized row. This
makes effect tracking an explicit constructor around the complete callable,
not a decoration on its result. Declarations use a callable-type/body boundary
only when this constructor is present; pure declarations retain their compact
form.

The surface migration preserves the existing function/effect IR and therefore
does not alter handler lowering, cleanup, or ownership. This matches recent
modal-effect work that separates effect tracking from the underlying arrow,
while the resource-safety results below motivate keeping non-local control and
destruction semantics invariant during the rewrite.

Primary references:

- [Rows and Capabilities as Modal Effects (POPL 2026)](https://doi.org/10.1145/3776674)
- [Linear Effects, Exceptions, and Resource Safety (ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
- [Handling Exceptions and Effects with Automatic Resource Analysis (OOPSLA 2026)](https://arxiv.org/abs/2603.02260)

### Host Authority and Host Errors Are Separate

The accepted [standard-library surface](../project/standard-library-surface.md)
uses a validated `io` effect to make host authority visible, owned resource
handles to attenuate that authority to particular files, and
`result(io_error)(t)` values for recoverable failures. Effect rows, capability
handles, and errors therefore keep distinct static roles.

This direction follows WASI's distinction between link-time capabilities and
unforgeable runtime handles. It is also consistent with recent work comparing
row-polymorphic, capability-passing, and modal effect systems: the models can
cooperate without pretending that a row label itself identifies a particular
runtime resource.

Primary references reviewed on 2026-07-27:

- [WASI capabilities](https://github.com/WebAssembly/WASI/blob/main/docs/Capabilities.md)
- [Rows and Capabilities as Modal Effects (HOPE 2025)](https://conf.researchr.org/details/icfp-splash-2025/hope-2025-papers/5/Rows-and-Capabilities-as-Modal-Effects-Extended-Abstract)
- [Zero-Overhead Lexical Effect Handlers (OOPSLA 2025)](https://cs.uwaterloo.ca/~yizhou/papers/zero-oopsla2025.pdf)

### Host Effects and Resource Handles Are Complementary Capabilities

Reviewed on 2026-07-30 for the accepted
[synchronous host I/O contract](../project/host-io.md). The exact embedded
`std.io.io` identity represents authority to ask the native environment to
perform I/O. An opened file is instead an owned, unforgeable runtime handle
that attenuates authority to one resource. Recoverable failures remain
`io_error` values, not effects.

The entry boundary may discharge only this exact standard identity. Resource
destruction is a once-only obligation across return, effect transfer, handler
abort, and ordinary scope exit; explicit close consumes the owner even when
the native close reports failure. Primitive reads and writes expose partial
progress and interruption, while bounded helpers define retry, EOF,
`write_zero`, and `unexpected_eof` behavior.

Current research and specifications:

- [Typestate via Revocable Capabilities (PLDI 2026)](https://doi.org/10.1145/3808323)
- [Pure Borrow (PLDI 2026)](https://doi.org/10.1145/3808259)
- [Toka: Explicit Resource Semantics (2026)](https://arxiv.org/abs/2606.01974)
- [Safe Coding (2026)](https://doi.org/10.1145/3795888)
- [Linear Effects, Exceptions, and Resource Safety (ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
- [Securing Agents With Tracked Capabilities (ACM CAIS 2026)](https://doi.org/10.1145/3786335.3813127)
- [Rows and Capabilities as Modal Effects (POPL 2026)](https://doi.org/10.1145/3776674)
- [WASI Capabilities](https://github.com/WebAssembly/WASI/blob/main/docs/Capabilities.md)
- [Rust 1.97 `Read`](https://doc.rust-lang.org/std/io/trait.Read.html)
- [Rust 1.97 `Write`](https://doc.rust-lang.org/std/io/trait.Write.html)
- [POSIX.1-2024 Issue 8](https://standards.ieee.org/ieee/1003.1/7700/)

### Test Failure Is a Handled, Resource-Safe Outcome

Reviewed on 2026-07-30 for the implemented
[structured test-support contract](../project/test-support.md). A registration
returns unit normally or transfers an owned message through the ordinary
`core.error.throwing(core.string.string)` effect. The runner interprets that
effect once, after ordinary cleanup, records the resulting outcome, and
continues with the next registration. This reuses the language's typed
exception abstraction instead of maintaining a test-only effect or boolean
failure protocol.

The result channel is a separate framed pipe instead of stderr or an exit-code
index, so test output cannot be mistaken for runner metadata and more than one
failure can be represented.

Current research:

- [ETAS: An Effect-Typed Language for Agent Systems (2026)](https://arxiv.org/abs/2607.17780)
- [Building Extensible Program Logics through Effect Handlers (2026)](https://arxiv.org/abs/2607.12642)
- [Yarrow: Reconciling Effect Handlers and Region-Based Memory Management (2026)](https://arxiv.org/abs/2607.15876)
- [Linear Effects, Exceptions, and Resource Safety (ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
- [A Relational Separation Logic for Effect Handlers (POPL 2026)](https://doi.org/10.1145/3776676)

### Practical Acceptance Uses Independent End-to-End Oracles

Reviewed on 2026-07-30 for the completed standard-library usability
milestone. Library acceptance is not inferred from isolated API compilation.
The repository runs a multi-module command as an external native process,
supplies fixed Unicode and numeric arguments, and compares stdout and file
bytes with an independently written oracle. Separate cases cover malformed
input, missing input, throwing cleanup, stdin behavior, host errors, and live
allocation balance.

This follows recent evidence in two ways:

- [Safe Coding (CACM 2026)](https://doi.org/10.1145/3795888) treats complete
  user journeys and expected error conditions as integration evidence for
  safely composed abstractions.
- [On the Risk of Coding Before Testing (2026)](https://arxiv.org/abs/2607.05139)
  finds that implementation-derived tests can reproduce the same fault in
  their oracle, motivating fixed external expected bytes and error statuses.
- [Handling Exceptions and Effects with Automatic Resource Analysis
  (OOPSLA 2026)](https://2026.splashcon.org/details/oopsla-2026/8/Handling-Exceptions-and-Effects-with-Automatic-Resource-Analysis)
  reinforces checking resource behavior across non-local control transfer;
  Salicin therefore probes allocation balance after both return and throw.
- [Virtualizing Continuations (PLDI 2026)](https://pldi26.sigplan.org/details/pldi-2026-papers/46/Virtualizing-Continuations)

The implementation deliberately keeps test failure one-shot. Work on
multi-shot continuations shows why copying or virtualizing handler stacks is a
separate runtime problem; TEST-1 needs only abortive transfer and therefore
retains Salicin's existing exactly-once cleanup model.

The common assertion layer preserves that same interpretation boundary:
operands are bound once before comparison, formatting is selected through
static traits, and deterministic owned messages cross the failure operation.
This also keeps nondeterminism control outside assertion internals; recent
flaky-test work reinforces that replay or API control belongs at an explicit
runner boundary rather than in hidden repeated operand evaluation.

- [Detecting Flaky Tests by Controlling Nondeterministic API Behavior (PACMPL 2026)](https://doi.org/10.1145/3798265)

The first runner-selection surface is deliberately explicit and
order-preserving: a case-sensitive substring chooses a source-order subset,
listing observes that same order, and the summary records its exact execution
population. It does not infer priorities from test code or CI history.

- [DANTE: Data-Driven Test Case Selection and Prioritization for Long-Running Test Suites (ICST 2026)](https://conf.researchr.org/details/icst-2026/icst-2026-research/44/DANTE-Data-Driven-Test-Case-Selection-and-Prioritization-for-Long-Running-Test-Suite)
- [How Far Are We from Detecting Flaky Tests? On the Limits of Code-Based Detection (2026)](https://arxiv.org/abs/2607.09345)

### Persistent Reuse Starts With Stable, Complete Identity

Reviewed on 2026-07-30 for the accepted
[persistent LLVM-IR cache contract](../project/incremental-cache.md). Salicin's
first persistent cache remains whole-graph and source-keyed: the compiler can
perform lookup before semantic lowering, while the cached payload is the exact
LLVM text that native commands already consume. The artifact schema is
separate from the input schema, publication makes a completed directory
visible atomically, and every read revalidates metadata, length, digest, and
UTF-8 before handing IR to Clang.

The review exposed one correctness dependency that was not represented by
schema 1: `salic test --filter` changes the emitted runner. Schema 2 therefore
hashes both filter presence and exact bytes. Output paths remain excluded
because they select a destination rather than source-to-IR semantics.

Current research:

- [Differential Execution with Lexical Tracing (OOPSLA 2026)](https://doi.org/10.1145/3798261)
  formalizes that reusable cache identities must be unique and stable under
  irrelevant surrounding changes. Salicin applies the conservative
  whole-graph version now and defers finer identities until dependency
  tracking exists.
- [Incr: Faster Re-Execution via Bolt-On Incrementalization (OSDI
  2026)](https://www.usenix.org/conference/osdi26/presentation/xie-yizheng)
  combines dependency tracking with stored intermediate results and checks
  behavioral equivalence. That supports separating this safe first cache from
  later per-package dependency reuse.
- [IRHash: Efficient Multi-Language Compiler Caching (USENIX ATC
  2025)](https://www.usenix.org/system/files/atc25-landsberg.pdf) reports the
  maintainability and reuse benefits of an LLVM-IR boundary, while warning
  that finer function caching is unsound without interprocedural dependency
  tracking.
- [On the Variability of Source Code in Maven Package Rebuilds
  (2026)](https://arxiv.org/abs/2602.19383) finds generated-source variation a
  major reproducibility problem. Salicin therefore hashes exact resolved
  source bytes and embedded library sources rather than timestamps or
  provenance assumptions.

Reviewed again on 2026-07-30 for the implemented local storage layer. Lookup
and publication are a separate module from fingerprint construction and
command execution. Every hit must re-establish its state invariant from
canonical metadata, expected invocation identity, exact length, SHA-256, and
UTF-8; invalid state produces a miss. Completed temporary directories become
visible by rename, including the cache-root ownership marker, so concurrent
readers do not observe partially initialized state.

- [Incremental Computation for Efficient Programmable Inference (PLDI
  2026)](https://doi.org/10.1145/3808316) identifies ad-hoc
  incrementalization as a source of soundness bugs and reasons modularly about
  the base computation and incremental transformation. Salicin likewise keeps
  identity, storage, and pipeline reuse as independently tested workstreams.
- [Stateful Differential Operators for Incremental Computing (POPL
  2026)](https://doi.org/10.1145/3776728) permits cached internal state only
  under an explicit maintained invariant. Salicin's storage invariant is
  executable validation, not trust in a filename or previous process.
- [Differential Execution with Lexical Tracing (OOPSLA
  2026)](https://doi.org/10.1145/3798261) warns that missing or stale cache
  entries can invalidate results. The storage API therefore treats every
  malformed or incompatible entry as unusable and never returns partial IR.

Reviewed again on 2026-07-30 for compile-pipeline integration. A cached test
runner is not completely described by LLVM text: the driver also needs the
ordered selected names to implement listing and map native failure indices.
Artifact schema 2 therefore binds that side data to canonical metadata and a
length-framed digest. Cache lookup remains behind complete input resolution;
`check` deliberately stays on the base analysis path.

- [IRHash: Efficient Multi-Language Compiler Caching (USENIX ATC
  2025)](https://www.usenix.org/conference/atc25/presentation/landsberg)
  supports LLVM IR as a practical cross-command reuse boundary while keeping
  native linking outside the reusable compiler result.
- [The Promise and Reality of Continuous Integration Caching (EASE
  2026)](https://conf.researchr.org/details/ease-2026/ease-2026-research-papers/64/The-Promise-and-Reality-of-Continuous-Integration-Caching-An-Empirical-Study-of-Trav)
  reports stale artifacts as an operational risk; Salicin validates identity,
  canonical metadata, side-data digest, payload length, and payload digest on
  every hit instead of trusting mere entry presence.
- [Incremental Computation for Efficient Programmable Inference (PLDI
  2026)](https://doi.org/10.1145/3808316) motivates keeping the incremental
  transformation modular. The uncached compiler remains the miss path and
  publication happens only after that path returns a complete artifact.

Reviewed again on 2026-07-30 for cache control and observability. Cache reuse
is now explicitly optional and inspectable: bypass retains the base compiler
path, tracing reports every reuse decision outside semantic stdout, and
cleanup detaches only a marker-owned artifact namespace. This makes stale or
damaged state diagnosable without allowing cache state to become language
semantics.

- [DeCo: A Core Calculus for Incremental Functional Programming with Generic
  Data Types (OOPSLA 2026)](https://doi.org/10.1145/3798264) gives explicit
  semantics to incremental reuse rather than treating caching as an invisible
  implementation accident. Salicin similarly exposes the operational
  decision while keeping the uncached result authoritative.
- [Differential Execution with Lexical Tracing (OOPSLA
  2026)](https://doi.org/10.1145/3798261) proves cache stability and
  correctness together. Salicin's trace names the complete stable identity
  and exact rejection reason so future invalidation tests can observe the same
  invariant enforced by lookup.
- [The Promise and Reality of Continuous Integration Caching (EASE
  2026)](https://arxiv.org/abs/2601.19146) finds stale cached artifacts in a
  substantial fraction of studied projects. Explicit bypass, corruption
  reasons, and ownership-bounded cleanup provide recovery and diagnosis
  without weakening validation.

Reviewed again on 2026-07-30 for the end-to-end invalidation proof. The test
matrix now perturbs every declared identity dimension independently, then
checks real cross-process lookup, failure, relocation, corruption, and
concurrency behavior. Cold and warm results are compared byte-for-byte rather
than inferred from a hit counter.

- [IRHash: Efficient Multi-Language Compiler Caching by IR-Level Hashing
  (USENIX ATC 2025)](https://www.usenix.org/conference/atc25/presentation/landsberg)
  evaluates its cache over histories of multiple real projects and publishes
  a reproducible artifact. Salicin adopts the same evidence principle at its
  smaller current scope: an executable matrix is part of the compiler suite,
  not an informal list of intended hash inputs.
- [Does Functional Package Management Enable Reproducible Builds at Scale?
  Yes (2025)](https://arxiv.org/abs/2501.15919) distinguishes rebuildability
  from bitwise reproducibility across 709,816 package rebuilds. Salicin
  therefore separately proves successful warm reuse and exact LLVM-byte
  equality; one is not treated as evidence for the other.
- [Verifiable Provenance of Software Artifacts with Zero-Knowledge
  Compilation (2026)](https://arxiv.org/abs/2602.11887) treats source,
  compiler, and output binding as distinct provenance obligations. Salicin
  does not claim cryptographic provenance, but its local identity tests
  explicitly bind compiler version, all source layers, host target, and
  artifact validation before reuse.
- [The Promise and Reality of Continuous Integration Caching (EASE
  2026)](https://conf.researchr.org/details/ease-2026/ease-2026-research-papers/64/The-Promise-and-Reality-of-Continuous-Integration-Caching-An-Empirical-Study-of-Trav)
  reports corrupted and outdated cache state as recurring maintenance
  failures. The acceptance suite damages metadata and payloads and verifies a
  diagnosed miss plus clean replacement, rather than merely testing ideal
  hits.

### Structured Diagnostics Are Compiler Data

Reviewed on 2026-07-30 for the transport-independent LSP diagnostics
baseline. Resolver and semantic producers now carry document identity and
source provenance as data. Human-readable rendering is a terminal boundary,
not an input to editor analysis. A missing source construct remains an absent
range rather than a plausible-looking byte-zero fallback.

- [Language Server Protocol 3.18](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#diagnostic)
  standardizes diagnostic range, severity, and code as protocol fields.
  Salicin retains the same distinctions before transport serialization and
  keeps UTF-16 conversion in the source-index boundary.
- [Detecting Bugs in Rust Compiler Fix Suggestions via
  Constraint-Violation-Guided Mutation (FSE
  2026)](https://conf.researchr.org/details/fse-2026/fse-2026-research-papers/91/Detecting-Bugs-in-Rust-Compiler-Fix-Suggestions-via-Constraint-Violation-Guided-Mutat)
  reports that most studied rustc suggestion faults arise in language-specific
  semantic components rather than generic diagnostic formatting. Salicin
  therefore attaches origins where parsing, resolution, and nominal semantic
  checks create failures instead of attempting recovery in the editor layer.
- [Compiler-Guided Inference-Time Adaptation: Improving GPT-5 Programming
  Performance in Idris (2026)](https://arxiv.org/abs/2602.11481) finds local
  compiler feedback substantially more effective than documentation-only
  guidance for a low-resource language. Stable machine fields and honest
  provenance keep that feedback usable by editors and automated consumers
  without making generated prose part of the API.

### Live Analysis Publishes Immutable Current Snapshots

Reviewed on 2026-07-30 for the versioned workspace-session baseline. Open
buffers now overlay caller-owned baseline text, every successful mutation
creates a new immutable revision, and completed analysis crosses an exact
session/revision gate. This separates correctness under concurrency from
future incremental-performance work: recomputing a snapshot may be slow, but
an old result cannot become current.

- [Language Server Protocol 3.18 text-document
  synchronization](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md)
  requires open/change/close synchronization as one capability and carries
  document versions through synchronization and diagnostic publication.
  Salicin keeps those versions in snapshot results before adding JSON-RPC.
- [Live Feedback through Incremental Program Analysis (JOT
  2026)](https://doi.org/10.5381/jot.2026.25.1.a6) describes precise live IDE
  feedback as analysis over evolving program state. Salicin first makes each
  state immutable and publication-safe; automatic incrementalization remains
  a later optimization rather than a prerequisite for correct versioning.
- [Incr: Faster Re-Execution via Bolt-On Incrementalization (OSDI
  2026)](https://www.usenix.org/conference/osdi26/presentation/xie-yizheng)
  separates dependency-aware reuse from behavioral equivalence of the base
  execution. Salicin likewise retains complete ordinary analysis as the
  authoritative snapshot result while establishing the identity boundary
  future reuse must preserve.

### The Transport Preserves the Snapshot Boundary

Reviewed on 2026-07-30 for the minimal stdio language-server transport.
`salic lsp` selects one ordinary compiler target before serving messages,
advertises full-document synchronization, and routes every accepted edit into
the versioned in-memory session. Framing is bounded and lifecycle state is
explicit. No transport notification writes a source file, and the later
diagnostic publisher must still cross the existing snapshot gate.

- [Language Server Protocol
  3.18](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)
  defines `Content-Length` JSON-RPC framing, initialize/shutdown/exit
  ordering, UTF-16 as the default position encoding, and open/change/save/close
  document synchronization. Salicin advertises exactly the implemented
  full-text mode rather than accepting range edits it cannot preserve.
- [Mechanically Translating Iterative Dataflow Analysis to Algebraic Program
  Analysis (OOPSLA 2026)](https://doi.org/10.1145/3798216) derives
  compositional analyses suitable for frequent program changes. That supports
  a later incremental layer, not coupling protocol mutation to analysis
  internals; Salicin retains a small transport whose semantic output is a new
  snapshot revision.
- [Code Less to Code More: Streamlining Language Server Protocol and type
  system development for language families (JSS
  2026)](https://doi.org/10.1016/j.jss.2025.112554) emphasizes reuse between
  language semantics and editor services. Salicin follows that separation
  manually: the server reuses the compiler's editor/session API and adds no
  second lexer, parser, or type system.

## Review Gate

Before extending the static language:

1. Record the new construct's Sort and normalization rule.
2. State whether equality remains decidable and how ambiguity is diagnosed.
3. Prove or test phase separation: runtime effects and storage cannot leak into static evaluation.
4. add positive, rejection, substitution, and nontermination/complexity tests.
5. Compare the change with primary literature or current production-language specifications and
   date this ledger.

Research review does not replace implementation evidence. A proposal is only part of Salicin once
the grammar, semantic IR, diagnostics, tests, and specification agree.
