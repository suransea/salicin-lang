# Semantic Occurrence Index

Status: implemented snapshot-local contract

The editor analysis publishes a `SemanticIndex` only after the complete
workspace snapshot passes lexing, parsing, resolution, and semantic checking.
The index is a source fact table for later navigation queries; it is not an
LSP response and does not perform rename.

## Identities

Every source-backed symbol receives a dense `SemanticSymbolId` in deterministic
document and byte order. IDs are meaningful only together with the immutable
`WorkspaceSnapshotId` that owns the analysis. Reanalyzing identical source in
the same ordered graph produces the same symbols, keys, and occurrences, but
clients must not retain an ID across snapshot revisions.

Symbols distinguish ordinary declarations, aliases, fields, variants,
overloads, trait/effect members, implementations, and extension members.
Overloads and implementations include their source position in the key so
same-named declarations remain distinct. Keys are compiler facts, not a public
ABI and not native linker names.

Compiler-generated specializations never enter the source index. Their
private names therefore cannot leak through later definition, hover, reference,
or rename responses.

## Occurrences

An occurrence contains its document, exact half-open UTF-8/UTF-16 range, role
(`Declaration` or `Reference`), and an ordered set of candidate symbol IDs.
One declaration occurrence names exactly one symbol. A reference may name:

- one symbol when its spelling has one source identity;
- several symbols when overload or member selection needs a later typed query;
- no symbol when it names a local/builtin entity outside this index or cannot
  be resolved without partial-program recovery.

The index preserves multiple candidates instead of selecting by declaration
or hash-table traversal order. NAV-2 owns typed candidate selection and public
navigation behavior; NAV-3 owns editability and rename refusal.

## Failure and ownership boundaries

Snapshots with any frontend or semantic diagnostic publish an empty index.
This avoids mixing trusted facts with a partially rewritten program; partial
program navigation remains a separate design problem.

The current workspace analysis owns the selected source graph. Package-aware
query exposure and dependency-owned read-only sources belong to NAV-2. The
index itself performs no file I/O and cannot mutate a source document.

## Verification

Regression coverage proves:

- every accepted symbol category has an exact source range;
- extension members point at their declarations rather than trait references;
- overload ambiguity remains an ordered multi-candidate occurrence;
- cross-module references point to the canonical declaration identity;
- Unicode identifiers retain exact byte/UTF-16 ranges and identity;
- local shadowing never misbinds a reference to a same-spelled top-level item;
- repeated analysis is byte-for-byte and identity-for-identity stable; and
- invalid snapshots expose neither symbols nor occurrences.
