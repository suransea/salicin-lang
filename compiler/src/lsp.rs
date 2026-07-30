use std::fmt;
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::editor::{WorkspaceSession, WorkspaceSessionError};

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
}

impl Server {
    pub fn new(session: WorkspaceSession) -> Self {
        Self {
            session,
            lifecycle: Lifecycle::WaitingForInitialize,
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
            "initialized" | "$/cancelRequest" => return Ok(()),
            "textDocument/didOpen" => self.did_open(params),
            "textDocument/didChange" => self.did_change(params),
            "textDocument/didSave" => self.did_save(params),
            "textDocument/didClose" => self.did_close(params),
            _ => return Ok(()),
        };
        if let Err(message) = result {
            write_message(
                writer,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "window/logMessage",
                    "params": { "type": 1, "message": message }
                }),
            )?;
        }
        Ok(())
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
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("missing integer field `{field}`"))
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
        let mut reader = Cursor::new(output);
        let _: Value =
            serde_json::from_slice(&read_message(&mut reader).unwrap().unwrap()).unwrap();
        let log: Value =
            serde_json::from_slice(&read_message(&mut reader).unwrap().unwrap()).unwrap();
        assert_eq!(log["method"], "window/logMessage");
        assert!(log["params"]["message"]
            .as_str()
            .unwrap()
            .contains("stale version 2"));
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
