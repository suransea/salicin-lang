# Semantic Navigation Queries

Status: implemented transport-independent and LSP contract

Definition, reference, and hover requests consume the semantic occurrence
index attached to one accepted immutable workspace snapshot. They never scan
source text independently and never expose compiler-generated specialization
or native-linker names.

## Query result model

`SemanticIndex::definition`, `references`, and `hover` accept a stable document
identity plus a zero-based UTF-16 position. Each returns `NotFound`,
`Ambiguous(ids)`, or `Found(value)`. Definitions return the declaration and
ownership. References return deterministically ordered locations and honor
`includeDeclaration`. Hover returns the occurrence range, source symbol kind,
ownership, and a concise source-backed declaration header.

Named call labels may reduce a same-named overload set when they identify one
accepted source signature. A remaining overload or member ambiguity is never
resolved by declaration order. The LSP surface reports it as `RequestFailed`;
missing definition and hover targets return `null`, while missing references
return an empty array.

## Package ownership

`WorkspaceSession::new_packages` preserves the resolved provider graph instead
of flattening dependencies into the primary package. Navigation may cross
package boundaries. Primary-package symbols are editable; dependency symbols
are read-only. Dependency documents participate in analysis and navigation,
but open, change, save, close, and baseline-update operations fail with
`ReadOnlyDocument` without advancing the revision. The
[safe rename contract](safe-rename.md) consumes this ownership fact and
performs no source writes.

## LSP surface

`salic lsp` advertises `definitionProvider`, `referencesProvider`, and
`hoverProvider`. Requests use the same current-analysis gate as semantic
tokens: they wait for their requested revision, cancellation yields
`RequestCancelled`, and a document change yields `ContentModified`. Responses
use file URIs and half-open UTF-16 ranges from the editor layer.

Partial-program recovery, completion, and persistent symbol IDs are not part
of this contract. Rename is specified separately by the
[safe rename contract](safe-rename.md).

## Verification

Unit and real-process transcript tests cover cross-module and cross-package
queries, dependency read-only enforcement, declaration inclusion, hover
ownership, exact ranges, Unicode coordinates, overload narrowing and explicit
ambiguity, unknown positions, restart, cancellation, and supersession.
