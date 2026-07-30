# Changelog

Salicin follows semantic versioning while the compiler is experimental. During 0.x, minor releases
may extend or tighten source semantics; patch releases preserve semantics within the implemented
subset.

## Unreleased

## 0.248.0 - 2026-07-30

- Added compiler-owned `constraint` and `declaration` static fragment sorts.
  They cannot have defaults, be supplied explicitly by source, appear as
  runtime types, lower to runtime storage, or participate in user equality.
- Core-bundle validation now requires the exact public abstract `sort(2)`
  declarations for both fragment classifiers. Parser and semantic diagnostics
  distinguish the two fragment kinds instead of treating them as ordinary
  named sorts.
- Recorded the phase boundary for the following contract-elaboration work:
  the existing `test` declaration remains an incomplete legacy contract, and
  `extend` and `requires` do not gain decorative runtime-callable stand-ins.

## 0.247.0 - 2026-07-30

- Completed LSP-5 by separating protocol input, analysis, and publication.
  The server now reads cancellation and document updates while analysis runs,
  coalesces queued snapshots, discards late stale completions, and completes
  pending semantic-token requests exactly once with `RequestCancelled` or
  `ContentModified`.
- Added editor-independent JSONL transcript replay against real stdio
  processes, including restart, Unicode, multiple documents, semantic tokens,
  stale versions, recoverable invalid JSON, malformed/duplicate/oversized/
  truncated frames, and clean shutdown. A deterministic worker barrier proves
  cancellation and stale-result suppression without timing assumptions.
- Completed the LSP diagnostics baseline and promoted syntax declaration
  contracts to the active milestone, with static constraint/declaration
  fragment sorts and contract-driven
  `test`/`extend`/`requires` elaboration; updated contracts, architecture,
  status, roadmap, TODO, and the current research ledger. Semantic navigation
  remains the accepted following milestone.

## 0.246.0 - 2026-07-30

- Completed LSP-4 by analyzing every accepted workspace revision and
  publishing per-document diagnostics with exact file URIs, open-document
  versions, UTF-16 ranges, stable compiler codes, and explicit
  lexer/parser/resolver/semantic phase data. Empty publications clear repaired
  diagnostics; unlocated workspace failures are logged without invented
  ranges.
- Advertised and implemented `textDocument/semanticTokens/full` using the
  compiler's ordered token model, a stable six-kind legend, UTF-16 delta
  encoding for Unicode identifiers and non-BMP strings, and exact
  session/revision result IDs. Requests never reuse analysis from a
  superseded snapshot.
- Added multi-file and spawned-process protocol coverage for all diagnostic
  phases, URI/version routing, parser-error repair, semantic-token requests,
  Unicode lengths, clean shutdown, and byte-identical source files. Updated
  the transport contract, architecture, status, roadmap, TODO, and current
  research ledger; LSP-5 is next.

## 0.245.0 - 2026-07-30

- Completed LSP-3 with `salic lsp` over standard input/output, bounded and
  validated `Content-Length` JSON-RPC framing, initialize/shutdown/exit
  lifecycle enforcement, UTF-16 negotiation, and explicit failure exit
  behavior for premature EOF or `exit`.
- Added ordinary CLI package, workspace, binary, and library target selection
  before server startup. Full-text open/change/save/close notifications now
  update strictly versioned in-memory overlays; save advances the baseline,
  close restores it, stale changes produce protocol log messages, and no
  synchronization operation writes source files.
- Added unit and real-process transcript coverage for malformed framing,
  Unicode file-URI round trips, lifecycle capabilities, stale versions,
  workspace package selection, complete synchronization, clean shutdown, and
  byte-identical on-disk source. Updated transport/snapshot contracts,
  architecture, status, roadmap, TODO, and the current research ledger;
  LSP-4 is next.

## 0.244.0 - 2026-07-30

- Completed LSP-2 with a transport-independent `WorkspaceSession` that
  overlays strictly increasing, full-text open document versions on a
  caller-supplied resolved source graph without reading or writing files.
- Added immutable `(session, revision)` workspace snapshots that can be
  analyzed on worker threads, preserve ordered per-document versions, restore
  the latest baseline on close, and consume/drop completed results from
  another session or superseded revision before publication.
- Added rejection contracts and tests for duplicate, unknown, already-open,
  not-open, equal-version, and older-version operations; proved rejected
  operations do not advance revisions and in-memory changes leave real source
  files byte-identical. Updated the snapshot contract, architecture, status,
  roadmap, TODO, and current research ledger; LSP-3 is next.

## 0.243.0 - 2026-07-30

- Completed LSP-1 with structured resolver and editor diagnostics carrying
  document identity, phase, error severity, stable phase code, message, and an
  optional exact UTF-8/UTF-16 range. Added `analyze_document_at` for named
  buffers and retained rendered strings only at existing terminal-facing
  compatibility boundaries.
- Removed resolver message parsing, byte-zero/first-token fallback ranges, and
  range-precision metadata. Item declarations, imports, name resolution, and
  nominal C-layout validation now retain production-time source provenance;
  document-wide failures honestly omit a range.
- Clarified that source-backed `core.test` accepts only a unit-returning
  `throwing(string)` action, removed the final stale boolean signature, and
  documented why declaration-forming `extend` and constraint-guard
  `requires` are grammar contracts rather than ordinary callable
  declarations. Updated the editor contract, architecture, status, roadmap,
  TODO, and current research ledger.

## 0.242.0 - 2026-07-30

- Completed INCR-6 and the Persistent Incremental Builds milestone with an
  executable identity and invalidation matrix covering input/artifact schemas,
  compiler and host identity, core/alloc/std embedded sources, package
  providers and dependency aliases, module paths, exact source bytes, target
  modes, test selection, and checkout relocation.
- Added multi-process CLI proofs for byte-identical cold/warm LLVM IR,
  relocated checkout hits, binary/library/test separation, canonical
  incompatible metadata replacement, failed-compilation zero publication,
  and eight concurrent readers observing only one complete warm artifact.
- Extended storage and fingerprint unit tests for canonical schema/compiler
  incompatibility, module-path changes, embedded bundle changes, and every
  environment identity dimension. Updated the cache/input contracts,
  architecture, status, research ledger, TODO, and roadmap, promoting LSP
  Diagnostics Baseline to the active milestone.

## 0.241.0 - 2026-07-30

- Completed INCR-5 with `--no-cache` lookup/publication bypass and
  `--cache-trace` stderr-only reporting of stable target fingerprints, hits,
  structured misses, disabled roots, and publication outcomes.
- Added `salic cache clean`, which validates the root ownership marker,
  atomically detaches only the compiler-owned `llvm-ir` namespace, preserves
  unrelated root data, rejects symbolic-link and non-directory namespaces,
  and treats a missing namespace as an idempotent success.
- Added storage and CLI acceptance tests for cold/warm/bypassed traces,
  stdout isolation, scoped flag parsing, idempotent cleanup, unowned-root
  refusal, unrelated-data preservation, and symbolic-link non-traversal.
  Updated the cache contract, architecture, status, TODO, roadmap, and current
  research ledger; INCR-6 is next.

## 0.240.0 - 2026-07-30

- Completed INCR-4 by connecting validated whole-graph LLVM-IR cache entries
  to `emit-ir`, `build`, `run`, and `test` after complete source resolution
  and fingerprinting; native linking remains outside the cache and `check`
  remains an unconditional source analysis.
- Upgraded artifact schema 2 to preserve ordered selected test names and their
  length-framed digest, keeping `test --list` and failure-index reporting
  correct on warm hits. Added CLI proofs that output paths share binary IR and
  that test listings are recovered from validated cache metadata.

## 0.239.0 - 2026-07-30

- Implemented INCR-3 as an independent persistent LLVM-IR cache storage API:
  absolute platform-root resolution, atomic ownership markers, private
  compiler-owned directories, sharded content-addressed lookup, canonical
  metadata, exact payload length/SHA-256/UTF-8 validation, and structured miss
  reasons.
- Added atomic directory publication, valid-winner reuse, corruption
  quarantine/replacement, abandoned staging cleanup, symbolic-link rejection,
  and lock-free concurrent writer behavior.
- Added storage acceptance tests for missing entries, malformed metadata,
  truncated payloads, incompatible identities, non-regular files, invalid
  roots and markers, symlinks, warm reuse, repair, and eight-way concurrent
  publication; updated TODO, roadmap, status, architecture, cache contract,
  and the current research ledger. INCR-4 pipeline integration is next.
- Recorded the declaration-consistency gap between the existing public `test`
  syntax contract and parser-owned `extend`/`requires` special forms as a
  design candidate requiring explicit declaration-former and constraint-guard
  sorts before source-defined contracts can replace parser ownership.

## 0.238.0 - 2026-07-30

- Accepted the artifact-schema-2 persistent whole-graph LLVM-IR cache
  contract, including platform root selection, ownership, sharded
  content-addressed layout, strict canonical metadata and payload validation,
  atomic publication, concurrent writers, corruption-as-miss behavior,
  bypass semantics, and explicit non-goals.
- Upgraded incremental input identity to schema 2 so test filter presence and
  exact UTF-8 bytes invalidate test-runner IR; added executable tests for
  selection sensitivity and output-path-independent cache mapping.
- Updated the active TODO, roadmap, status, compiler architecture, and
  research ledger using current incremental-computation, compiler-cache, and
  reproducible-build research. INCR-3 storage implementation is next.

## 0.237.0 - 2026-07-30

- Completed STD-3 and the standard-library usability milestone with a
  multi-module `examples/inventory` command that parses arguments, processes
  arrays, slices, vectors, and Unicode text, formats deterministic output,
  and writes through explicit file/stdout authority.
- Added source assertions plus native success, malformed/missing input,
  repeat-output, file, early-exit, and replacement-allocator coverage;
  allocations balance after both normal return and `throwing(string)`.
- Updated the research ledger, status, standard-library conformance, README,
  TODO, and roadmap, promoting persistent incremental builds to the active
  milestone.

## 0.236.0 - 2026-07-30

- Replaced boolean-returning test registrations and the dedicated
  `core.testing.failure` effect with one canonical callable contract:
  `with(core.error.throwing(core.string.string))((): ())`.
- Migrated all source test fixtures to unit-returning assertion bodies;
  failures now always throw an owned UTF-8 message, while normal `()` return
  passes.
- Simplified `core.testing.outcome`, `std.test`, diagnostics, runner coverage,
  grammar, specification, status, TODO, and test-support documentation around
  the ordinary typed throwing abstraction.

## 0.235.0 - 2026-07-30

- Completed TEST-3 with source-order `salic test --list`, case-sensitive UTF-8
  substring filtering, successful empty selections, and one-runner execution
  of the selected registrations.
- Added stable passed/failed/selected summaries, explicit exit behavior,
  package-wide duplicate registration-name diagnostics, and primary-package
  dependency isolation for listing and filtering.
- Kept selection deterministic and free of hidden history-based
  prioritization; updated the test-support contract, specification, status,
  roadmap, TODO, CLI help, and research ledger.

## 0.234.0 - 2026-07-30

- Completed TEST-2 with source-backed `std.test.fail`, boolean, equality,
  inequality, and common `option`/`result` expectation helpers.
- Added static comparison and assertion-formatting bounds, single evaluation
  of equality operands, deterministic owned diagnostic messages, and exact
  runner output without compiler-generated names.
- Renamed the private runner bridge module to the documented public
  `std.test` module and retained the compact declaration syntax for pure
  functions; only effectful declarations use the callable-type/body colon
  boundary.

## 0.233.0 - 2026-07-30

- Completed TEST-1 with the source-backed `core.testing.failure` effect,
  normalized `outcome`, and one-shot `run` handler; boolean `false` remains an
  unmessaged migration adapter into the same model.
- Replaced first-failure exit-code encoding with source-order interpretation
  of every registration and a validated schema-1 pipe carrying pass/fail
  records plus exact optional UTF-8 messages.
- Added all-failure CLI reporting, later-registration execution, unrelated
  effect rejection, exactly-once abort cleanup, empty/absent/Unicode message,
  malformed frame, dependency isolation, and ordinary-build coverage.

## 0.232.0 - 2026-07-30

- Accepted the structured test-support contract: a source-backed one-shot
  failure effect carries an owned optional formatted message and is
  interpreted independently for every registration after exactly-once
  cleanup.
- Fixed `false` as a migration adapter into the same outcome model and a
  dedicated, framed runner pipe as the authoritative multi-result channel;
  program stdout/stderr and process exit codes are not report protocols.
- Refined TEST-1 implementation and evidence requirements using current
  effect-handler, region, and destructor-safety research.

## 0.231.0 - 2026-07-30

- Implemented validated file open/create options, unique owned file handles,
  consuming close, deterministic once-only cleanup, short and exact/all
  reads/writes, flush, and start/current/end seek under `std.io.io`.
- Added byte-exact UTF-8 path handling with embedded-NUL rejection plus bounded
  `read_file` and complete `write_file` helpers.
- Added native coverage for create, truncate, append, create-new collision,
  missing and invalid paths, allocation limits, seek, flush, static authority,
  non-copyability, and explicit close.

## 0.230.0 - 2026-07-30

- Implemented synchronous stdin, stdout, and stderr byte primitives plus
  exact/all progress helpers, UTF-8 line input, text printing, stderr
  counterparts, and explicit unbuffered flush points under `std.io.io`.
- Added lossless process-argument bytes and checked UTF-8 argument collection;
  native entry points now preserve `argc`/`argv`.
- Added portable Unix error mapping and recoverable broken-pipe behavior by
  ignoring `SIGPIPE`, with native coverage for EOF, invalid UTF-8 arguments,
  stdout/stderr separation, and `EPIPE`.

## 0.229.0 - 2026-07-30

- Accepted the synchronous host I/O contract covering exact `io` authority,
  native entry handling, portable errors, byte/text boundaries, partial
  progress, interruption, owned handles, once-only close, and the supported
  Linux/x86-64 and macOS/arm64 matrix.
- Added the compiler-validated `std.io.io` effect plus allocation-free
  `io_error_kind` and `io_error` declarations. Native `main` may expose only
  this exact standard host effect; same-named user effects remain unhandled.
- Fixed the evidence required of upcoming console, process, and filesystem
  implementations and recorded the 2026 capability/effect/resource-safety
  research basis.

## 0.228.0 - 2026-07-30

- Made `with(E)(F)` the canonical effect-row constructor for callable type
  `F`, applying one normalized row to the complete multi-group callable and
  rejecting non-callable operands.
- Added the effectful declaration boundary syntax
  `let f(comptime e: effects): with(e)(value: i32): i32`, while preserving the
  compact `let f(value: i32): i32` spelling for pure functions.
- Migrated core, alloc, std, examples, fixtures, formatter coverage, grammar,
  specification, and effect documentation to prefix syntax. Edition 2026
  retains postfix parsing only as a transitional, non-canonical migration
  path.

## 0.227.0 - 2026-07-30

- Completed ITER-1 with source-backed `find`, `position`, `contains`, `any`,
  `all`, and `fold` shared by arrays, slices, and vectors through one slice
  kernel.
- Preserved increasing-index order, first-result short-circuiting, empty-input
  identities, source-retaining `find` borrows, and move-only fold accumulator
  cleanup. Membership is explicitly limited to copyable equality.
- Forwarded predicate and fold callback effect rows exactly, with static
  custom-effect coverage and no implicit allocation or authority.
- Added native array/slice/vector, resource-cleanup, empty,
  borrow-escape, mutation-conflict, and copy-bound coverage, plus the accepted
  collection-algorithms contract and current 2026 research basis.

## 0.226.0 - 2026-07-29

- Added the compile-time `constraint` sort and non-associative `is` relation.
- Added compiler-owned `requires(...)` guards for extension implementations and
  generic function bodies, such as `requires(t is copyable)`.
- Replaced colon-style predicates with `is` goals, separate projection
  equalities such as `t is iterator && t.item == i32`, and call-shaped trait
  prerequisites such as `trait(requires: self is movable)`.
- Satisfied guards provide solver evidence to their bodies, while false
  concrete goals reject instantiation.
- Completed COLL-4 with copyable `vec.extend_from_slice`, `fill`,
  equal-length `copy_from`, and overlap-safe `copy_within`, complementing the
  existing checked access and ownership-transferring vector operations.
- Reserved complete extension capacity before initializing the tail, advanced
  vector length only after each initialization, reused prevalidated slice-copy
  kernels, and statically rejected self-aliasing and move-only slice copies.
- Documented allocation termination, partial progress, overlap, and move-only
  behavior, with native success, empty, trap, alias, and constraint coverage.

## 0.225.0 - 2026-07-29

- Completed COLL-3 with common array and slice `swap`, `reverse`, `fill`,
  equal-length `copy_from`, and overlap-safe `copy_within` operations.
- Prevalidated every index, range, destination extent, and source length
  before mutation. Resource reordering moves without copying or dropping,
  while copy operations remain restricted to `copyable` elements.
- Added the access-preserving unsafe `raw_slice_ptr` core intrinsic used by
  the safe mutation kernel, and fixed concrete slice method API boundaries to
  narrow with private element types.
- Added native forward/backward overlap, empty-range, resource cleanup, bounds
  trap, alias rejection, copy-bound, and intrinsic authority coverage.

## 0.224.0 - 2026-07-29

- Completed COLL-2 with direct shared and mutable borrowed iteration for
  arrays as well as slices. `array.iter` reuses the access-preserving slice
  iterator and no longer requires elements to be copyable.
- Bound every yielded element borrow to the corresponding mutable `next`
  borrow. A live yield prevents another advance, each iterator retains its
  source loan, and no yielded borrow can escape a local array.
- Added native resource and mutable-array iteration coverage plus negative
  coverage for source mutation, overlapping mutable yields, and local escape.

## 0.223.0 - 2026-07-28

- Completed COLL-1 with a common `len`, `is_empty`, checked `get`, trapping
  `at`/index, and first/last vocabulary across fixed arrays, borrowed slices,
  and vectors. Access defaults to shared and accepts explicit mutable access.
- Added source-backed generic inherent array extensions and a validated
  array-to-slice intrinsic that retains the source access and borrow region.
  Checked misses validate bounds before constructing an element borrow.
- Added native coverage for successful, empty, missing, mutable, and trapping
  access, including mutation through array and vector views.

## 0.222.0 - 2026-07-28

- Completed FMT-2 with strict radix-2-through-36 `u64`/`i64` parsing,
  structured first-byte failure offsets, pre-operation overflow checks, and
  exact support for both 64-bit extrema. Signs, prefixes, whitespace,
  separators, and trailing bytes are accepted only where the concrete API
  explicitly permits them.
- Added effect-parameterized scalar/ASCII text writers and canonical decimal
  display/debug implementations for 64-bit and 128-bit signed and unsigned
  integers, lowercase booleans, Unicode scalars, `str`, and `string`.
  Formatting remains source-defined and forwards writer effects without
  reflection or implicit I/O.
- Added allocation-backed `alloc.string.string_writer`, including capacity
  reservation, incremental inspection, scalar-safe appends, and ownership
  transfer on completion. Native coverage exercises radix parsing, overflow
  offsets, integer extrema, booleans, and non-ASCII output.

## 0.221.0 - 2026-07-28

- Completed FMT-1 by accepting the source-backed parsing and formatting
  contract and adding `core.fmt` declarations for pure `parse`,
  effect-parameterized `text_writer`, and statically dispatched `display` and
  `debug`.
- Formatting forwards the writer's exact effect row and neither reflects on
  values nor acquires implicit I/O. Parsing selects a structured associated
  error and returns `result` without allocation or failure effects.

## 0.220.0 - 2026-07-28

- Completed TEXT-3 with borrowed `str` byte and Unicode-scalar iterators,
  scalar counting, and checked scalar lookup. Both iterators retain the source
  loan and yield copied values, so safe iteration cannot expose writable bytes
  or outlive its text.
- Scalar iteration decodes the already-validated UTF-8 representation without
  an invalid-input branch and advances by each scalar's canonical byte width.
  Exhausted iterators and out-of-range scalar lookup return `none`.

## 0.219.0 - 2026-07-28

- Completed TEXT-2 with checked owning `string.substring`, lexicographic
  `str`/`string` ordering, prefix and suffix checks, containment, and
  first-match search. Owning extraction copies only after both byte endpoints
  pass the shared UTF-8 boundary contract.
- Search returns UTF-8 byte offsets, matches an empty needle at zero, and
  returns only scalar boundaries. The owning APIs delegate to the same
  allocation-free `str` contracts instead of maintaining parallel search
  behavior.
- Scoped the built-in division LLVM regression assertion to its target
  function body, so unrelated library trap paths cannot perturb its
  instruction-order check.

## 0.218.0 - 2026-07-28

- Added owning `string` construction through `new`, `with_capacity`,
  `from_str`, and `from_unicode_scalar`, plus byte-capacity inspection and
  reservation. Copy construction accepts only validated borrowed `str`;
  scalar construction emits the canonical one-to-four-byte UTF-8 encoding.
- Added safe `push`, `push_str`, `truncate`, and `clear`. All public mutation
  transitions preserve valid UTF-8: append inputs already carry the text or
  scalar invariant, and truncation rejects out-of-range and continuation-byte
  offsets without changing the string.
- Static literal strings now detach into allocator-owned storage on their
  first growth. Reallocation copies initialized bytes before releasing the old
  allocation, while clear and successful truncation retain capacity.

## 0.217.0 - 2026-07-28

- Completed TEXT-1 with ownership-preserving UTF-8 conversion in
  `alloc.string`. Valid `vec(u8)` storage transfers into `string` without
  copying, while `from_utf8_error` retains the rejected vector and reports the
  valid-prefix length, including truncated sequences.
- Added consuming `string_into_bytes`: heap-backed strings transfer their
  allocation to `vec(u8)`, while static literal storage is copied into a new
  owned vector. Empty zero-capacity vectors are released normally instead of
  being confused with static string storage.
- Added package-private vector raw-parts adapters and unsafe core string
  raw-parts contracts. Owning strings now run source-defined drop glue exactly
  once for heap storage; static strings never deallocate their literal global.
- Raised the explicit generic-function instance budget from 256 to 512 so the
  complete fixture corpus and the new allocation adapters compose in one
  compilation without relaxing the separate nominal-layout budget.

## 0.216.0 - 2026-07-28

- Added `str.is_char_boundary` and checked `str.get(start, end)` subviews,
  including empty and one-past-end views. Successful subviews preserve the
  source region and loan; invalid, reversed, out-of-range, or continuation-byte
  endpoints return `none` before a view is formed.
- Added exact byte-wise `str` and `string` equality without normalization,
  case folding, or allocation. Extended operator dispatch and trait
  implementation collection so borrowed dynamically sized `str` operands use
  the source-defined equality protocol.
- Introduced the unsafe internal `raw_subview` projection shared by text and
  ordinary slices. It offsets the existing fat view while preserving its
  element type, access, region, and source loans; UTF-8 validation remains in
  the safe `str` wrapper.

## 0.215.0 - 2026-07-28

- Added the dynamically sized immutable `str` core type, a shared fat-view ABI,
  checked allocation-free UTF-8 validation, `string.as_str`, and
  `str.as_bytes`, `len`, and `is_empty`. Validation covers the exact Unicode
  well-formed sequence boundaries, including overlong, surrogate, truncated,
  and above-`U+10FFFF` rejection.
