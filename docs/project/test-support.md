# Structured Test Support

Status: implemented contract

This document fixes the first source and runner contract for built-in test
registrations. It covers throwing failure, cleanup, result transport, and the
common assertion vocabulary.

## Registration Contract

A registration remains a top-level, source-ordered declaration:

```salicin
test("parses a count") {
  std.test.assert(parse_count() == 3)
}
```

Its body has the conceptual callable type
`with(core.error.throwing(core.string.string))((): ())`. Normal return of `()`
passes. A failure throws an owned UTF-8 `string`, normally through a
`std.test` assertion or `std.test.fail`. Every other effect must be handled
inside the body; the registration boundary does not grant I/O, allocation,
unsafety, or arbitrary user effects. Boolean-returning registrations are
rejected rather than maintained as a compatibility model.

## Failure and Outcome

The source-backed `core.testing` contract has these shapes:

```salicin
let outcome = enum {
  passed,
  failed(string),
}

let run(
  move action: with(core.error.throwing(string))((): ()),
): outcome
```

`run` is an ordinary one-shot handler:

- `throw(message)` becomes `failed(message)`; and
- normal return becomes `passed`.

The message is an owned, validated UTF-8 `string`. This permits construction
through the existing source-backed formatting writer and makes its lifetime
independent of assertion operands and the registration stack. Every failure
has a message; an empty message remains an exact, valid message.

`std.test` exposes `fail`, `assert`, `assert_eq`, `assert_ne`, `expect_some`,
`expect_none`, `expect_ok`, and `expect_err` over this contract. Equality
helpers evaluate each operand once and require both `core.cmp.eq(t)` and the
static `std.test.assertion_debug` formatting contract. Expectations consume
their `option` or `result`, return the selected payload, and format only an
unexpected payload.

`assertion_debug` returns owned diagnostic text. Standard implementations
cover the core diagnostic scalar and owned-text types; user types opt in with
an ordinary extension. This keeps formatting selection static, makes the
writer choice private to `std.test`, and avoids reflection or generated
temporary names in output.

Failure messages are deterministic:

- `assert` reports `assertion failed`;
- equality and inequality report their formatted operands;
- expectation helpers identify the unexpected variant and payload; and
- `fail` preserves the supplied UTF-8 message exactly.

## Per-Registration Interpretation

The generated runner invokes registrations one at a time in source order.
For each registration it:

1. enters a fresh `throwing(string)` handler;
2. calls the body exactly once;
3. lets return or effect transfer run the body's cleanup exactly once;
4. converts the result to one `outcome`;
5. emits one failure record when needed; and
6. proceeds to the next registration regardless of that outcome.

The runner returns failure only after all selected registrations finish.
A normal test failure never aborts the process and never prevents a later
registration from running. A native trap, signal, or process termination is
not a structured failure and cannot be recovered by this in-process runner.

Handler transfer must preserve the language's ordinary ownership and cleanup
rules. In particular, owned message construction, resources live at the
failure point, the handler payload, and the final outcome are each destroyed
once along their selected path. The runner must not retain a previous
registration's message or failure state.

## Report Transport

The native test process sends length-delimited records over a dedicated
compiler-owned pipe inherited from `salic test`. The channel is not program
stdin, stdout, stderr, an environment-variable payload, or an exit-code
encoding. User output therefore cannot forge, split, or hide a test result.

Each schema-1 record contains the `SLT1` magic, a little-endian `u64`
registration index, one-byte status and message-presence fields, two zero
reserved bytes, a little-endian `u64` length, and message bytes for failures.
The terminal status uses the index as registration count and the length field
as failure count. Thus every record contains:

- a schema version;
- the source-order registration index;
- pass or fail status; and
- no message for a pass, or exact UTF-8 message bytes for a failure.

The parent validates the complete frame, index range, uniqueness, order, UTF-8
payload, and terminal record before reporting results. Truncation, duplicate
records, invalid indices, invalid UTF-8, or a child that exits without a
terminal record are runner failures rather than source test failures.
Transport diagnostics never expose compiler-generated function names.

