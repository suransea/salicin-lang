# Structured Test Support

Status: implemented contract

This document fixes the first source and runner contract for built-in test
registrations. It covers structured failure, cleanup, result transport, and
the boolean migration path and common assertion vocabulary. Selection and
final reporting ergonomics remain separate TODO items.

## Registration Contract

A registration remains a top-level, source-ordered declaration:

```salicin
test("parses a count") {
  true
}
```

Its body has the conceptual callable type
`with(core.testing.failure)((): bool)`. Every other effect must still be handled
inside the body. The compiler-supplied registration boundary handles exactly
`core.testing.failure`; it does not grant I/O, allocation, unsafety, or arbitrary
user effects.

Returning `true` passes. Returning `false` is the compatibility spelling for
an unmessaged failure. It remains supported during the experimental edition,
but structured failure is the canonical path used by standard assertions.

## Failure and Outcome

The source-backed `core.testing` contract has these shapes:

```salicin
let failure = effect {
  let abort(move message: option(string)): never
}

let outcome = enum {
  passed,
  failed(option(string)),
}

let run(move action: with(failure)((): bool)): outcome
```

`run` is an ordinary one-shot handler:

- `abort(message)` becomes `failed(message)`;
- normal `true` becomes `passed`;
- normal `false` becomes `failed(none)`.

The message is an owned, validated UTF-8 `string`. This permits construction
through the existing source-backed formatting writer and makes its lifetime
independent of assertion operands and the registration stack. An absent
message is distinct from an empty message.

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

1. enters a fresh `failure` handler;
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
reserved bytes, a little-endian `u64` length, and optional message bytes.
The terminal status uses the index as registration count and the length field
as failure count. Thus every record contains:

- a schema version;
- the source-order registration index;
- pass or fail status; and
- either no message or exact UTF-8 message bytes.

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

## Diagnostics and Migration

- A test body that returns neither `bool` under the compatibility rule nor the
  structured failure path receives a source diagnostic at the body.
- An effect other than `core.testing.failure` that escapes the body is diagnosed
  at the registration.
- A failure is reported with the source registration name and its optional
  message. Compiler-generated `$test$...` names are never printed.
- Ordinary `build`, `check`, and `run` continue to exclude registrations.

The edition has one canonical model: structured failure handled per
registration. Boolean return is a migration adapter into that model, not a
second runner protocol.

## Required Evidence

TEST-1 is complete only with:

- source-backed normal, unmessaged, and messaged outcome tests;
- multiple failures and a later passing registration in one native runner;
- exact Unicode and empty-message distinction;
- owned-resource probes covering pass, `false`, structured abort, message
  transfer, and subsequent registration;
- malformed/truncated report-channel tests;
- static rejection of an escaping unrelated effect;
- ordinary-build exclusion and dependency isolation; and
- formatter, grammar, specification, status, and CLI documentation updates.

## Non-Goals

This slice does not add test filtering, listing, duplicate-name policy,
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