- Preserved reference origins and source loans across raw `slice(u8)`/`str`
  view casts, reference-bearing `option` construction, and match payload
  binding. Local text views cannot escape, live views block writes to their
  bytes, and safe code cannot invoke the representation cast.

## 0.214.0 - 2026-07-28

- Accepted the runtime text contract for immutable static literal linkage,
  loan-retaining borrowed UTF-8 views, Unicode scalar validity, validation and
  byte boundaries, exact byte equality, ownership-preserving conversion
  failure, and byte/scalar iteration. Added source-backed `unicode_scalar`
  with checked `u32` construction, numeric projection, code-point equality,
  and exact one-to-four-byte canonical UTF-8 length.

## 0.213.0 - 2026-07-28

- Added `core.numeric` with source-declared integer `min`, `max`, `clamp`, and
  three-way `sign`; total same-width unsigned `magnitude`; and explicit
  `checked_into(output: integer)()` conversion returning `option`. CTFE and
  LLVM lowering agree on every signed and width boundary without overflow
  flags, implicit truncation, or backend undefined behavior.
- Tightened the canonical-module rule for embedded `std`: both private
  shortcut aliases and public mirror re-exports are rejected, and
  `std.async` now references its `core.async` contracts directly.
- Unified the runtime owning `string` under `core.string`, added explicit sort
  universes, and made array and string literal construction dispatch through
  source-declared traits.
- Split oversized compiler phases into focused parser, cleanup, async,
  expression, call, extension, and trait modules without changing their
  semantic boundaries, and parallelized the heaviest compiler corpus checks.

## 0.212.0 - 2026-07-27

- Reworked the embedded library dependency architecture as
  `core ← alloc ← std`. The freestanding `core` no longer owns optional
  algebraic or higher-kinded functional abstractions; `std.algebra` and
  `std.functional` now own those protocols and the `option`/`result`
  implementations. The concrete `spin` executor moved to `std.async`, while
  the allocation-free `future` and `executor` contracts remain in `core`.
- Made `std` a validated source-owning layer rather than an alias-only facade.
  Public definitions receive ordinary `std` identities, private lower-layer
  aliases may support implementations, and public mirror aliases are
  rejected. Removed the one-file-per-re-export mirror tree; callers now use
  canonical `core`, `alloc`, or genuinely owned `std` paths.
- Added allocation-free `option` and `result` inspection, region-preserving
  borrowed views, mapping and chaining, eager and lazy fallback, error
  mapping, and value projection helpers. Borrowed enum matching now inspects
  discriminants without moving payloads, and callback effects are forwarded.

## 0.211.0 - 2026-07-27

- Added the edition-matched `library/std` source bundle above `core` and
  `alloc`. The compiler now derives the mounted `std` namespace from explicit
  public source aliases, preserves each lower-layer canonical identity, and
  rejects definitions, private or duplicate aliases, non-library targets, and
  unresolved targets instead of granting authority by name or shape.
- Included the `std` source bundle in semantic preprocessing and incremental
  fingerprints, reserved its namespace for every package, and made the
  initial host boundary explicit: Linux/x86-64 and macOS/arm64 are supported,
  while other host pairs receive a target-specific diagnostic.
- Replaced category-encoded standard effect names with semantic behavior
  names: `throwing`, `suspension`, `unsafety`, `loop_exit`, `iteration_skip`,
  and `function_exit`. Embedded public library names now require strict ASCII
  `snake_case` and reject declaration-category suffixes such as `_effect`,
  `_trait`, and `_type`; ordinary user declarations remain unrestricted by
  this library policy.

## 0.210.0 - 2026-07-27

- Accepted the initial standard-library usability contract: edition-matched
  `core`, `alloc`, and host `std` layers; uniform `snake_case` public naming;
  a deliberately small prelude; consistent ownership and borrowing modes;
  explicit `io` authority separated from `result`-based host errors; portable
  error families; an initial Linux/macOS target boundary; and the minimum
  text, collection, conversion, formatting, I/O, and test API matrix.
- Synchronized the language, ABI, async, status, and standard-library
  documentation with the implemented all-`snake_case` source model and
  explicit `comptime` binders, removing the remaining pre-v0.209 spellings.

## 0.209.0 - 2026-07-27

- Replaced declaration-shaped extension headers with compiler-call syntax:
  inherent members use `extend(target) { ... }` and trait implementations use
  `extend(target, trait) { ... }`. The trailing block is the implementation
  argument. Generic extension parameters are now inferred by destructuring the
  target against its type-constructor signature, including curried,
  cross-module, `usize`, and finite-sort parameters; the former explicit
  compile-time header and colon form are rejected.
- Accepted the composite CTFE contract. It defines the runtime-typed value
  domain, phase rules, strict evaluation order, pure-call eligibility,
  resource exclusion, structural equality and normalization, target-width
  integers, deterministic complexity budgets, source diagnostics, and the
  boundary from erased `StaticValue` metadata.
- Replaced the private scalar CTFE value and untyped global aggregate
  `ConstValue` with one recursive `CtfeValue`. Integers retain their exact
  runtime type, tuples and arrays remain distinct, structs retain canonical
  nominal identity, and enums retain their source variant and active payload;
  LLVM emission only encodes this already typed value.
- Completed builtin scalar CTFE for unit, `bool`, all fixed-width integers,
  `isize`, and `usize` across pure function parameters, immutable locals,
  exact-width operators, literal and irrefutable patterns, and results.
  Integers now use normalized typed bit patterns, preserving signed minima and
  the full `u128` domain; literal conversion and every arithmetic result are
  fallible and checked. An explicit 64-bit native target description replaces
  accidental dependence on Rust host pointer widths.
- Added recursive tuple and fixed-array CTFE across literals, pure function
  parameters and results, immutable locals, tuple projection, `usize`
  bounds-checked array indexing, and nested tuple patterns. Aggregate
  construction enforces deterministic 64-level nesting and 65,536-element or
  node limits. Fixed-array runtime brackets now also use `usize`, removing the
  former phase-specific `i32` index.
- Added concrete generic and non-generic struct CTFE across labeled
  construction, inferred and annotated locals, nested values, function
  parameters and results, field projection, structural equality, and nested
  destructuring. Fields evaluate in source order and normalize in declaration
  order under canonical nominal identity. Recursive validation rejects
  unsized, address-dependent, allocation-backed, recursive, and
  custom-`droppable` storage before construction. Struct patterns now also
  complete ordinary runtime `match` lowering and LLVM emission.
- Added closed enum CTFE for unit, positional, and named variants, including
  generic instances materialized on first function-body use, declaration-order
  payload normalization, nested payload binding, literal patterns, guards,
  unit short patterns, and structural equality. Standard `option` and `result`
  computations use the same evaluator. Normalized values retain source
  variant identity instead of backend discriminants, and resource exclusion
  checks every possible variant before construction.
- Generalized dependent CTFE calls to preserve direct and nested runtime
  parameter groups and labels, explicit or inferred generic substitution,
  runtime-label overload selection, statically resolved inherent and unique
  trait methods or associated functions, and canonical cross-module
  declarations. Immutable blocks, `if`, `match`, guards, and `return` execute
  only the selected path. Value-changing recursion now uses the contracted
  16,384-step and 128-active-call limits while repeated equal calls remain
  immediate cycles. Effectful, borrowing, closure, mutation, foreign,
  builtin, and unavailable-body calls remain rejected.
- Routed global initializers through the same source CTFE evaluator as
  dependent array lengths and removed the separate HIR constant interpreter.
  Globals now call eligible ordinary, generic, member, and cross-module
  functions while retaining exact composite and nominal types. Generic
  `size_of`/`align_of` calls preserve their substituted queried type for
  target-layout LLVM constants. Regression coverage proves normalized values
  and emitted constants are independent of checkout path, declaration order,
  and source-unit traversal.
- Completed the composite CTFE acceptance proof across scalar and composite
  success paths, exact consumer type mismatches, overflow, invalid shifts and
  indexes, non-exhaustive guarded patterns, generic substitution, recursive
  layouts, repeated-call cycles, 128-active-call and 16,384-step limits,
  aggregate depth/element/node limits, cross-module identity, stable
  diagnostics, byte-deterministic IR, and native composite globals. Composite
  CTFE leaves the active queue; standard-library usability is now the P0
  milestone.
- Standardized all source identifiers on `snake_case`, including types,
  traits, parameters, functions, values, modules, and sorts. Compile-time
  length and metadata binders use `usize` and `string`.
- Made the private foreign/test syntax contracts reflect their erased inputs:
  `foreign(abi_value: abi)` now receives `c` from the finite
  `abi = sort { c }`, and `test(name: string)(body)` receives its UTF-8
  registration name through the compiler-owned metadata `string` sort.
- Moved `access` from a closed runtime enum to the finite compile-time sort
  `sort { shared mut }`, with prelude aliases `mut = access.mut` and
  `shared = access.shared`. Split the former combined effect classifier into
  `effect` for exactly one nominal identity and `effects` for normalized
  zero-or-more rows; row-polymorphic APIs now bind `e: effects`.
- Replaced source `domain` declarations with compiler-owned abstract `sort`s
  and user-defined finite sorts. Constructor Sorts now retain curried group
  boundaries and every parameter Sort, and trait constructor matching checks
  that complete signature instead of flattened arity.
- Added typed `StaticValue`, trait `Constraint`/`Goal`, and projection-equation
  IR boundaries. Ordinary pure `usize`/`bool` functions can now run under
  bounded CTFE in dependent `array(t)(expression)` lengths after generic
  substitution; mutation, borrowing, runtime effects, invalid arithmetic, and
  repeated nonterminating calls are rejected.
- Replanned the post-foundation work around daily compiler usability.
  Completed composite CTFE unifies scalar static evaluation and aggregate
  global constants across builtin scalars, tuples, arrays, structs, and enums
  in bounded pure compile-time calls. Practical standard-library text,
  collection, formatting, synchronous IO, and test support is now current,
  followed by persistent caching, LSP diagnostics, semantic
  navigation, and verified registry dependencies. The executable TODO assigns
  stable tasks and acceptance gates while separating design candidates and
  deferred language features from committed work.
- Reworked formatter indentation around parser-provided source spans instead
  of keyword or first-token heuristics. Nested blocks, delimiters, declaration
  parameter groups, `where` predicates, operator continuations, trailing
  closures, and prefix `match` arms now carry their semantic layout depth;
  directly nested blocks and leading closing braces expand onto separate
  lines.
- Unified passing modifiers, control-exit effects/functions, and lexical
  `defer` under the edition lang-item registry instead of special-name
  exceptions. Added canonical private declarations for `foreign(c, ...)` and
  `test("name") { ... }`, and changed the unique compiler-definition bootstrap
  to the self-recursive `let builtin() = builtin()` contract.
- Replaced derived primitive intrinsics with ordinary core source definitions:
  boolean negation and equality, signed integer negation, and every integer
  compound-assignment implementation now build on source control flow,
  primitive operations, and ordinary assignment.
- Moved `core.async.await` to a source definition expressed as `Future.poll`,
  `Async.suspend`, and a loop. `core.async.async` remains the direct intrinsic
  that materializes anonymous future state.

## 0.208.0 - 2026-07-26

- Added schema-1 SHA-256 incremental input fingerprints and the read-only
  `salic fingerprint` command. Keys include compiler/host, target, edition,
  embedded standard-library sources, resolved provider graph, dependency
  aliases, module paths, and source bytes while excluding graph IDs, absolute
  paths, mtimes, traversal order, and output locations. Path-migration and
  precise invalidation tests establish the contract without freezing a
  persistent cache format.

## 0.207.0 - 2026-07-26

- Added strict typed lockfile consumption and reproducible lock modes.
  `--locked` requires the complete workspace/path provider graph to match the
  current manifests; `--frozen` adds a no-network contract. Missing,
  malformed, unsupported, or stale lockfiles fail without being rewritten.
  The registry snapshot, version selection, yanking, checksum, cache, and
  offline algorithm is specified while registry transport remains deferred.

## 0.206.1 - 2026-07-26

- Migrated the tracked inventory example to source-aware lockfile format 2.

## 0.206.0 - 2026-07-26

- Added rooted and virtual workspaces with explicit members, `--package`
  selection, shared build and lock roots, workspace-wide formatting, and
  workspace-complete dependency lock graphs. Lockfile format 2 records
  portable workspace/path source identities, and compiler canonical names and
  native symbols now use resolved provider source plus package name and exact
  version rather than conflating equal name/version declarations.

## 0.205.0 - 2026-07-26

- Added a transport-independent editor analysis API with exact UTF-8 token
  byte ranges, zero-based UTF-16 positions, phased lexer/parser/resolver/
  semantic diagnostics, explicit exact-versus-fallback precision, and
  multi-document source routing. Unicode, cross-file, and the complete
  failure-fixture corpus are covered.

## 0.204.0 - 2026-07-26

- Added the conservative `salic fmt` formatter for individual files and root
  packages, with `--check` and dependency isolation. Formatting preserves
  every physical line and the complete logical token stream while normalizing
  two-space brace indentation, trailing whitespace, LF endings, and final
  newlines. Token-stream and reparsing guards prevent parenthesis-free calls
  or comments from changing meaning; the complete passing fixture corpus is
  idempotent.

## 0.203.0 - 2026-07-26

- Verified the bounded C interoperability surface against the native host
  Clang ABI. Cross-language tests cover every signed, unsigned, pointer-sized,
  and 128-bit integer in parameter and return positions, plus bidirectional
  raw-pointer access to nested `struct(c)` storage containing integer,
  pointer, fixed-array, and nested aggregate fields. The new support matrix
  explicitly rejects by-value arrays and aggregates, bool, borrows, Unit
  parameters, and typed function values until target ABI lowering supports
  their required coercions and attributes.

## 0.202.0 - 2026-07-26

- Exported concrete primary-package `pub` functions and non-Unit globals with
  package-qualified native symbols carrying ABI fingerprints for parameter
  modes, normalized types, results, and effects. Stable `name@version`
  identities now replace graph-local package numbers in dependency canonical
  names; duplicate identities are rejected before code generation,
  incompatible signatures cannot silently bind, and generic specializations
  remain consumer-owned and internal. Native tests link independently emitted
  library modules and verify identity stability across dependency graph order.

## 0.201.0 - 2026-07-26

- Defined the experimental whole-program native calling convention: compile
  groups disappear, runtime groups flatten in source order, borrows pass as
  pointers, owned values transfer cleanup at call entry, direct returns
  transfer ownership to callers, effects add no hidden direct-call arguments,
  and `Throws` uses its matching `Result` return. Caller and callee now share
  Unit and borrowed-Unit parameter erasure, fixing a mismatched LLVM signature.
  Source diagnostics and tests pin unsized boundary rejection before emission.
- Audited and documented the experimental native ABI representation for
  scalars, pointers, borrows, slices, aggregates, enums, callables, effects,
  ownership modes, continuations, and effect callables. Low-level tests pin
  value, parameter, field, and return mappings; the compiler now rejects
  non-64-bit hosts explicitly because allocator and slice records currently
  use 64-bit sizes. The audit records whole-program ownership and internal
  linkage as inputs to the following call and linkage milestones.

## 0.200.0 - 2026-07-26

- Specialized recurring async `loop`, pre-test `while`, and post-test `while`
  with one residual custom-effect or `Throws` child factory per iteration.
  Cold condition gating avoids constructing a child for a false pre-test;
  Ready backedges yield before the next effectful factory. Native coverage
  verifies success, child Pending cancellation, error, handler abandonment,
  repeated iteration, and exactly-once cleanup. Nested residual iteration
  polls, effectful conditions, and move-only factory or condition carry remain
  explicit source diagnostics.
- Specialized residual child construction and polling in later sequential
  async segments. The handler owns the active child while source code runs;
  Pending returns it to parent state and Ready advances without replaying
  prior segments. Native custom-effect and `Throws` coverage verifies success,
  cancellation, abandonment, and one-shot cleanup.

## 0.199.0 - 2026-07-26

- Specialized a final non-suspending async continuation that retains a custom
  effect or `Throws` after a pure child becomes Ready. The pure transition
  destroys the completed child and packages its output with continuation
  captures and retained locals before the source poll wrapper enters the
  handler. Native coverage verifies resume, error, abandonment, Pending
  cancellation, no replay, and exactly-once child and retained-state cleanup.

## 0.198.0 - 2026-07-26

- Allowed heterogeneous residual `if` and `match` selection to retain
  continuation locals across the selected await. Each pure branch bridge now
  reconstructs the complete private `(selected child, retained...)` bundle;
  Ready and cancellation coverage verifies exactly-once cleanup for both the
  active variant and move-only retained state, while self-referential bundles
  remain rejected by structural `Move`.

## 0.197.0 - 2026-07-26

- Extended heterogeneous residual async branch specialization from direct
  two-way `if` to general direct `match`. The source selection helper
  preserves pattern payload scope and move-only selector ownership, stages
  the selected concrete child once, and enters the existing pure
  active-variant polling state.

## 0.196.0 - 2026-07-26

- Specialized direct two-way heterogeneous residual async `if` selection.
  The condition and each concrete child factory remain source-typed through
  handler specialization, while pure branch bridges initialize the private
  active-variant future. Native coverage verifies both selected branches,
  Pending/Ready repolling, cancellation, and variant-specific cleanup.

## 0.195.0 - 2026-07-26

- Specialized residual one-shot `if` and `match` selection when every
  direct-tail branch produces the same concrete child future type. Only the
  selected effectful factory runs, and Pending, Ready, cancellation, and drop
  retain exactly that child. Residual heterogeneous branch-future enums now
  receive a source diagnostic instead of reaching LLVM with unresolved effect
  operations.

## 0.194.0 - 2026-07-26

- Added shared and mutable borrowed captures to suspended residual futures.
  Stored references preserve their external regions and ordinary alias
  restrictions through cold factory evaluation, Pending, Ready continuation,
  and cancellation; destroying the future ends its loans, while access to a
  still-borrowed referent remains diagnostic.

## 0.193.0 - 2026-07-26

- Extended suspended residual async specialization through finite sequential
  awaits after the effectful first segment. Each later child may independently
  return `Pending`; Ready destroys the completed child before starting the
  next, and cancellation drops only the active child chain. Later child poll
  rows with custom effects or `Throws` remain explicitly rejected.

## 0.192.0 - 2026-07-26

- Extended suspended residual async specialization to `Copy` and move-only
  locals retained across the first await. A dedicated starting drop-state now
  preserves move-only continuation captures while an effectful child factory
  runs; success, factory error, Pending cancellation, and Ready completion
  each clean factory locals and initialized future fields exactly once.

## 0.191.0 - 2026-07-26

- Extended suspended residual async specialization from direct tail `await`
  to one await followed by an optional pure linear continuation. Custom
  effects and standard `Throws(Error)` now preserve `Pending` repoll and run
  the Ready continuation once when neither segment retains a pre-await local,
  suspends again, or captures by borrow. Retained-local and other unsupported
  shapes continue to receive source-level specialization diagnostics.

## 0.190.0 - 2026-07-26

- Added the core-private complete `builtin()` initializer for every
  compiler-owned function, type, type constructor, and intrinsic extension
  method. The unique private `let builtin(): Never = builtin()` bootstrap is
  validated separately; unknown or malformed core markers and every user
  marker are rejected. Trait requirements, effect operations, and user opaque
  types remain genuinely bodyless, and intrinsic selection no longer infers
  compiler ownership from a missing body.
- Replaced grouped `extern "C"` declarations and `@link_name` with complete
  per-declaration `foreign(c)` and `foreign(c, "symbol")` initializers.
  Omitted symbols default to the Salicin declaration name; explicit symbols
  retain bounded ASCII validation; calls still require implicit `Unsafe`.
  Foreign definitions require one runtime group and an explicit result, and
  reject compile-time parameters, explicit effects, `where` clauses, and
  bodies. Legacy `extern` and all `@` syntax now report migration diagnostics.
- Added `struct(c)` as the dedicated C data representation constructor.
  Concrete layouts accept integers, raw pointers, non-zero fixed arrays, and
  nested C structs; reject empty or representation-unstable fields with
  source-level paths; preserve target C size and alignment; and validate
  generic instances after compile-time substitution. The option composes
  independently with `derive: Copy`.

## 0.189.0 - 2026-07-26

- Specialized the first suspended residual async shape: one direct tail await
  with no retained local or continuation state and only by-value `Copy` or
  move-only captures. Custom effects and standard `Throws(Error)` now pass
  through the enclosing handler; child construction runs once, Pending
  repolls only the stored child, and completion, error, and cancellation
  paths clean transferred state exactly once.
- Taught `try` selection to discover residual `Throws(Error)` inside an async
  body before the generated future and poll method have been registered,
  allowing a future to be constructed and polled within the same handler
  action without executing the cold body early.
- Added parenthesis-free application for runtime groups containing one
  positional parameter. Ordinary functions and methods accept `f value`,
  repeated forms preserve curried groups, and a tail closure can follow the
  bare argument. Application binds above infix operators; empty,
  multi-parameter, and labeled groups retain explicit parentheses.
- Promoted the confirmed `struct(c)`, `foreign(c, ...)`, and core-private
  `builtin()` work to the milestone immediately following residual async
  polling, ahead of formatter, LSP, package, and incremental-compilation work.

## 0.188.0 - 2026-07-26

- Specialized non-suspending residual `Throws(Error)` futures through the
  ordinary standard handler path. `try { future.poll() }` now has native
  success and error coverage, including exactly-once cleanup for a move-only
  capture on throw; suspended Throws remains explicitly diagnostic.
- Specialized non-suspending custom-residual futures with shared or mutable
  reference captures. Generated state stores the reference value rather than
  a reference to its temporary source slot, normalizes resume parameters back
  to ordinary borrow channels, and preserves source-loan exclusion while the
  future is alive.
- Migrated pass fixtures to source-declared
  `test("fixture") { main() == 42 }` registrations. Compatible groups now emit
  and link one runner directly from those registrations; trap fixtures remain
  independent so every expected process failure still executes.

## 0.187.0 - 2026-07-26

- Added contextual `test("name") { ... }` registrations and `salic test`.
  Registrations are private compile-time test metadata with pure `bool`
  bodies; the selected package is compiled and linked once, runs in source
  order, and reports the first failing name. Empty targets, empty names,
  visible registrations, non-string names, non-`bool` bodies, and targets
  above the current 254-test runner limit are rejected.
- Allowed a trailing closure to supply an ordinary function's first runtime parameter group
  directly, so single-callback APIs can use `run { action() }` without an artificial empty group.
  Existing parenthesized, bare-argument, named, and successive trailing groups retain their
  established grouping.
- Added source-backed lexical `defer`, with compiler lowering that captures actions at registration
  and runs them in LIFO order after exit-value evaluation on normal completion, `return`, `break`,
  `continue`, and standard `throw` paths.
- Lowered cold async blocks without suspension to private nominal state containing an explicit
  state word and captured fields. Generated future state satisfies structural `Move`, and
  cancellation drops moved captures exactly once without executing the body.
- Registered compiler-generated cold states as real `Future((), Output = T)` implementations.
  Their first poll transfers captures and returns the standard `Poll.Ready(T)` variant; completed
  states suppress capture cleanup and trap on a second poll. Generic `Future` bounds and native
  owned-capture polling are covered by regression tests.
- Added the ordinary zero-field `core.async.Spin` executor. It owns and repeatedly polls one
  `Future(E)` until `Ready` without allocation or implicit runtime selection. Public trait methods
  now remain callable through generic dispatch even when the concrete implementation type is
  private to the caller's package.
