use std::fmt;
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::editor::{
    DiagnosticPhase, EditorDiagnostic, EditorRange, EditorToken, WorkspaceAnalysis,
    WorkspaceSession, WorkspaceSessionError, WorkspaceSnapshotId,
};
use crate::lexer::TokenKind;

const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    MissingContentLength,
    DuplicateContentLength,
    InvalidContentLength,
    MessageTooLarge(usize),
    UnexpectedEof,
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::MissingContentLength => formatter.write_str("missing Content-Length header"),
            Self::DuplicateContentLength => formatter.write_str("duplicate Content-Length header"),
            Self::InvalidContentLength => formatter.write_str("invalid Content-Length header"),
            Self::MessageTooLarge(length) => {
                write!(
                    formatter,
                    "JSON-RPC message length {length} exceeds the limit"
                )
            }
            Self::UnexpectedEof => formatter.write_str("unexpected end of JSON-RPC message"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    WaitingForInitialize,
    Running,
    Shutdown,
}

/// A transport-only LSP server. It synchronizes full document text into a
/// caller-created workspace session and intentionally publishes no language
/// features until the diagnostics milestone.
pub struct Server {
    session: WorkspaceSession,
    lifecycle: Lifecycle,
    latest_analysis: Option<(WorkspaceSnapshotId, WorkspaceAnalysis)>,
}

impl Server {
    pub fn new(session: WorkspaceSession) -> Self {
        Self {
            session,
            lifecycle: Lifecycle::WaitingForInitialize,
            latest_analysis: None,
        }
    }

    pub fn session(&self) -> &WorkspaceSession {
        &self.session
    }

