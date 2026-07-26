use crate::lexer::{lex, TokenKind};
use crate::parser::parse;

/// Format one complete Salicin source while preserving its logical token
/// stream. Physical line breaks are retained because they participate in
/// parenthesis-free application.
pub fn format_source(source: &str) -> Result<String, String> {
    parse(source).map_err(|error| error.to_string())?;

    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.is_empty() {
        return Ok(normalized);
    }

    let mut state = ScanState::default();
    let mut output = String::with_capacity(normalized.len() + 1);
    for line in normalized.lines() {
        let content = line.trim_end_matches([' ', '\t']);
        if content.trim().is_empty() {
            output.push('\n');
            continue;
        }

        let content = content.trim_start_matches([' ', '\t']);
        let analysis = state.scan_line(content);
        let indent = state
            .brace_depth
            .saturating_sub(usize::from(analysis.starts_with_close));
        output.push_str(&"  ".repeat(indent));
        output.push_str(content);
        output.push('\n');
        state.brace_depth = state
            .brace_depth
            .saturating_add(analysis.opens)
            .saturating_sub(analysis.closes);
    }

    let before = semantic_token_kinds(source)?;
    let after = semantic_token_kinds(&output)?;
    if before != after {
        return Err("internal formatter error: formatting changed the logical token stream".into());
    }
    parse(&output).map_err(|error| {
        format!("internal formatter error: formatted source no longer parses: {error}")
    })?;
    Ok(output)
}

fn semantic_token_kinds(source: &str) -> Result<Vec<TokenKind>, String> {
    let mut kinds = lex(source)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|token| token.kind)
        .collect::<Vec<_>>();
    let eof = kinds.pop();
    while kinds.last() == Some(&TokenKind::Newline) {
        kinds.pop();
    }
    if let Some(eof) = eof {
        kinds.push(eof);
    }
    Ok(kinds)
}

#[derive(Default)]
struct ScanState {
    brace_depth: usize,
    block_comment_depth: usize,
}

#[derive(Default)]
struct LineAnalysis {
    starts_with_close: bool,
    opens: usize,
    closes: usize,
}

impl ScanState {
    fn scan_line(&mut self, line: &str) -> LineAnalysis {
        let chars = line.chars().collect::<Vec<_>>();
        let mut analysis = LineAnalysis::default();
        let mut index = 0;
        let mut string = false;
        let mut escaped = false;
        let mut saw_code = false;

        while index < chars.len() {
            let ch = chars[index];
            let next = chars.get(index + 1).copied();
            if self.block_comment_depth != 0 {
                if ch == '/' && next == Some('*') {
                    self.block_comment_depth += 1;
                    index += 2;
                } else if ch == '*' && next == Some('/') {
                    self.block_comment_depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }
            if string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    string = false;
                }
                index += 1;
                continue;
            }
            if ch == '/' && next == Some('/') {
                break;
            }
            if ch == '/' && next == Some('*') {
                self.block_comment_depth += 1;
                index += 2;
                continue;
            }
            if ch == '"' {
                string = true;
                saw_code = true;
                index += 1;
                continue;
            }
            if !ch.is_whitespace() {
                if !saw_code {
                    analysis.starts_with_close = ch == '}';
                    saw_code = true;
                }
                if ch == '{' {
                    analysis.opens += 1;
                } else if ch == '}' {
                    analysis.closes += 1;
                }
            }
            index += 1;
        }
        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::format_source;

    #[test]
    fn formats_indentation_comments_and_trailing_space_idempotently() {
        let source = "let main(): i32 = {   \n// { stays a comment\nif true {\n/* nested {\n   /* } */\n*/\n42\n} else {\n0\n}\n}\n";
        let expected = "let main(): i32 = {\n  // { stays a comment\n  if true {\n    /* nested {\n    /* } */\n    */\n    42\n  } else {\n    0\n  }\n}\n";
        let formatted = format_source(source).expect("format valid source");
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).expect("format output again"),
            formatted
        );
    }

    #[test]
    fn preserves_expression_newlines_and_parenthesis_free_calls() {
        let source = "let apply(value: i32): i32 = { value }\nlet main(): i32 = {\napply\n42\n}\n";
        let expected =
            "let apply(value: i32): i32 = { value }\nlet main(): i32 = {\n  apply\n  42\n}\n";
        assert_eq!(format_source(source).expect("format calls"), expected);
    }

    #[test]
    fn rejects_invalid_source_without_rewriting_it() {
        let error = format_source("let main( = {\n").expect_err("invalid source must fail");
        assert!(error.contains("expected"));
    }
}
