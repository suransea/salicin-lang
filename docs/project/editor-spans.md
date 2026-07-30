# Editor Span Contract

Status: implemented frontend contract

The compiler exposes token and diagnostic locations through `compiler::editor`.
This is the stable input boundary for a future Language Server Protocol
transport; it is not itself an LSP server.

## Coordinates

Every editor position contains:

- a zero-based UTF-8 byte offset into the source;
- a zero-based line;
- a zero-based UTF-16 character offset, as required by LSP.

Ranges are half-open. Lexer tokens retain their existing one-based Unicode
scalar line and column fields for compiler diagnostics while also carrying
exact UTF-8 byte ranges. The editor layer performs the UTF-16 conversion from
source text, including non-BMP characters.

## Analysis APIs

`analyze_document` lexes, parses, resolves, and semantically checks an
anonymous source document. `analyze_document_at` accepts an explicit stable
document identity. `analyze_workspace` applies the same phases to a complete
source module graph and routes diagnostics to their owning documents. Later
phases run only when earlier phases succeed.

Every diagnostic carries a non-empty document identity, lexer/parser/resolver/
semantic phase, error severity, stable phase code, human-readable message,
and an optional exact range. `salicin.lex`, `salicin.parse`,
`salicin.resolve`, and `salicin.semantic` are stable coarse codes; later work
may add more specific codes without changing these phase identities.

Resolver diagnostics are structured at their production boundary. Parser
errors, item declarations, imports, name resolution, and semantic nominal
layout checks preserve source origins without recovering paths or positions
from rendered text. A document-wide or graph-wide failure with no rejected
source construct has `range = None`. The API never invents byte-zero or
first-token fallback ranges. Terminal-facing compiler entry points continue
to render structured resolver diagnostics into their existing strings only at
the outer compatibility boundary.

## Verification

Regression coverage verifies:

- UTF-8 bytes and UTF-16 positions for Unicode identifiers and non-BMP text;
- exact parser and semantic ranges;
- explicit document, phase, severity, and stable code on every failure
  fixture;
- exact duplicate-declaration and import ranges with no rendered-message
  parsing or fallback coordinates;
- diagnostic routing across multiple source modules;
- honest omission of ranges for failures that do not identify a source
  construct.

Incremental parsing, document synchronization, completion, hover, rename, and
the JSON-RPC transport remain later LSP work.