    pub fn run<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<i32, TransportError> {
        while let Some(bytes) = read_message(reader)? {
            let value = match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) => value,
                Err(_) => {
                    write_message(writer, &json_rpc_error(Value::Null, -32700, "Parse error"))?;
                    continue;
                }
            };
            if let Some(exit_code) = self.handle_message(value, writer)? {
                return Ok(exit_code);
            }
        }
        Ok(1)
    }

    fn handle_message<W: Write>(
        &mut self,
        message: Value,
        writer: &mut W,
    ) -> Result<Option<i32>, TransportError> {
        let Some(object) = message.as_object() else {
            write_message(
                writer,
                &json_rpc_error(Value::Null, -32600, "Invalid Request"),
            )?;
            return Ok(None);
        };
        let id = object.get("id").cloned();
        let method = object.get("method").and_then(Value::as_str);
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
            write_message(
                writer,
                &json_rpc_error(id.unwrap_or(Value::Null), -32600, "Invalid Request"),
            )?;
            return Ok(None);
        }
        let method = method.expect("checked method");
        let params = object.get("params").unwrap_or(&Value::Null);

        if method == "exit" && id.is_none() {
            return Ok(Some(if self.lifecycle == Lifecycle::Shutdown {
                0
            } else {
                1
            }));
        }

        let Some(id) = id else {
            self.handle_notification(method, params, writer)?;
            return Ok(None);
        };

        match method {
            "initialize" if self.lifecycle == Lifecycle::WaitingForInitialize => {
                self.lifecycle = Lifecycle::Running;
                write_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "capabilities": {
                                "positionEncoding": "utf-16",
                                "textDocumentSync": {
                                    "openClose": true,
                                    "change": 1,
                                    "save": { "includeText": true }
                                },
                                "semanticTokensProvider": {
                                    "legend": {
                                        "tokenTypes": [
                                            "keyword",
                                            "variable",
                                            "typeParameter",
                                            "string",
                                            "number",
                                            "operator"
                                        ],
                                        "tokenModifiers": []
                                    },
                                    "full": true
                                }
                            },
                            "serverInfo": {
                                "name": "salic",
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }
                    }),
                )?;
            }
            "initialize" => {
                write_message(
                    writer,
                    &json_rpc_error(id, -32600, "initialize may be requested only once"),
                )?;
            }
            "shutdown" if self.lifecycle == Lifecycle::Running => {
                self.lifecycle = Lifecycle::Shutdown;
                write_message(writer, &json!({"jsonrpc": "2.0", "id": id, "result": null}))?;
            }
            "shutdown" => {
                write_message(writer, &json_rpc_error(id, -32600, "server is not running"))?;
            }
            "textDocument/semanticTokens/full" if self.lifecycle == Lifecycle::Running => {
                if self
                    .latest_analysis
                    .as_ref()
                    .is_none_or(|(snapshot, _)| *snapshot != self.session.snapshot_id())
                {
                    self.analyze_and_publish(writer)?;
                }
                match self.semantic_tokens(params) {
                    Ok(result) => write_message(
                        writer,
                        &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    )?,
                    Err(message) => write_message(writer, &json_rpc_error(id, -32602, &message))?,
                }
            }
            _ if self.lifecycle != Lifecycle::Running => {
                write_message(
                    writer,
                    &json_rpc_error(id, -32002, "server is not initialized"),
                )?;
            }
            _ => {
                write_message(writer, &json_rpc_error(id, -32601, "Method not found"))?;
            }
        }
        Ok(None)
    }

    fn handle_notification<W: Write>(
        &mut self,
        method: &str,
        params: &Value,
        writer: &mut W,
    ) -> Result<(), TransportError> {
        if self.lifecycle != Lifecycle::Running {
            return Ok(());
        }
        let result = match method {
            "initialized" => {
                self.analyze_and_publish(writer)?;
                return Ok(());
            }
            "$/cancelRequest" => return Ok(()),
            "textDocument/didOpen" => self.did_open(params),
            "textDocument/didChange" => self.did_change(params),
            "textDocument/didSave" => self.did_save(params),
            "textDocument/didClose" => self.did_close(params),
            _ => return Ok(()),
        };
        match result {
            Ok(()) => self.analyze_and_publish(writer)?,
            Err(message) => {
                write_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "window/logMessage",
                        "params": { "type": 1, "message": message }
                    }),
                )?;
            }
        }
        Ok(())
    }

    fn analyze_and_publish<W: Write>(&mut self, writer: &mut W) -> Result<(), TransportError> {
        let completed = self.session.snapshot().analyze();
        let snapshot = completed.id;
        let versions = completed.document_versions.clone();
        let Some(analysis) = self.session.accept_analysis(completed) else {
            return Ok(());
        };

        for document in &analysis.documents {
            let diagnostics = analysis
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.document == document.path)
                .filter_map(lsp_diagnostic)
                .collect::<Vec<_>>();
            let version = versions
                .iter()
                .find(|(path, _)| path == &document.path)
                .and_then(|(_, version)| *version);
            let mut params = serde_json::Map::new();
            params.insert(
                "uri".to_owned(),
                Value::String(path_to_file_uri(&document.path)),
            );
            params.insert("diagnostics".to_owned(), Value::Array(diagnostics));
            if let Some(version) = version {
                params.insert("version".to_owned(), Value::Number(version.into()));
            }
            write_message(
                writer,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": params
                }),
            )?;
        }

        for diagnostic in analysis.diagnostics.iter().filter(|diagnostic| {
            diagnostic.range.is_none()
                || !analysis
                    .documents
                    .iter()
                    .any(|document| document.path == diagnostic.document)
        }) {
            write_message(
                writer,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "window/logMessage",
                    "params": {
                        "type": 1,
                        "message": format!(
                            "{} diagnostic for {} has no exact source range: {}",
                            phase_name(diagnostic.phase),
                            diagnostic.document,
                            diagnostic.message
                        )
                    }
                }),
            )?;
        }
        self.latest_analysis = Some((snapshot, analysis));
        Ok(())
    }

    fn semantic_tokens(&self, params: &Value) -> Result<Value, String> {
        let document = required_object(params, "textDocument")?;
        let uri = required_string(document, "uri")?;
        let path = file_uri_to_path(uri)?;
        let Some((snapshot, analysis)) = &self.latest_analysis else {
            return Err("workspace analysis is not available".to_owned());
        };
        let document = analysis
            .documents
            .iter()
            .find(|document| document.path == path)
            .ok_or_else(|| format!("unknown workspace document `{path}`"))?;
        Ok(json!({
            "resultId": format!("{}:{}", snapshot.session, snapshot.revision),
            "data": encode_semantic_tokens(&document.tokens)
        }))
    }

    fn did_open(&mut self, params: &Value) -> Result<(), String> {
        let document = required_object(params, "textDocument")?;
        let uri = required_string(document, "uri")?;
        let path = file_uri_to_path(uri)?;
        let version = required_i64(document, "version")?;
        let text = required_string(document, "text")?;
        self.session
            .open_document(&path, version, text)
            .map(|_| ())
            .map_err(workspace_error)
    }

    fn did_change(&mut self, params: &Value) -> Result<(), String> {
        let document = required_object(params, "textDocument")?;
        let uri = required_string(document, "uri")?;
        let path = file_uri_to_path(uri)?;
        let version = required_i64(document, "version")?;
        let changes = params
            .get("contentChanges")
            .and_then(Value::as_array)
            .ok_or_else(|| "didChange requires contentChanges".to_owned())?;
        if changes.len() != 1 || changes[0].get("range").is_some() {
            return Err("Salicin accepts exactly one full-text didChange update".to_owned());
        }
        let change = changes[0]
            .as_object()
            .ok_or_else(|| "didChange content must be an object".to_owned())?;
        let text = required_string(change, "text")?;
        self.session
            .change_document(&path, version, text)
            .map(|_| ())
            .map_err(workspace_error)
    }

    fn did_save(&mut self, params: &Value) -> Result<(), String> {
        let document = required_object(params, "textDocument")?;
        let uri = required_string(document, "uri")?;
        let path = file_uri_to_path(uri)?;
        let text = params.get("text").and_then(Value::as_str);
        self.session
            .save_document(&path, text)
            .map(|_| ())
            .map_err(workspace_error)
    }

    fn did_close(&mut self, params: &Value) -> Result<(), String> {
        let document = required_object(params, "textDocument")?;
        let uri = required_string(document, "uri")?;
        let path = file_uri_to_path(uri)?;
        self.session
            .close_document(&path)
            .map(|_| ())
            .map_err(workspace_error)
    }
}

