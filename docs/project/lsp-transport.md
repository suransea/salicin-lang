# Minimal LSP Transport

Status: implemented LSP-3 contract

`salic lsp [path] [-p package] [--bin name | --lib] [--locked | --frozen]`
resolves exactly the same source/package/workspace target as compiler
commands, reads its source graph once as the session baseline, and then serves
Language Server Protocol 3.18 messages over standard input and output.
Protocol bytes never share stdout with logs or compiler prose.

## Framing and lifecycle

Each message has one ASCII `Content-Length` header, a blank line, and exactly
that many JSON bytes. Header names are case-insensitive. Missing, duplicate,
invalid, truncated, and messages above 16 MiB fail the transport; recoverable
JSON parse failures receive JSON-RPC error `-32700`. Every response is flushed
as one complete frame.

Before `initialize`, ordinary requests receive `-32002`. The one successful
initialize response advertises UTF-16 positions and:

```json
{
  "textDocumentSync": {
    "openClose": true,
    "change": 1,
    "save": { "includeText": true }
  }
}
```

`shutdown` succeeds only while running. `exit` after shutdown returns process
status 0; `exit` before shutdown, stream EOF without `exit`, or a framing
failure returns nonzero. Notifications after shutdown are ignored except for
`exit`. Unknown requests receive `-32601`; unknown notifications are ignored.

## Document synchronization

Only `file:` document URIs are accepted. UTF-8 path bytes use percent encoding
and map to stable paths in the selected source graph. `didOpen` supplies the
complete text and initial signed version. Every `didChange` must contain
exactly one complete replacement with a strictly greater version; incremental
ranges are rejected. Rejected notification operations produce
`window/logMessage` errors without stopping the server or changing the session
revision.

`didSave` records its included text, or the current overlay when text is
omitted, as the new in-memory baseline. `didClose` removes the overlay and
reveals that baseline. None of these operations writes a source file.
Documents outside the source graph selected at startup are rejected.

## Separation and non-goals

The transport owns JSON-RPC, lifecycle, URI decoding, and synchronization.
`WorkspaceSession` owns versions, overlays, baselines, immutable revisions,
and stale-analysis acceptance. The compiler owns package discovery and
analysis. This boundary avoids a transport-specific parser or type checker.

This milestone publishes no diagnostics or semantic tokens and starts no
analysis worker. LSP-4 connects accepted snapshots to those features.
Incremental range edits, dynamic workspace-folder changes, filesystem
watching, completion, hover, navigation, rename, editor extensions, and
network transports remain outside this contract.

## Verification

Unit transcripts cover framing rejection, lifecycle results, advertised
capabilities, Unicode/reserved URI bytes, full-text changes, and stale-version
logging. A spawned `salic lsp` process selects one member of a real workspace,
performs open/change/save/close, shuts down cleanly, emits only framed JSON on
stdout, and leaves the selected source byte-identical on disk.