The child exit status only summarizes whether the framed run completed and
whether any registration failed. It is not an index and cannot be the sole
result source. The descriptor number is passed in the private
`SALICIN_TEST_REPORT_FD` launcher environment entry; result bytes are never
placed in the environment. The dedicated descriptor is compiler-owned and is
not exposed as general source I/O authority.

## Selection and Reporting

`salic test --list` prints the selected package's registration names to
stdout, one exact UTF-8 name per line in source order, and does not link or run
the native runner. `--filter TEXT` selects names containing the non-empty
UTF-8 text with case-sensitive matching. Listing and filtering compose, and a
filter with no matches is a successful empty selection.

Registration names must be unique across the selected package. Duplicate
names are diagnosed before filtering, including duplicates from distinct file
modules, and diagnostics never expose encoded compiler names. Dependency
registrations remain excluded unless that dependency is selected as the
primary package.

An executing selection is compiled into one native runner and retains source
order. After all selected registrations finish, stderr receives the stable
summary:

```text
salic: test result: P passed; F failed; S selected
```

where `P + F = S`. Individual failures precede the summary in source order.
Exit status is `0` for listing or an execution with no selected failures
(including an empty selection), `1` for source/compiler errors, malformed
runner reports, native runner failures, or any failed registration, and `2`
for invalid CLI, package, or target selection.

## Diagnostics and Migration

- A test body whose normal result is not `()` receives a source diagnostic.
- An effect other than `core.error.throwing(core.string.string)` that escapes the body is diagnosed
  at the registration.
- A failure is reported with the source registration name and exact message.
  Compiler-generated `$test$...` names are never printed.
- Ordinary `build`, `check`, and `run` continue to exclude registrations.

The edition has one canonical model: a unit-returning, string-throwing callable
handled independently per registration.

## Required Evidence

The complete test-support contract requires:

- source-backed normal and throwing outcome tests;
- multiple failures and a later passing registration in one native runner;
- exact Unicode and empty-message distinction;
- owned-resource probes covering pass, throw, message
  transfer, and subsequent registration;
- malformed/truncated report-channel tests;
- static rejection of an escaping unrelated effect;
- ordinary-build exclusion and dependency isolation;
- source-order listing and case-sensitive UTF-8 substring filtering;
- zero-, one-, and multiple-match count and exit-status coverage;
- cross-module duplicate-name rejection without generated names;
- primary-package listing, dependency isolation, and one-runner batching; and
- formatter, grammar, specification, status, and CLI documentation updates.

## Non-Goals

This slice does not add regex or glob filters, historical prioritization,
parallel execution, subprocess isolation per registration, panic recovery,
captured user output, source locations in failure records, property testing,
mocking, snapshots, or benchmarks.

## Research Basis

Reviewed on 2026-07-30:

- [Building Extensible Program Logics through Effect Handlers (2026)](https://arxiv.org/abs/2607.12642)
  motivates treating a handler as the explicit interpreter of one operation
  family instead of hard-wiring failure into every test body.
- [Yarrow: Reconciling Effect Handlers and Region-Based Memory Management
  (2026)](https://arxiv.org/abs/2607.15876) motivates a one-shot handler
  boundary with explicit region and cleanup reasoning.
- [Linear Effects, Exceptions, and Resource Safety (ESOP
  2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
  motivates requiring destruction on the exceptional transfer path and
  interpreting failure only after that cleanup.
- [DANTE: Data-Driven Test Case Selection and Prioritization for Long-Running
  Test Suites (ICST 2026)](https://conf.researchr.org/details/icst-2026/icst-2026-research/44/DANTE-Data-Driven-Test-Case-Selection-and-Prioritization-for-Long-Running-Test-Suite)
  reports that simple selection heuristics can outperform costly learned
  policies under distribution shift. Salicin therefore starts with an
  explicit stable filter and no hidden historical reordering.
- [How Far Are We from Detecting Flaky Tests? On the Limits of Code-Based
  Detection (2026)](https://arxiv.org/abs/2607.09345) finds that execution
  evidence and environment often matter beyond test code. Salicin's summary
  therefore records the exact selected execution population rather than
  inferring or suppressing flaky outcomes.
