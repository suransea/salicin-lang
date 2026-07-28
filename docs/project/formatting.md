# Parsing and Formatting Contract

Status: accepted implementation contract  
Accepted: 2026-07-28

Parsing and formatting use ordinary source-defined traits in `core.fmt`.
Dispatch is static and bounded by the existing monomorphization rules; there
is no reflection, run-time type inspection, format-string syntax, macro, or
implicit I/O.

`parse` consumes the complete borrowed source logically but never owns or
mutates it. Standard implementations bind associated `source` to `str`, select
a structured associated `error`, and return `result(error)(self)`. Parsing is
pure and allocation-free. Integer
implementations distinguish empty input, invalid sign, invalid digit, and
overflow, reporting the first failing UTF-8 byte offset where applicable.
Whitespace, radix prefixes, separators, and trailing input are rejected unless
a later concrete API explicitly opts into them.

`text_writer(e)` accepts Unicode scalars and checked ASCII bytes through a
mutable writer borrow and declares the exact effect row `e`. Scalar-level
input makes the protocol independent of any owning string representation and
does not require a temporary allocation. Implementations trap if the ASCII
entry point receives a value above 127. Partial host writes remain an
implementation detail of later I/O writers. `display` and `debug` are
separately implemented traits whose methods are generic over the writer and
forward exactly its effect row. They do not allocate by contract.
`alloc.string.string_writer` is the pure allocation-backed implementation; it
can reserve capacity, expose the text written so far, and transfer the
completed owning `string`.

Display is stable, user-facing text. Debug is deterministic diagnostic text
and never exposes private addresses or nondeterministic identities. The first
implementation covers `u64`, `u128`, `i64`, and `i128` using canonical decimal
output; booleans use lowercase `true`/`false`; scalars write their canonical
UTF-8 encoding; and `str`/`string` preserve their exact scalar sequence.
Minimal debug output is deliberately identical to display output. Quoting,
escaping, padding, locale rules, and formatting syntax remain outside this
milestone.

Strict integer parsing is implemented for `u64` and `i64`, both directly and
through the decimal `parse` trait. The radix must be 2 through 36. Digits are
ASCII `0-9`, `a-z`, or `A-Z`; unsigned input rejects either sign, while signed
input accepts one leading `+` or `-`. Overflow is checked before multiply/add
or multiply/subtract, so neither safe path performs an overflowing operation.

The protocol follows the modular safety principle described by 2026 *Safe
Coding*: risky sink behavior is localized behind a safe typed abstraction.
POPL 2026 *Typing Strictness* notes that higher-order parameter effects must
remain visible for sound composition; accordingly writer effects are explicit
in the trait bound and are forwarded unchanged by formatting. POPL 2026
*What Is a Monoid?* develops increasingly general unit-and-composition
structures; this implementation uses the corresponding narrow engineering
shape for builders—one empty state and one ordered scalar-append path, with
ASCII adapted into that path—rather than a second concatenation protocol.

- [Safe Coding (2026)](https://doi.org/10.1145/3795888)
- [Typing Strictness (POPL 2026)](https://doi.org/10.1145/3776657)
- [What Is a Monoid? (POPL 2026)](https://doi.org/10.1145/3776727)