- Inferred residual requirements from cold async bodies without suspension. `Unsafe` remains an
  ordinary poll requirement, while custom residual effects in non-suspending bodies with
  by-value `Copy` or move-only captures now specialize generated poll/resume source through an
  enclosing handler. Move-only capture fields transfer and drop exactly once. Generic `Future(E)`
  bounds infer a defaulted effect row from the concrete implementation, and effectful trait-method
  calls participate in handler inlining. Borrowed, suspended, and `Throws` residual futures remain
  pending work.
- Lowered one tail-position `await` into a real parent/child polling state machine. The first poll
  creates and stores the child future, `Pending` preserves it for the next poll, and `Ready`
  transfers the output and completes the parent. Suspended cancellation and successful completion
  each drop the child exactly once. Source implementations of `Future(())` now normalize the pure
  effect argument when checking the trait contract.
- Extended a single `await` to a linear `let value = await child` segment followed by ordinary
  continuation code. Values captured by that continuation remain in parent state across
  suspension; Ready transfers them into the continuation, while cancellation drops owned captures
  exactly once.
- Composed linear continuation segments recursively, allowing multiple sequential awaits to
  preserve earlier Ready values across later Pending states. Each parent keeps only its active
  nested future, cancellation follows that state chain, Copy captures are stored by value without
  consuming their source, and non-Copy captures transfer ownership into the future.
- Completed the typed polling and deterministic cancellation milestones for cold and linearly
  suspended futures. Control-flow suspension and residual handler specialization now remain
  explicit follow-up tasks rather than open-ended parts of the polling transition milestone.
- Retained ordinary pre-await locals in generated state when a linear continuation uses them,
  including Copy aliases and owned resources. Ready transfers those locals into the continuation;
  cancellation drops initialized retained state exactly once.
- Completed the first async borrowing boundary: a borrow chain whose local referent would occupy
  the same generated future is rejected as self-referential and non-`Move`, while external borrows
  remain region-checked and keep their ordinary alias restrictions.
- Hoisted a tail await over homogeneous `if` and `match` branches, evaluating branch selection once
  before the selected child enters the existing Pending, Ready, and cancellation state machine.
- Added compiler-owned active-variant futures for heterogeneous `if` and `match` child types with a
  common Output. Poll and cancellation borrow or drop only the selected variant, and mismatched
  branch outputs receive a source-level diagnostic.
- Allowed async `if` and `match` branches to contain linear prefixes and continuations around
  await. Branch-local owned values retain lexical drop timing across Pending, and branches without
  suspension are represented as immediate Ready futures.
- Preserved contextual `await` while parsing `while` and `do ... while` control blocks, which use
  trailing-closure surface syntax without creating a closure boundary. Loops proven to exit on
  their first entered iteration now hoist suspension into the existing state machine; false or
  suspended pre-test conditions complete without a backedge, and child and enclosing future
  outputs may differ. Recurring loops receive a source-level iteration-state diagnostic that
  identifies their suspension and backedge forms.
- Added the first reusable async loop backedge. A loop with one await and a boolean
  break/continue decision lowers to a private `Continue(next_child) | Break(Output)` step enum;
  polling reinitializes one child field across Pending and consumes consecutive immediately-ready
  iterations in an HIR loop without recursive future layout or host-stack recursion. Break outputs
  are inferred from source expressions and transfer move-only values. Completed iteration children
  are destroyed before slot reuse, and cancelling a suspended loop drops only the active child.
  Omitted `else` branches and non-suspending branch bodies execute as ordinary loop fallthrough.
  Recurring pre-test and post-test `while` loops recheck a pure condition before constructing each
  child, skip rechecks while that child is Pending, and finish directly when the condition is false.
  Iterations with multiple sequential awaits lower to one finite iteration future, infer break
  outputs from all awaited bindings, and cancel only the active nested child chain.
  Move-only continuation state transfers through `Continue(Carry)` and is restored before the next
  iteration, with exact completion and cancellation cleanup. Loops without a reachable break use
  uninhabited `Never` output.
  General unit-valued iteration bodies rewrite nested `if` and `match` control exits into early
  step returns while preserving nested loop and async boundaries.
- Declared `Continuation(Input, Output)` and `EffectCallable(Input, Output, Answer)` as bodyless
  type forms, matching their compiler-owned representations instead of describing them as empty
  structures. Lang-item validation now requires the exact type-form declarations.
- Split capability declarations by semantic ownership. `core.effect` now contains only generic
  handler infrastructure; `Throws`, `throw`, and `try` live in `core.error`; `Unsafe` and `unsafe`
  live in `core.unsafe`; and `Async`, `Poll`, `Future`, `Executor`, `async`, and `await` live in
  `core.async`. `Result` remains an ordinary data module, and `core.control` now contains only
  structural control flow.
- Added contextual parsing for `async { ... }` and prefix `await` inside async bodies. Dedicated AST
  nodes preserve their closure and suspension boundaries without reserved-call spellings.
- Added the source-backed structural `core.marker.Move` auto marker, made `Copy` inherit `Move`,
  and enforce relocation capability at owned place moves while leaving direct in-place
  initialization unconstrained. Generic `where T: Move` bounds and ordinary resource relocation
  are covered by semantic tests.
- Accepted the initial async contract: source-backed structural `Move`, cold anonymous
  `Future(E)` state machines, explicit polling and spin execution, deterministic cancellation,
  residual effect forwarding, and explicit future erasure. The first slice rejects self-referential
  suspended state through the `Move` requirement and introduces no `Pin` placeholder.
- Standardized compile-time argument diagnostics around source binders and kinds. Argument-group
  arity errors now print the expected binder schema, unresolved arguments include their kind and
  owner, unknown labels list valid binders, and type-constructor and effect mismatches distinguish
  expected kind from the supplied expression or constructor arity.
- Materialized direct capturing callable arguments after generic custom-effect parameters resolve
  to a concrete row. Handler lowering now recognizes instantiated effect parameters, reuses the
  concrete action-specialization path before CPS conversion, preserves moved captures and
  exactly-once cleanup, and rejects unrelated selected effects.
- Reorganized the documentation around single sources of truth for language semantics, grammar,
  implementation status, roadmap direction, and unfinished work. The English specification now
  reflects the current domain and region models, stale milestone and completed-design documents
  were removed, and the README and documentation index expose only active references.
- Made generic trait method binders alpha-equivalent across concrete, blanket, constructor, and
  default implementations. Method-level where predicates and associated equalities are now parsed
  and checked as part of the trait contract; implementations cannot strengthen them. Type, effect,
  `usize`, `access`, and region binders normalize by group and position, while method binders cannot
  capture implementation-header binders. Static dispatch remains monomorphized and works across
  file modules.
- Bridged capturing closure arguments into statically known source callees by specializing callable
  parameters into ordinary shared, mutable, or moved capture parameters. `Chain`, `Coalesce`, and
  higher-order calls preserve laziness, cleanup, effect rows, source evaluation order, and
  source-level diagnostics. Equivalent normalized closure shapes reuse deterministic
  specializations; native tests cover repeated mutable calls, zero/one resource invocation,
  algebraic and generic `Unsafe` effects, borrow conflicts, and escaping captures.
- Added bounded generic associated-constructor equalities in where predicates, including explicit
  binder groups, alpha normalization, transparent-alias comparison, and terminating signature
  rewriting.
- Migrated `Iterator` to the region-indexed `Item(R)` GAT and added access-preserving
  `SliceIter(A)(T)`. Shared slice iteration now yields borrows for non-`Copy` elements, mutable
  iteration yields exclusive element borrows, and each result is shortened to the active
  `next(R)` receiver borrow. Closed compile-time values such as `access` now parameterize ordinary
  nominal types through abstract validation and concrete monomorphization. Region provenance is
  retained through reference-carrying values, enum payload patterns, and reference-returning calls;
  regression coverage rejects overlapping mutable yields, source mutation while borrowed, and
  escaped iterator results.
- Accepted the minimum owning string model for LIB1. `String` will privately own `Vec(u8)` storage
  with an always-valid UTF-8 invariant, byte-based length and capacity, shared-only byte views, and
  a consuming validation error that retains the original allocation. Character scalars, borrowed
  `Str`, indexing, Unicode algorithms, and general string literal expressions remain separate
  gated designs.
- Implemented `std.string.String` and `FromUtf8Error` in ordinary allocation-library source.
  Consuming conversion validates all UTF-8 sequence boundaries, failed conversion retains the
  original `Vec(u8)` allocation, shared byte views cannot mutate the representation, and append,
  clear, reserve, and byte recovery preserve ownership and cleanup. Added `Vec.take()` to transfer
  a complete allocation while replacing its receiver with an empty vector.
- Added the multi-module native `examples/inventory` LIB1 acceptance package. It composes owning
  UTF-8 strings, recoverable invalid-byte errors, a vector of non-`Copy` products, consuming
  iteration, and user-defined valuation/summary traits, and exits with status 42.
- Lowered generic associated constructors across `type`, `access`, `region`, `usize`, and closed
  compile-time value kinds. Trait implementations may bind direct nominal constructors or partially
  applied type aliases, with exact kind checking and substitution into method templates before
  runtime lowering. Native tests cover shared/mutable borrow families and compile-time array-length
  families; constructor equality predicates remain a separate solver task.
- Added the source-backed, access-polymorphic `Index(Key)` protocol. User types, Array, Slice, and
  Vec now support bracket reads, explicit shared/mutable element borrows, and assignment through
  one `index(A)` method. Receiver and key evaluation remains single-shot, temporary loans end with
  the consuming expression, and all standard container implementations retain checked bounds.
- Completed the unsized, non-prelude `Slice(T)` core model. Region-bound Array borrows unsize to
  fat Slice borrows, `Vec.as_slice` preserves shared or mutable access through an anchored loan,
  and source-backed `len` / bounds-checked `at` methods retain regions and element ownership.
  Bare Slice storage is rejected consistently by check and compile.
- Added executable documentation fencing. Every Salicin Markdown block is now classified as a
  checked source unit, a non-standalone normative fragment, an explicitly deferred design, or an
  intentional failure; the CLI suite rejects unclassified/unterminated fences and compiles every
  checked example.
- Continued the `Analyzer` boundary split without changing data ownership: type compatibility and
  unification now live in `types.rs`, call-site effect requirements in `effects.rs`, and source-level
  diagnostic function naming in `names.rs`. Existing exact-diagnostic and deterministic-IR tests
  preserve behavior across the move.
- Closed the M0 deterministic-output gate. Repeated clean compiler builds now produce
  byte-identical diagnostics, LLVM symbol order, generated IR, and regenerated dependency
  lockfiles; handler action selection, handler result probing, and recursive generic nominal
  diagnostics no longer depend on hash-map iteration order.
- Closed the M0 clean-build quality gate. A fresh target directory passes formatting, strict
  all-target clippy, the complete `cargo test` suite, and the native ledger acceptance program
  with exit status 42.
- Added the bounded M0 C FFI import path. `extern "C"` blocks preserve validated optional
  `@link_name` symbols, admit one runtime parameter group over integer scalars and raw pointers,
  require `unsafe` at every imported call, and emit direct LLVM declarations. Native libc tests
  cover scalar and pointer round trips; diagnostics reject private ABI types, currying, unsupported
  ABIs, and duplicate or compiler-reserved symbols.
- Lowered all twelve declared runtime integer types through source identity, inference, ownership,
  target layout, operator dispatch, and LLVM emission. `isize` and `usize` track the native target
  pointer width without becoming aliases, integer literals cover `i128::MIN` and all of `u128`,
  and native/negative fixtures lock down arithmetic, layout, range diagnostics, and the absence of
  implicit widening.
- Implemented structural non-unit tuples across parsing, type inference, diagnostics, ownership,
  cleanup planning, and LLVM emission. Tuple types and literals use the frozen trailing-comma
  distinction, decimal projections are places, and tuple patterns delay non-`Copy` moves until
  their guard succeeds. Native tests cover projection, destructuring, partial moves, root rebuild,
  guarded fallback, and deterministic resource cleanup.
- Semantic diagnostics now retain defining source paths, declaration positions, and executable
  expression ranges through module resolution, generic instantiation, trait dispatch, and handler
  lowering, including local initializers and trailing source-closure calls. Single-file CLI errors
  use `path:line:column: error` formatting, all fail fixtures reject generated `$...` names, and
  internal generic types/functions render with source-level names.
- Source and region identifiers now follow Unicode XID and normalize to NFC. Native fixtures cover
  Unicode identifiers and logical newline continuation, while non-XID format characters and
  cross-script lookalike file-module names receive stable diagnostics at the UTF-8/ASCII boundary.
- Abstract compile-time domains now use `let Name: domain`; a domain with a known empty member set
  uses `let Name = domain {}`. Bare `let Name = domain` is rejected so abstract and empty domains
  cannot collapse into the same source form. Core `type`, `region`, `effect`, and `parameters`
  declarations use the abstract form.
- Added an explicit M0 conformance matrix covering positive, negative, diagnostic, and native
  evidence. The audit tracks non-unit tuples, remaining primitive scalar widths, C FFI, Unicode
  frontend evidence, and semantic source spans as release-blocking gaps rather than extensions.
- Removed declaration-value `type` forms. Opaque primitive declarations now use the abstract
  `type` domain as in `pub let i32: type`; `bool` and compile-time `access` are ordinary closed
  enums, and the legacy `let Name = type` / `let Name = type { ... }` spellings are rejected.
- Routed prefix and postfix `match` syntax through the validated heterogeneous case-pack contract.
  Each case remains a pattern partial function, while the compiler statically expands the complete
  control call before effect, throws, ownership, and cleanup analysis. `if` now forms a boolean
  case pair over the same path, and `core.control.if` has an ordinary source body implemented with
  `match`.
- Added first-class, contextually typed pattern partial functions. Applying
  `{ Pattern [if guard] -> body }` now returns `core.control.Attempt(Input)(Output)`, preserves the
  original non-`Copy` input on `Miss`, delays payload moves until `Hit`, and carries closure
  captures and latent effects through storage and calls.
- Added multi-stage partial application for curried capturing closures. Partial environments combine
  original closure captures with applied arguments, preserve remaining parameter labels and modes,
  defer effect checks until the final call, retain `FnMut`/`FnOnce` behavior, and transfer or drop
  owned captures exactly once across invocation, abandonment, and early argument exits.
- Inventoried every retained source-level algebraic-handler rejection by stable ID, implementation
  location, diagnostic fragment, negative fixture, and implementation-status boundary. Added
  coverage for continuation and callable escape, alias binding, finite-target assignment, and
  overload selection; operation effect errors now report source names such as `State(i32).get`
  instead of generated `$effect$operation$...` symbols.
- Extended the built-in `Ptr(A)(T)` family through ordinary source declarations. A generic
  `extend(Ptr(A)(T))` supplies `offset` to both access modes, while
  `extend(Ptr(mut)(T))` specializes `init` and `take` for mutable pointers. Pointer methods
  use concrete non-nominal method owners internally, retain source-level diagnostics, obey the core
  package orphan boundary, and do not require general compile-time equality predicates.
- Moved the closed `access` type into `core.borrow`, and replaced the former closed
  `passing` type and `auto` value with validated compile-time `copy(P: parameters): parameters` and
  `move(P: parameters): parameters` functions in `core.passing`. Also allowed `bool` and
  user-declared closed types in compile-time parameter groups with typed defaults and stable
  monomorphization identities. Runtime parameter prefixes now accept
  `M: (P: parameters): parameters` function parameters and compose parameter-schema modifiers
  rather than using a parser-only passing slot.
- Unified shared and mutable raw pointers under the access-parameterized
  `Ptr(A: access = shared)(T: type)` family. `Ptr(T)` remains the shared spelling,
  `Ptr(mut)(T)` selects mutable access, and the separate mutable-pointer declaration has been
  removed. Pointer constructors, raw intrinsics, alloc containers, diagnostics, and handler-owned
  recursive borrow channels now carry the selected access through the single validated lang item.
- Reduced the public allocation API to the `std.boxed.Box` and `std.vec.Vec` types plus their
  inherent methods; prefixed `box_*` and `vec_*` functions are now implementation-private.
  Replaced `Box.as_mut_ptr()` with consuming `Box.into_raw()` and unsafe `Box.from_raw(pointer)`
  ownership conversion.
- Added bounded `L: usize` compile-time parameters with non-negative literal arguments, explicit
  forwarding, array-driven inference, and monomorphization identity. Fixed arrays now use the
  validated curried `core.memory.Array(T)(L)` declaration; the old `Array(T, L)` grouping is rejected.
- Added validated `core.memory` declarations for the `Ptr(A)(T)` family, its explicit-borrow value
  constructor, `size_of(T)`, and `align_of(T)`. Pointer and layout lowering now follows validated
  lang-item identities instead of an unvalidated reserved-name list.
- Moved the borrow type and value lang-item declarations from the compile-time domain module into
  the dedicated `core.borrow` module.
- Fused eligible `Copy`-element indexed borrow arguments into non-recursive handler frames. Index
  expressions are materialized once in source order, carried across resume and abandonment, and
  bounds-checked when the element address is rebuilt from the frame-owned root. Native regressions
  cover mutation, side effects, bounds traps, and exactly-once root cleanup.
- Added a milestone-based language roadmap and a stable-ID project TODO. The roadmap bounds the
  current handler-completion work, restores M0 hardening as the next expansion gate, and defers
  async, ecosystem, and ABI work until their ownership and runtime prerequisites are met.
- Replaced the compiler-generated `for` iterator name exception with an explicit handler-owned
  closure capture policy. Generated resumable frames now move any non-`Copy` owned nominal root
  while retaining its mutability.
- Allowed closures and handler frames to capture projected assignment targets by their mutable root.
  Native regressions cover repeated operations, direct field mutation, resumption, abandonment, and
  exactly-once cleanup of user-defined nominal state.
- Fused eligible non-recursive effectful calls into their caller's handler frame when borrow
  parameters receive root locals or stable field/index paths whose mutable projections are
  statically disjoint. Distinct fields and constant indexes may share one owned root across resume
  and abandonment; identical, parent/child, and same-root dynamic-index places receive a direct
  overlapping-borrow diagnostic. Value arguments remain materialized in source order, while each
  borrowed place is shared with the following continuation instead of being borrowed and moved by
  competing frames.
- Extended borrowed-root frame fusion across concrete residual `Throws(E)`, `Unsafe`, and nominal
  effect rows. Standard `try` now encloses the complete CPS-transformed computation, preserving its
  physical `Result(E)(T)` boundary. Native regressions cover both handler orderings, success,
  failure, resumption, abandonment, and exactly-once root cleanup; unresolved effect-row parameters
  are rejected before entering the shared-root path.
- Added internal access-parameterized `Ptr` ownership channels for borrowed roots crossing direct and mutually
  recursive effectful calls. Call-graph cycle detection selects recursive frames, while base
  returns, resumption, abandonment, and each one-shot environment retain exactly-once cleanup.
- Allowed an unsafe local raw-pointer dereference to serve as a field or constant-index place base.
  A selected Copy field can be read without copying its non-Copy root, and writes still require
  `Ptr(mut)(T)`.
- Staged shared and mutable root/field arguments at their original positions before materializing a
  direct reusable handler action. Explicit reference locals preserve source places and keep loans
  live through action capture and the one-shot call; overlapping mutable captures are rejected, and
  resume or abandonment releases the loans before following code.
- Completed the owned `EffectCallable` path for open runtime action parameters inside active
  handlers. Compatible zero- and one-input action parameters now move through named CPS frames and
  reusable handler boundaries via the call/drop/environment/flag ABI. Native coverage exercises
  shared, mutable, and moved captures, resumption, abandonment, and uninvoked cleanup; repeated use,
  unsupported rows/shapes, and escaping borrowed environments receive source-level diagnostics.
- Restored `examples/ledger.sc` to effectful state processing: its `for` loop now consumes
  non-`Copy` transactions and performs overdraft operations while retaining both iterator and
  mutable ledger state.
- Allowed `for` bodies to perform handled algebraic operations, including standard `Throws`.
  Compiler-generated iterator state now moves through one-shot continuation and recursive loop-frame
  environments while retaining mutability; resumption and abandonment both run iterator cleanup
  exactly once.
- Scoped generated recursion tokens to their owning closure, preventing an enclosing continuation
  from claiming an internal loop frame's recursive backedge and imposing the wrong parameters.
- Added source-backed `core.flow.Raise` and `core.flow.Unwrap` lang-item traits: postfix `value!`
  turns stored `Result` errors into `Throws`, while `value!!` force-extracts or aborts. `Option`
  and `Result` implement `Unwrap`, `Result` implements `Raise`, and user containers may implement
  either protocol.
- Routed effectful trait-method calls discovered after concrete receiver and generic-implementation
  selection back through handler lowering, so `Raise.raise` keeps its direct
  `Output with(Throws(Error))` contract instead of exposing an intermediate `Result`.
- Added the compile-time `parameters` domain and complete parameter-group expansion with `...`.
  `core.effect.Handle` now declares `Clauses(Value, Answer): parameters`, so every
  compiler-derived effect implementation has the same expanded `handle` shape as its source trait.
- Made the validated `core.unsafe.unsafe` helper an ordinary source definition over the
  zero-operation `core.effects.Unsafe` handler; only its lexical raw-operation authority remains
  syntax-directed in the compiler.
- Refactored the compiler backend into a `codegen/` module directory, splitting out cleanup-plan
  construction, LLVM emission, and codegen regression tests while preserving behavior.
- Normalized standard-library, documentation, fixture, and diagnostic snippets to write generic
  type declarations without a space before compile-time parameter groups, such as
  `let Box(T: type)`.

## 0.186.0 - 2026-07-23

- Changed `Result` to the curried constructor `Result(Error)(Value)`, making `Result(Error)` the
  standard unary constructor for higher-kinded protocols.
- Removed the now-redundant `ResultWith(Error)` adapter and its root export.
- Updated `Result` chaining, coalescing, `try`, and standard `Monad` implementations to use the
  new error-first constructor order.
- Added parsing for consecutive type argument groups in type positions, so curried type
  constructors can be written as `Result(Error)(Value)`.

## 0.185.0 - 2026-07-23

- Moved `Option(T)`, `Result(T, E)`, and `ResultWith(Error)` to the `core` root so source writes
  `use core.Option`, `use core.Result`, and `use core.ResultWith` instead of reaching through
  submodules or the prelude.
- Shrunk the edition prelude to `Never`, `Copy`, and `Drop`; `Option` and `Result` are now ordinary
  core definitions that must be imported when named.
- Made standard `Option`/`Result` chaining, coalescing, and `Monad` implementations source-backed
  around the root `core` identities.
- Replaced the remaining `throw expr` syntax with the ordinary `throw(error)` function path and
  made `do`, `try`, and `throw` real source definitions in `core.control`; only `unsafe` and `loop`
  remain bodyless compiler-authorized control contracts.

## 0.184.0 - 2026-07-23

- Added standard `Functor`, `Applicative`, and `Monad` implementations for `Option` and for the
  `ResultWith(Error)` unary type-constructor adapter over `Result(Value, Error)`.
- Added semantic support for partially applied transparent type aliases as higher-kinded
  constructor trait implementation targets, enabling source-level adapters such as
  `extend(ResultWith(Error), Monad)`.
- Exported `ResultWith` as a normal non-prelude standard-library item.

## 0.183.0 - 2026-07-22

