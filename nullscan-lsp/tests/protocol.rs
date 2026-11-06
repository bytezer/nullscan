//! End-to-end protocol test: spawn the real binary and speak JSON-RPC over pipes.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn publishes_diagnostics_on_open_and_exits_cleanly() {
    let folder = temp_workspace("protocol");
    let file = folder.join("a.ts");
    let folder_uri = file_uri(&folder);
    let file_uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let (mut stdin, messages) = process.take_pipes();

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "capabilities": {},
                "workspaceFolders": [{ "uri": folder_uri, "name": "fixture" }]
            }
        }),
    );

    let initialize = response_for(&messages, 1);
    let capabilities = &initialize["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"]["openClose"], true);
    assert_eq!(capabilities["textDocumentSync"]["change"], 1);
    assert_eq!(capabilities["textDocumentSync"]["save"]["includeText"], false);
    assert_eq!(capabilities["positionEncoding"], "utf-16");
    assert_eq!(initialize["result"]["serverInfo"]["name"], "nullscan-lsp");

    send(
        &mut stdin,
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": file_uri,
                    "languageId": "typescript",
                    "version": 1,
                    "text": "console.log(x);\nconst y: any = 1;\n"
                }
            }
        }),
    );

    let published = notification_for(&messages, "textDocument/publishDiagnostics");
    assert_eq!(published["params"]["uri"], file_uri);
    assert_eq!(published["params"]["version"], 1);

    let diagnostics = published["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    let console = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "no-console")
        .expect("no-console diagnostic");
    assert_eq!(console["severity"], 3);
    assert_eq!(console["source"], "nullscan");
    assert_eq!(console["range"]["start"], json!({ "line": 0, "character": 0 }));
    assert_eq!(console["range"]["end"], json!({ "line": 0, "character": 14 }));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "no-explicit-any"),
        "expected no-explicit-any in {diagnostics:?}"
    );

    send(
        &mut stdin,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
    );
    let shutdown = response_for(&messages, 2);
    assert!(shutdown["error"].is_null());

    send(
        &mut stdin,
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    );

    let status = process.wait_for_exit(TIMEOUT).expect("server did not exit");
    assert_eq!(status.code(), Some(0));

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn version_flag_prints_the_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_nullscan-lsp"))
        .arg("--version")
        .output()
        .expect("run --version");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("nullscan-lsp {}", env!("CARGO_PKG_VERSION"))
    );
}

/// Child process guard: never leave a hung server behind when an assertion fails.
struct ServerProcess {
    child: Child,
}

impl ServerProcess {
    fn spawn() -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_nullscan-lsp"))
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn nullscan-lsp");
        Self { child }
    }

    fn take_pipes(&mut self) -> (ChildStdin, Receiver<Value>) {
        let stdin = self.child.stdin.take().expect("stdin");
        let stdout = self.child.stdout.take().expect("stdout");
        (stdin, spawn_reader(stdout))
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return Some(status);
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads framed messages off stdout so the test can use timeouts instead of blocking.
fn spawn_reader(stdout: ChildStdout) -> Receiver<Value> {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Some(message) = read_message(&mut reader) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    receiver
}

fn send(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_string(&message).expect("serialize message");
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write message");
    stdin.flush().expect("flush message");
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let mut body = vec![0; content_length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn response_for(messages: &Receiver<Value>, id: i64) -> Value {
    loop {
        let message = messages
            .recv_timeout(TIMEOUT)
            .expect("timed out waiting for a response");
        if message["id"] == id {
            return message;
        }
    }
}

fn notification_for(messages: &Receiver<Value>, method: &str) -> Value {
    loop {
        let message = messages
            .recv_timeout(TIMEOUT)
            .expect("timed out waiting for a notification");
        if message["method"] == method {
            return message;
        }
    }
}

fn temp_workspace(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nullscan-lsp-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create workspace folder");
    std::fs::canonicalize(&dir).expect("canonicalize workspace folder")
}

fn file_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display().to_string().replace(' ', "%20"))
}
