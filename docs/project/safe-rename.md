# Safe Rename

Status: implemented binding-preserving workspace-edit contract

Rename is a semantic transformation over one immutable analyzed workspace
snapshot. It returns edits only; the compiler and language server never write
source files or apply the returned workspace edit.

## Eligibility and names

`prepare_rename` requires one uniquely resolved source symbol. Rename refuses:

- a missing or multi-candidate occurrence;
- compiler-generated entities, which never enter the source index;
- dependency-owned declarations or any operation requiring a dependency edit;
- `foreign(...)` and compiler-owned `builtin()` declarations;
- implementation markers and other constructs without an identifier; and
- an empty, keyword, `_`, non-NFC, multi-token, malformed, or unchanged name.

The replacement must lex as exactly one NFC Salicin identifier followed by
EOF. String literals, comments, native link names, test names, and other text
with the same bytes are not edited.

## Complete edit construction

The selected symbol ID defines one equivalence class of declaration and
reference occurrences. Every member must uniquely target that symbol and live
in editable source. Edits are sorted by document and original byte range,
contain exact half-open UTF-8/UTF-16 ranges, and are rejected if any pair
overlaps.

The LSP response uses versioned `TextDocumentEdit` entries in
`WorkspaceEdit.documentChanges`. This allows the client to reject an edit if
an open document changed after the analyzed snapshot.

## Binding-preservation proof

Before returning edits, the planner applies them to a cloned in-memory package
snapshot and runs the complete lexer, parser, resolver, and semantic checker.
It then locates the renamed declaration in the new occurrence index and proves:

1. every original equivalence-class member still exists at its transformed
   range and uniquely selects that declaration;
2. no new occurrence has started selecting the renamed declaration; and
3. the number and declaration/reference roles of occurrences are unchanged.

Any diagnostic, capture by a local or parameter, declaration collision,
missing occurrence, added binding, or changed candidate set yields
`BindingConflict` and no partial edit. The first version deliberately refuses
capture instead of synthesizing new qualifiers.

## LSP surface and non-goals

`salic lsp` advertises a rename provider with prepare support.
`textDocument/prepareRename` returns the exact occurrence range and source
placeholder. `textDocument/rename` returns a complete versioned workspace edit
or `RequestFailed` with a source-level reason.

Coordinated conceptual renames, comment/string updates, new-name suggestions,
minimal qualifier synthesis, partial-program rename, and persistent symbol IDs
are outside this contract.

## Verification

Unit and real-process transcript coverage includes aliases, selected and
ambiguous overloads, Unicode replacement, cross-module edits, shadow capture,
same-scope collision, invalid and non-NFC names, unchanged names, foreign and
compiler-private boundaries, dependency ownership, stable non-overlap,
versioned LSP edits, restart, cancellation, and supersession infrastructure.