- Changed struct declarations and value construction to the braced form: `let A = struct { ... }`
  and `A { field: value }`. Parenthesized `A(...)` is no longer a built-in struct constructor.
- Added `struct(derive: Copy) { ... }`, lowering supported derives through ordinary source-backed
  trait implementations.
- Allowed an ordinary function and a type to share the same top-level name, so libraries can offer
  explicit same-name constructor functions such as `let Pair(left: i32, right: i32): Pair = { ... }`.
- Fixed generic struct literal type-head inference for unit type arguments such as `Box(()) { ... }`.
- Updated core/alloc libraries, fixtures, and documentation for braced struct literals while keeping
  standard effects and protocols outside the prelude.

## 0.182.0 - 2026-07-22

- Split trait parameters from the implemented subject by adding explicit trait self-kind headers
  such as `trait(Self: type)` and `trait(Self: (Value: type): type)`. Omitting the header still
  means `Self: type`, so `let Copy = trait {}` remains the simple first-order form.
- Migrated `core.algebra` to `Self`-subject protocols, with `Monoid where Self: Semigroup`.
- Migrated `core.functional` to higher-kinded `Self` subjects. `map`, `apply`, and `flat_map` are
  receiver methods on `Self(...)` values, while `pure` remains a constructor associated function.
- Added constructor-receiver trait method dispatch from concrete nominal instances and allowed
  generic functions to take explicit type-constructor arguments with constructor predicates such as
  `where M: Monad`.

## 0.181.0 - 2026-07-22

- Added trait-level `where` constraints so higher-kinded standard protocols can express
  inheritance, starting with `Applicative(F) where F: Functor` and
  `Monad(M) where M: Applicative`.

## 0.180.0 - 2026-07-22

- Removed dedicated parser migration diagnostics for removed bare effect groups such as
  `let f(unsafe): T`, `let f(try): T`, and return-type forms like `T(E)`.

## 0.179.0 - 2026-07-22

- Removed dedicated `!` effect-syntax migration diagnostics; old `T ! Effect` forms now fail
  through the ordinary declaration/type parser paths.

## 0.178.0 - 2026-07-22

- Removed the dedicated postfix `.try` parser migration diagnostic; `.try` now fails through the
  ordinary member-name syntax path because call propagation is modeled by `Throws(Error)` effects.

## 0.177.0 - 2026-07-22

- Required effect declarations and nominal effect references in `with(...)` to use an uppercase
  final name segment, keeping standard and user-defined effects aligned with type-like nominal
  spelling and rejecting accidental lowercase custom effect names before semantic analysis.

## 0.176.0 - 2026-07-22

- Allowed generic nominal trait implementations to bind direct generic associated constructors,
  so `extend(Maybe(T), Chain) { let Rebind = Maybe ... }` materializes each concrete
  instance with the same source-backed constructor substitution as concrete implementations.
- Allowed generic trait implementation methods to carry compile-time parameter groups matching
  their trait declarations, including effect rows such as `chain(E: effect, U: type)`.
- Materialized blanket trait implementations for nominal source probes before lowering custom
  `?.`, so cached generic nominal instances can still dispatch through `core.ops.Chain`.

## 0.175.0 - 2026-07-22

- Implemented concrete generic associated type constructor bindings such as `let Rebind = Maybe`
  for nominal trait implementations, storing constructor sources separately from ordinary lowered
  associated types so `Rebind(U)` can substitute to `Maybe(U)` in method templates.
- Routed `?.` on non-`Option`/`Result` nominal values through the validated `core.ops.Chain.chain`
  method when the synthesized transform closure can be represented as a no-capture lifted function.
- Fixed call-argument staging for direct lifted function values, keeping bare function pointers out
  of temporary locals that have no runtime storage.

## 0.174.0 - 2026-07-22

- Routed `??` on non-`Option`/`Result` nominal values through the validated `core.ops.Coalesce`
  trait method, selecting the standard `pure` effect row for the synthesized fallback action.
- Added a narrow no-capture closure-to-function-parameter bridge, allowing closure literals such as
  `{ 42 }` to fill `(): T` parameters by using their lifted function symbol directly.
- Documented the current custom `Coalesce` limit: captured fallback closures still require the
  general callable-to-function bridge before they can be represented as ordinary trait arguments.

## 0.173.0 - 2026-07-22

- Allowed concrete nominal trait implementation methods to keep matching compile-time parameter
  groups, registering those methods as function templates so calls such as
  `value.method(T)(...)` can instantiate the implementation body.
- Removed the remaining dedicated parser migration branch for lowercase `with(unsafe)` and
  `with(try...)`; standard effects are parsed only as ordinary effect names such as `Unsafe`,
  `Throws(E)`, and `Async`.
- Updated language and standard-library documentation snippets to use the `.sc` code fence and the
  uppercase `Async` effect spelling.

## 0.172.0 - 2026-07-22

- Added source-backed `core.ops.Chain` and `core.ops.Coalesce` protocol declarations for `?.` and
  `??`, exported outside the prelude and validated as ordinary core lang-item contracts.
- Allowed trait declarations and method signatures to mention one-argument generic associated type
  constructors, while keeping GAT implementations and where-predicate equalities explicitly
  rejected until the semantic lowering slice is implemented.
- Covered malformed `Chain` and `Coalesce` core contracts in bundle validation tests.

## 0.171.0 - 2026-07-22

- Routed standard `throw` lowering through the validated `core.error.throw` declaration by
  substituting its `Error` parameter and reading its declared `Throws(Error)` effect before invoking
  `Throws.raise`.
- Covered the source-backed `core.error.throw` function template in core lang-item registration
  tests, so its declared effect row remains visible to semantic lowering.

## 0.170.0 - 2026-07-22

- Added `core.error.throw` as an edition-validated compiler-provided control contract:
  `throw(Error)(error): Never with(core.effects.Throws(Error))`.
- Allowed the embedded core bundle to declare `throw` with the same keyword-declaration path used by
  `do`, `try`, `unsafe`, and `loop`, and exported it from `core.control` rather than leaving
  `throw` as a syntax-only compiler capability.
- Reserved user-defined `throw` control contracts outside `core.control`, matching the existing
  protection for compiler-lowered control spellings.

## 0.169.0 - 2026-07-22

- Migrated the public residual-handler fixture from lowercase `throws(E)` to ordinary
  `core.effects.Throws(E)`, leaving public pass/fail fixtures on the standard effect spelling.
- Removed the remaining accepted lowercase `with(throws(E))` and explicit `throws(E)` effect-row
  argument paths; lowercase `throws` no longer has parser/codegen special handling, while
  `Throws(E)` is kept as an ordinary standard effect identity.
- Threaded an outer handler continuation into nested handler action closures, so residual
  `Throws(E)` rows exposed by inner algebraic handlers compose through the ordinary handler/CPS
  path instead of relying on the older carrier.
- Reconstructed effect identity strings back into source effect applications when generic effect
  rows, cached signatures, or local function values need their original arguments, allowing
  standard `Throws(E)` rows to forward like user-defined effect rows.
- Taught contextual standard `try { ... }` detection to recognize method and associated-function
  calls that require `Throws(E)`, and improved missing-standard-`Throws(E)` diagnostics to suggest
  handling with `try { ... }` or propagating the effect.
- Kept generated nested handler calls lexically inside `unsafe { ... }` after CPS rewriting and
  rolled back validation/specialization continuation adapters together with discarded lifted
  closures, preventing stale adapter references in final IR.
- Documented the design rule that compiler-backed capabilities should be expressed as validated
  source-level effects, traits, or protocols first; compiler lowering should be the implementation
  hook rather than a separate magical surface.

## 0.168.0 - 2026-07-22

- Migrated the public mixed unsafe/error fixture from lowercase `throws(E)` to the ordinary
  `core.effects.Throws(E)` spelling while keeping `Unsafe` in the same effect row.
- Preserved lexical `unsafe { ... }` authorization across algebraic-handler CPS frame generation,
  so standard `Throws(E)` handling can forward residual `Unsafe` without requiring a second
  explicit unsafe boundary at generated frame call sites.
- Updated the mixed unsafe/error unit coverage to assert the standard `Throws(E)` row rather than
  the removed dedicated Result-ABI shape.

## 0.167.0 - 2026-07-22

- Taught standard `Throws(E)` detection and handler lowering to recognize explicitly instantiated
  generic functions, so calls such as `fail(bool)(true)` are handled by contextual `try { ... }`
  through the ordinary standard effect path.
- Migrated the public generic throw fixture from lowercase `throws(E)` to `core.effects.Throws(E)`.
- Kept remaining lowercase carrier coverage limited to residual-handler and mixed unsafe/error
  paths.

## 0.166.0 - 2026-07-22

- Stopped retaining lifted native closures produced while only validating ordinary functions whose
  rows contain standard `Throws(E)`; those functions are emitted through handler/CPS lowering
  instead of a direct native body.
- Migrated the public `do` return-boundary fixture and the matching row-forwarding regression to
  standard `Throws(E)`, covering `do { return fallible() }` inside contextual `try { ... }`.
- Kept the remaining lowercase `throws(E)` carrier coverage limited to unresolved generic,
  residual-handler, and mixed unsafe/error paths.

## 0.165.0 - 2026-07-22

- Made `Unsafe` the public standard effect spelling in `with(...)` rows and effect compile-time
  arguments; lowercase `with(unsafe)` now reports a migration diagnostic instead of being accepted.
- Changed the validated `core.unsafe.unsafe` contract to require `core.effects.Unsafe`, then
  normalized that standard effect identity onto the existing checked-unsafe semantic bit so raw
  pointer checks, callable rows, method signatures, and generic effect arguments continue to use one
  enforcement path.
- Updated public fixtures, docs, and diagnostics to use `Unsafe` while keeping the remaining
  lowercase `throws(E)` carrier tests explicit as implementation debt until those paths are fully
  migrated to ordinary `Throws(E)`.

## 0.164.0 - 2026-07-22

- Extended context-free `try { ... }` inference to ordinary standard `Throws(E)` function calls:
  when the body has one escaping standard `Throws(Error)` row and a probeable success type, the
  compiler now infers `Result(T, Error)` without a `let result: Result(T, Error)` annotation.
- Added a direct standard-effect fixture covering unannotated `try` over a `with(Throws(i32))`
  function call, keeping remaining `Never`-only, generic, `do`-return, residual-handler, and mixed
  unsafe/error cases explicit implementation gaps.

## 0.163.0 - 2026-07-22

- Changed the validated `core.error.try` contract to require ordinary `Throws(E)` in its action
  row instead of the lowercase `throws(E)` carrier spelling, so the standard library now expresses
  recoverable failure with the same nominal effect declaration users write.
- Updated the core docs, grammar notes, and implementation status to make `Throws(E)` the public
  standard effect model; legacy lowercase carrier wording is now treated as implementation debt
  rather than a compatibility surface.
- Migrated the public direct-throw/try fixtures that already lower through ordinary `Throws(E)`;
  generic, `do`-return, residual-handler, and mixed unsafe cases still exercise the old internal
  carrier until their lowering is unified.

## 0.162.0 - 2026-07-22

- Allowed contextual `try { ... }` expressions with an expected `Result(T, E)` to handle the ordinary
  standard `Throws(E)` custom effect by generating a normal `Throws(E).handle` with `done -> Ok` and
  `raise -> Err` clauses.
- Preserved the existing dedicated lowercase `throws(E)` Result ABI path when the body calls
  functions with `with(throws(E))`, so current propagation behavior stays stable while ordinary
  `Throws` handling moves onto the algebraic-effect path.
- Covered both standard-effect function calls and direct `throw` inside contextual `try`, while
  leaving context-free inference for ordinary `Throws(E)` as future work.

## 0.161.0 - 2026-07-22

- Allowed `throw error` to target the ordinary standard `Throws(Error)` effect when no dedicated
  `with(throws(Error))` ABI boundary is active, desugaring it through `Throws(Error).raise(error)`.
- Reused algebraic handler source transformation for `throw` inside `Throws(Error).handle { ... }`,
  so handler clauses see the same abort operation as an explicit `Throws.raise` call.
- Added source-backed active effect metadata so closure and handler lowering can preserve the exact
  standard effect instance needed for ordinary `Throws` sugar, while rejecting ambiguous multiple
  `Throws(Error)` rows instead of guessing.

## 0.160.0 - 2026-07-22

- Treated `Never`-returning algebraic effect operations as abort operations: their handler clauses
  omit `resume`, discard the suspended continuation, and directly produce the handler answer.
- Allowed `Throws(Error).raise` to be invoked and handled through the same operation/handler path as
  user-defined standard effects, covering both direct and cross-function handler lowering.
- Documented the remaining special boundary: `throw`/`try` still use the current compiler ABI while
  `Throws.raise` now exposes the ordinary source-level abort-operation shape.

## 0.159.0 - 2026-07-22

- Added callable dispatch for constructor trait associated functions, so a generic nominal
  constructor with `extend(Carrier, Functor) { let map... }` can call `Carrier.map(...)` and route to
  the registered implementation method template.
- Reused the existing generic function instance pipeline for HKT associated methods, preserving
  ordinary compile-time argument inference, named-argument overload selection, and effect checking.
- Added conservative type probing for constructor trait associated function calls when compile-time
  arguments are explicit, without adding side effects to the probe phase.
- Rewrote `core.effects` declarations to use complete effect block syntax: `Unsafe` is an explicit
  empty effect, `Throws(Error)` declares `raise(move error: Error): Never`, and `Async` declares a
  minimal `suspend(): ()` operation.
- Updated the core lang-item validator so `Throws` is checked by its ordinary operation shape rather
  than by marker-effect special casing.

## 0.158.0 - 2026-07-22

- Registered constructor trait implementation methods as ordinary generic function templates, so
  implementations such as `extend(Carrier, Functor) { let map(E, A, B)... }` are shape-checked and
  their bodies are validated by the existing generic template pipeline.
- Added constructor-kind substitution for applied type constructors, allowing trait signatures using
  `F(A)` to normalize to implementation signatures such as `Carrier(A)`.
- Added storage for constructor trait implementation method identities, leaving callable HKT trait
  dispatch as the next semantic slice rather than routing through runtime `Ty`-based trait tables.

## 0.157.0 - 2026-07-22

- Added a dedicated constructor-trait-implementation header table so generic nominal constructors can
  implement marker traits whose first compile-time parameter is a matching type-constructor kind.
- Allowed declarations such as `extend(Carrier, Higher) {}` and `extend(Carrier, Tagged(i32)) {}` when
  `Higher`/`Tagged` abstract over `F: (Value: type): type`, including duplicate, arity, orphan, and
  unsupported-member diagnostics.
- Kept constructor trait implementations limited to marker traits for now; generic method lowering,
  associated types, `where` clauses, and executable implementations of `Functor`/`Applicative`/`Monad`
  remain future work.

## 0.156.0 - 2026-07-22

- Added constructor compile-time kinds such as `F: (Value: type): type` and
  `E: (Error: type): effect` to the AST and parser, with disambiguating lookahead so function types
  like `action: (): T with(E)` remain runtime parameter types rather than compile-time parameters.
- Extended trait signature validation to understand type-constructor parameters in method types and
  effect-constructor parameters in `with(...)` rows, while keeping constructor-valued generic
  functions and trait implementations explicitly unsupported with diagnostics.
- Added non-prelude `core.functional.Functor`, `core.functional.Applicative`, and
  `core.functional.Monad` source definitions over HKT constructor kinds, and mounted
  `core.functional` in the standard-library namespace with ordinary `use` diagnostics.
- Documented the current HKT boundary: standard protocols can be declared, while trait inheritance,
  executable `Functor`/`Applicative`/`Monad` implementations, and broad constructor equation solving
  remain future semantic work.

## 0.155.0 - 2026-07-22

- Renamed the prelude uninhabited type from `never` to `Never` without a compatibility alias, and
  updated parser reservations, diagnostics, tests, and documentation to use the source spelling.
- Split validated core capabilities into ordinary standard-library modules: `core.effects` now owns
  `Unsafe`, `Throws(E)`, and the ordinary `Async` marker; `core.domains` owns access markers;
  `core.control` now contains only the control/runtime contracts for `do`, `try`, `unsafe`, `loop`,
  `Continuation`, and `EffectCallable`.
- Added `core.algebra.Semigroup(T)` and `core.algebra.Monoid(T)` as first-order standard-library
  protocols outside the prelude, and relaxed core validation so non-lang-item standard modules can
  contain ordinary public declarations while lang-item modules remain shape-checked.

## 0.154.0 - 2026-07-22

- Added labeled type-constructor arguments in type positions, allowing declarations such as
  `Pair(V: bool, K: i32)` to normalize against the constructor's `K: type, V: type` labels before
  semantic lowering.
- Preserved labels in the parser as a temporary AST form, then erased them before alias expansion so
  existing type checking, monomorphization, and LLVM lowering continue to operate on positional
  nominal applications.
- Extended type-constructor alias coverage so labeled applications of aliases use the alias labels
  and still preserve the target nominal identity.
- Added diagnostics for unknown, duplicate, missing, and arity-mismatched labeled type arguments.

## 0.153.0 - 2026-07-22

- Extended direct trailing-closure actions to complete reusable-handler calls with earlier `copy`
  or `move` runtime arguments, including arguments in preceding curried groups.
- Materialized preceding values and the action as typed locals in source order before capture
  lifting, preserving observable side effects and ownership transfer.
- Kept preceding shared and mutable borrow parameters out of this transformation until their loan
  lifetime can be represented without converting the borrowed place into an owned temporary.
- Added a native ordered-evaluation regression whose mutable capture and resumed operation produce
  exit status 42.

## 0.152.0 - 2026-07-22

- Accepted a direct trailing-closure literal for a reusable handler action when no earlier runtime
  argument requires evaluation.
- Materialized the literal as a compiler-generated, explicitly typed local binding before entering
  the same capture-lifting and selective-CPS path as named source closures.
- Preserved capture acquisition at the action's call position and added a native mutable-capture
  direct-action regression exiting with 42.
- Renamed injected source bindings to the handler's declared action parameter, removing an implicit
  assumption that caller locals were also named `action`.

## 0.151.0 - 2026-07-22

- Propagated original captured-action source metadata through immutable and mutable callable alias
  moves, allowing alias chains to enter reusable-handler specialization without rebuilding captures.
- Fixed callable-environment relocation to copy borrowed capture pointer slots as pointers rather
  than loading them as the pointee value type; this removes a native `FnMut` alias crash.
- Kept owned capture relocation and drop flags distinct from borrowed slots, preserving exactly-once
  `FnOnce` cleanup through both resumed and abandoned aliased actions.
- Promoted the former alias diagnostic to native `FnMut`/`FnOnce` coverage and retained a focused
  diagnostic for direct trailing-closure action literals, the next unconnected source shape.

## 0.150.0 - 2026-07-22

- Removed the adjacency restriction for directly bound captured actions passed to reusable
  handlers, including calls nested inside larger expressions.
- Retained each source closure's capture names and original binding metadata, then specialized a
  later handler call by borrowing or moving fields from the already-created callable environment.
- Preserved binding-time borrow/move semantics instead of reconstructing captures at the later call
  site, and consumed the stored action according to the handler's owned parameter contract.
- Released stored closure capture loans after consumption and suppressed the obsolete direct-style
  effectful closure body; native non-adjacent `FnMut` and `FnOnce` regressions still exit with 42.
- Replaced the former non-adjacent diagnostic with a focused diagnostic for callable aliases and
  other erased action values that do not retain their original source binding yet.

## 0.149.0 - 2026-07-22

- Extended adjacent captured reusable-handler actions from block-tail calls to complete calls used
  as ordinary `let` initializers or expression statements.
- Rewrote the statement immediately following the action binding to the same capture-lifted handler
  specialization while preserving the binding-time-to-call evaluation boundary.
- Released lifted mutable borrows when the handler call returns, allowing subsequent statements to
  observe `FnMut` state safely.
- Strengthened the native `FnMut` and `FnOnce` regressions to inspect mutable state and exactly-once
  drop counters immediately after resumed and abandoned non-tail calls; all still exit with 42.

## 0.148.0 - 2026-07-22

- Extended captured reusable-handler actions from shared `Copy` environments to `FnMut` mutable
  captures and `FnOnce` owned root captures.
- Lifted mutable captures as `borrow(mut)` parameters and consuming captures as `move` parameters,
  preserving the closure's source ownership mode through handler specialization and selective CPS.
- Verified native `FnMut` state across a resumed operation and exactly-once `FnOnce` resource cleanup
  on both resumption and continuation abandonment; all three regressions exit with 42.
- Kept unsupported non-adjacent and non-tail source shapes on a dedicated user-facing diagnostic.

## 0.147.0 - 2026-07-22

- Allowed a complete reusable-handler call in block-tail position to consume the immediately
  preceding, explicitly typed effectful closure when it captures immutable `Copy` locals.
- Lifted those shared captures into target-specific handler parameters, injected the closure into
  the lexical handler action, and ran the existing selective-CPS transformation there.
- Kept the original higher-order source signature for type checking while classifying its handled
  action slot with the internal `EffectCallable(Input, Output, Answer)` runtime contract.
- Added a native captured-action regression that resumes the operation and exits with 42.

## 0.146.0 - 2026-07-22

- Added compiler-internal HIR operations that erase an owned CPS closure into
  `EffectCallable(Input, Output, Answer)` and invoke it with an input plus
  `Continuation(Output, Answer)`.
- Added target-specific call/drop adapters that retain captured callable environments behind the
  four-field erased ABI.
- Made both erasure and invocation explicit ownership transfers in cleanup planning, with a guarded
  one-shot invocation branch that traps reuse and leaves abandonment to the erased drop entry.
- Added an LLVM regression covering the complete CPS entry shape without exposing low-level erasure
  intrinsics as source API.

## 0.145.0 - 2026-07-22

- Added the source-backed `core.control.EffectCallable(Input, Output, Answer)` lang-item contract for
  owned erased actions passed into algebraic handlers.
- Validated its exact three-type-parameter compiler-owned type contract alongside `Continuation` and
  exposed it only through the ordinary `core.control` module.
- Added a distinct semantic type, canonical identity, API-visibility traversal, non-`Copy` ownership
  classification, cleanup move path, and LLVM `%salicin.effect_callable` representation.
- Added flag-guarded drop glue for its `{ call, drop, environment, flag }` ABI and a native imported
  contract regression producing 42.

## 0.144.0 - 2026-07-22

- Allowed a complete reusable-handler call to receive a finite nested `if` selection between known
  algebraic-effect functions as its first action parameter.
- Distributed the call across target-specific handler specializations instead of materializing
  direct-style effectful function pointers that have no native body.
- Preserved selector-before-later-group evaluation order, named argument binding, branch laziness,
  curried runtime arguments, immutable aliases, resumption, and abandonment.
- Added a native nested-selection and side-effect-order regression producing 42.

## 0.143.0 - 2026-07-22

- Specialized reusable handler functions at their call sites when an algebraic-effect callable
  parameter is supplied by a statically known named function or immutable function alias.
- Removed specialized callable parameters from the runtime ABI and rewrote their uses before the
  handler's selective-CPS pass, so the operation body receives the handler's real continuation.
- Preserved remaining curried groups, named-argument ordering, ordinary runtime arguments, effect-row
  compatibility checks, and distinct specializations for different action functions.
- Replaced a previously invalid native path—which emitted a reference to an intentionally absent
  direct-style effectful function—with a runnable reusable-handler regression producing 42.

