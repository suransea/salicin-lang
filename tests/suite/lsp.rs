use crate::support::*;
use serde_json::Value;
use std::io::{BufReader, BufWriter, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio};

struct LspProcess {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
    error: ChildStderr,
}

impl LspProcess {
    fn start(workspace: &Path) -> Self {
        let mut child = salic()
            .arg("lsp")
            .arg(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start language server");
        Self {
            input: BufWriter::new(child.stdin.take().unwrap()),
            output: BufReader::new(child.stdout.take().unwrap()),
            error: child.stderr.take().unwrap(),
            child,
        }
    }

    fn send(&mut self, value: &Value) {
        salicin_lang::lsp::write_message(&mut self.input, value).unwrap();
    }

    fn receive(&mut self) -> Value {
        let bytes = salicin_lang::lsp::read_message(&mut self.output)
            .expect("read LSP response")
            .expect("server remains connected");
        serde_json::from_slice(&bytes).expect("valid JSON response")
    }

    fn finish(mut self) {
        drop(self.input);
        let status = self.child.wait().expect("wait for language server");
        let mut stderr = String::new();
        self.error.read_to_string(&mut stderr).unwrap();
        assert_eq!(status.code(), Some(0), "LSP stderr:\n{stderr}");
        assert!(stderr.is_empty(), "LSP stderr:\n{stderr}");
    }
}

fn workspace() -> (TestDirectory, PathBuf, PathBuf) {
    let workspace = TestDirectory::new();
    workspace.write(
        "salicin.toml",
        "[package]\nname = \"lsp-acceptance\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    let root = workspace.write(
        "src/main.sc",
        "pub let root_value(): i32 = { helper.value() }\n",
    );
    let module = workspace.write("src/helper.sc", "pub let value(): i32 = { 42 }\n");
    (
        workspace,
        fs::canonicalize(root).unwrap(),
        fs::canonicalize(module).unwrap(),
    )
}

fn is_subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|actual| is_subset(value, actual))
        }),
        (Value::Array(expected), Value::Array(actual)) => {
            expected.len() <= actual.len()
                && expected
                    .iter()
                    .zip(actual)
                    .all(|(expected, actual)| is_subset(expected, actual))
        }
        _ => expected == actual,
    }
}

fn replay(path: &str, workspace: &Path, root: &Path, module: &Path) -> Vec<Value> {
    let root_uri = salicin_lang::lsp::path_to_file_uri(&root.display().to_string());
    let module_uri = salicin_lang::lsp::path_to_file_uri(&module.display().to_string());
    let transcript = fs::read_to_string(fixture("lsp", path))
        .unwrap()
        .replace("$ROOT_URI", &root_uri)
        .replace("$MODULE_URI", &module_uri);
    let mut server = LspProcess::start(workspace);
    let mut received = Vec::new();
    for (line, action) in transcript.lines().enumerate() {
        let action: Value = serde_json::from_str(action)
            .unwrap_or_else(|error| panic!("{path}:{}: {error}", line + 1));
        if let Some(message) = action.get("send") {
            server.send(message);
            continue;
        }
        let expected = action.get("expect").expect("send or expect action");
        loop {
            let actual = server.receive();
            let matched = is_subset(expected, &actual);
            received.push(actual);
            if matched {
                break;
            }
        }
    }
    server.finish();
    received
}

#[test]
fn recorded_transcripts_cover_restart_unicode_multiple_documents_and_stale_versions() {
    let (workspace, root, module) = workspace();
    for _ in 0..2 {
        let messages = replay("clean-restart.jsonl", &workspace.0, &root, &module);
        assert!(messages.iter().any(|message| message["id"] == 1));
    }

    let messages = replay("unicode-multidoc.jsonl", &workspace.0, &root, &module);
    let semantic = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("semantic token response");
    let data = semantic["result"]["data"].as_array().unwrap();
    assert!(!data.is_empty());
    assert_eq!(data.len() % 5, 0);
    assert!(messages.iter().any(|message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("stale version 8"))
    }));
}

fn framed(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap();
    let mut message = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    message.extend(body);
    message
}

#[test]
fn malformed_protocol_input_is_bounded_and_parse_errors_are_recoverable() {
    let (workspace, _, _) = workspace();
    let mut recoverable = b"Content-Length: 1\r\n\r\n{".to_vec();
    recoverable.extend(framed(&serde_json::json!({
        "jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}
    })));
    recoverable.extend(framed(&serde_json::json!({
        "jsonrpc":"2.0", "id":2, "method":"shutdown"
    })));
    recoverable.extend(framed(
        &serde_json::json!({"jsonrpc":"2.0", "method":"exit"}),
    ));
    let output = salic()
        .arg("lsp")
        .arg(&workspace.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(&recoverable)?;
            child.wait_with_output()
        })
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", output_text(&output));
    let mut reader = BufReader::new(output.stdout.as_slice());
    let first: Value = serde_json::from_slice(
        &salicin_lang::lsp::read_message(&mut reader)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["error"]["code"], -32700);

    for malformed in [
        b"X: 1\r\n\r\n".as_slice(),
        b"Content-Length: 0\r\nContent-Length: 0\r\n\r\n".as_slice(),
        b"Content-Length: 999999999\r\n\r\n".as_slice(),
        b"Content-Length: 4\r\n\r\n{}".as_slice(),
    ] {
        let output = salic()
            .arg("lsp")
            .arg(&workspace.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(malformed)?;
                child.wait_with_output()
            })
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("LSP transport failed"),
            "{}",
            output_text(&output)
        );
    }
}