fn lsp_diagnostic(diagnostic: &EditorDiagnostic) -> Option<Value> {
    let range = diagnostic.range?;
    Some(json!({
        "range": lsp_range(range),
        "severity": 1,
        "code": diagnostic.code,
        "source": "salicin",
        "message": diagnostic.message,
        "data": { "phase": phase_name(diagnostic.phase) }
    }))
}

fn lsp_range(range: EditorRange) -> Value {
    json!({
        "start": {
            "line": range.start.line,
            "character": range.start.utf16_character
        },
        "end": {
            "line": range.end.line,
            "character": range.end.utf16_character
        }
    })
}

fn phase_name(phase: DiagnosticPhase) -> &'static str {
    match phase {
        DiagnosticPhase::Lexer => "lexer",
        DiagnosticPhase::Parser => "parser",
        DiagnosticPhase::Resolver => "resolver",
        DiagnosticPhase::Semantic => "semantic",
    }
}

fn encode_semantic_tokens(tokens: &[EditorToken]) -> Vec<u32> {
    let mut encoded = Vec::new();
    let mut previous_line = 0;
    let mut previous_start = 0;
    let mut first = true;
    for token in tokens {
        let Some(token_type) = semantic_token_type(&token.kind) else {
            continue;
        };
        if token.range.start.line != token.range.end.line {
            continue;
        }
        let length = token
            .range
            .end
            .utf16_character
            .saturating_sub(token.range.start.utf16_character);
        if length == 0 {
            continue;
        }
        let delta_line = if first {
            token.range.start.line
        } else {
            token.range.start.line.saturating_sub(previous_line)
        };
        let delta_start = if first || delta_line != 0 {
            token.range.start.utf16_character
        } else {
            token
                .range
                .start
                .utf16_character
                .saturating_sub(previous_start)
        };
        encoded.extend([delta_line, delta_start, length, token_type, 0]);
        previous_line = token.range.start.line;
        previous_start = token.range.start.utf16_character;
        first = false;
    }
    encoded
}

