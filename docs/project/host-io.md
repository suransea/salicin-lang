# Synchronous Host I/O Contract

Status: accepted for Edition 2026

This contract fixes the authority, error, byte/text, progress, ownership,
cleanup, and target rules used by the first synchronous host APIs.
Console/process operations are implemented; filesystem operations are IO-3.

## Authority and the native boundary

`std.io.io` is the one compiler-validated host-authority effect:

```salicin
pub let io = effect {}
```

The empty operation set is intentional. Compiler-validated `std` host
functions perform the native operation directly and carry `with(std.io.io)`;
the effect row proves authority at every call boundary. It is not an error
transport and it is not a file handle.

Only the exact embedded identity `std::io::io` is privileged. A user effect
named `io`, an alias with the same surface spelling, an imported module, a
path, or a forged representation grants no authority. Safe host functions
carry `io` and never implicitly add `core.unsafe.unsafety`. Their trusted
implementation may cross the compiler/runtime FFI boundary, but raw-address
or unchecked-representation APIs remain separately `with(unsafety)`.

A native binary entry point has exactly one of these shapes:

```salicin
let main(): ()
let main(): i32
let main: with(std.io.io)(): ()
let main: with(std.io.io)(): i32
```

The native launcher discharges only the validated `io` identity. It does not
discharge a user effect, `unsafety`, or `throwing(E)`. Imports do not change a
pure `main`, and libraries remain effect-polymorphic. Test registrations
receive no host authority until a later test-runner contract explicitly
defines it.

## Errors

Host failures are ordinary values:

```salicin
pub let io_error = struct {
  failure: io_error_kind,
  host_code: core.option(i32),
}
```

`kind()` is portable control-flow information. `raw_code()` is optional,
signed diagnostic data in the host's native error-code domain. It may be
absent, reused by the host, or mean something different on another target.
Unknown native errors map to `other` while retaining a representable raw
code. No error formatting or construction requires allocation.

The initial closed `io_error_kind` set is:

- `not_found`, `permission_denied`, `already_exists`;
- `invalid_input`, `invalid_data`;
- `interrupted`, `would_block`, `write_zero`, `unexpected_eof`,
  `broken_pipe`;
- `unsupported`, `out_of_memory`, and `other`.

IO-2 and IO-3 must map every native failure into this set and include
target-specific mapping tables in tests. Adding portable distinctions is an
edition-visible API change; callers must retain an `other` branch.

## Bytes, text, and paths

Primitive I/O is byte-oriented. A read writes initialized `u8` storage; a
write borrows initialized bytes. Neither primitive validates UTF-8.

Text output accepts `borrow(str)` and writes its existing UTF-8 bytes exactly.
Text input first collects bytes, then validates them. Invalid UTF-8 returns
`invalid_data`; it is never replaced, normalized, or decoded lossily.
Newline helpers append exactly byte `0x0a`; they do not translate to a native
line ending.

The first filesystem API accepts `borrow(str)` paths. On the supported Unix
targets it passes the UTF-8 bytes unchanged, rejects an embedded NUL as
`invalid_input`, and performs no Unicode or lexical normalization. This is a
deliberate initial restriction: paths not representable as Salicin text require
a later byte-path API.

Process arguments have a lossless byte view and a checked text view. The text
view reports `invalid_data` for a non-UTF-8 host argument rather than replacing
bytes. Standard input/output/error are byte streams; text helpers are adapters.

The implemented console/process surface includes `read_stdin`,
`read_stdin_exact`, `read_line`, one-attempt and all-byte stdout/stderr writes,
`print`/`println` and stderr counterparts, explicit flush points,
`argument_count`, `argument_bytes`, `arguments_bytes`, and `arguments`.
Bulk lossless arguments use the nominal `process_argument` wrapper; each value
can be consumed into its byte vector. Direct descriptor writes are unbuffered,
so flush is an explicit successful synchronization point.

## Progress, EOF, and interruption

`reader.read(buffer)` and `writer.write(bytes)` make one host attempt:

- success returns `n` with `0 <= n <= buffer.len()`;
- a short success is not an error;
- an error reports zero progress for that call;
- an empty input returns zero without making a host call;
- zero from a non-empty read is EOF for that attempt;
- zero from a non-empty write is allowed at the primitive layer.

