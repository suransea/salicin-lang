use std::collections::HashSet;

use crate::lexer::{lex, TokenKind};
use crate::parser::{parse, parse_with_source_layout, SourceLayout};

/// Format one complete Salicin source while preserving its logical token
/// stream. Existing physical line breaks are retained because they participate
/// in parenthesis-free application; nested block boundaries may add lines.
pub fn format_source(source: &str) -> Result<String, String> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.is_empty() {
        return Ok(normalized);
    }

    let (_, source_layout) =
        parse_with_source_layout(&normalized).map_err(|error| error.to_string())?;
    let expanded = expand_nested_blocks(&normalized, &source_layout);
    let (_, source_layout) = parse_with_source_layout(&expanded).map_err(|error| {
        format!("internal formatter error: expanded source no longer parses: {error}")
    })?;
    let layout = analyze_layout(&expanded, &source_layout)?;
    let mut state = ScanState::default();
    let mut output = String::with_capacity(expanded.len() + 1);
    for (line_index, line) in expanded.lines().enumerate() {
        let content = line.trim_end_matches([' ', '\t']);
        if content.trim().is_empty() {
            output.push('\n');
            continue;
        }

        let content = content.trim_start_matches([' ', '\t']);
        let analysis = state.scan_line(content);
        let syntax = &layout[line_index];
        let continuation = syntax.continuation;
        let indent = state
            .brace_depth
            .saturating_sub(usize::from(analysis.starts_with_close))
            .saturating_add(syntax.delimiter_indent)
            .saturating_add(continuation);
        output.push_str(&"  ".repeat(indent));
        output.push_str(content);
        output.push('\n');
        state.brace_depth = state
            .brace_depth
            .saturating_add(analysis.opens)
            .saturating_sub(analysis.closes);
    }

    let before = semantic_token_kinds(&expanded)?;
    let after = semantic_token_kinds(&output)?;
    if before != after {
        return Err("internal formatter error: formatting changed the logical token stream".into());
    }
    parse(&output).map_err(|error| {
        format!("internal formatter error: formatted source no longer parses: {error}")
    })?;
    Ok(output)
}

fn expand_nested_blocks(source: &str, layout: &SourceLayout) -> String {
    let mut regions = layout
        .blocks
        .iter()
        .chain(&layout.closures)
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| region.open_byte);
    let mut insertions = Vec::new();
    for (parent_index, parent) in regions.iter().enumerate() {
        for child in &regions[parent_index + 1..] {
            if child.open_byte >= parent.close_byte {
                break;
            }
            if child.close_byte < parent.close_byte && child.open_line == parent.open_line {
                insertions.push(parent.body_start_byte);
            }
            if child.close_byte < parent.close_byte && child.close_line == parent.close_line {
                insertions.push(parent.close_byte);
            }
        }
    }

    let tokens = lex(source).expect("a parsed source must lex");
    let mut current_line = 0usize;
    let mut first_code_on_line = true;
    for (index, token) in tokens.iter().enumerate() {
        if token.line != current_line {
            current_line = token.line;
            first_code_on_line = true;
        }
        if token.kind == TokenKind::Newline {
            continue;
        }
        if token.kind == TokenKind::Eof {
            break;
        }
        if first_code_on_line && token.kind == TokenKind::RBrace {
            let mut previous = token;
            for close in &tokens[index + 1..] {
                if close.line != token.line
                    || close.kind != TokenKind::RBrace
                    || !source[previous.end_byte..close.start_byte]
                        .chars()
                        .all(char::is_whitespace)
                {
                    break;
                }
                insertions.push(close.start_byte);
                previous = close;
            }
        }
        first_code_on_line = false;
    }

    insertions.sort_unstable();
    insertions.dedup();
    let mut expanded = source.to_owned();
    for byte in insertions.into_iter().rev() {
        expanded.insert(byte, '\n');
    }
    expanded
}

#[derive(Default)]
struct LineSyntax {
    first: Option<TokenKind>,
    first_byte: Option<usize>,
    delimiter_depth: usize,
    leading_delimiter_closes: usize,
    code_token_count: usize,
    has_parameter_group: bool,
    has_repeated_parameter_group: bool,
    is_parameter_group: bool,
    is_repeated_parameter_group: bool,
    is_where_predicate: bool,
    match_arm_depth: usize,
    trailing_closure_depth: usize,
    last: Option<TokenKind>,
    delimiter_indent: usize,
    continuation: usize,
}

