use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ast::{
    EnumDef, ExtendDef, ExtendMember, Item, Program, SourceLocation, TraitMember, VariantFields,
};
use crate::lexer::{lex, Token, TokenKind};
use crate::modules::{resolve_sources_diagnostics, ResolverDiagnostic, SourceUnit};
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
pub enum DiagnosticSeverity {
    Error,
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
    pub document: String,
    pub phase: DiagnosticPhase,
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub range: Option<EditorRange>,
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
    /// Source-backed semantic facts for this complete, successfully checked
    /// snapshot. Invalid snapshots deliberately publish no partial index.
    pub semantic_index: SemanticIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SemanticSymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticSymbolKind {
    Declaration,
    Alias,
    Field,
    Variant,
    Overload,
    TraitMember,
    Implementation,
    ExtensionMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticOccurrenceRole {
    Declaration,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticSymbol {
    pub id: SemanticSymbolId,
    /// Deterministic within one immutable snapshot. This is a source identity,
    /// never a generated specialization or native linker name.
    pub key: String,
    pub kind: SemanticSymbolKind,
    pub document: String,
    pub range: EditorRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticOccurrence {
    pub document: String,
    pub range: EditorRange,
    pub role: SemanticOccurrenceRole,
    /// Empty means unresolved, and multiple entries preserve ambiguity for a
    /// later navigation query instead of selecting by traversal order.
    pub symbols: Vec<SemanticSymbolId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticIndex {
    pub symbols: Vec<SemanticSymbol>,
    pub occurrences: Vec<SemanticOccurrence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceSnapshotId {
    pub session: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshotDocument {
    pub path: String,
    pub module_path: Vec<String>,
    pub source: String,
    pub is_root: bool,
    /// The client version for an open overlay, or `None` for baseline text.
    pub version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub id: WorkspaceSnapshotId,
    pub target: DocumentTarget,
    pub documents: Vec<WorkspaceSnapshotDocument>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceSnapshotAnalysis {
    pub id: WorkspaceSnapshotId,
    pub document_versions: Vec<(String, Option<i64>)>,
    pub analysis: WorkspaceAnalysis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSessionError {
    DuplicateDocument(String),
    UnknownDocument(String),
    DocumentAlreadyOpen(String),
    DocumentNotOpen(String),
    StaleDocumentVersion {
        document: String,
        current: i64,
        received: i64,
    },
    RevisionExhausted,
}

impl fmt::Display for WorkspaceSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDocument(document) => {
                write!(formatter, "duplicate workspace document `{document}`")
            }
            Self::UnknownDocument(document) => {
                write!(formatter, "unknown workspace document `{document}`")
            }
            Self::DocumentAlreadyOpen(document) => {
                write!(formatter, "workspace document `{document}` is already open")
            }
            Self::DocumentNotOpen(document) => {
                write!(formatter, "workspace document `{document}` is not open")
            }
            Self::StaleDocumentVersion {
                document,
                current,
                received,
            } => write!(
                formatter,
                "workspace document `{document}` has version {current}; rejected stale version {received}"
            ),
            Self::RevisionExhausted => {
                formatter.write_str("workspace snapshot revision space is exhausted")
            }
        }
    }
}

impl std::error::Error for WorkspaceSessionError {}

#[derive(Debug, Clone)]
struct SessionDocument {
    path: String,
    module_path: Vec<String>,
    baseline: String,
    is_root: bool,
}

#[derive(Debug, Clone)]
struct OpenDocument {
    version: i64,
    source: String,
}

/// Mutable editor state whose analyses always run against immutable snapshots.
///
/// Baseline source text is supplied by the caller. Opening or changing a
/// document only updates an in-memory overlay; this type performs no file I/O.
pub struct WorkspaceSession {
    id: u64,
    revision: u64,
    target: DocumentTarget,
    documents: Vec<SessionDocument>,
    document_indexes: HashMap<String, usize>,
    open_documents: HashMap<String, OpenDocument>,
}

static NEXT_WORKSPACE_SESSION_ID: AtomicU64 = AtomicU64::new(1);

impl WorkspaceSession {
    pub fn new(
        sources: &[EditorSource<'_>],
        target: DocumentTarget,
    ) -> Result<Self, WorkspaceSessionError> {
        let mut document_indexes = HashMap::with_capacity(sources.len());
        let mut documents = Vec::with_capacity(sources.len());
        for source in sources {
            let index = documents.len();
            if document_indexes
                .insert(source.path.to_owned(), index)
                .is_some()
            {
                return Err(WorkspaceSessionError::DuplicateDocument(
                    source.path.to_owned(),
                ));
            }
            documents.push(SessionDocument {
                path: source.path.to_owned(),
                module_path: source.module_path.to_vec(),
                baseline: source.source.to_owned(),
                is_root: source.is_root,
            });
        }
        Ok(Self {
            id: NEXT_WORKSPACE_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            target,
            documents,
            document_indexes,
            open_documents: HashMap::new(),
        })
    }

    pub fn snapshot_id(&self) -> WorkspaceSnapshotId {
        WorkspaceSnapshotId {
            session: self.id,
            revision: self.revision,
        }
    }

    pub fn open_document(
        &mut self,
        document: &str,
        version: i64,
        source: impl Into<String>,
    ) -> Result<WorkspaceSnapshotId, WorkspaceSessionError> {
        self.require_document(document)?;
        if self.open_documents.contains_key(document) {
            return Err(WorkspaceSessionError::DocumentAlreadyOpen(
                document.to_owned(),
            ));
        }
        self.bump_revision()?;
        self.open_documents.insert(
            document.to_owned(),
            OpenDocument {
                version,
                source: source.into(),
            },
        );
        Ok(self.snapshot_id())
    }

    pub fn change_document(
        &mut self,
        document: &str,
        version: i64,
        source: impl Into<String>,
    ) -> Result<WorkspaceSnapshotId, WorkspaceSessionError> {
        self.require_document(document)?;
        let current = self
            .open_documents
            .get(document)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen(document.to_owned()))?
            .version;
        if version <= current {
            return Err(WorkspaceSessionError::StaleDocumentVersion {
                document: document.to_owned(),
                current,
                received: version,
            });
        }
        self.bump_revision()?;
        self.open_documents.insert(
            document.to_owned(),
            OpenDocument {
                version,
                source: source.into(),
            },
        );
        Ok(self.snapshot_id())
    }

    pub fn close_document(
        &mut self,
        document: &str,
    ) -> Result<WorkspaceSnapshotId, WorkspaceSessionError> {
        self.require_document(document)?;
        if !self.open_documents.contains_key(document) {
            return Err(WorkspaceSessionError::DocumentNotOpen(document.to_owned()));
        }
        self.bump_revision()?;
        self.open_documents.remove(document);
        Ok(self.snapshot_id())
    }

    /// Record the saved text as the new baseline without writing it to disk.
    /// If the client omits text, the current open overlay becomes the baseline.
    pub fn save_document(
        &mut self,
        document: &str,
        source: Option<&str>,
    ) -> Result<WorkspaceSnapshotId, WorkspaceSessionError> {
        let index = self.require_document(document)?;
        let overlay = self
            .open_documents
            .get(document)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen(document.to_owned()))?;
        let baseline = source.unwrap_or(&overlay.source).to_owned();
        self.bump_revision()?;
        self.documents[index].baseline = baseline;
        Ok(self.snapshot_id())
    }

    /// Replace caller-owned baseline text without touching the filesystem.
    /// An open overlay continues to take precedence until it is closed.
    pub fn update_baseline(
        &mut self,
        document: &str,
        source: impl Into<String>,
    ) -> Result<WorkspaceSnapshotId, WorkspaceSessionError> {
        let index = self.require_document(document)?;
        self.bump_revision()?;
        self.documents[index].baseline = source.into();
        Ok(self.snapshot_id())
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let documents = self
            .documents
            .iter()
            .map(|document| {
                let overlay = self.open_documents.get(&document.path);
                WorkspaceSnapshotDocument {
                    path: document.path.clone(),
                    module_path: document.module_path.clone(),
                    source: overlay
                        .map(|overlay| overlay.source.clone())
                        .unwrap_or_else(|| document.baseline.clone()),
                    is_root: document.is_root,
                    version: overlay.map(|overlay| overlay.version),
                }
            })
            .collect();
        WorkspaceSnapshot {
            id: self.snapshot_id(),
            target: self.target,
            documents,
        }
    }

    /// Accept only an analysis produced for the current immutable snapshot.
    /// A superseded result is consumed and dropped.
    pub fn accept_analysis(&self, result: WorkspaceSnapshotAnalysis) -> Option<WorkspaceAnalysis> {
        (result.id == self.snapshot_id()).then_some(result.analysis)
    }

    fn require_document(&self, document: &str) -> Result<usize, WorkspaceSessionError> {
        self.document_indexes
            .get(document)
            .copied()
            .ok_or_else(|| WorkspaceSessionError::UnknownDocument(document.to_owned()))
    }

    fn bump_revision(&mut self) -> Result<(), WorkspaceSessionError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(WorkspaceSessionError::RevisionExhausted)?;
        Ok(())
    }
}

impl WorkspaceSnapshot {
    pub fn analyze(&self) -> WorkspaceSnapshotAnalysis {
        let sources = self
            .documents
            .iter()
            .map(|document| EditorSource {
                path: &document.path,
                module_path: &document.module_path,
                source: &document.source,
                is_root: document.is_root,
            })
            .collect::<Vec<_>>();
        WorkspaceSnapshotAnalysis {
            id: self.id,
            document_versions: self
                .documents
                .iter()
                .map(|document| (document.path.clone(), document.version))
                .collect(),
            analysis: analyze_workspace(&sources, self.target),
        }
    }
}

/// Lex, parse, resolve, and type-check one editor document without generating
/// code. Later phases run only when every earlier phase succeeds.
pub fn analyze_document(source: &str, target: DocumentTarget) -> DocumentAnalysis {
    analyze_document_at("<document>", source, target)
}

/// Analyze one named editor document. The identity is preserved on every
/// diagnostic, including document-wide failures that have no exact range.
pub fn analyze_document_at(
    document: &str,
    source: &str,
    target: DocumentTarget,
) -> DocumentAnalysis {
    let index = SourceIndex::new(source);
    let tokens = match lex(source) {
        Ok(tokens) => tokens,
        Err(error) => {
            let range = index.scalar_range(error.line, error.column, error.line, error.column + 1);
            return DocumentAnalysis {
                tokens: Vec::new(),
                diagnostics: vec![EditorDiagnostic {
                    document: document.to_owned(),
                    phase: DiagnosticPhase::Lexer,
                    severity: DiagnosticSeverity::Error,
                    code: "salicin.lex".to_owned(),
                    message: error.message,
                    range: Some(range),
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
                document: document.to_owned(),
                phase: DiagnosticPhase::Parser,
                severity: DiagnosticSeverity::Error,
                code: "salicin.parse".to_owned(),
                message: error.message,
                range: Some(index.byte_range(error.start_byte, error.end_byte)),
            }],
        };
    }

    let program = match resolve_sources_diagnostics(&[SourceUnit {
        path: document.into(),
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
                    .map(|diagnostic| resolver_diagnostic(&index, diagnostic, document))
                    .collect(),
            };
        }
    };
    let checked = match target {
        DocumentTarget::Library => codegen::check_library(&program),
        DocumentTarget::Binary => codegen::check(&program),
    };
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
                document: source
                    .as_ref()
                    .and_then(|location| location.path.clone())
                    .unwrap_or_else(|| document.to_owned()),
                phase: DiagnosticPhase::Semantic,
                severity: DiagnosticSeverity::Error,
                code: "salicin.semantic".to_owned(),
                message: diagnostic.message,
                range: source
                    .as_ref()
                    .map(|location| index.location_range(location)),
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
    let mut source_tokens = Vec::with_capacity(sources.len());
    for (source, index) in sources.iter().zip(&indexes) {
        let tokens = match lex(source.source) {
            Ok(tokens) => tokens,
            Err(error) => {
                diagnostics.push(EditorDiagnostic {
                    document: source.path.to_owned(),
                    phase: DiagnosticPhase::Lexer,
                    severity: DiagnosticSeverity::Error,
                    code: "salicin.lex".to_owned(),
                    message: error.message,
                    range: Some(index.scalar_range(
                        error.line,
                        error.column,
                        error.line,
                        error.column + 1,
                    )),
                });
                documents.push(WorkspaceDocumentAnalysis {
                    path: source.path.to_owned(),
                    tokens: Vec::new(),
                });
                source_tokens.push(Vec::new());
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
        if let Err(error) = parser::parse_tokens(tokens.clone()) {
            diagnostics.push(EditorDiagnostic {
                document: source.path.to_owned(),
                phase: DiagnosticPhase::Parser,
                severity: DiagnosticSeverity::Error,
                code: "salicin.parse".to_owned(),
                message: error.message,
                range: Some(index.byte_range(error.start_byte, error.end_byte)),
            });
        }
        documents.push(WorkspaceDocumentAnalysis {
            path: source.path.to_owned(),
            tokens: editor_tokens,
        });
        source_tokens.push(tokens);
    }
    if !diagnostics.is_empty() {
        return WorkspaceAnalysis {
            documents,
            diagnostics,
            semantic_index: SemanticIndex::default(),
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
    let program = match resolve_sources_diagnostics(&units) {
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
                semantic_index: SemanticIndex::default(),
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
        let document = location
            .and_then(|location| location.path.clone())
            .unwrap_or_else(|| "<workspace>".to_owned());
        let range = location.and_then(|location| {
            let path = location.path.as_deref()?;
            let document = sources.iter().position(|source| source.path == path)?;
            Some(indexes[document].location_range(location))
        });
        diagnostics.push(EditorDiagnostic {
            document,
            phase: DiagnosticPhase::Semantic,
            severity: DiagnosticSeverity::Error,
            code: "salicin.semantic".to_owned(),
            message: diagnostic.message,
            range,
        });
    }
    let semantic_index = if diagnostics.is_empty() {
        build_semantic_index(sources, &indexes, &source_tokens, &program)
    } else {
        SemanticIndex::default()
    };
    WorkspaceAnalysis {
        documents,
        diagnostics,
        semantic_index,
    }
}

#[derive(Debug)]
struct PendingSemanticSymbol {
    key: String,
    kind: SemanticSymbolKind,
    document_index: usize,
    token_index: usize,
}

fn build_semantic_index(
    sources: &[EditorSource<'_>],
    indexes: &[SourceIndex<'_>],
    tokens: &[Vec<Token>],
    program: &Program,
) -> SemanticIndex {
    let mut pending = Vec::new();
    let mut overload_counts = HashMap::<String, usize>::new();
    for item in &program.items {
        if let Item::Function(function) = item {
            *overload_counts.entry(function.name.clone()).or_default() += 1;
        }
    }

    for (item_index, (item, origin)) in program.items.iter().zip(&program.item_origins).enumerate()
    {
        let Some(location) = origin.source.as_deref() else {
            continue;
        };
        let Some(path) = location.path.as_deref() else {
            continue;
        };
        let Some(document_index) = sources.iter().position(|source| source.path == path) else {
            continue;
        };
        let document_tokens = &tokens[document_index];
        let Some(start) = document_tokens
            .iter()
            .position(|token| token.line == location.line && token.column == location.column)
        else {
            continue;
        };
        let end = program
            .item_origins
            .iter()
            .skip(item_index + 1)
            .filter_map(|next| next.source.as_deref())
            .find(|next| next.path.as_deref() == Some(path))
            .and_then(|next| {
                document_tokens
                    .iter()
                    .position(|token| token.line == next.line && token.column == next.column)
            })
            .unwrap_or(document_tokens.len());

        match item {
            Item::Extend(extension) => collect_extension_symbols(
                extension,
                path,
                document_index,
                start,
                end,
                document_tokens,
                &mut pending,
            ),
            _ => {
                let Some((canonical, name)) = item_identity(item) else {
                    continue;
                };
                let Some(name_token) = find_ident(document_tokens, start, end, name) else {
                    continue;
                };
                let kind = match item {
                    Item::TypeAlias(_) => SemanticSymbolKind::Alias,
                    Item::Function(_)
                        if overload_counts.get(canonical).copied().unwrap_or_default() > 1 =>
                    {
                        SemanticSymbolKind::Overload
                    }
                    _ => SemanticSymbolKind::Declaration,
                };
                let key = if kind == SemanticSymbolKind::Overload {
                    format!(
                        "{canonical}#{}:{}",
                        path, document_tokens[name_token].start_byte
                    )
                } else {
                    canonical.to_owned()
                };
                pending.push(PendingSemanticSymbol {
                    key: key.clone(),
                    kind,
                    document_index,
                    token_index: name_token,
                });
                collect_item_children(
                    item,
                    &key,
                    document_index,
                    name_token + 1,
                    end,
                    document_tokens,
                    &mut pending,
                );
            }
        }
    }

    collect_import_aliases(sources, tokens, &mut pending);
    pending.sort_by_key(|symbol| {
        (
            symbol.document_index,
            tokens[symbol.document_index][symbol.token_index].start_byte,
            symbol.key.clone(),
        )
    });
    pending.dedup_by(|left, right| {
        left.document_index == right.document_index && left.token_index == right.token_index
    });
    let key_counts = pending.iter().fold(HashMap::new(), |mut counts, symbol| {
        *counts.entry(symbol.key.clone()).or_insert(0usize) += 1;
        counts
    });
    for symbol in &mut pending {
        if key_counts.get(&symbol.key).copied().unwrap_or_default() > 1 {
            let token = &tokens[symbol.document_index][symbol.token_index];
            symbol.key = format!(
                "{}@{}:{}",
                symbol.key, sources[symbol.document_index].path, token.start_byte
            );
        }
    }

    let mut symbols = Vec::with_capacity(pending.len());
    let mut declaration_tokens = HashMap::<(usize, usize), SemanticSymbolId>::new();
    let mut spellings = HashMap::<String, Vec<SemanticSymbolId>>::new();
    for pending in pending {
        let id = SemanticSymbolId(symbols.len() as u32);
        let token = &tokens[pending.document_index][pending.token_index];
        let range = indexes[pending.document_index].byte_range(token.start_byte, token.end_byte);
        let spelling = identifier_text(&token.kind).expect("semantic definitions use identifiers");
        declaration_tokens.insert((pending.document_index, pending.token_index), id);
        spellings.entry(spelling.to_owned()).or_default().push(id);
        symbols.push(SemanticSymbol {
            id,
            key: pending.key,
            kind: pending.kind,
            document: sources[pending.document_index].path.to_owned(),
            range,
        });
    }

    let mut occurrences = symbols
        .iter()
        .map(|symbol| SemanticOccurrence {
            document: symbol.document.clone(),
            range: symbol.range,
            role: SemanticOccurrenceRole::Declaration,
            symbols: vec![symbol.id],
        })
        .collect::<Vec<_>>();
    for (document_index, document_tokens) in tokens.iter().enumerate() {
        let local_names = local_or_parameter_names(document_tokens);
        for (token_index, token) in document_tokens.iter().enumerate() {
            let Some(spelling) = identifier_text(&token.kind) else {
                continue;
            };
            if declaration_tokens.contains_key(&(document_index, token_index))
                || is_contextual_word(spelling)
                || looks_like_label_or_parameter(document_tokens, token_index)
            {
                continue;
            }
            let mut targets = if local_names.contains(spelling) {
                Vec::new()
            } else {
                spellings.get(spelling).cloned().unwrap_or_default()
            };
            targets.retain(|target| {
                let symbol = &symbols[target.0 as usize];
                !(symbol.kind == SemanticSymbolKind::Alias
                    && symbol.document == sources[document_index].path
                    && usize::try_from(symbol.range.start.line).ok()
                        == Some(token.line.saturating_sub(1))
                    && symbol.range.start.byte < token.start_byte)
            });
            targets.sort_unstable();
            targets.dedup();
            occurrences.push(SemanticOccurrence {
                document: sources[document_index].path.to_owned(),
                range: indexes[document_index].byte_range(token.start_byte, token.end_byte),
                role: SemanticOccurrenceRole::Reference,
                symbols: targets,
            });
        }
    }
    occurrences.sort_by_key(|occurrence| {
        let document = sources
            .iter()
            .position(|source| source.path == occurrence.document)
            .unwrap_or(usize::MAX);
        (
            document,
            occurrence.range.start.byte,
            occurrence.role == SemanticOccurrenceRole::Reference,
        )
    });
    SemanticIndex {
        symbols,
        occurrences,
    }
}

fn item_identity(item: &Item) -> Option<(&str, &str)> {
    let canonical = match item {
        Item::Function(definition) => &definition.name,
        Item::Global(definition) => &definition.name,
        Item::Struct(definition) => &definition.name,
        Item::Enum(definition) => &definition.name,
        Item::Effect(definition) => &definition.name,
        Item::Sort(definition) => &definition.name,
        Item::TypeForm(definition) => &definition.name,
        Item::TypeAlias(definition) => &definition.name,
        Item::Trait(definition) => &definition.name,
        Item::Extend(_) => return None,
    };
    Some((
        canonical,
        canonical.rsplit("::").next().unwrap_or(canonical),
    ))
}

fn collect_item_children(
    item: &Item,
    owner: &str,
    document_index: usize,
    start: usize,
    end: usize,
    tokens: &[Token],
    pending: &mut Vec<PendingSemanticSymbol>,
) {
    let body_start = match item {
        Item::Struct(_) => find_body_start(tokens, start, end, |kind| kind == &TokenKind::Struct),
        Item::Enum(_) => find_body_start(tokens, start, end, |kind| kind == &TokenKind::Enum),
        Item::Trait(_) => find_body_start(tokens, start, end, |kind| kind == &TokenKind::Trait),
        Item::Effect(_) => find_body_start(
            tokens,
            start,
            end,
            |kind| matches!(kind, TokenKind::Ident(word) if word == "effect"),
        ),
        _ => None,
    };
    let mut cursor = body_start.unwrap_or(start);
    let mut add = |name: &str, kind: SemanticSymbolKind, suffix: &str| {
        if let Some(token_index) = find_ident(tokens, cursor, end, name) {
            pending.push(PendingSemanticSymbol {
                key: format!("{owner}.{suffix}:{name}"),
                kind,
                document_index,
                token_index,
            });
            cursor = token_index + 1;
        }
    };
    match item {
        Item::Struct(definition) => {
            for field in &definition.fields {
                add(&field.name, SemanticSymbolKind::Field, "field");
            }
        }
        Item::Enum(definition) => collect_enum_children(definition, &mut add),
        Item::Trait(definition) => {
            for member in &definition.members {
                let name = match member {
                    TraitMember::Function(function) => &function.name,
                    TraitMember::AssociatedType { name, .. } => name,
                };
                add(name, SemanticSymbolKind::TraitMember, "member");
            }
        }
        Item::Effect(definition) => {
            for operation in &definition.operations {
                add(
                    &operation.name,
                    SemanticSymbolKind::TraitMember,
                    "operation",
                );
            }
        }
        _ => {}
    }
}

fn collect_enum_children(
    definition: &EnumDef,
    add: &mut impl FnMut(&str, SemanticSymbolKind, &str),
) {
    for variant in &definition.variants {
        add(&variant.name, SemanticSymbolKind::Variant, "variant");
        if let VariantFields::Named(fields) = &variant.fields {
            for field in fields {
                add(&field.name, SemanticSymbolKind::Field, "field");
            }
        }
    }
}

fn collect_extension_symbols(
    extension: &ExtendDef,
    document: &str,
    document_index: usize,
    start: usize,
    end: usize,
    tokens: &[Token],
    pending: &mut Vec<PendingSemanticSymbol>,
) {
    let key = format!("implementation@{document}:{}", tokens[start].start_byte);
    pending.push(PendingSemanticSymbol {
        key: key.clone(),
        kind: SemanticSymbolKind::Implementation,
        document_index,
        token_index: start,
    });
    let mut cursor = tokens[start..end]
        .iter()
        .position(|token| token.kind == TokenKind::LBrace)
        .map(|offset| start + offset + 1)
        .unwrap_or(start + 1);
    for member in &extension.members {
        let name = match member {
            ExtendMember::Function(function) => &function.name,
            ExtendMember::Const(binding) => &binding.name,
        };
        if let Some(token_index) = find_ident(tokens, cursor, end, name) {
            pending.push(PendingSemanticSymbol {
                key: format!("{key}.member:{name}"),
                kind: SemanticSymbolKind::ExtensionMember,
                document_index,
                token_index,
            });
            cursor = token_index + 1;
        }
    }
}

fn collect_import_aliases(
    sources: &[EditorSource<'_>],
    tokens: &[Vec<Token>],
    pending: &mut Vec<PendingSemanticSymbol>,
) {
    for (document_index, source) in sources.iter().enumerate() {
        let Ok(program) = parser::parse(source.source) else {
            continue;
        };
        for declaration in program.uses {
            let Some(span) = declaration.source else {
                continue;
            };
            let explicit_alias = declaration.alias.as_deref();
            let Some(name) = explicit_alias.or_else(|| declaration.path.last().map(String::as_str))
            else {
                continue;
            };
            let candidates = tokens[document_index]
                .iter()
                .enumerate()
                .filter(|(_, token)| {
                    token.line >= span.line
                        && token.end_line <= span.end_line
                        && identifier_text(&token.kind) == Some(name)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let token_index = explicit_alias
                .and_then(|_| {
                    candidates.iter().copied().find(|candidate| {
                        matches!(
                            tokens[document_index]
                                .get(candidate + 1)
                                .map(|token| &token.kind),
                            Some(TokenKind::Equal)
                        ) || candidate.checked_sub(1).is_some_and(|previous| {
                            matches!(
                                &tokens[document_index][previous].kind,
                                TokenKind::Ident(word) if word == "as"
                            )
                        })
                    })
                })
                .or_else(|| candidates.last().copied());
            let Some(token_index) = token_index else {
                continue;
            };
            let module = source.module_path.join("::");
            pending.push(PendingSemanticSymbol {
                key: format!("{module}::alias:{name}"),
                kind: SemanticSymbolKind::Alias,
                document_index,
                token_index,
            });
        }
    }
}

fn find_ident(tokens: &[Token], start: usize, end: usize, name: &str) -> Option<usize> {
    tokens[start..end]
        .iter()
        .position(|token| identifier_text(&token.kind) == Some(name))
        .map(|offset| start + offset)
}

fn find_body_start(
    tokens: &[Token],
    start: usize,
    end: usize,
    marker: impl Fn(&TokenKind) -> bool,
) -> Option<usize> {
    let marker = tokens[start..end]
        .iter()
        .position(|token| marker(&token.kind))?
        + start;
    tokens[marker + 1..end]
        .iter()
        .position(|token| token.kind == TokenKind::LBrace)
        .map(|offset| marker + 1 + offset + 1)
}

fn identifier_text(kind: &TokenKind) -> Option<&str> {
    match kind {
        TokenKind::Ident(name) | TokenKind::RegionName(name) => Some(name),
        TokenKind::Extend => Some("extend"),
        _ => None,
    }
}

fn looks_like_label_or_parameter(tokens: &[Token], index: usize) -> bool {
    matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Colon)
    ) && !matches!(
        index
            .checked_sub(1)
            .and_then(|index| tokens.get(index))
            .map(|token| &token.kind),
        Some(TokenKind::Dot)
    )
}

fn local_or_parameter_names(tokens: &[Token]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let mut brace_depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => brace_depth += 1,
            TokenKind::RBrace => brace_depth = brace_depth.saturating_sub(1),
            TokenKind::Let if brace_depth > 0 => {
                if let Some(Token {
                    kind: TokenKind::Ident(name),
                    ..
                }) = tokens.get(index + 1)
                {
                    names.insert(name.clone());
                }
            }
            TokenKind::Ident(ref name) if looks_like_label_or_parameter(tokens, index) => {
                names.insert(name.clone());
            }
            _ => {}
        }
    }
    names
}

fn is_contextual_word(word: &str) -> bool {
    matches!(
        word,
        "use"
            | "as"
            | "effect"
            | "test"
            | "requires"
            | "foreign"
            | "builtin"
            | "sort"
            | "self"
            | "shared"
            | "parameters"
            | "effects"
            | "access"
            | "c"
    )
}

fn resolver_diagnostic(
    index: &SourceIndex<'_>,
    diagnostic: ResolverDiagnostic,
    default_document: &str,
) -> EditorDiagnostic {
    let document = diagnostic
        .document
        .unwrap_or_else(|| default_document.to_owned());
    let range = diagnostic
        .location
        .as_ref()
        .map(|location| index.location_range(location));
    EditorDiagnostic {
        document,
        phase: DiagnosticPhase::Resolver,
        severity: DiagnosticSeverity::Error,
        code: diagnostic.code.to_owned(),
        message: diagnostic.message,
        range,
    }
}

fn workspace_resolver_diagnostic(
    sources: &[EditorSource<'_>],
    indexes: &[SourceIndex<'_>],
    diagnostic: ResolverDiagnostic,
) -> EditorDiagnostic {
    let document = diagnostic
        .document
        .clone()
        .unwrap_or_else(|| "<workspace>".to_owned());
    let range = diagnostic.location.as_ref().and_then(|location| {
        let source_index = sources.iter().position(|source| source.path == document)?;
        Some(indexes[source_index].location_range(location))
    });
    EditorDiagnostic {
        document,
        phase: DiagnosticPhase::Resolver,
        severity: DiagnosticSeverity::Error,
        code: diagnostic.code.to_owned(),
        message: diagnostic.message,
        range,
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
        assert_eq!(semantic.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(semantic.diagnostics[0].code, "salicin.semantic");
        assert_eq!(semantic.diagnostics[0].document, "<document>");
        let range = semantic.diagnostics[0]
            .range
            .expect("semantic diagnostic range");
        assert_eq!((range.start.line, range.start.utf16_character), (1, 2));
        assert_eq!((range.end.line, range.end.utf16_character), (1, 9));
    }

    #[test]
    fn resolver_diagnostics_use_structured_origins_without_fallback_ranges() {
        let analysis = analyze(
            "let value: i32 = 1\nlet value: i32 = 2\n",
            DocumentTarget::Library,
        );
        assert_eq!(analysis.diagnostics[0].phase, DiagnosticPhase::Resolver);
        assert_eq!(analysis.diagnostics[0].document, "<document>");
        assert_eq!(analysis.diagnostics[0].code, "salicin.resolve");
        let range = analysis.diagnostics[0]
            .range
            .expect("duplicate declaration origin");
        assert_eq!((range.start.line, range.start.utf16_character), (1, 0));
        assert_ne!(range.start.byte, 0);
    }

    #[test]
    fn named_document_import_diagnostics_preserve_identity_and_exact_span() {
        let analysis =
            analyze_document_at("src/main.sc", "use missing.item\n", DocumentTarget::Library);
        let [diagnostic] = analysis.diagnostics.as_slice() else {
            panic!("expected one import diagnostic");
        };
        assert_eq!(diagnostic.document, "src/main.sc");
        assert_eq!(diagnostic.phase, DiagnosticPhase::Resolver);
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
        assert_eq!(diagnostic.code, "salicin.resolve");
        let range = diagnostic.range.expect("import source range");
        assert_eq!((range.start.byte, range.end.byte), (0, 16));
    }

    #[test]
    fn document_wide_failures_do_not_invent_a_range() {
        let analysis = analyze_document_at(
            "src/lib.sc",
            "let answer: i32 = 42\n",
            DocumentTarget::Binary,
        );
        let diagnostic = analysis
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.message.contains("no `main`"))
            .expect("missing-main diagnostic");
        assert_eq!(diagnostic.document, "src/lib.sc");
        assert_eq!(diagnostic.code, "salicin.semantic");
        assert_eq!(diagnostic.range, None);
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
        assert_eq!(diagnostic.document, "part.sc");
        let range = diagnostic.range.expect("cross-file semantic range");
        assert_eq!((range.start.line, range.start.utf16_character), (1, 2));
        assert_eq!((range.end.line, range.end.utf16_character), (1, 9));
    }

    #[test]
    fn semantic_index_covers_source_identities_references_and_ambiguity() {
        let module = Vec::new();
        let source = "let option = core.option\nlet read = trait {\n  let read(self: borrow(self))(): i32\n}\n\nlet cell = struct { value: i32 }\nlet event = enum { value( value: i32 ), empty }\n\nlet choose(value: i32): i32 = { value }\nlet choose(other: u64): u64 = { other }\n\nextend(cell, read) {\n  let read(self: borrow(self))(): i32 = { self.value }\n}\n\nlet answer(value: cell): i32 = {\n  choose(value: value.read())\n}\n";
        let analysis = analyze_workspace(
            &[EditorSource {
                path: "src/lib.sc",
                module_path: &module,
                source,
                is_root: true,
            }],
            DocumentTarget::Library,
        );
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let index = &analysis.semantic_index;
        for kind in [
            SemanticSymbolKind::Declaration,
            SemanticSymbolKind::Alias,
            SemanticSymbolKind::Field,
            SemanticSymbolKind::Variant,
            SemanticSymbolKind::Overload,
            SemanticSymbolKind::TraitMember,
            SemanticSymbolKind::Implementation,
            SemanticSymbolKind::ExtensionMember,
        ] {
            assert!(
                index.symbols.iter().any(|symbol| symbol.kind == kind),
                "missing {kind:?}: {:?}",
                index.symbols
            );
        }
        let extension_member = index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticSymbolKind::ExtensionMember)
            .expect("extension member");
        assert!(
            extension_member.range.start.byte > source.find("extend(cell, read)").unwrap(),
            "implementation identity must point at the member declaration, not the trait reference"
        );
        let alias = index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticSymbolKind::Alias)
            .expect("source alias");
        assert_eq!(
            &source[alias.range.start.byte..alias.range.end.byte],
            "option"
        );
        assert_eq!(alias.range.start.byte, 4);
        assert_eq!(
            index
                .symbols
                .iter()
                .map(|symbol| symbol.id.0)
                .collect::<Vec<_>>(),
            (0..index.symbols.len() as u32).collect::<Vec<_>>()
        );
        let unique_keys = index
            .symbols
            .iter()
            .map(|symbol| symbol.key.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_keys.len(), index.symbols.len());
        let choose_references = index
            .occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.role == SemanticOccurrenceRole::Reference
                    && &source[occurrence.range.start.byte..occurrence.range.end.byte] == "choose"
            })
            .collect::<Vec<_>>();
        assert_eq!(choose_references.len(), 1);
        assert_eq!(
            choose_references[0].symbols.len(),
            2,
            "overload ambiguity must remain explicit"
        );

        let repeated = analyze_workspace(
            &[EditorSource {
                path: "src/lib.sc",
                module_path: &module,
                source,
                is_root: true,
            }],
            DocumentTarget::Library,
        );
        assert_eq!(index, &repeated.semantic_index);
    }

    #[test]
    fn semantic_index_routes_cross_module_references_and_rejects_partial_facts() {
        let root_module = Vec::new();
        let part_module = ["part".to_owned()];
        let root = "let main(): i32 = { part.answer() }\n";
        let part = "pub(package) let answer(): i32 = { 42 }\n";
        let sources = [
            EditorSource {
                path: "src/main.sc",
                module_path: &root_module,
                source: root,
                is_root: true,
            },
            EditorSource {
                path: "src/part.sc",
                module_path: &part_module,
                source: part,
                is_root: false,
            },
        ];
        let analysis = analyze_workspace(&sources, DocumentTarget::Binary);
        assert!(analysis.diagnostics.is_empty());
        let answer = analysis
            .semantic_index
            .symbols
            .iter()
            .find(|symbol| symbol.key == "part::answer")
            .expect("cross-module declaration");
        let reference = analysis
            .semantic_index
            .occurrences
            .iter()
            .find(|occurrence| {
                occurrence.role == SemanticOccurrenceRole::Reference
                    && occurrence.document == "src/main.sc"
                    && &root[occurrence.range.start.byte..occurrence.range.end.byte] == "answer"
            })
            .expect("cross-module reference");
        assert_eq!(reference.symbols, vec![answer.id]);

        let invalid = analyze_workspace(
            &[EditorSource {
                path: "src/main.sc",
                module_path: &root_module,
                source: "let main(): i32 = { missing }\n",
                is_root: true,
            }],
            DocumentTarget::Binary,
        );
        assert!(!invalid.diagnostics.is_empty());
        assert_eq!(invalid.semantic_index, SemanticIndex::default());
    }

    #[test]
    fn semantic_index_preserves_unicode_and_never_misbinds_shadowed_names() {
        let module = Vec::new();
        let source = "let 值(): i32 = { 40 }\nlet shadowed(): i32 = { 1 }\nlet use_value(): i32 = { 值() + 2 }\nlet use_shadow(shadowed: i32): i32 = { shadowed }\n";
        let analysis = analyze_workspace(
            &[EditorSource {
                path: "src/unicode.sc",
                module_path: &module,
                source,
                is_root: true,
            }],
            DocumentTarget::Library,
        );
        assert!(analysis.diagnostics.is_empty());
        let unicode = analysis
            .semantic_index
            .symbols
            .iter()
            .find(|symbol| symbol.key == "值")
            .expect("Unicode declaration");
        let unicode_reference = analysis
            .semantic_index
            .occurrences
            .iter()
            .find(|occurrence| {
                occurrence.role == SemanticOccurrenceRole::Reference
                    && occurrence.range.start.byte > unicode.range.start.byte
                    && &source[occurrence.range.start.byte..occurrence.range.end.byte] == "值"
            })
            .expect("Unicode reference");
        assert_eq!(unicode_reference.symbols, vec![unicode.id]);

        let shadowed_references = analysis
            .semantic_index
            .occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.role == SemanticOccurrenceRole::Reference
                    && &source[occurrence.range.start.byte..occurrence.range.end.byte] == "shadowed"
            })
            .collect::<Vec<_>>();
        assert!(!shadowed_references.is_empty());
        assert!(
            shadowed_references
                .iter()
                .all(|occurrence| occurrence.symbols.is_empty()),
            "a local parameter must never bind to the same-spelled top-level declaration"
        );
    }

    #[test]
    fn workspace_session_overlays_versions_and_discards_superseded_results() {
        let root_module = Vec::new();
        let part_module = ["part".to_owned()];
        let root = "let main(): i32 = { part.answer() }\n";
        let part = "pub(package) let answer(): i32 = { 42 }\n";
        let mut session = WorkspaceSession::new(
            &[
                EditorSource {
                    path: "main.sc",
                    module_path: &root_module,
                    source: root,
                    is_root: true,
                },
                EditorSource {
                    path: "part.sc",
                    module_path: &part_module,
                    source: part,
                    is_root: false,
                },
            ],
            DocumentTarget::Binary,
        )
        .expect("workspace session");

        let initial = session.snapshot();
        assert_eq!(initial.id.revision, 0);
        assert!(initial.analyze().analysis.diagnostics.is_empty());

        session
            .open_document(
                "part.sc",
                1,
                "pub(package) let answer(): i32 = { missing }\n",
            )
            .expect("open overlay");
        let stale_snapshot = session.snapshot();
        let stale_worker = std::thread::spawn(move || stale_snapshot.analyze());

        session
            .change_document("part.sc", 2, "pub(package) let answer(): i32 = { 42 }\n")
            .expect("newer overlay");
        let stale_result = stale_worker.join().expect("stale analysis completes");
        assert!(stale_result
            .analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.document == "part.sc"));
        assert!(
            session.accept_analysis(stale_result).is_none(),
            "superseded analysis must be consumed and dropped"
        );

        let current_snapshot = session.snapshot();
        assert_eq!(
            current_snapshot
                .documents
                .iter()
                .find(|document| document.path == "part.sc")
                .and_then(|document| document.version),
            Some(2)
        );
        let current_result = current_snapshot.analyze();
        assert_eq!(
            current_result.document_versions,
            vec![
                ("main.sc".to_owned(), None),
                ("part.sc".to_owned(), Some(2))
            ]
        );
        let accepted = session
            .accept_analysis(current_result)
            .expect("current result");
        assert!(accepted.diagnostics.is_empty());
    }

    #[test]
    fn workspace_session_rejects_stale_versions_and_close_restores_baseline() {
        let root_module = Vec::new();
        let mut session = WorkspaceSession::new(
            &[EditorSource {
                path: "main.sc",
                module_path: &root_module,
                source: "let main(): i32 = { 42 }\n",
                is_root: true,
            }],
            DocumentTarget::Binary,
        )
        .expect("workspace session");
        session
            .open_document("main.sc", 7, "let main(): i32 = { missing }\n")
            .expect("open document");
        let before_rejection = session.snapshot_id();
        assert!(matches!(
            session.change_document("main.sc", 7, "let main(): i32 = { 0 }\n"),
            Err(WorkspaceSessionError::StaleDocumentVersion {
                current: 7,
                received: 7,
                ..
            })
        ));
        assert_eq!(session.snapshot_id(), before_rejection);

        session
            .update_baseline("main.sc", "let main(): i32 = { 41 + 1 }\n")
            .expect("replace baseline");
        assert!(
            !session.snapshot().analyze().analysis.diagnostics.is_empty(),
            "open text must continue to override a newer baseline"
        );
        session.close_document("main.sc").expect("close document");
        let closed = session.snapshot();
        assert_eq!(closed.documents[0].version, None);
        assert_eq!(closed.documents[0].source, "let main(): i32 = { 41 + 1 }\n");
        assert!(closed.analyze().analysis.diagnostics.is_empty());
    }

    #[test]
    fn workspace_sessions_validate_identity_without_writing_source_files() {
        let unique = format!(
            "salicin-editor-session-{}-{}",
            std::process::id(),
            NEXT_WORKSPACE_SESSION_ID.load(Ordering::Relaxed)
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir(&directory).expect("create editor session fixture");
        let path = directory.join("main.sc");
        let baseline = "let main(): i32 = { 42 }\n";
        std::fs::write(&path, baseline).expect("write editor session fixture");
        let path_string = path.to_string_lossy().into_owned();
        let root_module = Vec::new();
        let sources = [EditorSource {
            path: &path_string,
            module_path: &root_module,
            source: baseline,
            is_root: true,
        }];
        let mut session =
            WorkspaceSession::new(&sources, DocumentTarget::Binary).expect("workspace session");
        let duplicate_sources = [
            EditorSource {
                path: &path_string,
                module_path: &root_module,
                source: baseline,
                is_root: true,
            },
            EditorSource {
                path: &path_string,
                module_path: &root_module,
                source: baseline,
                is_root: false,
            },
        ];
        assert!(matches!(
            WorkspaceSession::new(&duplicate_sources, DocumentTarget::Binary),
            Err(WorkspaceSessionError::DuplicateDocument(_))
        ));
        let other =
            WorkspaceSession::new(&sources, DocumentTarget::Binary).expect("second session");
        let foreign_result = other.snapshot().analyze();
        assert!(session.accept_analysis(foreign_result).is_none());
        assert!(matches!(
            session.open_document("unknown.sc", 1, ""),
            Err(WorkspaceSessionError::UnknownDocument(_))
        ));
        assert!(matches!(
            session.close_document(&path_string),
            Err(WorkspaceSessionError::DocumentNotOpen(_))
        ));
        session
            .open_document(&path_string, 1, "let main(): i32 = { 0 }\n")
            .expect("open memory overlay");
        assert!(matches!(
            session.open_document(&path_string, 2, ""),
            Err(WorkspaceSessionError::DocumentAlreadyOpen(_))
        ));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read unchanged source"),
            baseline
        );
        std::fs::remove_file(&path).expect("remove editor session fixture");
        std::fs::remove_dir(&directory).expect("remove editor session directory");
    }

    #[test]
    fn every_failure_fixture_produces_structured_editor_diagnostics() {
        use std::sync::{Arc, Mutex};

        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fail");
        let mut paths = std::fs::read_dir(directory)
            .expect("read failure fixtures")
            .map(|entry| entry.expect("read failure fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "sc"))
            .collect::<Vec<_>>();
        paths.sort();
        let worker_count = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(2)
            .clamp(1, 8)
            .min(paths.len());
        let jobs = Arc::new(Mutex::new(paths.into_iter()));

        std::thread::scope(|scope| {
            for _ in 0..worker_count {
                let jobs = Arc::clone(&jobs);
                scope.spawn(move || loop {
                    let Some(path) = jobs.lock().expect("lock failure fixtures").next() else {
                        break;
                    };
                    let source = std::fs::read_to_string(&path)
                        .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
                    let analysis = analyze(&source, DocumentTarget::Binary);
                    assert!(
                        !analysis.diagnostics.is_empty(),
                        "{} unexpectedly had no diagnostics",
                        path.display()
                    );
                    assert!(
                        analysis.diagnostics.iter().all(|diagnostic| {
                            !diagnostic.document.is_empty()
                                && diagnostic.severity == DiagnosticSeverity::Error
                                && diagnostic.code.starts_with("salicin.")
                        }),
                        "{} had an unstructured diagnostic: {:?}",
                        path.display(),
                        analysis.diagnostics
                    );
                });
            }
        });
    }
}
