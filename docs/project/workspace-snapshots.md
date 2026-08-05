# Versioned Workspace Snapshots

Status: implemented transport-independent contract

`WorkspaceSession` owns the mutable editor view of one selected compilation
target. The caller supplies the discovered source graph, including stable
document identities, resolved module paths, root designation, baseline text,
and binary/library target mode. The session does not discover packages, read
manifests, read source files, or write files.

## Baseline and overlays

Every document has caller-owned baseline text. `open_document` installs a
full-text in-memory overlay with a signed client version. `change_document`
replaces that overlay only when the received version is strictly greater than
the current open version. Equal and lower versions are rejected without
changing session state or advancing its revision.

`update_baseline` replaces the in-memory baseline supplied by the caller. An
open overlay continues to win. `save_document` records either client-supplied
saved text or the current overlay as the new baseline, again without writing
it. `close_document` removes the overlay and reveals that latest baseline.
Unknown, duplicate, already-open, and not-open document operations are
explicit errors. These operations never write the corresponding source path.

This layer accepts full document text. Incremental range edits, filesystem
watching, and URI normalization belong to the protocol transport.

## Immutable snapshots

Every successful mutation advances one checked `u64` session revision.
Rejected mutations leave it unchanged. `snapshot()` clones one immutable,
ordered source graph whose identity is:

```text
(process-unique session ID, session revision)
```

Each snapshot document records the open client version or `None` when it uses
baseline text. A snapshot is independent of later mutations and can be
analyzed on another thread. Analysis uses the ordinary lexer, parser,
resolver, and semantic checker over the complete snapshot; this milestone
does not claim incremental recomputation.

`WorkspaceSnapshotAnalysis` returns the exact snapshot identity and ordered
document-version vector alongside tokens, diagnostics, and the semantic
occurrence index. Symbol IDs are interpreted only within that identity. This
is the version that an LSP transport must attach to published results.

## Supersession

`accept_analysis` consumes a completed result and yields its analysis only
when its session and revision equal the session's current identity. Results
from another session or any older revision are dropped. This comparison is
the single publication gate: an analysis may finish after a newer edit, but
it cannot replace the newer editor state.

The gate prevents stale publication; it does not forcibly interrupt compiler
work. Cooperative cancellation and JSON-RPC request cancellation belong to
the transport milestone.

## Verification

Regression coverage proves:

- baseline analysis and open-buffer precedence across multiple modules;
- strictly increasing document versions and revision stability on rejection;
- analysis on a worker thread followed by stale-result rejection;
- acceptance of the current result with its exact document versions;
- baseline replacement under an overlay and restoration on close;
- cross-session result rejection;
- duplicate, unknown, already-open, and not-open failures; and
- byte-for-byte unchanged source files after opening and changing overlays.