fn analyze_layout(source: &str, source_layout: &SourceLayout) -> Result<Vec<LineSyntax>, String> {
    let mut lines = (0..source.lines().count())
        .map(|_| LineSyntax::default())
        .collect::<Vec<_>>();
    let parameter_groups = source_layout
        .parameter_groups
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let repeated_parameter_groups = source_layout
        .repeated_parameter_groups
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let where_predicates = source_layout
        .where_predicates
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut delimiter_depth = 0usize;
    let mut brace_delimiter_baselines = vec![0usize];
    let tokens = lex(source).map_err(|error| error.to_string())?;
    for token in &tokens {
        if token.kind == TokenKind::Newline {
            continue;
        }
        if token.kind == TokenKind::Eof {
            break;
        }
        let line = &mut lines[token.line - 1];
        if line.first.is_none() {
            line.first = Some(token.kind.clone());
            line.first_byte = Some(token.start_byte);
            line.delimiter_depth = delimiter_depth.saturating_sub(
                brace_delimiter_baselines
                    .last()
                    .copied()
                    .unwrap_or_default(),
            );
        }
        if parameter_groups.contains(&token.start_byte) {
            line.has_parameter_group = true;
        }
        if repeated_parameter_groups.contains(&token.start_byte) {
            line.has_repeated_parameter_group = true;
        }
        if line.first_byte == Some(token.start_byte) {
            line.is_parameter_group = parameter_groups.contains(&token.start_byte);
            line.is_repeated_parameter_group =
                repeated_parameter_groups.contains(&token.start_byte);
            line.is_where_predicate = where_predicates.contains(&token.start_byte);
        }
        if line.code_token_count == line.leading_delimiter_closes
            && matches!(token.kind, TokenKind::RParen | TokenKind::RBracket)
        {
            line.leading_delimiter_closes += 1;
        }
        line.code_token_count += 1;
        line.last = Some(token.kind.clone());
        match token.kind {
            TokenKind::LBrace => brace_delimiter_baselines.push(delimiter_depth),
            TokenKind::RBrace => {
                brace_delimiter_baselines.pop();
                if brace_delimiter_baselines.is_empty() {
                    brace_delimiter_baselines.push(0);
                }
            }
            TokenKind::LParen | TokenKind::LBracket => delimiter_depth += 1,
            TokenKind::RParen | TokenKind::RBracket => {
                delimiter_depth = delimiter_depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    for closure in &source_layout.trailing_closures {
        let Some(start) = tokens
            .iter()
            .find(|token| token.start_byte == closure.start_byte)
        else {
            continue;
        };
        for line in &mut lines[start.line - 1..] {
            let Some(first_byte) = line.first_byte else {
                continue;
            };
            if first_byte > closure.close_byte {
                break;
            }
            if first_byte >= closure.start_byte {
                line.trailing_closure_depth = 1;
            }
        }
    }

    for arm in &source_layout.match_arms {
        let Some(start) = tokens
            .iter()
            .find(|token| token.start_byte == arm.open_byte)
        else {
            continue;
        };
        for line in &mut lines[start.line - 1..] {
            let Some(first_byte) = line.first_byte else {
                continue;
            };
            if first_byte > arm.close_byte {
                break;
            }
            line.match_arm_depth = 1;
        }
    }

    let mut declaration_continuation = false;
    let mut previous_last = None;
    for (index, line) in lines.iter_mut().enumerate() {
        line.delimiter_indent = line
            .delimiter_depth
            .saturating_sub(line.leading_delimiter_closes);
        let continues_declaration = line.is_parameter_group
            || line.is_repeated_parameter_group
            || declaration_continuation
                && matches!(line.first, Some(TokenKind::Colon | TokenKind::Equal));
        let operator_continuation = index != 0
            && line.first.is_some()
            && previous_last.as_ref().is_some_and(is_continuation_operator)
            && line.delimiter_indent == 0;
        line.continuation = usize::from(continues_declaration)
            + line.match_arm_depth
            + line.trailing_closure_depth
            + usize::from(operator_continuation);
        if line.is_where_predicate && index != 0 {
            line.continuation = 1;
        }
        declaration_continuation = (line.has_parameter_group || line.has_repeated_parameter_group)
            && !matches!(line.first, Some(TokenKind::Equal));
        if continues_declaration && line.first == Some(TokenKind::Colon) {
            declaration_continuation = true;
        }
        previous_last = line.last.clone();
    }
    Ok(lines)
}

fn is_continuation_operator(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Dot
            | TokenKind::QuestionDot
            | TokenKind::QuestionQuestion
            | TokenKind::Equal
            | TokenKind::EqualEqual
            | TokenKind::BangEqual
            | TokenKind::Plus
            | TokenKind::PlusEqual
            | TokenKind::Minus
            | TokenKind::MinusEqual
            | TokenKind::Star
            | TokenKind::StarEqual
            | TokenKind::Slash
            | TokenKind::SlashEqual
            | TokenKind::Percent
            | TokenKind::PercentEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual
            | TokenKind::AndAnd
            | TokenKind::OrOr
            | TokenKind::Amp
            | TokenKind::AmpEqual
            | TokenKind::Pipe
            | TokenKind::PipeEqual
            | TokenKind::Caret
            | TokenKind::CaretEqual
            | TokenKind::Shl
            | TokenKind::ShlEqual
            | TokenKind::Shr
            | TokenKind::ShrEqual
    )
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
                if ch == '{' {
                    analysis.opens += 1;
                } else if ch == '}' {
                    analysis.closes += 1;
                    if !saw_code {
                        analysis.starts_with_close = true;
                    }
                }
                saw_code = true;
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
    fn expands_nested_blocks_and_their_leading_closing_braces() {
        let source = "let run(move action: (): i32): i32 = { action() }\nlet main(): i32 = { run { 42 } }\nlet other(): i32 = { unsafe {\n0\n} }\n";
        let expected = "let run(move action: (): i32): i32 = { action() }\nlet main(): i32 = {\n  run { 42 }\n}\nlet other(): i32 = {\n  unsafe {\n    0\n  }\n}\n";
        let formatted = format_source(source).expect("format nested blocks");
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).expect("format output again"),
            formatted
        );
    }

    #[test]
    fn indents_parameter_groups_and_match_arms_as_continuations() {
        let source = "let apply(comptime e: effects)\n(action: (i32): i32 with(e))\n(value: i32): i32 with(e) = { action(value) }\n\nlet main(): i32 = {\nmatch true\n{ true -> match false\n{ false -> apply()(42) }\n{ true -> 0 } }\n{ false -> 0 }\n}\n";
        let expected = "let apply(comptime e: effects)\n  (action: (i32): i32 with(e))\n  (value: i32): i32 with(e) = { action(value) }\n\nlet main(): i32 = {\n  match true\n    { true -> match false\n      { false -> apply()(42) }\n      { true -> 0 } }\n    { false -> 0 }\n}\n";
        let formatted = format_source(source).expect("format continuations");
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).expect("format output again"),
            formatted
        );
    }

    #[test]
    fn formats_delimiters_where_clauses_and_expression_continuations() {
        let source = "let marker = trait {}\nlet duplicate(comptime t: type)(value: t): t\nwhere comptime t: copyable,\nT: marker, = {\nvalue\n}\n\nlet add(\nleft: i32,\nright: i32,\n): i32 = {\nleft +\nright\n}\n\nlet main(): i32 = {\nlet values = [\n40,\n2,\n]\nlet grouped =\n(values[0] + values[1])\nadd(\nvalues[0],\nvalues[1],\n) + grouped - 42\n}\n";
        let expected = "let marker = trait {}\nlet duplicate(comptime t: type)(value: t): t\nwhere comptime t: copyable,\n  comptime t: marker, = {\n  value\n}\n\nlet add(\n  left: i32,\n  right: i32,\n): i32 = {\n  left +\n    right\n}\n\nlet main(): i32 = {\n  let values = [\n    40,\n    2,\n  ]\n  let grouped =\n    (values[0] + values[1])\n  add(\n    values[0],\n    values[1],\n  ) + grouped - 42\n}\n";
        let formatted = format_source(source).expect("format syntax continuations");
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).expect("format output again"),
            formatted
        );
    }

    #[test]
    fn does_not_treat_closure_parameters_as_declaration_continuations() {
        let source = "let main(): i32 = {\nlet closure = {\n(left: i32) -> do {\nleft\n}\n}\nclosure(42)\n}\n";
        let expected = "let main(): i32 = {\n  let closure = {\n    (left: i32) -> do {\n      left\n    }\n  }\n  closure(42)\n}\n";
        let formatted = format_source(source).expect("format closure parameters");
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).expect("format output again"),
            formatted
        );
    }

    #[test]
    fn rejects_invalid_source_without_rewriting_it() {
        let error = format_source("let main( = {\n").expect_err("invalid source must fail");
        assert!(error.contains("expected"));
    }
}