fn semantic_token_type(kind: &TokenKind) -> Option<u32> {
    match kind {
        TokenKind::Let
        | TokenKind::Pub
        | TokenKind::Package
        | TokenKind::Root
        | TokenKind::Super
        | TokenKind::Mut
        | TokenKind::Copy
        | TokenKind::Move
        | TokenKind::Comptime
        | TokenKind::Borrow
        | TokenKind::Type
        | TokenKind::Region
        | TokenKind::Unsafe
        | TokenKind::Do
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::Return
        | TokenKind::Throw
        | TokenKind::While
        | TokenKind::For
        | TokenKind::In
        | TokenKind::Loop
        | TokenKind::Break
        | TokenKind::Continue
        | TokenKind::Extend
        | TokenKind::Struct
        | TokenKind::Enum
        | TokenKind::Trait
        | TokenKind::Where
        | TokenKind::Match
        | TokenKind::Try
        | TokenKind::True
        | TokenKind::False => Some(0),
        TokenKind::Ident(_) => Some(1),
        TokenKind::RegionName(_) => Some(2),
        TokenKind::String(_) => Some(3),
        TokenKind::Integer(_) => Some(4),
        TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::Equal
        | TokenKind::EqualEqual
        | TokenKind::Bang
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
        | TokenKind::QuestionDot
        | TokenKind::QuestionQuestion => Some(5),
        TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::Colon
        | TokenKind::Dot
        | TokenKind::Ellipsis
        | TokenKind::Comma
        | TokenKind::Semicolon
        | TokenKind::Newline
        | TokenKind::Eof => None,
    }
}

fn workspace_error(error: WorkspaceSessionError) -> String {
    format!("document synchronization rejected: {error}")
}

fn required_object<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    value
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing object field `{field}`"))
}

fn required_string<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field `{field}`"))
}

fn required_i64(value: &serde_json::Map<String, Value>, field: &str) -> Result<i64, String> {
    let value = value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("missing integer field `{field}`"))?;
    i32::try_from(value)
        .map(i64::from)
        .map_err(|_| format!("integer field `{field}` is outside the LSP integer range"))
}

fn json_rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>, TransportError> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return if saw_header {
                Err(TransportError::UnexpectedEof)
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        if line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.trim_end_matches(['\r', '\n']).split_once(':') else {
            return Err(TransportError::InvalidContentLength);
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            if content_length.is_some() {
                return Err(TransportError::DuplicateContentLength);
            }
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| TransportError::InvalidContentLength)?,
            );
        }
    }
    let length = content_length.ok_or(TransportError::MissingContentLength)?;
    if length > MAX_MESSAGE_BYTES {
        return Err(TransportError::MessageTooLarge(length));
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => TransportError::UnexpectedEof,
            _ => TransportError::Io(error),
        })?;
    Ok(Some(body))
}