## 0.142.0 - 2026-07-22

- Forwarded capturing callable environments through a suspended nested dynamic selector instead of
  requiring every target to be a named non-capturing function.
- Rebased moved `FnOnce` captures onto the continuation-owned callable environment and materialized
  borrowed callable aggregates when passing `Fn` and `FnMut` environments by reference.
- Kept callable storage metadata available while emitting mutually exclusive control-flow branches;
  source-level ownership analysis still rejects sequential reuse.
- Disarmed the complete nested drop-flag subtree when moving a callable capture, preventing child
  resources from being destroyed by both the old and new environment.
- Promoted the former owning-environment diagnostic to positive shared-capture coverage and added
  native suspended-union regressions for repeated `FnMut` state and exactly-once `FnOnce` cleanup.

## 0.141.0 - 2026-07-22

- Allowed finite dynamic callable selections to use existing dynamic callable values as branches.
- Merged branch metadata into one deduplicated target-set union and remapped every branch tag into
  the union's index space.
- Snapshotted internal branch tags outside selective CPS so an effectful selector can suspend without
  exposing those tags as ordinary callable values.
- Preserved three capturing `FnOnce` environments through a non-suspending nested union and verified
  selected plus unselected abandonment cleanup exactly once.
- Added a dedicated diagnostic for a suspending selector over capturing dynamic branches until the
  owning closure-environment ABI can forward those environments into its continuation.
- Added named-target effectful-selector and capturing-drop native regressions, both exiting with 42.

## 0.140.0 - 2026-07-22

- Allowed mutable handler-local dynamic callable aliases to be assigned from another callable with
  the same call-group shape and finite target set.
- Remapped runtime tags when source and destination metadata list the same targets in different
  orders, preserving the selected callable rather than the raw integer index.
- Rejected assignments with different targets or signatures before lowering them as ordinary `i32`
  writes.
- Preserved captured `FnOnce` environments across tag assignment; selected and unselected resources
  still drop exactly once when the selected call aborts its continuation.
- Added native named-target and capturing-drop assignment regressions, both producing exit code 42.

## 0.139.0 - 2026-07-22

- Allowed finite dynamic effectful callable selection tags to be copied into immutable local aliases
  while remaining under their lexical handler.
- Propagated the selected target set and call-group shape to each alias so direct calls continue to
  use handler-aware resumable dispatch.
- Allowed such aliases to specialize effectful higher-order named frames without falling back to an
  ordinary function-pointer call.
- Preserved the selected capturing closure environment, including repeated `FnMut` state updates,
  across calls through the copied tag.
- Kept mutable aliases rejected until assignments can update the target-set metadata together with
  the runtime tag.

## 0.138.0 - 2026-07-22

- Allowed suspended match guards to inspect referenced non-`Copy` payload bindings without copying
  or transferring their ownership.
- Replaced each such closure capture with a read-only projected alias rebuilt from the continuation's
  owned enum capture, including reconstruction through nested generated continuations.
- Kept moves through reconstructed guard views rejected as moves out of borrowed values; the original
  pattern still commits ownership only after the complete guard resumes `true`.
- Taught cleanup planning to exclude borrowed projection aliases rooted in owned storage from the
  owned move-path forest.
- Replaced the former non-`Copy` guard failure fixture with a native resource regression covering
  `false` and `true` resumptions, exactly-once destruction, and exit code 42.

## 0.137.0 - 2026-07-22

- Allowed a suspended match guard over a non-`Copy` enum to retain pattern bindings that themselves
  implement `Copy`.
- Preserved only syntactically referenced bindings in the non-owning inspection pattern and checked
  their concrete types during typed match lowering.
- Materialized a distinct owned commit input before the resumed branch rematches, preventing the
  compiler's internal inspection marker from leaking into the ownership-transfer match.
- Kept non-`Copy` retained bindings rejected with a more precise binding-level diagnostic.
- Added a native mixed-payload regression that copies an `i32` into the guard while transferring and
  dropping the sibling resource exactly once on both `false` and `true` resumptions.

## 0.136.0 - 2026-07-22

- Delayed non-`Copy` payload transfers in suspended match guards whose guard expression does not
  itself reference the candidate's pattern bindings.
- Matched a binding-erased inspection pattern first, moved the sole owned enum into the generated
  continuation, and rematched the original pattern only after the guard resumed `true`.
- Collapsed the non-effectful tail of a candidate chain back into one ordinary match so a `false`
  resumption can continue without consuming and reconstructing the scrutinee between candidates.
- Kept guards that retain projected payload bindings across a suspension rejected until continuation
  frames can rebuild those projections from their owned scrutinee.
- Added a native resource regression exercising both `false` and `true` resumptions; two inputs are
  each consumed and dropped exactly once and the program exits with code 42.

## 0.135.0 - 2026-07-22

- Allowed finite handler-local callable selections to target explicitly typed capturing resumable
  closures as well as named functions.
- Preserved selected `FnMut` state, rejected a second selected `FnOnce` invocation through ordinary
  flow joins, and covered selected plus unselected resource cleanup on abandonment.
- Added a non-owning enum inspection read for effectful match guards whose candidate patterns bind no
  payload values, then moved the sole owned input into the suspended guard continuation.
- Kept effectful guards with payload bindings rejected until their pattern transfers can be committed
  only after the resumed guard succeeds.
- Added three native regressions covering dynamic `FnMut`, dynamic `FnOnce` cleanup, and a false
  suspended guard over a resource-owning enum; all produce exit code 42.

## 0.134.0 - 2026-07-22

- Generalized handler-local conditional selection from two named effectful callables to any finite
  `if / else if / else` tree of named targets.
- Lowered the selection tree once at the binding site to an `i32` tag and dispatched that tag at
  every call while preserving the current resumable continuation.
- Kept target resolution name-based and handler-local, avoiding type-based overload search or a
  general escaping dynamic-callable ABI.
- Extended native dynamic-dispatch coverage to a three-target selection forwarded through a
  higher-order frame.

## 0.133.0 - 2026-07-22

- Carried lexical handler capabilities through compiler-generated named, loop, and continuation CPS
  closures without adding them to the callable's public effect row.
- Composed an inner operation's residual algebraic requirement through an outer source-specialized
  handler frame.
- Inferred untransformed handler action success types separately from outer CPS answer types.
- Wrapped throwing handler tail continuations in their `Result` return boundary when a direct tail
  call cannot preserve the physical ABI by itself.
- Extended native residual-row coverage to nested algebraic, `unsafe`, and `throws(bool)` effects.

## 0.132.0 - 2026-07-22

- Preserved every residual `unsafe`, `throws`, and nominal effect requirement when a handler
  removes one algebraic effect from an operation or specialized named frame.
- Added an internal effect gate for intercepted operations so selective CPS lowering cannot bypass
  the ordinary call-site capability diagnostics.
- Distinguished logical operation results from their `Result(T, E)` throws ABI when constructing
  resume continuations and handler answer types.
- Added native coverage combining a handled algebraic operation with forwarded `unsafe` and
  `throws(bool)` requirements.

## 0.131.0 - 2026-07-22

- Lowered two-way conditional selection between named effectful callables to a binding-site boolean
  tag and call-time dispatch between resumable entries.
- Forwarded tagged dynamic selections through statically specialized higher-order frames without
  materializing unresolved ordinary function pointers.
- Permitted function-valued conditionals during validation of source-only resumable functions while
  keeping escaping dynamic callable values explicitly rejected.
- Added native runtime-selection coverage whose chosen branch resumes through the active handler.

## 0.130.0 - 2026-07-22

- Lowered explicitly typed local effectful closures to handler-answer CPS with a hidden erased
  continuation parameter appended to their final runtime group.
- Preserved ordinary capture environments across that ABI, including repeated `FnMut` state and
  `FnOnce` move-capture cleanup when a clause abandons the suspended computation.
- Allowed statically known capturing closures to specialize higher-order effectful frames and added
  native shared-capture, mutable-repeat, and exactly-once Drop coverage.
- Materialized borrowed callable environments when deferred continuations forward `Fn` or `FnMut`
  closures through another closure boundary.

## 0.129.0 - 2026-07-22

- Specialized higher-order effectful frames when callable arguments resolve to named functions or
  statically tracked aliases.
- Rewrote uses of those callable parameters to direct resumable calls and erased the specialized
  parameters from the runtime frame ABI.
- Added native higher-order resumption coverage and a dedicated diagnostic for genuinely dynamic
  effectful callable targets that still require the handler-aware runtime ABI.

## 0.128.0 - 2026-07-22

- Extended selective CPS into arguments of effect-propagating named calls before constructing the
  callee's resumable frame.
- Preserved grouped-call reconstruction and left-to-right traversal across multiple suspending
  arguments.
- Added native order-sensitive coverage for two operation calls used as one effectful call's
  arguments.

## 0.127.0 - 2026-07-22

- Propagated statically known effectful function identity through inferred immutable local aliases
  and alias chains inside handlers.
- Lowered calls through those aliases with the existing named-function CPS frames instead of
  materializing an unresolved ordinary function pointer.
- Added native execution coverage for a chained non-capturing effectful alias resumed by a handler.

## 0.126.0 - 2026-07-22

- Added effect-operation overload sets distinguished only by runtime parameter names, consistently
  rejecting type-only overloads and ambiguous positional calls.
- Allowed repeated operation labels in derived handlers, using clause parameter names before
  `resume` to select the handled signature.
- Preserved named-call selection through selective CPS and added native execution coverage for two
  same-named operations handled by distinct clauses.

## 0.125.0 - 2026-07-22

- Scoped recursive named-frame visibility to callee-body transformation instead of the caller's
  remaining continuation.
- Stopped misclassifying a later sequential call to the same effectful named function as a
  recursive backedge with an extra hidden continuation argument.
- Preserved direct and mutual recursion on the erased continuation ABI and added native sequential
  repeated-call coverage.

## 0.124.0 - 2026-07-22

- Extended selective algebraic-effect CPS through fully applied optional method calls, evaluating
  the receiver before arguments and skipping all arguments on `None` and `Err` paths.
- Rewrapped successful calls and residual values into the original `Option` or `Result` family
  before entering the surrounding continuation, including preservation of `Result` error payloads.
- Moved owned non-Copy receivers into argument continuations so they remain valid across suspension
  and are consumed or dropped exactly once.
- Added native `Option` and `Result` coverage proving that effectful arguments execute only on the
  two success paths.

## 0.123.0 - 2026-07-22

- Extended selective algebraic-effect CPS through match guards, preserving both successful arm
  selection and false-guard fallthrough to later candidates.
- Detected suspension through direct operations and effectful named calls in guards, while leaving
  ordinary pure guards on the existing ownership-aware lowering path.
- Required `Copy` match inputs for the initial effectful-guard slice and added a focused diagnostic;
  retaining non-Copy payload ownership across suspended candidate selection remains future work.
- Added native true/false guard coverage and a compile-fail ownership regression.

## 0.122.0 - 2026-07-22

- Extended selective algebraic-effect CPS through `??` while preserving its lazy fallback and
  left-to-right evaluation semantics.
- Added an internal typed coalescing node that defers `Option` versus `Result` pattern selection
  until the scrutinee type is known, without exposing compiler machinery in source syntax.
- Added native coverage for effectful `Option` and `Result` coalescing across both success and
  residual paths, including a `bool` payload under an `i32` handler answer type.

## 0.121.0 - 2026-07-22

- Verified the erased continuation ownership contract on both terminal paths: abandoning a
  continuation runs its environment drop entry, while invoking it transfers the environment and
  prevents a second drop.
- Added native regressions using a move-captured `Drop` resource to require exactly one destructor
  call after either handler abandonment or one-shot resumption.

## 0.120.0 - 2026-07-22

- Added the internal erased continuation ABI `{ call_entry, drop_entry, environment, one_shot_flag }`
  behind the source-backed `core.control.Continuation(Input, Output)` contract.
- Generated typed call and abandonment adapters for each concrete continuation closure, including
  dynamic drop glue for owned environments and a runtime defense against repeated invocation.
- Passed erased continuations as explicit hidden named-frame parameters and created a fresh
  continuation node at every recursive call site, preserving the remaining computation without
  duplicating a one-shot environment.
- Unified direct and mutually recursive algebraic-effect frames on the CPS ABI and removed the
  same-answer-type legacy selection path.
- Added native mutual-recursion coverage where the recursive functions return `bool` and the
  complete handler returns `i32`.

## 0.119.0 - 2026-07-22

- Composed lexically nested handlers of different user-defined effects by carrying the outer
  selective-CPS transformation through the inner handler's action boundary.
- Transformed outer-effect operations in inner operation and `done` clauses while respecting the
  inner clause's own `resume` binding.
- Traversed compiler-generated frame and continuation closures when a nested handler handles a
  second effect, preserving both effects across specialized named calls.
- Added native composition coverage with one outer operation in the action and another in the inner
  handler clause.

## 0.118.0 - 2026-07-22

- Extended selective algebraic-effect CPS traversal through arrays, indexes, ordinary and optional
  members, `match` scrutinees and arm bodies, `do`, `unsafe`, and `try` wrappers.
- Preserved lazy `&&` and `||` semantics by lowering them as CPS branches instead of evaluating an
  effectful right operand eagerly.
- Extended closure capture discovery through generated match continuations, including bindings,
  guards, nested arm bodies, and recursion tokens inside compiler-created closures.
- Added native coverage for effectful structural expressions, short-circuit abandonment, and a
  cross-function continuation whose input and handler answer types differ.

## 0.117.0 - 2026-07-22

- Replaced callback-style named-call completion with typed one-shot continuation closures and an
  explicit tail-continuation HIR terminator, so abandoning an operation clause now aborts the full
  suspended cross-function computation.
- Preserved clause computation after resumption, including answer-producing forms such as
  `resume(value) + adjustment`, while retaining callee cleanup before the caller continuation.
- Forwarded shared and mutable captures through moved callable environments by loading their
  rebased environment pointers instead of retaining stale lexical places.
- Extended recursive-token discovery through compiler-generated continuation closures; direct
  recursion and resumable loop backedges use the CPS frames, while mutually recursive SCCs retain
  the previous same-answer frame lowering until the erased continuation ABI lands.
- Added native regression coverage for cross-function abandonment and post-resume computation.

## 0.116.0 - 2026-07-22

- Lowered resumable `while` and value-producing `loop` backedges as recursive lifted iteration
  frames with the handler answer type.
- Routed normal iteration, `continue`, condition failure, and value-carrying `break` through explicit
  internal frame continuations while preserving current-frame cleanup.
- Supported handled operations in loop conditions and bodies without compile-time unfolding.
- Added native combined coverage for effectful conditions, bodies, continue edges, and break values.

## 0.115.0 - 2026-07-22

- Lowered direct and mutually recursive resumable calls to direct calls between lifted handler
  frame functions instead of compile-time unfolding or self-referential callable values.
- Forwarded captured handler environments explicitly across recursive frame calls, including
  compatible shared and mutable reborrows through nested closure frames.
- Added function-type annotations for local closures to establish recursive frame result boundaries
  while retaining the concrete callable environment type internally.
- Added native direct- and mutual-recursion coverage with captured handler state.

## 0.114.0 - 2026-07-22

- Specialized handled named calls as real local closure frames rather than flattening callee scope
  into the caller, preserving explicit-return boundaries and cleanup before caller continuation.
- Allowed closure parameters to retain inferred, copy, move, shared-borrow, and mutable-borrow modes,
  enabling resumable functions with ordinary borrow parameters.
- Extended closure capture analysis through nested closures, compound assignment, and struct
  construction needed by specialized continuation frames.
- Added native coverage proving explicit-return resource cleanup order and shared/mutable borrow
  behavior across handled operations.

## 0.113.0 - 2026-07-22

- Extended selective CPS traversal through ordinary call and operation arguments while preserving
  left-to-right evaluation, so handled operations can feed other calls and operations.
- Added native coverage for `done:` answer-type transformation and nearest matching nested handlers.
- Preserved one-shot resumption across nested argument continuations and exact effect-instance
  selection without treating closure construction as effect execution.

## 0.112.0 - 2026-07-22

- Propagated handled operations through fully applied ordinary named functions by selectively
  specializing their source bodies under the active handler.
- Added hygienic renaming for parameters, locals, closure parameters, and match bindings so inlined
  handler continuations preserve caller and callee lexical scope even when names collide.
- Stopped emitting unspecialized resumable source functions into native modules, preventing
  unreachable operation placeholders from becoming linker references.
- Added native cross-function state-handler coverage; recursion, borrow parameters, explicit
  returns, indirect calls, and loop backedges remain reserved for the typed continuation ABI.

## 0.111.0 - 2026-07-22

- Added the derived `Effect(...).handle(clauses...) { action }` member for lexically visible
  operations, using labeled contextual closures and exact instantiated effect identities.
- Implemented selective one-shot continuation transformation: a clause may resume once or omit
  `resume` to abandon the suspended remainder, while duplicate use and escape are diagnosed.
- Added the edition-validated `core.control.Continuation(Input, Output)` contract outside the
  prelude, plus native state-handler and abort coverage.
- Kept operation propagation through separately compiled effectful functions and loop backedges
  explicitly unsupported until the typed CPS lowering is completed.

## 0.110.0 - 2026-07-22

- Added parameterized user effect declarations with typed operation requirements.
- Added exact nominal effect applications such as `State(i32)` to callable rows, generic
  substitution, module resolution, visibility checking, inference, and mangling.
- Added qualified operation calls with ordinary parameter groups, passing modes, result checking,
  partial application, and automatic row propagation.
- Documented the function-shaped derived handler and one-shot continuation design that the next
  implementation slice will lower; this release does not claim handler support.

## 0.109.0 - 2026-07-22

- Added transparent concrete aliases and parameterized type-family aliases.
- Added first-class type-constructor binding syntax such as
  `let Constructor: (T: type): type = Box` without a runtime representation.
- Expanded aliases across signatures, bodies, constructors, traits, extensions, and modules before
  semantic lowering while preserving nominal identity and constructor inference.
- Added deterministic diagnostics for recursive aliases and constructor-arity mismatches.

## 0.108.0 - 2026-07-22

- Added source-backed `BitAndAssign`, `BitOrAssign`, `BitXorAssign`, `ShlAssign`, and `ShrAssign`
  protocols to `core.ops`.
- Implemented `&=`, `|=`, `^=`, `<<=`, and `>>=` for built-in integer places and nominal values.
- Preserved single evaluation of the left place, checked shift semantics, and forced nominal
  dispatch through the validated assignment lang item.

## 0.107.0 - 2026-07-22

- Added source-backed, edition-validated `AddAssign`, `SubAssign`, `MulAssign`, `DivAssign`, and
  `RemAssign` traits to `core.ops`.
- Implemented `+=`, `-=`, `*=`, `/=`, and `%=` for mutable integer places and nominal values that
  implement the matching assignment trait.
- Resolved the left place once, preserved integer division/remainder traps, and forced nominal
  dispatch through the validated lang-item trait instead of same-named inherent methods.
- Added native coverage for locals, fields, constant array places, and user-defined assignment
  implementations, plus diagnostics for immutable places and missing implementations.

## 0.106.0 - 2026-07-22

- Implemented `while let pattern = value { ... }` as a unit-producing loop whose scrutinee is
  reevaluated on every iteration.
- Lowered each iteration through ordinary `match`, preserving pattern binding scope, ownership,
  mutable receiver calls, and cleanup behavior.
- Routed `break` and `continue` through the generated ordinary loop and added native coverage for
  repeated `Option` extraction with `continue`.

## 0.105.0 - 2026-07-22

- Implemented `if let pattern = value { ... }` with optional `else` and `else if` branches.
- Lowered conditional destructuring through the existing `match` machinery so the scrutinee is
  evaluated once and pattern bindings, moves, borrows, branch joins, and cleanup use one semantic
  path.
- Kept pattern bindings scoped to the successful branch and made a missing `else` produce `()`.
- Added native enum-destructuring coverage and a diagnostic fixture for bindings escaping their
  successful branch.

## 0.104.0 - 2026-07-22

- Added source-backed, edition-validated `Iterator` and `IntoIterator` lang-item traits in the new
  non-prelude `core.iter` module.
- Implemented `for name in value { ... }` and wildcard iteration, evaluating the iterable once and
  lowering exclusively through the validated iteration traits rather than ordinary same-named
  method lookup.
- Reused loop ownership, cleanup, `break`, and `continue` lowering for iteration while fixing `for`
  to the unit result type and rejecting value-carrying breaks.
- Added native execution coverage for a user-defined consuming iterable and mutable iterator, plus
  diagnostics for missing iteration conformance and invalid break values.

## 0.103.0 - 2026-07-22

- Added validated, edition-pinned `core.control` declarations for the `do`, `try`, `unsafe`, and
  `loop` trailing-closure control functions instead of leaving their contracts solely in compiler
  implementation code.
- Added source-backed `Unsafe` and parameterized `Throws(E)` effect identities plus `Shared` and
  `Mutable` access identities, all outside the prelude.
- Allowed bodyless top-level function signatures and `access` declarations only as the parsed form
  of validated compiler-provided core contracts; ordinary packages cannot define control intrinsics
  or bodyless functions.
- Established that future compiler-lowered features must land their function, type, effect, and
  capability contracts in the matching standard-library release. Async/`await` declarations remain
  absent until their lowering is implemented.

## 0.102.0 - 2026-07-22

- Removed the source-level `void` alias completely; the unit type and no-result function type now
  have the single spelling `()`.
- Stopped reserving or normalizing `void`, so it is treated as an ordinary unresolved name unless a
  program declares that name itself.
- Updated the grammar, language specification, and core-library documentation to use `()` while
  retaining `void` only where it is the correct LLVM or C ABI spelling.

## 0.101.0 - 2026-07-22

- Completed label-directed overloads for methods and associated functions declared by blanket
  generic inherent extensions.
- Precomputed overload identities across all applicable generic extension blocks and reproduced
  the same candidate set for every concrete nominal instance.
- Preserved direct generic associated-function inference and per-instance method dispatch without
  allowing type arguments or where predicates to participate in overload choice.

## 0.100.0 - 2026-07-22

- Implemented `continue` for `while` and value-producing `loop`, targeting the nearest enclosing
  loop and rejecting use outside a loop or across an immediate closure boundary.
- Added continue edges to ownership-flow joins and loop-carried move validation.
- Routed continue edges through lexical cleanup planning and LLVM cleanup emission before branching
  to the while condition or loop body.

## 0.99.0 - 2026-07-22

- Allowed generic and concrete top-level functions to coexist in one label-directed overload set.
- Selected a generic template from explicit runtime argument labels before inferring or consuming
  its compile-time argument groups, keeping types and compile-time values out of overload choice.
- Extended the same ordering to generic members declared on concrete inherent extensions, for both
  methods and associated functions.
- Kept named compile-time arguments from serving as overload evidence; a runtime parameter label is
  still required, whether compile-time arguments are inferred or explicit.

## 0.98.0 - 2026-07-22

- Extended label-directed overloads to trait requirements, concrete implementations, default
  implementations, blanket implementations, and assumed where-bound dispatch.
- Used each requirement's complete runtime parameter-label shape as its conformance identity, so
  implementations are checked against the exact overload without consulting argument types.
- Added named-argument selection for trait instance methods and associated functions, including
  optional-chain probing and automatic `throws` propagation.
