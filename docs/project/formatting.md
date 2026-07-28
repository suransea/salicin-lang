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

`text_writer(e)` binds its associated `text` to `str`, accepts validated
borrowed UTF-8 fragments through a mutable writer borrow, and declares the
exact effect row `e`. A successful `write`
writes the complete fragment; partial host writes remain an implementation
detail of I/O writers. `display` and `debug` are separately implemented traits
whose methods are generic over the writer and forward exactly its effect row.
They do not allocate by contract. Allocation-backed `string_writer` and
`to_string` belong in `alloc.fmt`.

Display is stable, user-facing text. Debug is deterministic diagnostic text
and may expose type-oriented delimiters, but never private addresses or
nondeterministic identities. Integers use canonical decimal display, booleans
use lowercase `true`/`false`, scalars write their canonical UTF-8 encoding,
and `str`/`string` display their exact contents. Debug escaping is defined by
the concrete FMT-2 implementation.

The protocol follows the modular safety principle described by 2026 *Safe
Coding*: risky sink behavior is localized behind a safe typed abstraction.
POPL 2026 *Typing Strictness* notes that higher-order parameter effects must
remain visible for sound composition; accordingly writer effects are explicit
in the trait bound and are forwarded unchanged by formatting.

- [Safe Coding (2026)](https://doi.org/10.1145/3795888)
- [Typing Strictness (POPL 2026)](https://doi.org/10.1145/3776657)
