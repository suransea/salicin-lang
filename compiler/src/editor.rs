use crate::ast::SourceLocation;
use crate::lexer::{lex, TokenKind};
use crate::modules::{resolve_sources, SourceUnit};
use crate::{codegen, parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentTarget {
    Library,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticPhase {
    Lexer,
    Parser,
    Resolver,
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangePrecision {
    Exact,
    Fallback,
}

/// One source position in both compiler-native UTF-8 bytes and the UTF-16
/// coordinates required by the Language Server Protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPosition {
    pub byte: usize,
    pub line: u32,
    pub utf16_character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorRange {
    pub start: EditorPosition,
    pub end: EditorPosition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorToken {
    pub kind: TokenKind,
    pub range: EditorRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDiagnostic {
    pub path: Option<String>,
    pub phase: DiagnosticPhase,
    pub message: String,
    pub range: Option<EditorRange>,
    pub range_precision: Option<RangePrecision>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentAnalysis {
    pub tokens: Vec<EditorToken>,
    pub diagnostics: Vec<EditorDiagnostic>,
}

pub struct EditorSource<'a> {
    pub path: &'a str,
    pub module_path: &'a [String],
    pub source: &'a str,
    pub is_root: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceDocumentAnalysis {
    pub path: String,
    pub tokens: Vec<EditorToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceAnalysis {
    pub documents: Vec<WorkspaceDocumentAnalysis>,
    pub diagnostics: Vec<EditorDiagnostic>,
}

/// Lex, parse, resolve, and type-check one editor document without generating
/// code. Later phases run only when every earlier phase succeeds.
pub fn analyze_document(source: &str, target: DocumentTarget) -> DocumentAnalysis {
    let index = SourceIndex::new(source);
    let tokens = match lex(source) {
        Ok(tokens) => tokens,
        Err(error) => {
            let range = index.scalar_range(error.line, error.column, error.line, error.column + 1);
            return DocumentAnalysis {
                tokens: Vec::new(),
                diagnostics: vec![EditorDiagnostic {
                    path: None,
                    phase: DiagnosticPhase::Lexer,
                    message: error.message,
                    range: Some(range),
                    range_precision: Some(RangePrecision::Exact),
                }],
            };
        }
    };
    let editor_tokens = tokens
        .iter()
        .map(|token| EditorToken {
            kind: token.kind.clone(),
            range: index.byte_range(token.start_byte, token.end_byte),
        })
        .collect();
    if let Err(error) = parser::parse_tokens(tokens) {
        return DocumentAnalysis {
            tokens: editor_tokens,
            diagnostics: vec![EditorDiagnostic {
                path: None,
                phase: DiagnosticPhase::Parser,
                message: error.message,
                range: Some(index.byte_range(error.start_byte, error.end_byte)),
                range_precision: Some(RangePrecision::Exact),
            }],
        };
    }

    let program = match resolve_sources(&[SourceUnit {
        path: "<document>".into(),
        module_path: Vec::new(),
        source: source.into(),
        is_root: true,
    }]) {
        Ok(program) => program,
        Err(diagnostics) => {
            return DocumentAnalysis {
                tokens: editor_tokens,
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| resolver_diagnostic(&index, diagnostic))
                    .collect(),
            };
        }
    };
    let checked = match target {
        DocumentTarget::Library => codegen::check_library(&program),
        DocumentTarget::Binary => codegen::check(&program),
    };
    let fallback_range = editor_tokens.first().map(|token| token.range);
    let diagnostics = checked
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|diagnostic| {
            let source = diagnostic
                .origin
                .as_ref()
                .and_then(|origin| origin.source.as_deref())
                .cloned();
            EditorDiagnostic {
                path: None,
                phase: DiagnosticPhase::Semantic,
                message: diagnostic.message,
                range: source
                    .as_ref()
                    .map(|location| index.location_range(location))
                    .or(fallback_range),
                range_precision: Some(if source.is_some() {
                    RangePrecision::Exact
                } else {
                    RangePrecision::Fallback
                }),
            }
        })
        .collect();
    DocumentAnalysis {
        tokens: editor_tokens,
        diagnostics,
    }
}

/// Analyze a complete set of source documents as one module graph.
pub fn analyze_workspace(
    sources: &[EditorSource<'_>],
    target: DocumentTarget,
) -> WorkspaceAnalysis {
    let indexes = sources
        .iter()
        .map(|source| SourceIndex::new(source.source))
        .collect::<Vec<_>>();
    let mut documents = Vec::with_capacity(sources.len());
    let mut diagnostics = Vec::new();
    for (source, index) in sources.iter().zip(&indexes) {
        let tokens = match lex(source.source) {
            Ok(tokens) => tokens,
            Err(error) => {
                diagnostics.push(EditorDiagnostic {
                    path: Some(source.path.to_owned()),
                    phase: DiagnosticPhase::Lexer,
                    message: error.message,
                    range: Some(index.scalar_range(
                        error.line,
                        error.column,
                        error.line,
                        error.column + 1,
                    )),
                    range_precision: Some(RangePrecision::Exact),
                });
                documents.push(WorkspaceDocumentAnalysis {
                    path: source.path.to_owned(),
                    tokens: Vec::new(),
                });
                continue;
            }
        };
        let editor_tokens = tokens
            .iter()
            .map(|token| EditorToken {
                kind: token.kind.clone(),
                range: index.byte_range(token.start_byte, token.end_byte),
            })
            .collect();
        if let Err(error) = parser::parse_tokens(tokens) {
            diagnostics.push(EditorDiagnostic {
                path: Some(source.path.to_owned()),
                phase: DiagnosticPhase::Parser,
                message: error.message,
                range: Some(index.byte_range(error.start_byte, error.end_byte)),
                range_precision: Some(RangePrecision::Exact),
            });
        }
        documents.push(WorkspaceDocumentAnalysis {
            path: source.path.to_owned(),
            tokens: editor_tokens,
        });
    }
    if !diagnostics.is_empty() {
        return WorkspaceAnalysis {
            documents,
            diagnostics,
        };
    }

    let units = sources
        .iter()
        .map(|source| SourceUnit {
            path: source.path.to_owned(),
            module_path: source.module_path.to_vec(),
            source: source.source.to_owned(),
            is_root: source.is_root,
        })
        .collect::<Vec<_>>();
    let program = match resolve_sources(&units) {
        Ok(program) => program,
        Err(errors) => {
            diagnostics.extend(
                errors
                    .into_iter()
                    .map(|error| workspace_resolver_diagnostic(sources, &indexes, error)),
            );
            return WorkspaceAnalysis {
                documents,
                diagnostics,
            };
        }
    };
    let checked = match target {
        DocumentTarget::Library => codegen::check_library(&program),
        DocumentTarget::Binary => codegen::check(&program),
    };
    for diagnostic in checked.err().unwrap_or_default() {
        let location = diagnostic
            .origin
            .as_ref()
            .and_then(|origin| origin.source.as_deref());
        let path = location.and_then(|location| location.path.clone());
        let range = location.and_then(|location| {
            let path = location.path.as_deref()?;
            let document = sources.iter().position(|source| source.path == path)?;
            Some(indexes[document].location_range(location))
        });
        diagnostics.push(EditorDiagnostic {
            path,
            phase: DiagnosticPhase::Semantic,
            message: diagnostic.message,
            range,
            range_precision: range.map(|_| RangePrecision::Exact),
        });
    }
    WorkspaceAnalysis {
        documents,
        diagnostics,
    }
}

fn resolver_diagnostic(index: &SourceIndex<'_>, diagnostic: String) -> EditorDiagnostic {
    let stripped = diagnostic
        .strip_prefix("<document>:")
        .unwrap_or(&diagnostic);
    let mut parts = stripped.trim_start().splitn(3, ':');
    let line = parts.next().and_then(|part| part.parse::<usize>().ok());
    let column = parts.next().and_then(|part| part.parse::<usize>().ok());
    let (message, range) = match (line, column) {
        (Some(line), Some(column)) => {
            let remainder = parts.next().unwrap_or(stripped).trim_start();
            (
                remainder
                    .strip_prefix("error:")
                    .unwrap_or(remainder)
                    .trim_start()
                    .to_owned(),
                Some(index.scalar_range(line, column, line, column + 1)),
            )
        }
        _ => (
            stripped
                .trim_start()
                .strip_prefix("error:")
                .unwrap_or(stripped.trim_start())
                .trim_start()
                .to_owned(),
            Some(index.byte_range(0, 0)),
        ),
    };
    EditorDiagnostic {
        path: None,
        phase: DiagnosticPhase::Resolver,
        message,
        range,
        range_precision: Some(if line.is_some() && column.is_some() {
            RangePrecision::Exact
        } else {
            RangePrecision::Fallback
        }),
    }
}

fn workspace_resolver_diagnostic(
    sources: &[EditorSource<'_>],
    indexes: &[SourceIndex<'_>],
    diagnostic: String,
) -> EditorDiagnostic {
    for (source, index) in sources.iter().zip(indexes) {
        let Some(stripped) = diagnostic
            .strip_prefix(source.path)
            .and_then(|diagnostic| diagnostic.strip_prefix(':'))
        else {
            continue;
        };
        let mut parts = stripped.trim_start().splitn(3, ':');
        let line = parts.next().and_then(|part| part.parse::<usize>().ok());
        let column = parts.next().and_then(|part| part.parse::<usize>().ok());
        if let (Some(line), Some(column)) = (line, column) {
            let remainder = parts.next().unwrap_or(stripped).trim_start();
            return EditorDiagnostic {
                path: Some(source.path.to_owned()),
                phase: DiagnosticPhase::Resolver,
                message: remainder
                    .strip_prefix("error:")
                    .unwrap_or(remainder)
                    .trim_start()
                    .to_owned(),
                range: Some(index.scalar_range(line, column, line, column + 1)),
                range_precision: Some(RangePrecision::Exact),
            };
        }
        return EditorDiagnostic {
            path: Some(source.path.to_owned()),
            phase: DiagnosticPhase::Resolver,
            message: stripped
                .trim_start()
                .strip_prefix("error:")
                .unwrap_or(stripped.trim_start())
                .trim_start()
                .to_owned(),
            range: Some(index.byte_range(0, 0)),
            range_precision: Some(RangePrecision::Fallback),
        };
    }
    EditorDiagnostic {
        path: None,
        phase: DiagnosticPhase::Resolver,
        message: diagnostic,
        range: None,
        range_precision: None,
    }
}

struct SourceIndex<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SourceIndex<'a> {
    fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .match_indices('\n')
                .map(|(offset, _)| offset.saturating_add(1)),
        );
        Self {
            source,
            line_starts,
        }
    }

    fn byte_range(&self, start: usize, end: usize) -> EditorRange {
        EditorRange {
            start: self.position(start),
            end: self.position(end),
        }
    }

    fn location_range(&self, location: &SourceLocation) -> EditorRange {
        self.scalar_range(
            location.line,
            location.column,
            location.end_line,
            location.end_column,
        )
    }

    fn scalar_range(
        &self,
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
    ) -> EditorRange {
        self.byte_range(
            self.byte_at_scalar(start_line, start_column),
            self.byte_at_scalar(end_line, end_column),
        )
    }

    fn byte_at_scalar(&self, line: usize, column: usize) -> usize {
        let start = self
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.source.len());
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.source.len());
        self.source[start..end]
            .char_indices()
            .nth(column.saturating_sub(1))
            .map_or(end, |(offset, _)| start + offset)
    }

    fn position(&self, byte: usize) -> EditorPosition {
        let byte = byte.min(self.source.len());
        let line = self
            .line_starts
            .partition_point(|start| *start <= byte)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        let utf16_character = self.source[line_start..byte]
            .encode_utf16()
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        EditorPosition {
            byte,
            line: line.try_into().unwrap_or(u32::MAX),
            utf16_character,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(source: &str, target: DocumentTarget) -> DocumentAnalysis {
        let source = source.to_owned();
        std::thread::Builder::new()
            .name("editor-analysis".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || analyze_document(&source, target))
            .expect("spawn editor analysis")
            .join()
            .expect("editor analysis completes")
    }

    #[test]
    fn unicode_tokens_expose_utf8_and_utf16_ranges() {
        let analysis = analyze(
            "let 变量 = 1\nlet symbol(value: i32): i32 = foreign(c, \"😀\")\n",
            DocumentTarget::Library,
        );
        let name = analysis
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::Ident("变量".into()))
            .expect("Unicode identifier token");
        assert_eq!(name.range.start.byte, 4);
        assert_eq!(name.range.end.byte, 10);
        assert_eq!(name.range.start.utf16_character, 4);
        assert_eq!(name.range.end.utf16_character, 6);
        let string = analysis
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::String("😀".into()))
            .expect("Unicode string token");
        assert_eq!(
            string.range.end.utf16_character - string.range.start.utf16_character,
            4
        );
    }

    #[test]
    fn parser_and_semantic_diagnostics_have_editor_ranges() {
        let parser = analyze("let value = )\n", DocumentTarget::Library);
        assert_eq!(parser.diagnostics[0].phase, DiagnosticPhase::Parser);
        assert_eq!(parser.diagnostics[0].range.unwrap().start.byte, 12);

        let semantic = analyze(
            "let main(): i32 = {\n  missing\n}\n",
            DocumentTarget::Binary,
        );
        assert_eq!(semantic.diagnostics[0].phase, DiagnosticPhase::Semantic);
        assert_eq!(
            semantic.diagnostics[0].range_precision,
            Some(RangePrecision::Exact)
        );
        assert_eq!(semantic.diagnostics[0].path, None);
        let range = semantic.diagnostics[0]
            .range
            .expect("semantic diagnostic range");
        assert_eq!((range.start.line, range.start.utf16_character), (1, 2));
        assert_eq!((range.end.line, range.end.utf16_character), (1, 9));
    }

    #[test]
    fn line_less_resolver_diagnostics_mark_fallback_ranges() {
        let analysis = analyze(
            "let value: i32 = 1\nlet value: i32 = 2\n",
            DocumentTarget::Library,
        );
        assert_eq!(analysis.diagnostics[0].phase, DiagnosticPhase::Resolver);
        assert_eq!(
            analysis.diagnostics[0].range_precision,
            Some(RangePrecision::Fallback)
        );
        let range = analysis.diagnostics[0].range.unwrap();
        assert_eq!(range.start.byte, 0);
        assert_eq!(range.end.byte, 0);
    }

    #[test]
    fn workspace_diagnostics_return_to_the_owning_document() {
        let root_modules = ["part".to_owned()];
        let empty = Vec::new();
        let root = "let main(): i32 = { part.answer() }\n";
        let part = "pub(package) let answer(): i32 = {\n  missing\n}\n";
        let analysis = std::thread::Builder::new()
            .name("workspace-editor-analysis".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                analyze_workspace(
                    &[
                        EditorSource {
                            path: "main.sc",
                            module_path: &empty,
                            source: root,
                            is_root: true,
                        },
                        EditorSource {
                            path: "part.sc",
                            module_path: &root_modules,
                            source: part,
                            is_root: false,
                        },
                    ],
                    DocumentTarget::Binary,
                )
            })
            .expect("spawn workspace analysis")
            .join()
            .expect("workspace analysis completes");
        assert_eq!(analysis.documents.len(), 2);
        let diagnostic = analysis
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.phase == DiagnosticPhase::Semantic)
            .expect("semantic diagnostic");
        assert_eq!(diagnostic.path.as_deref(), Some("part.sc"));
        let range = diagnostic.range.expect("cross-file semantic range");
        assert_eq!((range.start.line, range.start.utf16_character), (1, 2));
        assert_eq!((range.end.line, range.end.utf16_character), (1, 9));
    }

    #[test]
    fn every_failure_fixture_produces_ranged_editor_diagnostics() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fail");
        let mut paths = std::fs::read_dir(directory)
            .expect("read failure fixtures")
            .map(|entry| entry.expect("read failure fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "sc"))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
            let analysis = analyze(&source, DocumentTarget::Binary);
            assert!(
                !analysis.diagnostics.is_empty(),
                "{} unexpectedly had no diagnostics",
                path.display()
            );
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.range.is_some()),
                "{} had an unranged diagnostic: {:?}",
                path.display(),
                analysis.diagnostics
            );
        }
    }
}