- Allowed named arguments to disambiguate otherwise competing same-named methods from multiple
  visible traits while retaining a focused ambiguity diagnostic for positional calls.

## 0.97.0 - 2026-07-22

- Extended label-directed overload sets to concrete inherent methods and associated functions,
  including declarations spread across multiple extension blocks.
- Excluded the implicit method receiver group from disambiguation evidence: instance and qualified
  calls must name an explicit parameter that uniquely selects the member overload.
- Preserved selected member identity through optional-chain probing, throws inference, effect
  lowering, stable mangling, and native calls.
- Kept generic inherent members and trait requirements outside this slice so their compile-time
  parameter and conformance identities can be added without type-directed fallback rules.

## 0.96.0 - 2026-07-22

- Added label-directed overload sets for concrete top-level functions: overload declarations must
  differ in runtime parameter-group labels, and a call must contain a named argument that selects
  exactly one candidate.
- Allowed all supplied curried groups to participate in selection, so a later named group can
  disambiguate candidates while other groups remain positional.
- Kept argument types, passing modes, effects, and return types out of overload identity; duplicate
  label shapes, positional-only calls, missing candidates, and incomplete disambiguation receive
  focused diagnostics.
- Preserved overload sets through module resolution, imports, type probing, immediate local closure
  lowering, throws-handler inference, stable internal mangling, and native LLVM calls.

## 0.95.0 - 2026-07-22

- Made an unannotated `try { ... }` infer `Result(T, E)` when its body has exactly one escaping
  `throws(E)` error type; contextual `Result` annotations remain available to select or convert an
  otherwise ambiguous boundary.
- Included complete direct, method, and non-capturing indirect calls plus explicit `throw` sites in
  handler inference, while nested `try` handlers do not leak their already-handled errors outward.
- Added focused diagnostics for bodies with no escaping error source, multiple error types, or an
  unprobeable success type.
- Generalized type probing for local function and callable values and preserved zero-capture named
  function aliases across immediate handler closure lowering.

## 0.94.0 - 2026-07-22

- Defined named functions as closure-declaration sugar: their parameter groups are lifted beside
  the binding name and every implementation body must use `= { ... }`; direct `= expression`
  function, method, and trait-default bodies were removed before 1.0.
- Made every brace expression a closure, including non-empty zero-parameter closures such as
  `{ expensive_work() }`; `do { ... }` remains the effect-polymorphic immediate invocation form.
- Removed the redundant `{ -> expression }` zero-parameter closure spelling while preserving
  `->` for explicit closure parameter groups, including curried groups.
- Migrated the standard library, examples, compiler suites, fixtures, and language documentation,
  with focused diagnostics for both removed spellings.

## 0.93.0 - 2026-07-21

- Made `do { ... }` transparently forward the complete active effect row through its immediate
  closure boundary, including blocks whose local `return` requires a lifted function.
- Preserved parameterized `throws(Error)` carrier lowering across `do`, then automatically
  propagated the immediate call back into the enclosing throws boundary.
- Forwarded `unsafe` authorization and nominal marker effects into lifted `do` and `try` closure
  bodies instead of checking them as accidentally pure nested functions.
- Added combined throws/unsafe/custom lowering coverage, exact mismatched-error diagnostics, and a
  native success/error regression for throwing `do` blocks.

## 0.92.0 - 2026-07-21

- Removed the edition language items `Try`, `FromResidual`, `FromError`, and their supporting
  `ControlFlow` type; automatic error propagation is now defined solely by `throws(Error)`.
- Removed implicit normal-tail and `return` wrapping for `Option` and `Result`; ordinary container
  return types now require explicit `Some`, `None`, `Ok`, or `Err` construction.
- Extended `E: effect` rows to carry `throws(Error)`, including error-type inference from
  higher-order callable arguments, explicit effect arguments, forwarding, monomorphization, and the
  current `Result(T, Error)` ABI specialization.
- Updated core validation, fixtures, implementation status, and standard-library documentation to
  remove the obsolete control-container protocol model.

## 0.91.0 - 2026-07-21

- Replaced `with(try)` / `with(try(Error))` with the explicit error effect
  `with(throws(Error))`; a throwing function keeps logical result `T` while the current native ABI
  lowers it through `Result(T, Error)`.
- Made complete direct, method, partial, and non-capturing indirect throwing calls propagate
  automatically through a matching throws boundary, with exact error types and no postfix `.try`.
- Added `try { ... }` as the error handler that removes `throws(Error)` and yields an explicit
  `Result(T, Error)`, including normal-tail and `throw` lowering through an immediate closure.
- Removed the obsolete postfix propagation implementation and migrated its end-to-end fixtures;
  the deleted spellings now receive focused migration diagnostics rather than compatibility aliases.
- Unified the language design for future control effects: async calls will be suspension points by
  virtue of their effect row, while `async { ... }` will handle that effect without postfix `.await`.

## 0.90.0 - 2026-07-21

- Added requirement subtyping for callable effect rows: a callable requiring fewer effects can fill
  a wider pure/unsafe/custom slot, while narrowing an effectful callable to a less demanding slot is
  rejected.
- Preserved the widened slot's effect checks through local annotations, higher-order parameters,
  and native indirect calls without changing the function-pointer ABI.
- Kept `E: effect` inference exact to the callable argument's actual row and kept trait requirement
  and implementation signatures exact, separating polymorphic inference and protocol conformance
  from ordinary callable assignment compatibility.
- Added native and diagnostic regressions for pure-to-unsafe/custom widening and effectful-to-pure
  rejection.

## 0.89.0 - 2026-07-21

- Replaced the pre-result effect group with one contextual post-result spelling for declarations and
  callable types: `let read(): T with(unsafe)` and `(): T with(unsafe)`. The removed
  `(unsafe): T`, `(E): T`, `T(effect)`, and `T ! effect` forms have no compatibility aliases.
- Added nominal marker effects declared by `let UI = effect`, including module qualification,
  imports, visibility boundaries, duplicate/unknown diagnostics, callable identity, and static call
  checking through `with(UI)`.
- Generalized `E: effect` from the built-in pure/unsafe choice to a complete inferred effect row.
  Higher-order functions can now infer and forward pure, unsafe, and custom rows from callable
  arguments while keeping effect arguments compile-time-only.
- Added the first native ABI for first-class non-capturing named functions. Function values are
  `Copy`, lower to LLVM function pointers, and can be passed to and invoked by higher-order code;
  effect requirements remain checked at indirect calls.
- Kept `with(try)` and `with(try(Error))` as explicit Option/Result carrier normalization rather
  than treating failure representation as a hidden runtime channel or a marker row.

## 0.88.0 - 2026-07-21

- Moved effects from return-type arguments into a dedicated function-signature group after all
  runtime parameter groups and before the result colon, such as `(value: T)(unsafe): U`.
- Removed the pre-1.0 `T(effect)` spelling in declarations and callable types, with a direct
  migration diagnostic instead of a compatibility alias.
- Defined effect groups as compile-time signature metadata rather than runtime or currying groups;
  partial application still produces the effect only when the final runtime group is applied.
- Added source callable types with the same shape, such as `(i32)(unsafe): i32`, and made unsafe
  color part of their internal identity, substitution, generic constraints, diagnostics, and
  canonical encoding.
- Preserved `try` carrier normalization in the new position: `(try): T` returns `Option(T)` and
  `(try(E)): T` returns `Result(T, E)`.

## 0.87.0 - 2026-07-21

- Added the contextual `effect` compile-time kind for functions and generic inherent members, with
  concrete `pure` and `unsafe` values and pure-by-default group omission.
- Allowed declared effect parameters in return groups (`T(E)` and `T(unsafe, E)`) and in ordinary
  positional or named compile-time forwarding calls such as `callee(E)(value)`.
- Specialized effect-generic functions per selected row so direct, forwarded, method, and native
  calls enforce the resulting unsafe requirement without adding a runtime argument.
- Rejected effect parameters on data, trait, associated-type, and extend headers, as well as their
  misuse in runtime type positions.
- Kept `try` and `try(Error)` as explicit carrier-selecting return effects rather than hiding an ABI
  change behind the current call-requirement generic.

## 0.86.0 - 2026-07-21

- Replaced the punctuation-separated `T ! unsafe` spelling with return-type compile-time effect
  groups such as `T(unsafe)` and removed the old spelling before 1.0.
- Added `T(try)` as the Option-return boundary form and `T(try(E))` as the Result-return boundary
  form, reusing the existing source-backed `Try`, `FromResidual`, and `FromError` protocols.
- Allowed deterministic combinations such as `T(try(E), unsafe)`: try selects the returned carrier
  while unsafe remains a statically checked call requirement discharged by `unsafe { ... }`.
- Made duplicate effects and effect groups on non-function values parse errors, while preserving
  exact effect matching for trait methods and final-group-only behavior for curried functions.
- Added parser, semantic, LLVM, and native execution coverage for Option, Result, throw,
  propagation, unsafe handling, methods, aliases, and partial applications under the new syntax.

## 0.85.0 - 2026-07-21

- Changed the only recognized Salicin source extension from `.sali` to `.sc`, with the shorter
  spelling suggesting “successor C” or “super C”.
- Renamed all compiler-owned core and alloc sources, examples, and 524 end-to-end fixtures to
  `.sc`.
- Updated single-file input resolution, recursive file-module discovery, manifest target
  validation, and default `src/lib.sc` / `src/main.sc` target discovery.
- Removed the old extension before 1.0 rather than retaining a compatibility alias, with explicit
  CLI and manifest rejection coverage for `.sali` inputs.

## 0.84.0 - 2026-07-21

- Added `! unsafe` to function and method signatures as the first statically checked source-level
  effect annotation.
- Allowed declared unsafe functions to perform unsafe operations and call one another without an
  inner handler, while requiring pure callers to use `unsafe { ... }`.
- Preserved the effect requirement through complete direct calls, methods, named callable aliases,
  and curried partial applications; partial application itself remains effect-free until the final
  parameter group is applied.
- Made unsafe color part of concrete and generic trait method signature matching, and rejected an
  effectful `main` whose native caller could not discharge the effect.
- Specified the intended `effect` row kind, the distinction between `access` and effects, and the
  initial `unsafe`, `try(R)`, and async effect/handler model without prematurely treating IO,
  allocation, or ordinary state as built-in effects.

## 0.83.0 - 2026-07-21

- Redefined `do { ... }` as an immediately invoked zero-parameter trailing closure whose `return`
  and `break` behavior follows an ordinary function boundary.
- Removed the pre-1.0 `try do` and `try Container do` forms; a contextually Try-typed `do` closure
  now handles `.try` and `throw` through the ordinary closure return protocol.
- Replaced `unsafe do { ... }` with the uniform trailing-closure form `unsafe { ... }` and rejected
  the removed spelling instead of retaining a compatibility alias.
- Made `do` forward the implemented unsafe color through lifted immediate closures, matching the
  effect-polymorphic design intended for future async and typed try colors.
- Migrated the allocation library and compiler fixtures to the new syntax and added native coverage
  for local `do` returns, Try propagation, and unsafe-color forwarding.

## 0.82.0 - 2026-07-21

- Added fixed lexical tokens and conventional precedence for `&`, `|`, `^`, `<<`, and `>>`.
- Added edition-validated `BitAnd`, `BitOr`, `BitXor`, `Shl`, and `Shr` protocols with consuming
  operands and associated output types, including generic where-bound dispatch.
- Kept primitive integer bitwise operations on direct LLVM instructions, using arithmetic signed
  right shifts and logical unsigned right shifts.
- Defined negative and out-of-width shift counts to fail deterministically: compile-time constants
  are rejected and runtime values trap before reaching an LLVM poison-producing shift.
- Added malformed-core, import, parser, LLVM, ownership, and native execution coverage.

## 0.81.0 - 2026-07-21

- Added edition-validated `core.ops.Neg` and `core.ops.Not` protocols with consuming operands and
  associated `Output` types.
- Lowered unary `-` and `!` on user-defined nominal types to statically selected protocol methods,
  including non-boolean overloaded `!` results and checked temporary consumption.
- Recognized built-in signed-integer `Neg` and boolean `Not` implementations in generic where
  predicates while preserving their direct LLVM lowering.
- Added exact malformed-core rejection, missing-import and missing-implementation diagnostics,
  output-context checking, move-after-use coverage, and native generic execution tests.

## 0.80.0 - 2026-07-21

- Added the edition-validated `PartialOrdering` four-state enum and borrowing
  `core.ops.PartialOrd(Rhs)` protocol.
- Lowered `<`, `<=`, `>`, and `>=` on user-defined nominal types through one statically selected
  `partial_cmp` call, while preserving direct LLVM comparisons for primitive integers.
- Defined `Unordered` to make all four relational operators false and preserved single evaluation
  of both operands and the protocol method.
- Added exact core-shape validation, explicit-import diagnostics, LLVM dispatch assertions, and
  native execution coverage for ordered and unordered values.

## 0.79.0 - 2026-07-21

- Added the edition-validated `core.ops.Eq(Rhs)` language protocol with borrowing `eq` operands and
  a fixed `bool` result.
- Lowered `==` on user-defined nominal types to statically selected `Eq.eq` implementations and
  lowered `!=` to one such call followed by boolean negation, preserving single evaluation.
- Kept primitive integer and boolean equality as direct LLVM comparisons and required ordinary
  imports only when source names or implements `Eq`.
- Added malformed-core rejection, HIR/LLVM dispatch checks, and native execution coverage for both
  equality operators.
- Updated the implementation status to reflect the completed generalized `Try` protocol rather
  than listing it as future work.

## 0.78.0 - 2026-07-21

- Added prefix `try do { ... }` expressions and explicitly typed `try Container do { ... }`
  propagation blocks.
- Inferred an omitted block container from the surrounding expected type and required every chosen
  container to implement the edition-validated `Try` protocol.
- Lowered each propagation block through a private immediate boundary closure so `.try` and `throw`
  leave the nearest block while ordinary outer locals remain available through checked captures.
- Applied `Try.from_output`, `FromResidual`, and `FromError` inside the block exactly as for function
  boundaries, including custom containers.
- Added parser coverage and native execution coverage for explicit/contextual containers, captured
  locals, success wrapping, residual propagation, and `throw` without exiting the outer function.
- Diagnosed plain `return` inside `try do` until non-local return lowering is added, avoiding the
  incorrect behavior of treating the implementation closure as a source-level closure boundary.

## 0.77.0 - 2026-07-21

- Generalized explicit function propagation boundaries from built-in `Option` and `Result` to any
  nominal return type implementing the edition-validated `core.control.Try` protocol.
- Wrapped custom-boundary function tails and explicit `return` success values through the
  implementation's receiver-free `Try.from_output` function while preserving already wrapped
  container returns.
- Propagated `.try` residuals through custom `FromResidual` implementations and accepted both
  built-in and user-defined `Try` operands at custom boundaries.
- Lowered `throw error` through a custom boundary's matching `FromError(E)` implementation with
  contextual error inference and deterministic missing-conversion diagnostics.
- Added a native execution fixture covering successful custom propagation, residual propagation,
  explicit-return wrapping, and custom `throw`, all through ordinary library traits.

## 0.76.0 - 2026-07-21

- Allowed user-defined nominal types implementing the validated `core.control.Try` language trait
  to serve as postfix `.try` operands.
- Lowered custom propagation through the implementation's consuming `branch()` method and the
  edition-pinned `ControlFlow.Continue` / `ControlFlow.Break` variants, evaluating the operand once.
- Routed custom residuals through the enclosing `Option` or `Result` boundary's validated
  `FromResidual` implementation and rejected unsupported residual conversions before lowering.
- Checked custom `Try.Output` against the surrounding expected type and added end-to-end HIR/LLVM
  coverage for a user enum propagating into `Result`.
- Kept propagation boundaries deliberately limited to `Option` and `Result` in this release; custom
  `Try` return boundaries remain the next generalization step.

## 0.75.0 - 2026-07-21

- Implemented `Try`, `FromResidual`, and `FromError` for the edition-pinned generic `Option` and
  `Result` types in ordinary `core.control` Salicin source.
- Added receiver-free trait implementation functions and static `Type.function(...)` dispatch,
  including generic blanket implementations, ambiguity checks, receiver-shape validation, and
  source-shadowing isolation for compiler-owned core templates.
- Required built-in `.try` propagation boundaries and operands to carry the validated `Try`
  language-item implementation, and required matching `FromResidual` or `FromError` implementations
  before `.try` or `throw` may propagate.
- Prevented generic enum variant inference from intercepting trait associated functions on explicit
  generic type heads such as `Option(i32).from_output(42)`.
- Removed the dedicated pre-1.0 `mut borrow` compatibility diagnostic paths; `borrow(mut)` is the
  sole mutable-borrow grammar and legacy token sequences are ordinary invalid syntax.

## 0.74.0 - 2026-07-21

- Added the edition-pinned `core.control` module with validated `ControlFlow`, `Try`,
  `FromResidual`, and `FromError` language protocol declarations.
- Mounted error-control names in the reserved standard-library namespace and required ordinary
  `use core.control...` imports whenever source names them, with precise missing-import diagnostics.
- Allowed receiver-free functions in trait schemas so conversion constructors such as
  `from_output`, `from_residual`, and `from_error` have direct declaration shapes.
- Extended the core bundle contract, canonical identities, provenance checks, export synchronization,
  standard-library documentation, and resolver coverage to the new module.
- Removed stale compatibility wording from the embedded prelude; `.try` and `throw` remain syntax
  and do not inject protocol names into lexical scope.

## 0.73.0 - 2026-07-21

- Split embedded `core.prelude` and `core.ops` semantic identities: `Option`, `Result`, `never`,
  `Copy`, and `Drop` remain implicit, while arithmetic protocol traits use qualified identities.
- Required ordinary `use core.ops...` imports whenever source names `Add`, `Sub`, `Mul`, `Div`, or
  `Rem`, with precise missing-import diagnostics, grouped imports, renaming, and re-export support.
- Kept operator tokens independent of lexical imports: built-in arithmetic and overloaded operator
  dispatch continue to use the edition-validated lang-item identity.
- Allowed user declarations named `Add`, `Sub`, `Mul`, `Div`, or `Rem` without granting them operator
  behavior or colliding with compiler-owned traits.
- Reserved both `core` and `alloc` against top-level file modules and dependency aliases, including
  manifest validation, and migrated all arithmetic trait fixtures and package tests to explicit
  imports.

## 0.72.0 - 2026-07-21

- Mounted the compiler-owned `alloc.boxed` and `alloc.vec` modules in ordinary package name
  resolution, including grouped imports, renaming, module aliases, qualified paths, and re-exports.
- Removed `Box`, `Vec`, and all alloc free functions from implicit prelude visibility; a bare use now
  reports the exact `use alloc...` declaration required.
- Gave embedded alloc declarations qualified internal identities so user packages may define their
  own unimported `Box` or `Vec` without colliding with the standard library.
- Reserved the top-level `alloc` namespace against file modules, declarations, and dependency
  aliases, and migrated all native ownership, borrow, bounds, and allocator fixtures to explicit
  imports.
- Removed the obsolete failure fixture that treated a user-defined `Box` as a forbidden prelude
  redefinition.

## 0.71.0 - 2026-07-21

- Added the contextual compile-time `passing` kind with `auto`, `copy`, and `move` values.
- Allowed a declared passing parameter directly in parameter keyword position, as in
  `let identity(P: passing, T: type)(P value: T): T = value`, without reserving parameter names.
- Defaulted omitted passing arguments to `auto`, accepted positional and named explicit arguments,
  and included the selected strategy in monomorphization keys and generic forwarding.
- Enforced Copy constraints for `copy` instances and preserved explicit move semantics even for Copy
  types, with native and diagnostic coverage for functions and generic inherent methods.
- Kept borrowing orthogonal under `borrow(A)` and rejected `passing` parameters on data, trait, and
  extend headers where a parameter keyword has no coherent meaning.

## 0.70.0 - 2026-07-21

- Enabled generic methods and associated functions inside generic inherent extensions, combining
  outer nominal parameters with member-owned `type`, `access`, and region groups.
- Added access inference for generic method receivers and expected reference types, including
  `value.view()` for shared access and `value.view(mut)()` for exclusive access.
- Replaced the source spelling `mut borrow` with the single compositional form `borrow(mut)` across
  parameters, types, expressions, receivers, traits, core, alloc, and diagnostics.
- Removed the pre-1.0 compatibility APIs `box_as_mut`, `vec_at_mut`, `Box.as_mut`, `Vec.at_mut`, and
  `raw_mut_borrow`; their access-generic counterparts are now the only APIs.
- Reclassified generic-extension member coverage from a rejected fixture to native execution and
  retained deterministic diagnostics for member parameters that shadow outer parameters.

## 0.69.0 - 2026-07-21

- Added the compile-time `access` kind with `shared` and `mut` values, default-shared inference,
  named or positional explicit arguments, and distinct monomorphized instances.
- Allowed `borrow(A)` in parameter modes, borrow types, borrow expressions, and unsafe raw pointer
  borrows, plus `borrow(A)(R)` for borrow types that also carry a region, with access-aware
  substitution before ownership and borrow checking.
- Unified the canonical free borrow APIs as `box_as_ref(A: access, ...)` and
  `vec_at(A: access, ...)`; retained the mutable free functions and methods as compatibility wrappers.
- Added scope diagnostics for undeclared or duplicate access parameters and documented the boundary
  between access capability generics and future general effect polymorphism.

## 0.68.0 - 2026-07-21

- Reorganized the repository around explicit `compiler`, `library`, `runtime`, `docs`, `examples`,
  and `tests` boundaries while preserving the `salic` crate and binary entry points.
- Reduced the root README to project orientation and moved language, grammar, architecture,
  standard-library, runtime, and implementation-status material under a single `docs` entry point.
- Made this changelog the sole release-history document and removed release narratives from the
  language specification and library documentation.
- Split compiler-embedded `core` into `prelude` and `ops` modules and `alloc` into `boxed` and `vec`
  modules, retaining precise module provenance during bootstrap validation.
- Defined a narrow edition-prelude policy and documented the remaining compatibility-injection step
  before non-prelude standard-library names are enforced through ordinary `use` declarations.

## 0.67.0 - 2026-07-21

- Added checked `Vec.swap(left, right)` for all element types, exchanging distinct initialized
  slots through ownership moves and treating identical indices as a no-op.
- Added in-place `Vec.reverse()` over the same resource-safe swap path, including empty and
  single-element vectors without unsigned underflow.
- Added native coverage for resource ordering, zero premature destructor calls, exact final
  destruction, identical-index swaps, empty reversal, and independent left/right bounds traps.

## 0.66.0 - 2026-07-21

- Added `box_as_ref`/`Box.as_ref(): borrow T` and `box_as_mut`/`Box.as_mut(): mut borrow T`, giving
  Copy and resource pointees safe lifetime-bound access without exposing raw pointers.
- Tied Box pointee references to the Box receiver loan, rejecting replace, into-inner, mutation, or
  conflicting access while a returned reference remains live.
- Applied compile-time region erasure uniformly to embedded core, embedded alloc, and user source,
  allowing region-generic bootstrap functions to infer ordinary type arguments at call sites.
- Added native and diagnostic coverage for free and method borrow forms, resource field reads and
  writes, region inference, consume/replace conflicts, and shared/mutable alias exclusion.

## 0.65.0 - 2026-07-21

