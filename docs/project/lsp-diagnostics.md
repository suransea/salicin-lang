# LSP Diagnostics and Semantic Tokens

Status: implemented LSP-4 contract

The LSP transport derives both features from one accepted
`WorkspaceSnapshotAnalysis`. It does not re-lex, parse rendered diagnostics,
or recover locations at the protocol boundary.

## Diagnostic publication

Every successful initialized/open/change/save/close synchronization analyzes
the complete selected graph. The completed `(session, revision)` must still be
current before any notification is emitted. Publication then covers every
document, so a repaired file receives an empty list instead of retaining stale
editor state.

Each ranged compiler diagnostic becomes an LSP error with:

- the exact percent-encoded file URI of its owning source;
- an open-buffer version when one exists;
- the compiler's zero-based UTF-16 range;
- its stable code and `salicin` source;
- its message; and
- `data.phase` equal to `lexer`, `parser`, `resolver`, or `semantic`.

The LSP Diagnostic shape requires a range. A compiler failure without an exact
source construct is reported through `window/logMessage`; the server never
manufactures `(0, 0)` or assigns a workspace failure to an arbitrary root.

## Semantic-token encoding

The server advertises full-document semantic tokens with this fixed legend:

```text
0 keyword
1 variable
2 typeParameter
3 string
4 number
5 operator
```

The input is the ordered `EditorToken` sequence retained by compiler analysis.
Identifiers remain compiler variables at this layer; richer declaration and
reference classification depends on the later semantic occurrence index.
Region names map to `typeParameter`. Fixed language keywords, literals, and
operators map directly. Delimiters, punctuation, logical newlines, and EOF are
omitted.

Every emitted token is single-line and non-empty. The five integers are
delta-line, delta-start, UTF-16 length, legend index, and zero modifiers.
The response `resultId` is `<session>:<revision>`, making reuse identity
explicit even though delta-token requests are not yet advertised.

## Correctness and non-goals

Analysis is currently synchronous, so protocol input waits while a snapshot is
checked. Correctness does not depend on that scheduling choice: publication
still crosses the same acceptance gate used by future worker-thread analysis.
LSP-5 owns cancellation, malformed/recorded transcript breadth, restart, and
stale concurrent completion acceptance.

Semantic AST classification, token deltas, partial analysis recovery,
incremental parsing, completion, hover, references, rename, and editor-specific
extensions are outside this milestone.

## Verification

Regression transcripts prove all four phases survive serialization, exact
multi-file URIs and open versions are retained, Unicode identifiers and
non-BMP strings use UTF-16 lengths, repaired errors are cleared, unknown
documents cannot return tokens, and real stdio execution produces only framed
protocol output without writing source files.
