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

`analyze_document` lexes, parses, resolves, and semantically checks one source
document. `analyze_workspace` applies the same phases to a complete source
module graph and routes diagnostics to their owning paths. Later phases run
only when earlier phases succeed.

Diagnostics identify their lexer, parser, resolver, or semantic phase.
`RangePrecision::Exact` means the range came from the rejected source
construct. `RangePrecision::Fallback` marks a synthetic range used when an
older resolver or semantic diagnostic has no source location. Graph-level
diagnostics that cannot belong to one document may omit both range and path.

## Verification

Regression coverage verifies:

- UTF-8 bytes and UTF-16 positions for Unicode identifiers and non-BMP text;
- exact parser and semantic ranges;
- explicit fallback precision for location-free diagnostics;
- diagnostic routing across multiple source modules;
- a ranged diagnostic for every repository failure fixture.

Incremental parsing, document synchronization, completion, hover, rename, and
the JSON-RPC transport remain later LSP work.