- Added unsafe `raw_borrow(pointer, borrow anchor)` and
  `raw_mut_borrow(pointer, mut borrow anchor)` intrinsics, converting initialized raw storage to a
  reference whose lexical loan and returned region are tied to an explicit owner anchor.
- Added bounds-checked `Vec.at(index): borrow T` and `Vec.at_mut(index): mut borrow T` for Copy and
  resource elements, with inferred method regions and explicit-region free-function forms.
- Lowered raw borrows as pointer-preserving LLVM values while integrating their anchor loans with
  reference escape proof, cleanup planning, alias conflicts, and mutable-reference move semantics.
- Added native and diagnostic coverage for shared reads, mutable writes, resource fields, bounds
  traps, safe-context rejection, pointer and anchor kind checks, and mutation/growth conflicts.

## 0.64.0 - 2026-07-21

- Added ownership-aware ordered `Vec.insert(index)(value)` and `Vec.remove(index): T`, shifting
  elements with moves so resource vectors preserve order without requiring `Copy`.
- Added `Vec.append(other)` through two mutable borrows; every source element transfers to the
  destination and the source remains allocated but logically empty.
- Added `Vec.shrink_to_fit()`, moving initialized elements into an exact-length allocation without
  running their destructors.
- Added native coverage for Copy ordering, resource transfer and exact destruction, insertion and
  removal bounds, post-append source reuse invariants, and rejection of self-append aliasing.

## 0.63.0 - 2026-07-21

- Added checked `Vec.reserve(additional)` and made `push` reuse it as the single capacity-growth
  path, including overflow checks for both required capacity and target layout size.
- Added ownership-aware `swap_remove(index): T`, moving the last element into the removed slot
  without copying resource owners.
- Added `truncate(new_length)`, `clear()`, and `is_empty()`; shrinking eagerly drops each removed
  resource exactly once while retaining the allocation for reuse.
- Added native coverage for Copy and resource operations, growth without premature destruction,
  no-op and repeated clearing, exact destructor counts, reserve overflow, and swap-remove bounds.

## 0.62.0 - 2026-07-21

- Generalized `Vec.new`, `with_capacity`, and `push` from `T: Copy` to every represented element
  type, using inferred parameter passing so Copy inputs copy while resource inputs move.
- Moved initialized elements during growth with `raw_take`/`raw_init`, avoiding premature or double
  destruction when the old allocation is released.
- Added ownership-aware `Vec.replace(index)(value): T` and `Vec.pop(): Option(T)` operations with
  checked indexing and exact transfer of element ownership.
- Made the source-backed Vec destructor move out and drop every remaining initialized element before
  deallocating storage, including zero-sized resource elements.
- Added native coverage for resource growth, preallocation, replace/pop, empty pop, move diagnostics,
  exact destructor counts, Copy reuse, bounds traps, and zero-sized resource destruction.

## 0.61.0 - 2026-07-21

- Clarified that `_` is never a type or compile-time argument inference placeholder, including in
  named arguments and array lengths; it remains available only in non-inference ignore slots such
  as wildcard patterns.
- Added the first source-defined `Vec(T)` for `T: Copy`, with `new`, `with_capacity`, `len`,
  `capacity`, `push`, checked `read`/`write`, geometric growth, and allocation-owning `Drop`.
- Checked capacity doubling and `capacity * size_of(T)` before allocation, including zero-sized
  elements, and added the unsafe diverging `raw_trap()` intrinsic used to close safe wrapper paths.
- Recognized unit as an explicit expression-level type argument and kept abstract generic type
  markers valid in named compile-time arguments during generic-body validation.
- Added native coverage for growth, preallocation, zero-sized elements, bounds and layout traps,
  rejected resource elements, and deallocation through an overridden allocator ABI.

## 0.60.0 - 2026-07-21

- Added the unsafe `raw_offset(pointer, index)` intrinsic for both `Ptr(T)` and `Ptr(mut)(T)`, with a
  `u64` element index and result mutability matching the input pointer.
- Lowered pointer offsets to LLVM `getelementptr` using the concrete pointee layout, so aggregate
  padding and target-specific element size determine the byte displacement without host guesses.
- Defined zero-sized pointee offsets as the same pointer and added native tests for multi-element
  allocation, move initialization/take, deallocation, unit pointees, and unsafe/type diagnostics.

## 0.59.0 - 2026-07-21

- Added borrow value types to ordinary function parameters: shared references follow `Copy`,
  mutable references follow `move`, and explicit `copy` or `move` modes retain their usual
  ownership semantics.
- Distinguished loading a reference value from automatically dereferencing its pointee, enabling
  direct and generic calls, mutable writes through parameters, and safe reference-value forwarding
  through explicit or inferred regions.
- Carried loans with reference locals across value-producing block scopes, rejected use after a
  mutable or explicitly moved reference, local-source escape, and partial applications that would
  capture a reference before callable lifetime tracking is available.

## 0.58.0 - 2026-07-21

- Inferred an omitted returned-borrow region when a function or method has exactly one borrow
  parameter, covering shared, mutable, generic, inherent-method, and forwarding cases.
- Contextualized an inferred-region call result to an explicit expected return region, allowing a
  concise helper to forward safely through an explicitly region-bound public API.
- Diagnosed omitted result regions with zero or multiple borrow sources and continued to require an
  explicit region for those ambiguous signatures.

## 0.57.0 - 2026-07-21

- Added returned borrows from inherent and trait methods, including instance syntax,
  type-qualified receiver calls, generic methods, and forwarding through another returned-borrow
  function.
- Promoted the receiver and any other result-region argument loans into the caller's lexical scope
  for complete bound-method calls, preserving shared and mutable access conflicts.
- Rejected references returned from temporary receivers and method results that would let a local
  receiver escape its function.

## 0.56.0 - 2026-07-21

- Added reference-returning free and associated functions whose result is tied to an explicit
  region on one or more borrow parameters, including field projections, generic identities, and
  forwarding calls.
- Lowered returned references through the LLVM pointer ABI and added indirect reference locals
  with automatic reads, field access, and mutable assignment.
- Promoted source loans into the caller's lexical scope and diagnosed local or temporary escape,
  mutable escalation, conflicting access, missing source regions, by-value reference parameters,
  and reference fields. Reference-returning methods remain deferred until receiver-loan promotion
  is implemented.

## 0.55.0 - 2026-07-21

- Added the `region` compile-time kind with uppercase region parameters such as `R: region`, while
  keeping apostrophe-prefixed region entities such as `'static` reserved for concrete regions.
- Parsed explicit regions on borrow pass modes and borrow types, such as `borrow(R) value: T` and
  `borrow(R) T`, while erasing regions before monomorphization so they never become type arguments.
- Added whole-item region scope validation for functions, local annotations, closures, nominal
  fields, traits, extensions, and where predicates, with diagnostics for undeclared, duplicate,
  malformed, or redeclared predefined regions.

## 0.54.0 - 2026-07-21

- Added explicit `borrow T` and `mut borrow T` type syntax to the AST, parser, module resolver,
  generic substitution, visibility traversal, and type-pattern infrastructure.
- Allowed those types on local `let` aliases, validating that the initializer is a borrow with the
  same mutability and pointee type while preserving alias-based ownership and cleanup semantics.
- Kept borrow types out of function and data signatures until region parameters can prove escape
  safety, with dedicated diagnostics instead of silently treating a borrow as an owned value.

## 0.53.0 - 2026-07-21

- Added expression-level `Self` inside concrete and generic inherent or trait extensions, including
  constructors, associated members, type-qualified method calls, and enum constructor patterns.
- Allowed default trait methods to use `Self.method(self: value)(...)`; abstract validation rewrites
  the qualified receiver while concrete implementations retain static type-qualified dispatch.
- Added a contextual diagnostic for expression-level `Self` outside extension members and fixed
  move/copy temporary receiver staging for chained calls.

## 0.52.0 - 2026-07-21

- Added type-qualified inherent and trait method calls such as `Number.read(number)()` while
  reusing the same borrow, mutable-borrow, move, partial-application, and temporary-receiver rules
  as instance syntax.
- Allowed `self:` as a receiver argument label to select a method when an associated function has
  the same name; unlabelled type calls continue to prefer associated functions and enum variants.
- Inferred omitted generic nominal arguments from a concrete receiver, allowing
  `Cell.read(cell)()` to resolve a `Cell(i32)` method without `_` syntax.

## 0.51.0 - 2026-07-21

- Extended `match` to boolean and integer scrutinees with literal, binding, wildcard, and guarded
  arms.
- Staged each scalar scrutinee exactly once and gave every guard and arm body an independent
  lexical scope while preserving source-order candidate fallback.
- Recognized unguarded `true` plus `false` as an exhaustive boolean match; integer matches and
  guarded boolean coverage still require an unguarded wildcard or binding fallback.

## 0.50.0 - 2026-07-21

- Added integer and boolean literal patterns inside enum payloads and nested struct patterns.
- Lowered literal tests into short-circuit candidate guards, preserving source-order fallback and
  evaluating explicit guards only after every literal test succeeds.
- Kept resource pattern bindings speculative until literal tests pass, then committed their moves
  exactly once; added type and integer-range diagnostics for invalid literal payload patterns.

## 0.49.0 - 2026-07-21

- Allowed a constant index to move a resource element directly out of an array temporary,
  including array literals and arrays returned from calls.
- Transferred the selected element from its cleanup constant-index path while dropping every
  unselected resource element exactly once.
- Continued to require `Copy` elements for dynamic indexing because a runtime-selected element
  does not yet have a finite static move-path identity.

## 0.48.0 - 2026-07-21

- Allowed fixed arrays to contain move-only nominal and resource values instead of requiring every
  element to implement `Copy`.
- Completed resource-array cleanup by discovering local array drop glue, expanding runtime drop
  slots per element, and preserving exactly-once drop across element moves, reinitialization, and
  overwrites.
- Kept dynamic and temporary-array indexing limited to `Copy` elements; resource elements must be
  accessed through a bound array and a constant-index place so ownership has a finite identity.

## 0.47.0 - 2026-07-21

- Promoted fixed-array constant indices to full places, allowing reads, shared and mutable
  borrows, assignment, explicit moves followed by reinitialization, and raw-pointer construction.
- Tracked every fixed-array element as a distinct ownership and loan leaf, so disjoint constant
  indices can be accessed independently while conflicting accesses are rejected.
- Mapped typed nested struct/array projections into cleanup constant-index paths and retained
  runtime-checked dynamic indexing as a read-only expression for now.

## 0.46.0 - 2026-07-21

- Allowed complete calls to pass temporary values directly to shared and mutable borrow
  parameters, matching the temporary receiver behavior from v0.42-v0.43.
- Staged every value argument in source order whenever a call contains a borrowed temporary,
  preserving left-to-right effects, stable addresses, early-exit cleanup, and exactly-once drop.
- Applied the same lowering to named and inferred generic functions, bound-method argument groups,
  and local partial calls while continuing to reject partial applications that capture borrows.

## 0.45.0 - 2026-07-21

- Added safe source-backed `box_write(boxed)(value)` and `boxed.write(value)` APIs for `Box(T)`
  when `T: Copy`, complementing v0.44's safe Copy reads.
- Required a mutable Box borrow while copying the replacement into heap storage; non-Copy boxes
  continue to use ownership-aware `replace` and never materialize the write member.
- Extended alloc bootstrap validation and taught raw Copy stores to preserve the zero-sized unit
  representation.

## 0.44.0 - 2026-07-21

- Added safe source-backed `box_read(boxed)` and `boxed.read()` APIs for `Box(T)` when `T: Copy`,
  without exposing a raw pointer or transferring pointee ownership.
- Defined `read` in a separate constrained generic inherent extension, so resource boxes retain all
  owning operations but do not acquire an unsound copying method.
- Extended strict alloc bootstrap validation for the new function, Copy proof, and extension shape,
  and taught raw Copy loads to preserve the zero-sized unit representation.

## 0.43.0 - 2026-07-21

- Allowed temporary nominal values to receive inherent and trait methods through
  `mut borrow self`.
- Marked compiler-generated receiver bindings mutable only when the selected method requires it,
  preserving a stable address and running resource destruction exactly once after the call.
- Continued to reject partial applications that would capture either shared or mutable receiver
  borrows beyond the complete call.

## 0.42.0 - 2026-07-21

- Allowed temporary nominal values to receive methods through shared `borrow self`, including both
  inherent and trait-dispatched methods.
- Materialized each borrowed temporary receiver as a compiler-generated lexical binding so it
  remains addressable through the complete call and is destroyed exactly once afterward.
- Kept mutable borrowing of temporary receivers explicit: `mut borrow self` still requires a
  mutable local binding and now reports a focused diagnostic.

## 0.41.0 - 2026-07-21

- Enabled source-backed default trait methods for concrete and blanket implementations; omitted
  methods now use the trait body while explicitly supplied methods override it.
- Substituted `Self`, trait parameters, and associated types into each concrete default method and
  retained the trait definition's source provenance for name and visibility resolution.
- Added abstract definition-time checking for default bodies using an assumed `Self: Trait` proof,
  including all associated-type equalities, even when the trait has no implementations.
- Verified default methods that call required methods, consume `self`, return associated types, and
  dispatch through nested blanket implementations.

## 0.40.0 - 2026-07-21

- Added definition-time signature checking for every blanket trait method, including trait type
  arguments, `Self`, and associated-type substitutions.
- Reused generic-function abstract validation for blanket method bodies through hidden templates,
  so unused implementations are checked under their declared where proofs.
- Allowed abstract trait arguments only inside guarded template validation and rolled every
  temporary nominal, method, signature, and assumed implementation back afterward.
- Added eager diagnostics for invalid associated type expressions, signature mismatches, missing
  names, and incomplete blanket `Drop` implementations without requiring concrete instantiation.

## 0.39.0 - 2026-07-21

- Replaced conservative same-trait blanket rejection with first-order unification of trait argument
  patterns, including nested nominal and array types with occurs checking.
- Allowed provably disjoint blanket implementations such as `Convert(i32)` and `Convert(i64)` on
  the same generic target.
- Added source-order-independent coherence checks between blanket and concrete implementations,
  grounding blanket parameters from each concrete target before deciding overlap.
- Kept where predicates out of disjointness proofs, so potentially compatible implementations
  remain rejected without specialization or a proof of mutually exclusive constraints.

## 0.38.0 - 2026-07-21

- Enabled blanket generic `Copy` implementations with `where` proofs, including nested generic
  structs and enums.
- Added definition-time abstract structural validation, initial fixed-point participation, and
  immediate validation for nominal instances materialized after the fixed point.
- Enabled blanket generic `Drop` implementations and integrated their concrete methods into the
  existing recursive drop glue and structured cleanup paths.
- Preserved defining-package ownership for both language traits and rejected every concrete
  instance that would acquire both `Copy` and `Drop`.

## 0.37.0 - 2026-07-21

- Implemented blanket generic trait extensions such as
  `extend(Cell(T), Read) where T: Read`, selected lazily for each concrete nominal
  instance.
- Substituted target parameters through trait arguments, method signatures and bodies, where
  predicates, and associated-type definitions including `let Item = T`.
- Added conditional nested selection, deterministic blanket-overlap rejection, and the general
  package orphan rule requiring either the trait or target type to be local.
- Kept blanket `Copy` and `Drop` implementations reserved for their structural fixed-point and
  destruction-coherence work.

## 0.36.0 - 2026-07-21

- Added `where` predicates to blanket generic inherent extensions with the documented multiline
  syntax.
- Constrained methods are materialized only for nominal instances whose concrete type arguments
  satisfy every predicate; unsatisfied instances do not acquire the member at all.
- Propagated extension predicates onto generic associated-function templates, producing precise
  call-site constraint errors, and made assumed generic proofs participate in conditional member
  selection.
- Restricted extension member API boundaries by predicate traits and associated equality types, and
  added module-level leak checks for public constrained extensions.

## 0.35.0 - 2026-07-21

- Added associated-type equality bindings in where trait references, including
  `where T: Add(T, Output = T)` and user traits such as `Produce(Item = i32)`.
- Validated equality names and duplicates at template definition time, substituted equality types
  into assumed method signatures, and checked every concrete impl's associated types during
  monomorphization.
- Enabled generic arithmetic through the source-backed `Add`, `Sub`, `Mul`, `Div`, and `Rem` lang
  items when their `Output` is determined, including intrinsic integer implementations.
- Forwarded associated-type proofs through nested constrained generic calls while retaining full
  rollback of assumed selection state.

## 0.34.0 - 2026-07-21

- Enabled static method dispatch on abstract values through ordinary generic-function trait bounds,
  so a body constrained by `where T: Measure` may call `value.measure()`.
- Added temporary assumed trait implementations during template checking. Their signatures and
  selection entries are fully rolled back, while concrete monomorphizations select the real impl.
- Propagated bounds through nested generic calls: a constrained generic function can satisfy the
  equivalent predicate required by another generic function without prematurely choosing a concrete
  type.
- Kept methods involving associated types disabled until associated-type equality predicates can
  determine their signatures.

## 0.33.0 - 2026-07-21

- Added source-level `where T: Trait` predicates to generic functions, including multiple predicates
  and trailing commas without introducing a new argument delimiter.
- Generic template checking now treats `where T: Copy` as proof that abstract `T` values may be
  copied, while every concrete monomorphization verifies all predicates against collected impls.
- Added module rewriting and public-API visibility checks for predicate subjects, traits, and trait
  arguments; unknown traits, arity mismatches, duplicate predicates, and unsatisfied bounds are
  deterministic errors.
- Kept associated-type equalities, bounded abstract method dispatch, constrained extensions, and
  generic trait implementations reserved for subsequent constraint-system milestones.

## 0.32.0 - 2026-07-21

- Implemented blanket generic inherent extensions with the specified
  `extend(Cell(T)) { ... }` syntax. Pattern parameters survive module qualification and are
  substituted when each concrete struct or enum instance is materialized.
- Added inferred generic associated-function dispatch, including runtime arguments, expected result
  types, named type arguments, reordered target parameters, and qualified file-module types. Methods
  use the existing concrete receiver ABI and preserve move, mutable-borrow, and recursive cleanup.
- Added `Box.new`, `Box(T).new`, `boxed.as_mut_ptr()`, `boxed.into_inner()`, and `boxed.replace(value)`
  in ordinary alloc source while retaining the free-function compatibility surface.
- Enforced defining-package ownership, determined extension parameters, duplicate-member rejection,
  and first-slice diagnostics for generic members, associated constants, specialization, and generic
  trait implementations that still await `where`-clause selection.

## 0.31.0 - 2026-07-21

- Added safe source-backed `box_into_inner` and `box_replace` operations. The former consumes the
  unique Box and transfers its pointee to the caller; the latter mutably borrows a Box, returns its
  old pointee, and installs a new owner without an intermediate drop.
- Added unsafe `raw_take(Ptr(mut)(T)): T` for move-initialized storage and safe `forget(value)` for
  explicitly abandoning an owner. Both participate in move analysis and cleanup verification;
  use-after-forget and safe-context raw takes are rejected.
- Added native custom-`Drop` coverage proving into-inner destruction exactly once, replacement of
  two resources exactly twice, and intentional forget without destructor execution.

## 0.30.0 - 2026-07-21

- Embedded an edition-pinned ordinary Salicin `alloc` bundle with validated `Box(T)`, `box_new`, and
  `box_ptr` declarations.
- Added move-initializing `raw_init`, recursive Box pointee drop glue, and target-layout-matched
  deallocation. Native tests cover nested and recursive Box layouts, ZSTs, custom Drop, and unique
  ownership relocation.

## 0.29.0 - 2026-07-21

- Added target-aware `size_of(T)` and `align_of(T)` intrinsics lowered through LLVM layout constant
  expressions, including aggregates and concrete generic instances.

## 0.28.0 - 2026-07-21

- Defined replaceable `salicin_alloc` and `salicin_dealloc` ABI symbols with a weak default C
  runtime and strong-symbol override coverage.

## 0.27.0 - 2026-07-21

- Added `Ptr(T)` and `Ptr(mut)(T)`, explicit `unsafe do`, raw load/store, and reserved allocator
  intrinsics as the first audited unsafe boundary.

## 0.26.0 - 2026-07-21

- Gave every closure and partial application a concrete compiler-generated environment type. Its
  identity includes the static call target, remaining call shape, capability, capture modes, and
  capture types; different anonymous callable types therefore do not silently erase into one ABI.
- Allowed owning partial applications and `FnOnce` closures to return across function boundaries.
  LLVM uses named environment structs passed by value, with no allocator, hidden code pointer, or
  dynamic dispatch. Callers can move, invoke, or abandon the returned value.
- Extended cleanup forests, runtime flag trees, and generated drop glue over concrete environment
  fields. Native tests cover Copy captures, resource captures, post-return relocation, successful
  consumption, and abandoned returned environments with observable destruction.
- Continued to reject escaping shared or mutable borrow captures. Callable parameters remain gated
  on generic `Fn`/`FnMut`/`FnOnce` constraints so anonymous concrete types do not need source names.

## 0.25.0 - 2026-07-21

- Allowed named functions, local closures, and partial applications to move into another local
  binding. Callable identity and `Fn`/`FnMut`/`FnOnce` behavior follow the destination binding, while
  every use of the moved source is rejected by ordinary ownership flow.
- Relocated concrete callable environments at LLVM emission. Owning captures move to fresh stable
  storage, old flags are cleared, and the destination environment assumes exact recursive cleanup;
  borrowed closure captures retain their existing lexical loans.
- Made consuming a `FnOnce` closure or partial application explicit in cleanup IR by moving out its
  callable root after argument staging. This covers later-argument early exits without duplicating
  environment ownership.
- Kept cross-function callable escape rejected. Bare function signatures still describe call shape,
  not an implicitly boxed or dynamically erased environment; concrete return/parameter ABI remains
  the next callable boundary.

## 0.24.0 - 2026-07-21

- Made enum match refinement explicit in cleanup IR with `AssumeDiscriminant`. The verifier rejects
  invalid variants, non-enum paths, dead storage, and enum roots that are not fully initialized.
- Lowered pattern ownership into ordinary atomic `Transfer` operations. Unguarded arms commit at arm
  entry; guarded resource bindings commit only on the guard-success edge, leaving the scrutinee whole
  on guard failure and early return.
- Removed `MatchDispatch`, `PatternBindingTransfer`, `MaybeOverwrite`, and the complete
  `PendingCapability` infrastructure. Executable cleanup plans are now self-contained inputs to
  move-state and drop-flag verification rather than carrying parallel promises about later lowering.

## 0.23.0 - 2026-07-21

- Lowered definite overwrite through `mut borrow` parameters for values that need drop. Root and
  projected field assignments now invoke the old referent's exact drop glue before storing the new
  owned value, without inventing an owned flag for borrowed storage.
- Preserved evaluation order: the replacement is evaluated first, so an early return leaves the old
  referent intact; once available, old cleanup precedes the ownership-transferring store. Caller-side
  cleanup subsequently owns only the replacement.
- Removed `BorrowedPlaceMutation` from pending capabilities and added native success plus observable
  root/field trap tests. Conditional initialization remains an owned-storage concern; mutable-borrow
  assignment is verified as a definite overwrite.

## 0.22.0 - 2026-07-21

- Allowed local partial applications to capture owning `move` arguments, including values with
  custom or recursive `Drop`, instead of restricting every captured argument to `Copy`.
- Classified any partial with a move capture as `FnOnce`, rejected repeated or maybe-repeated use,
  and transferred captures through chained currying, final invocation, conditional calls, and
  later-argument early exits using stable environment flags.
