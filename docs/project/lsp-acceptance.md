# LSP Protocol Acceptance

Status: implemented LSP-5 contract

The language server separates protocol input, workspace analysis, and result
publication. A reader thread continues accepting JSON-RPC messages while one
analysis worker checks immutable snapshots. Before starting its next check,
the worker drains queued snapshots and keeps only the newest. Compilation
itself is not interrupted mid-phase; cancellation is logical and completed
obsolete work is discarded.

## Requests and revisions

Every requested analysis carries its exact `(session, revision)`. Only a
completion equal to the current workspace snapshot may publish diagnostics or
answer a pending semantic-token request. An accepted synchronization invalidates
pending requests for an older revision with LSP `ContentModified` (`-32801`).

`$/cancelRequest` removes a matching pending request and completes it exactly
once with `RequestCancelled` (`-32800`). Cancelling an unknown or already
completed request is a no-op. Shutdown cancels remaining requests before its
own successful response. Old analysis may finish internally, but it cannot
publish, satisfy a request, or clear the newer scheduled revision.

## Acceptance corpus

Editor-independent JSONL transcripts alternate `send` and subset-matching
`expect` records. They run against the real `salic lsp` stdio process and
cover:

- clean initialize/initialized/shutdown/exit replay across two server starts;
- Unicode identifiers and non-BMP strings;
- diagnostics and semantic tokens for multiple source documents;
- repaired documents, strictly increasing versions, and stale edit rejection;
- malformed, duplicate, missing, oversized, and truncated frames;
- recoverable invalid JSON; and
- clean and premature termination exit behavior.

A deterministic worker barrier separately proves cancellation, content
modification, queued-revision coalescing, and suppression of a deliberately
late stale completion. This avoids timing-dependent assertions.

The corpus is an acceptance baseline, not a claim of exhaustive fuzzing.
Incremental text edits, filesystem watching, navigation, completion, dynamic
workspace folders, and cooperative cancellation inside compiler phases remain
outside this contract.
