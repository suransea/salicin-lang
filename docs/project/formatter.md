# Source Formatter

Status: implemented conservative formatter

The Salicin formatter normalizes layout without reconstructing source from
the semantic AST. This is required because physical newlines participate in
parenthesis-free application and because comments are intentionally absent
from the AST.

## Preservation Invariants

Formatting must preserve the complete lexer token-kind stream, including
every logical `Newline` token. The only permitted terminal difference is
adding the conventional final newline before `Eof`.

Consequently, the formatter:

- never inserts, removes, joins, splits, or moves a source line;
- never changes token spelling, string contents, comment delimiters or
  non-layout content, semicolons, or other delimiters;
- never changes horizontal spacing between non-trivia tokens;
- retains blank-line count;
- normalizes line endings to LF;
- removes trailing spaces and tabs;
- indents nonblank lines by two spaces per unmatched source brace;
- outdents a line whose first code token is `}`;
- writes exactly one final newline for nonempty source.

Brace scanning ignores strings, line comments, and nested block comments.
Comment-only lines follow the surrounding block indentation. Leading and
trailing horizontal trivia inside a comment line is layout rather than
comment payload and may be normalized.

Before returning formatted text, the implementation lexes both inputs and
rejects any token-stream difference. It then parses the output again. This
guard makes formatter preservation an executable invariant rather than an
assumption about the indentation scanner.

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
trailing whitespace, closing-brace indentation, final newlines, invalid
source, `--check`, package selection, dependency isolation, and repeated
formatting. Every passing language fixture is formatted twice and must be
idempotent.
