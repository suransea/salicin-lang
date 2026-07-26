# Source Formatter

Status: implemented conservative formatter

The Salicin formatter normalizes layout without reconstructing source from
the semantic AST. This is required because physical newlines participate in
parenthesis-free application and because comments are intentionally absent
from the AST. The parser records source-backed layout spans for parameter
groups, `where` predicates, trailing closures, match arms, and braced regions;
the formatter consumes those roles without recognizing library declaration
names.

## Preservation Invariants

Formatting preserves every existing source line because logical `Newline`
tokens participate in parenthesis-free application. It may add separators at
unambiguous nested block boundaries, such as between the two blocks in
`= { unsafe {`, and between leading consecutive closing braces. The expanded
source must parse before indentation proceeds.

Consequently, the formatter:

- only inserts source lines when expanding directly nested block expressions
  or leading runs of closing braces;
- never changes token spelling, string contents, comment delimiters or
  non-layout content, semicolons, or other delimiters;
- never changes horizontal spacing between non-trivia tokens;
- retains blank-line count;
- normalizes line endings to LF;
- removes trailing spaces and tabs;
- indents nonblank lines by two spaces per unmatched source brace;
- tracks `()` and `[]` delimiter depth within the current braced region;
- adds one continuation level to parser-identified parameter groups,
  trailing closures, match arms, and operator continuations;
- adds one continuation level to subsequent `where` predicates;
- expands directly nested semantic blocks and leading closing-brace runs into
  one visible block level per line;
- writes exactly one final newline for nonempty source.

Brace scanning ignores strings, line comments, and nested block comments.
Comment-only lines follow the surrounding block indentation. Leading and
trailing horizontal trivia inside a comment line is layout rather than
comment payload and may be normalized.

Before returning formatted text, the implementation compares the expanded
source with the indented output and rejects any token-stream difference. It
then parses the output again. This keeps all non-layout syntax and every
pre-existing line boundary stable while validating the deliberately inserted
block separators.

## Command

`salic fmt file.sc` formats one source file in place.

`salic fmt package` formats every target and `.sc` file under the root
package's `src` directory. It does not rewrite path dependencies. With no
path, the command discovers `salicin.toml` from the current directory.

`salic fmt --check path` performs the same validation without writing and
returns status 1 when any selected file differs.

All selected files are read and validated before the first write. Invalid
source receives its parser diagnostic and remains unchanged.

## Deliberate Limits

This first formatter contract does not normalize spaces around punctuation,
wrap long lines, collapse or expand literals, reorder imports or
declarations, or rewrite between parenthesized and parenthesis-free calls.
Those operations require a lossless concrete syntax tree with byte spans and
trivia ownership. The following LSP span milestone provides that shared
foundation rather than adding an independent second parser.

## Verification

Unit and CLI tests cover nested block comments, braces inside comments,
delimiter nesting, ordinary parenthesized expressions, declaration and
operator continuations, `where` predicates, trailing and nested blocks,
prefix-match arms, final newlines, invalid source, `--check`, package
selection, dependency isolation, and repeated formatting. Every passing
language fixture is formatted twice and must be idempotent.