- Removed `PartialApplicationCapture`, the final callable-capture pending capability. Native tests
  cover invocation, continued currying, abandonment, conditional use, early return, and observable
  sibling cleanup. Borrowed captures and escaping/first-class callables remain outside this ABI.

## 0.21.0 - 2026-07-21

- Allowed local `FnOnce` closures to own nominal values that need drop. Each move capture now has
  stable environment storage and a recursive runtime drop slot from closure construction until
  abandonment or invocation.
- Transferred capture ownership through the existing early-exit argument staging before calling
  the lifted function. Successful calls clear environment flags; later-argument return paths clean
  staged captures; uncalled and conditionally called closures clean exactly the captures they retain.
- Removed the `LocalClosureCapture` pending capability and added cleanup-plan plus native coverage
  for single, multiple, conditional, abandoned, custom-`Drop`, and early-return capture paths.
  General partial applications remain Copy-only and still retain their separate pending marker.

## 0.20.0 - 2026-07-21

- Enabled drop-bearing pattern bindings in guarded match arms through speculative binding storage.
  Guards may inspect bindings without owning them; ownership flags and active-variant remainder
  slots are committed only on the successful edge into the arm body.
- Preserved the intact enum root on guard failure and on early return from guard evaluation, so a
  later candidate or enclosing cleanup retains exactly one owner. Non-`Copy` bindings remain
  non-movable while their guard is evaluating.
- Added native true, false, early-return, and sibling-trap coverage for guarded payload transfer.
  Whole-value guarded bindings also work for custom-`Drop` enums, while extracting a payload from
  such an enum remains prohibited.

## 0.19.0 - 2026-07-21

- Extended enum match ownership transfer through nested structural payload patterns. Recursive
  remainder lowering now partitions each moved path from still-owned siblings at every struct
  level and assigns cleanup slots only to the surviving subtrees.
- Preserved nested remainder cleanup across normal arm completion and early return, with native
  success and trap tests covering siblings both inside the destructured struct and beside it in the
  active enum variant.
- Rejected nested movement through a type with custom `Drop`, whose destructor requires an intact
  `self`. Guarded resource moves remain pending rollback-aware lowering.

## 0.18.0 - 2026-07-21

- Lowered ownership transfer from enum match payloads into pattern bindings. Moving a payload clears
  whole-enum cleanup, gives each moved drop-bearing binding its own runtime flag, and materializes
  cleanup slots for active-variant siblings left behind by wildcards.
- Preserved cleanup across normal arm completion and early return, while isolating compiler cleanup
  state between candidates. Native trap coverage proves that unmatched resource siblings are
  destroyed and moved bindings are not destroyed twice.
- Kept values with custom `Drop` indivisible and rejected nested drop-bearing extraction and guarded
  payload moves until downcast projection trees and guard rollback are implemented.

## 0.17.0 - 2026-07-21

- Materialized recursive runtime flag trees for fields of structs without custom `Drop`. A field
  move clears the root, its projection ancestors, and the moved subtree while preserving initialized
  sibling flags.
- Lowered `children_when_clear` behavior: a complete root invokes whole-value glue once, while an
  incomplete root recursively drops only live child projections, including nested structs.
- Restored projection flags on field initialization and re-enabled the root when semantic flow proves
  every field complete. Conditional field overwrite now consults its own flag before cleaning a
  possibly present old value.
- Added native coverage for direct, nested, and conditional field moves, sibling fallback cleanup,
  field reconstruction, and conditional reconstruction. Types with custom `Drop` remain indivisible;
  drop-bearing enum patterns and closure environments are still rejected pending their lowering.

## 0.16.0 - 2026-07-21

- Connected each LLVM function emitter to its verified `CleanupPlan` and materialized drop flags
  for owned parameters and locals whose typed root move paths need destruction.
- Updated flags on root moves and reinitialization, conditionally dropped overwritten values before
  stores, and emitted reverse-order cleanup on normal block exit, explicit and implicit returns,
  loop breaks, match scrutinees, discarded expressions, and callee-owned parameters.
- Staged owned aggregate fields and call arguments until construction or invocation commits. If a
  later operand returns early, already evaluated resources are cleaned; successful completion clears
  the staging flags and transfers ownership exactly once.
- Added native observable cleanup tests using runtime traps, covering once-only destruction,
  conditional moves, overwrite, return, break, match, discarded values, and partial-construction
  early exits.
- Explicitly rejected projection moves/rebuilds, drop-bearing pattern bindings and temporary field
  extraction, and resource-owning closure captures until projection-level and closure-environment
  cleanup lowering can execute those cases correctly.

## 0.15.0 - 2026-07-21

- Added the canonical `Drop` trait to the edition 2026 core source and validated its exact
  `drop(mut borrow self)(): ()` contract through the normal parser and trait schema pipeline.
- Restricted `Drop` implementations to the package defining the nominal type, rejected types that
  also implement `Copy`, and prohibited direct source calls to `Drop.drop` so automatic cleanup
  cannot be preceded by a manual double-drop.
- Replaced conservative nominal `needs_drop` with recursive classification based on custom drop and
  field layouts. Containing structs and active enum variants inherit cleanup only from fields that
  actually need it.
- Emitted deterministic recursive LLVM drop glue for custom-drop types, structs, enums, and arrays,
  including discriminant dispatch for enums, and added native link/run coverage. Scope-exit calls
  and LLVM materialization of the v0.14 flags remain pending, so destructor side effects are not yet
  observable.

## 0.14.0 - 2026-07-21

- Added a semantic `needs_drop` classification to every static cleanup move path. Built-in `Copy`
  values remain trivial; non-`Copy` nominal aggregates and callables conservatively retain cleanup
  obligations until source-backed `Drop` provides exact recursive glue.
- Derived tree-shaped drop obligations from the cached `may_init`/`must_init` fixed point at every
  `StorageDead`. Definitely initialized values use static obligations, conditionally complete values
  use stable drop flags, and partial aggregates recurse into live children without double-dropping a
  parent and its fields.
- Added deterministic flag set/clear actions for storage lifetime changes, initialization,
  overwrite, move, transfer, and discriminant updates. Cleanup verification recomputes this analysis
  and rejects stale caches.
- Added cleanup and HIR regression coverage for `Copy` versus resource paths, static and conditional
  drops, partial aggregate cleanup, flag transitions, malformed drop forests, and cache consistency.
  This release plans destruction but does not yet expose `Drop` or emit LLVM flag storage and calls.

## 0.13.0 - 2026-07-20

- Removed `_` type and expression inference nodes from the AST and parser. Generic calls now infer
  omitted compile-time parameter groups from runtime arguments, constructor fields, variant payloads,
  fallible-container context, and expected result types.
- Kept ordinary parentheses for both compile-time and runtime application. Positional groups made only
  of recognizable type expressions remain explicit compile-time application; other groups begin runtime
  application, so `identity(20)`, `Cell(20)`, and `Option.Some(20)` infer without placeholder syntax.
- Added named compile-time arguments, including partial groups such as
  `Result(E: bool).Err(false)`, and named runtime arguments such as `make(value: 10)`.
  Named runtime arguments must follow declaration order, preserving left-to-right evaluation.
- Preserved `_` exclusively where it already means an ignored name, including wildcard patterns and
  anonymous callable signature slots. Type annotations such as `Cell(_)` and expressions such as
  `identity(_)(20)` now receive targeted parse diagnostics.
- Migrated the core regression suite and CLI fixtures to omission-based inference, and added coverage
  for contextual inference combined with named compile-time and runtime arguments.

## 0.12.0 - 2026-07-20

- Extended the cached cleanup fixed point with `may_live` and `must_live` state for every local.
  `StorageLive` now requires definitely dead storage, value initialization, movement, overwrite,
  transfer, discriminant writes, branch conditions, and return places require definitely live
  storage, and operation-position queries expose dead, maybe-live, or live state.
- Made structural `StorageDead` idempotent so one scope-exit summary safely closes storage that was
  entered on all, some, or none of the incoming paths. Scope-exit edges also end storage in the
  dataflow state, giving conditional temporaries a definite restart point without treating a
  cleanup marker as an executed destructor.
- Gave every `while` condition and loop body its own per-iteration temporary scope. Condition edges
  end the condition lifetime after its branch value is consumed, and normal body backedges end the
  body lifetime before re-entering it; scope verification now accepts that explicit exit followed
  by entry into a descendant evaluation scope.
- Removed `TemporaryStorageLiveness` from pending capabilities and added fixed-point, conditional
  join, idempotent-end, `while`, and `loop` regression coverage. Destruction remains disabled:
  `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, and LLVM cleanup emission are
  still future work.

## 0.11.0 - 2026-07-20

- Pre-registered a complete static move-path forest for every owned argument, return place, user or
  pattern binding, and planner temporary. Struct fields, every enum downcast and payload field,
  every constant array index, `Copy` values, and empty or zero-sized aggregates keep explicit
  paths; borrow aliases keep none. A checked per-function limit of 65,536 paths prevents oversized
  aggregate layouts from exhausting the compiler.
- Made both constant and dynamic array indexing explicit `Copy` extraction. The base (and runtime
  index when present) is still evaluated and staged exactly once, but indexing initializes the
  result without consuming an array element or inventing a non-finite dynamic move path.
- Added a cached control-flow fixed point over all move-path nodes. `may_init` joins by union,
  `must_init` by intersection, unreachable predecessors are ignored, scope-exit edges and
  `StorageLive`/`StorageDead` clear local state, and operation-position replay validates each
  `MoveOut`, `Overwrite`, and atomic `Transfer` against the stable state.
- Tracked enum discriminants alongside initialization. Active-downcast checks, field-to-root
  recomposition, overwrite invalidation, compatible transfer forests, initialized branch
  conditions, and complete return places are now verifier invariants in reachable control flow;
  malformed enum topology is rejected even in unreachable blocks.
- Removed `MovePathStateDataflow` from the pending capabilities. `Init` remains an idempotent
  initialization summary rather than an underlying write, and callable environment forests remain
  expression-backed because function types do not yet carry capture layouts.
- Kept destruction deliberately disabled. Temporary-storage liveness, conditional cleanup for
  maybe-overwrite, mutation through borrowed places, match/pattern transfer, and partial or local
  closure captures remain pending. There is still no `needs_drop`, runtime drop flag,
  source-backed `Drop`, drop glue, resource-bearing global cleanup, or LLVM cleanup emission.

## 0.10.0 - 2026-07-20

- Replaced the cleanup planner's result-presence boolean with concrete `CleanupDestination` places
  and explicit store-versus-discard uses. Resource-valued bindings, discarded expressions,
  assignments, function bodies, explicit returns, and every value-bearing `break` now have stable
  storage before ownership can cross a control-flow edge.
- Added atomic `Transfer` operations with initialize, overwrite, and maybe-overwrite destination
  states. Every transfer names distinct, non-overlapping source and destination move paths and
  consumes its source; verifier-enforced pending state dataflow keeps that not-yet-executable part
  visible in both directions.
- Represented aggregate construction through field, constant-index, enum-downcast, and closure-
  capture projections. Enum construction records its discriminant first, while a struct, array,
  enum, partial application, or closure root becomes initialized only after all children complete.
- Staged call arguments and field or index bases, and made uninhabited parameter entries and
  expressions terminate the cleanup CFG, including calls whose uninhabited result was hidden by
  contextual coercion. A later return, `break`, or diverging argument cannot commit an incomplete
  value to its final destination; nested `break` values abandon partial outer staging while
  transferring only the successful inner value.
- Removed the pending capabilities for unmaterialized resource results and loop-break value
  transfer. Temporary-storage liveness, move-path state dataflow, maybe-overwrite state,
  borrowed-place mutation, match dispatch and pattern transfer, and partial or closure capture
  details remain explicit pending capabilities.
- Kept LLVM emission unchanged: `CleanupPlan` still verifies ownership structure but does not emit
  destruction. `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, resource-bearing
  global semantics, and LLVM cleanup edges remain future work.

## 0.9.0 - 2026-07-20

- Replaced the single moved-place set with normalized alternatives of uninitialized move-path
  leaves. Root and projected moves can now be rebuilt by assigning the root or every field, while
  branch joins preserve correlated alternatives and loop backedges validate the resulting state.
- Bounded exact initialization alternatives at 64. Larger state spaces conservatively widen to
  fully initialized versus the union of possibly uninitialized leaves, preventing exponential
  growth without accepting a use that the exact analysis would reject.
- Distinguished initialized, uninitialized, and maybe-uninitialized uses and assignment kinds, so
  root/field reinitialization and conditional overwrites receive stable ownership diagnostics.
- Prevented a non-`Copy` pattern binding from being moved in a `match` guard: a failed guard may try
  a later candidate, so only a `Copy` binding may be explicitly consumed there.
- Added and verified one type-independent `CleanupPlan` per lowered HIR function. The plan records
  lexical, loop, and match-arm scopes, owned and borrowed locals, move paths, storage/init/move/
  overwrite events, and real branch, loop, guard, break, and return edges. Both checking and code
  generation build and verify this ownership CFG before continuing.
- Kept cleanup lowering deliberately non-executable. Pending capabilities explicitly cover
  unmaterialized resource results, move-path state dataflow, temporary-storage liveness, loop-break
  value transfer, mutation through borrowed places, maybe-overwrite state, match dispatch and
  pattern-binding transfer, and partial-application or closure captures.
- Did not add `needs_drop`, runtime drop flags, source-backed `Drop`, drop glue, or LLVM destructor
  emission. Compile-time globals are still constants materialized independently at each use and do
  not participate in `CleanupPlan`; resource-bearing globals and their `Drop` semantics must be
  fixed before `Drop` becomes observable.

## 0.8.0 - 2026-07-20

- Added the canonical source-backed `pub let Copy = trait {}` marker to the edition `core` bundle,
  with strict declaration-shape and lang-item identity validation; same-named user traits cannot
  acquire compiler `Copy` semantics.
- Made primitive types, `()`, `never`, the compiler's internal error-recovery type, and arrays whose
  elements are `Copy` intrinsically copyable.
- Added explicit nominal `Copy` implementations with `extend(T, Copy) {}`. Struct fields and every
  enum variant payload are checked recursively, including private representation fields and arrays,
  and only the package defining the nominal type may provide the implementation.
- Kept concrete generic implementations local to the exact instance: for example,
  `extend(Cell(i32), Copy) {}` does not make `Cell(bool)` or the generic template `Copy`; blanket and
  generic `Copy` implementations and `where`-based proofs remain unsupported.
- Routed validated nominal `Copy` through ordinary reads, inferred parameter modes, closure captures,
  and function or bound-method partial application. Unannotated parameters copy `Copy` values and
  move other values, while an explicit `move` still consumes even a `Copy` value.
- Kept function and closure types non-`Copy` in this implementation. `Drop` is not public yet: the
  next ownership step is internal scope cleanup and drop flags, before `Drop`, raw pointers, and the
  allocator ABI can unlock `alloc`; platform-facing `std` follows later.

## 0.7.0 - 2026-07-20

- Added source-backed `Sub(Rhs)`, `Mul(Rhs)`, `Div(Rhs)`, and `Rem(Rhs)` core lang-item traits
  alongside `Add(Rhs)`. Each strictly validated contract has an `Output` associated type and a
  matching method that receives both `self` and `rhs` by move.
- Extended `+`, `-`, `*`, `/`, and `%` to statically dispatch through their compiler-matched core
  trait for nominal left operands, while preserving direct built-in integer lowering.
- Generalized arithmetic-trait candidate selection to use expected `Output` constraints and integer
  literal range filtering, including type probing through local bindings in nonempty blocks, with
  deterministic mismatch and ambiguity diagnostics and single evaluation of both operands.
- Kept lang-item semantics identity-based: user traits with the same names cannot spoof an arithmetic
  operator contract or intercept its lowering.
- Guarded built-in integer division and remainder before LLVM lowering: a zero divisor and signed
  `MIN / -1` or `MIN % -1` trap at runtime, while the equivalent invalid constant expressions are
  rejected during compilation.

## 0.6.0 - 2026-07-20

- **Breaking:** struct and named enum payload fields are now private by default. Existing
  cross-module access must mark fields `pub(package)`, and cross-package access must mark them `pub`.
- Added private-by-default, `pub(package)`, and `pub` visibility to struct fields and named enum
  payload fields; effective field visibility is capped by the enclosing nominal declaration, while
  positional enum payloads inherit their enum's visibility.
- Enforced field boundaries across reads, writes, borrows, positional and labeled construction,
  generic inference, optional chaining, and struct/enum pattern destructuring, while preserving
  public core `Option` and `Result` payload access.
- Preserved nominal visibility and source provenance through generic instance construction and
  validation snapshots so monomorphization cannot erase or widen a template's access boundary.
- Rejected API signatures and exposed fields whose recursively nested nominal types have a narrower
  audience, covering functions, annotated globals, structs, enums, trait methods, and associated
  type defaults after module canonicalization.
- Added post-inference API checks for omitted function result and global annotations, including
  private types nested in generic containers such as `Option(Hidden)`.
- Made inherent extension members inherit their target type's API boundary, and prevented public
  trait implementations from selecting associated types narrower than the effective trait/target
  audience; private generic trait arguments also narrow method-candidate visibility.
- Prevented unqualified unit variants from discovering private enums outside their module boundary,
  and made single-source compiler entry points run the same canonical API validation as projects.
- Added cross-module and cross-package regression coverage for opaque private fields, package fields,
  public construction and named payload matching, and source-backed core positional variants.

## 0.5.0 - 2026-07-20

- Started the standard-library bootstrap with an edition-pinned `library/core` source bundle that is
  embedded into the compiler and parsed through the ordinary Salicin frontend.
- Moved `Option`, `Result`, and `never` out of hand-built Rust AST, promoted `Add` from per-program
  declarations, and placed all four in ordinary `.sali` source with deterministic shape validation.
- Added a structured lang-item registry that records the validated core declaration identities used
  by optional chaining, coalescing, propagation, throwing, and overloaded addition.
- Made `Add` part of the edition prelude, so implementations use the compiler-matched core trait
  without redeclaring it in each program.
- Ensured one core identity is shared across package graphs; same-spelled declarations, module
  aliases, child modules, and shadowing type parameters cannot acquire compiler language semantics.

## 0.4.0 - 2026-07-20

- Added strict `salicin.toml` package loading, default and explicit library/binary targets, target
  selection, project discovery, protected inputs, and project-local build outputs.
- Defined prelude `void` as the ordinary alias of `()` and `never` as an empty enum, including
  uninhabited control-flow coercion and empty-match elimination.
- Added automatically discovered file modules with deterministic two-pass package name resolution,
  nested qualified value/type/constructor paths, and canonical lowering across functions, nominal
  types, traits and extensions.
- Added private, `pub(package)` and `pub` top-level visibility, descendant access to private names,
  lexical shadowing during module resolution, and diagnostics for private, duplicate, conflicting,
  invalid and unknown qualified module names.
- Added declaration and module imports with aliases and grouped syntax, explicit `root` / `self` /
  `super` anchors, visibility-preserving `pub use` facades, cycle diagnostics, and access-chain
  checks that prevent private aliases from becoming visibility backdoors.
- Added strict local path dependencies, canonical dependency-cycle detection, library-only dependency
  source discovery, and deterministic atomically updated `salicin.lock` files.
- Added stable package identities and package-local dependency aliases, preserving nominal identity
  through shared diamond dependencies, preventing accidental transitive-dependency access, and
  enforcing private and `pub(package)` boundaries across packages.
- Preserved package and module provenance through semantic analysis so private trait methods cannot
  leak into method lookup across visibility boundaries, including optional chains and generic bodies.
- Required portable relative dependency paths and extended protected-output checks to hardlinks on
  both Unix and Windows.

## 0.3.0 - 2026-07-20

M2 development follows the v0.2 language foundation.

- Added compile-time `type` parameter groups and explicit generic named-function application.
- Added deterministic, cached, on-demand monomorphization while preserving runtime parameter groups
  and local partial application.
- Added definition-time abstract checking for generic function bodies, including return types and
  ownership, so invalid unused templates are diagnosed before instantiation.
- Added explicit generic struct and enum application, including nested data instances, constructors,
  variants, pattern matching, stable instance metadata and recursive-layout validation.
- Added side-effect-free `_` type-argument inference for generic functions, structs and enum
  variants, using expected and runtime argument constraints before lowering each argument once.
- Added concrete trait implementations with required associated types, exact grouped-signature and
  pass-mode validation, coherence checks, stable static method dispatch, inherent-method precedence,
  and support for concrete generic nominal instances such as `Cell(i32)`.
- Added name-based `Add(Rhs)` language-trait validation and statically dispatched `+` for concrete
  nominal left operands, including `Output`-guided inference, deterministic ambiguity diagnostics,
  literal range filtering and single-evaluation ownership semantics; integer addition remains built in.
- Added reserved prelude `Option(T)` and `Result(T, E)` generic enums as ordinary nominal templates,
  reusing existing constructors, type-argument inference, matching, monomorphization and LLVM layouts.
- Added `?.` optional chaining over owned prelude `Option` and `Result` values for struct fields and
  fully applied methods, preserving lazy arguments, single evaluation, residuals and nested outputs.
- Added right-associative `??` for prelude `Option` and `Result`, with single evaluation of the
  consumed container, lazy fallback control flow, payload-aware inference and ownership-flow joins.
- Added postfix `.try` propagation for explicitly annotated named functions returning prelude
  `Option` or `Result`, including single-evaluation residual returns, exact `Result` error matching,
  inferred operands, and automatic `Some` / `Ok` wrapping of normal tails and `return` values.
- Added `throw error` for explicitly annotated `Result` functions, lowering the error exactly once
  into the enclosing result's `Err` variant with precise error-type checking and terminating flow.

## 0.2.0 - 2026-07-20

- Added nominal structs and enums with positional or labeled construction.
- Added field access and mutable field assignment.
- Added exhaustive enum matching, payload bindings, guards and nested matches.
- Added local non-escaping partial application for multi-group named functions.
- Added place-aware `copy` and `move` checking, including use-after-move diagnostics.
- Added shared and mutable borrows, explicit borrow values, and pointer-based borrow parameters.
- Added path-sensitive move-state joins for conditionals, short-circuit expressions and matches.
- Added lambda lifting for local, non-escaping closures with shared/mutable scalar captures and
  single-use nominal move captures.
- Added complete grouped calls for curried capturing closures, with explicit diagnostics for
  unsupported partial application.
- Added `while`, value-yielding `loop`, and value-carrying `break` expressions.
- Added conservative loop back-edge checking that rejects moving bindings declared outside the
  loop body.
- Added inline fixed `Array(T, N)` values, array literals, and runtime-checked `i32` indexing;
  the initial subset is limited to `Copy` elements and read-only indexing.
- Added inherent `extend` blocks with statically dispatched `borrow self`, `mut borrow self`, and
  `move self` methods, plus type-namespace associated functions and constants.
- Added separate instance/type member lookup and conservative diagnostics for temporary receivers
  and non-`Copy` bound-method partial application; trait-backed extensions remain reserved for M2.

## 0.1.0 - 2026-07-20

- Added the `salic` single-file compiler and `.sali` source format.
- Added scalar types, named functions, parameter groups, local bindings, control flow and operators.
- Added textual LLVM IR generation and native linking through Clang.
- Added `build`, `check`, `emit-ir` and `run` commands.