Primitive operations preserve `interrupted` and `would_block`. They never spin.
`read_exact`, `write_all`, line reading, and bounded whole-input helpers retry
`interrupted`. `write_all` converts a successful zero write with remaining
input to `write_zero`; `read_exact` converts EOF before completion to
`unexpected_eof`. They preserve the first other error. Mutation already
completed before a later helper error remains visible in the caller-owned
buffer; an error never claims that the whole operation was atomic.

All count arithmetic is checked before pointer formation. A native count that
exceeds the supplied buffer is an implementation-contract violation and must
trap before safe code can observe an out-of-bounds length.

## Resource ownership and close

An opened `file` is a non-copyable, non-forgeable owner of one native handle.
Operations borrow it; they neither duplicate nor transfer it. Opening requires
`io` and returns `result(io_error)(file)`.

`file.close(move self)`:

1. makes at most one native close attempt;
2. consumes and invalidates the logical owner before reporting the outcome;
3. returns `result(io_error)(())`;
4. never retries an interrupted close, because the native descriptor may
   already have been released and reused.

Deterministic destruction uses the same once-only state transition and ignores
an unreportable close error. Explicit close prevents the destructor from
closing again, including when close reports an error. Every early return,
effect transfer, handler abort, and ordinary scope exit must run this cleanup
exactly once. IO-3 must test successful close, close failure, explicit-close
then drop, and non-local exits.

## Blocking and target matrix

The API is synchronous and may block the current native thread. An inherited
nonblocking stream or file reports `would_block`; synchronous helpers do not
poll or busy-wait. Async integration and cancellation are outside this
contract.

The first implementation matrix is exact:

| Target | Status |
| --- | --- |
| `x86_64-unknown-linux-gnu` | supported and CI-conformant |
| `aarch64-apple-darwin` | supported and release-conformant |
| every other OS/architecture, WASI, and every 32-bit target | rejected before host-library lowering |

`core` remains host-independent. `alloc` requires only the allocator ABI.
Loading `std` on an unsupported host produces a target-specific diagnostic;
it never silently substitutes another ABI.

## Evidence required from IO-2 and IO-3

Each concrete host primitive must have:

- source signatures proving `io` without implicit `unsafety`;
- pure-caller rejection and `main with(io)` acceptance;
- native success, partial progress, EOF, interruption, and error mapping;
- byte-exact output kept separate from compiler diagnostics;
- initialized-buffer and checked-count coverage;
- ownership and exactly-once cleanup coverage for every resource;
- Linux/x86-64 CI and macOS/arm64 release conformance.

IO-2 evidence covers byte-exact stdout/stderr, stdin line input, EOF from
`read_stdin_exact`, non-UTF-8 argv preservation and checked rejection, and
`EPIPE` recovery with `SIGPIPE` ignored. Primitive calls expose native short
progress; all/exact helpers loop until completion or the specified error.

## Research and specification basis

The contract combines static effect authority with runtime resource handles:

- [Rows and Capabilities as Modal Effects (POPL 2026)](https://doi.org/10.1145/3776674)
  supports keeping effect authority distinct from particular runtime handles.
- [Linear Effects, Exceptions, and Resource Safety (ESOP 2026)](https://link.springer.com/chapter/10.1007/978-3-032-22720-1_8)
  motivates a once-only destructor obligation across exceptional control.
- [Securing Agents With Tracked Capabilities (ACM CAIS 2026)](https://doi.org/10.1145/3786335.3813127)
  provides current evidence that type-tracked capabilities and local purity
  prevent untracked library effects in practical code.
- [WASI Capabilities](https://github.com/WebAssembly/WASI/blob/main/docs/Capabilities.md)
  distinguishes link-time host services from unforgeable runtime handles.
- [Rust `Read`](https://doc.rust-lang.org/std/io/trait.Read.html) and
  [Rust `Write`](https://doc.rust-lang.org/std/io/trait.Write.html) define the
  partial-progress, EOF, interruption, `write_all`, and `write_zero` behavior
  used here.