pub fn write_message<W: Write>(writer: &mut W, value: &Value) -> Result<(), TransportError> {
    let body = serde_json::to_vec(value).expect("JSON-RPC values are serializable");
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub fn path_to_file_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(char::from(*byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

pub fn file_uri_to_path(uri: &str) -> Result<String, String> {
    let encoded = uri
        .strip_prefix("file://")
        .ok_or_else(|| "Salicin supports only file document URIs".to_owned())?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        let byte = encoded.as_bytes()[index];
        if byte == b'%' {
            let digits = encoded
                .get(index + 1..index + 3)
                .ok_or_else(|| "invalid percent escape in file URI".to_owned())?;
            bytes.push(
                u8::from_str_radix(digits, 16)
                    .map_err(|_| "invalid percent escape in file URI".to_owned())?,
            );
            index += 3;
        } else {
            bytes.push(byte);
            index += 1;
        }
    }
    String::from_utf8(bytes).map_err(|_| "file URI path is not UTF-8".to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::editor::{DocumentTarget, EditorSource};

    fn session(path: &str) -> WorkspaceSession {
        WorkspaceSession::new(
            &[EditorSource {
                path,
                module_path: &[],
                source: "let main(): i32 = { 0 }\n",
                is_root: true,
            }],
            DocumentTarget::Binary,
        )
        .unwrap()
    }

    fn framed(value: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(value).unwrap();
        format!("Content-Length: {}\r\n\r\n", body.len())
            .into_bytes()
            .into_iter()
            .chain(body)
            .collect()
    }

    fn messages(output: Vec<u8>) -> Vec<Value> {
        let mut reader = Cursor::new(output);
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            messages.push(serde_json::from_slice(&message).unwrap());
        }
        messages
    }

    #[test]
    fn lifecycle_advertises_full_sync_and_exits_cleanly() {
        let mut input = framed(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        input.extend(framed(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "shutdown"
        })));
        input.extend(framed(&json!({"jsonrpc": "2.0", "method": "exit"})));
        let mut output = Vec::new();
        let code = Server::new(session("/tmp/main.sc"))
            .run(&mut Cursor::new(input), &mut output)
            .unwrap();
        assert_eq!(code, 0);
        let mut reader = Cursor::new(output);
        let initialize: Value =
            serde_json::from_slice(&read_message(&mut reader).unwrap().unwrap()).unwrap();
        assert_eq!(
            initialize["result"]["capabilities"]["textDocumentSync"]["change"],
            1
        );
        let shutdown: Value =
            serde_json::from_slice(&read_message(&mut reader).unwrap().unwrap()).unwrap();
        assert!(shutdown["result"].is_null());
        assert!(read_message(&mut reader).unwrap().is_none());

        let mut shutdown_without_exit = framed(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        shutdown_without_exit.extend(framed(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "shutdown"
        })));
        assert_eq!(
            Server::new(session("/tmp/main.sc"))
                .run(&mut Cursor::new(shutdown_without_exit), &mut Vec::new())
                .unwrap(),
            1
        );
    }

    #[test]
    fn full_text_sync_rejects_stale_versions_without_stopping_server() {
        let path = "/tmp/盐 main.sc";
        let uri = path_to_file_uri(path);
        let mut input = framed(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        for message in [
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri,"languageId":"salicin","version":2,"text":"let main(): i32 = { 1 }\n"}
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":uri,"version":2},
                "contentChanges":[{"text":"let main(): i32 = { 2 }\n"}]
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":uri,"version":3},
                "contentChanges":[{"text":"let main(): i32 = { 3 }\n"}]
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
                "textDocument":{"uri":uri},"text":"let main(): i32 = { 3 }\n"
            }}),
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":uri}
            }}),
            json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ] {
            input.extend(framed(&message));
        }
        let mut output = Vec::new();
        let mut server = Server::new(session(path));
        let code = server.run(&mut Cursor::new(input), &mut output).unwrap();
        assert_eq!(code, 0);
        let snapshot = server.session().snapshot();
        assert_eq!(snapshot.documents[0].source, "let main(): i32 = { 3 }\n");
        assert_eq!(snapshot.documents[0].version, None);
        let messages = messages(output);
        let log = messages
            .iter()
            .find(|message| message["method"] == "window/logMessage")
            .expect("stale change log");
        assert_eq!(log["method"], "window/logMessage");
        assert!(log["params"]["message"]
            .as_str()
            .unwrap()
            .contains("stale version 2"));
    }

    #[test]
    fn multi_file_diagnostics_and_unicode_semantic_tokens_use_exact_document_versions() {
        let root_path = "/tmp/main.sc";
        let module_path = "/tmp/helper.sc";
        let helper_module = vec!["helper".to_owned()];
        let sources = [
            EditorSource {
                path: root_path,
                module_path: &[],
                source: "pub let root_value(): i32 = { helper.value() }\n",
                is_root: true,
            },
            EditorSource {
                path: module_path,
                module_path: &helper_module,
                source: "pub let value(): i32 = { 42 }\n",
                is_root: false,
            },
        ];
        let workspace = WorkspaceSession::new(&sources, DocumentTarget::Library).unwrap();
        let uri = path_to_file_uri(module_path);
        let mut input = framed(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        for message in [
            json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{
                    "uri":uri,
                    "languageId":"salicin",
                    "version":7,
                    "text":"pub let 盐(: i32 = { \"😀\" }\n"
                }
            }}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full",
                "params":{"textDocument":{"uri":uri}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"textDocument/semanticTokens/full",
                "params":{"textDocument":{"uri":"file:///tmp/unknown.sc"}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ] {
            input.extend(framed(&message));
        }
        let mut output = Vec::new();
        let code = Server::new(workspace)
            .run(&mut Cursor::new(input), &mut output)
            .unwrap();
        assert_eq!(code, 0);
        let messages = messages(output);

        let published = messages
            .iter()
            .rfind(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message["params"]["uri"] == uri
                    && message["params"]["version"] == 7
            })
            .expect("versioned module diagnostics");
        let diagnostics = published["params"]["diagnostics"].as_array().unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["data"]["phase"], "parser");
        assert_eq!(diagnostics[0]["source"], "salicin");
        assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
        assert!(
            diagnostics[0]["range"]["start"]["character"]
                .as_u64()
                .unwrap()
                >= 8
        );

        let semantic = messages
            .iter()
            .find(|message| message["id"] == 2)
            .expect("semantic token response");
        let data = semantic["result"]["data"].as_array().unwrap();
        assert!(!data.is_empty());
        assert_eq!(data.len() % 5, 0);
        assert!(semantic["result"]["resultId"]
            .as_str()
            .unwrap()
            .ends_with(":1"));
        let data = data
            .iter()
            .map(|value| value.as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        let mut line = 0;
        let mut start = 0;
        let mut decoded = Vec::new();
        for token in data.chunks_exact(5) {
            line += token[0];
            start = if token[0] == 0 {
                start + token[1]
            } else {
                token[1]
            };
            decoded.push((line, start, token[2], token[3]));
        }
        assert!(decoded.contains(&(0, 8, 1, 1)), "{decoded:?}");
        assert!(decoded
            .iter()
            .any(|(_, _, length, token_type)| { *length == 4 && *token_type == 3 }));
        let unknown = messages
            .iter()
            .find(|message| message["id"] == 4)
            .expect("unknown semantic token response");
        assert_eq!(unknown["error"]["code"], -32602);
        assert!(messages.iter().any(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == path_to_file_uri(root_path)
                && message["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
        }));
    }

    #[test]
    fn diagnostic_serialization_preserves_every_compiler_phase() {
        let range = EditorRange {
            start: crate::editor::EditorPosition {
                byte: 1,
                line: 2,
                utf16_character: 3,
            },
            end: crate::editor::EditorPosition {
                byte: 5,
                line: 2,
                utf16_character: 6,
            },
        };
        for (phase, name) in [
            (DiagnosticPhase::Lexer, "lexer"),
            (DiagnosticPhase::Parser, "parser"),
            (DiagnosticPhase::Resolver, "resolver"),
            (DiagnosticPhase::Semantic, "semantic"),
        ] {
            let diagnostic = EditorDiagnostic {
                document: "/tmp/main.sc".to_owned(),
                phase,
                severity: crate::editor::DiagnosticSeverity::Error,
                code: format!("salicin.{name}"),
                message: format!("{name} failed"),
                range: Some(range),
            };
            let serialized = lsp_diagnostic(&diagnostic).unwrap();
            assert_eq!(serialized["data"]["phase"], name);
            assert_eq!(serialized["range"]["start"]["line"], 2);
            assert_eq!(serialized["range"]["start"]["character"], 3);
            assert_eq!(serialized["range"]["end"]["character"], 6);
        }
    }

    #[test]
    fn framing_rejects_missing_duplicate_and_oversized_lengths() {
        assert!(matches!(
            read_message(&mut Cursor::new(b"X: 1\r\n\r\n")),
            Err(TransportError::MissingContentLength)
        ));
        assert!(matches!(
            read_message(&mut Cursor::new(
                b"Content-Length: 0\r\nContent-Length: 0\r\n\r\n"
            )),
            Err(TransportError::DuplicateContentLength)
        ));
        let oversized = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert!(matches!(
            read_message(&mut Cursor::new(oversized)),
            Err(TransportError::MessageTooLarge(_))
        ));
    }

    #[test]
    fn file_uris_round_trip_utf8_and_reserved_bytes() {
        let path = "/tmp/盐 main#1.sc";
        assert_eq!(file_uri_to_path(&path_to_file_uri(path)).unwrap(), path);
    }
}
