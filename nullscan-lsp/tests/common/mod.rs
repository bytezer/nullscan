//! Shared JSON-RPC framing helpers for the LSP integration tests.
//!
//! The server is spawned as a real process and spoken to over pipes, so the tests cover
//! the transport, the dispatch loop and the worker thread rather than just the handlers.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub const TIMEOUT: Duration = Duration::from_secs(20);

/// A spawned server process plus the framed client used to talk to it.
pub struct ServerProcess {
    child: Child,
    pub client: Client,
}

impl ServerProcess {
    pub fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_nullscan-lsp"))
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn nullscan-lsp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Self {
            child,
            client: Client::new(stdin, stdout),
        }
    }

    pub fn wait_for_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
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

/// Framed JSON-RPC client. Notifications read while waiting for a response are kept so a
/// later lookup still sees them.
pub struct Client {
    stdin: ChildStdin,
    messages: Receiver<Value>,
    pending: Vec<Value>,
}

impl Client {
    fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            messages: spawn_reader(stdout),
            pending: Vec::new(),
        }
    }

    pub fn send(&mut self, message: Value) {
        let body = serde_json::to_string(&message).expect("serialize message");
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())
            .expect("write message");
        self.stdin.flush().expect("flush message");
    }

    /// Sends `initialize`, then `initialized`, and returns the initialize response.
    pub fn initialize(&mut self, folders: &[&Path]) -> Value {
        let workspace_folders: Vec<Value> = folders
            .iter()
            .map(|folder| json!({ "uri": file_uri(folder), "name": "fixture" }))
            .collect();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "capabilities": {},
                "workspaceFolders": workspace_folders
            }
        }));
        let response = self.response(1);
        self.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        response
    }

    /// Reads until the response with `id`, returning it with every notification seen.
    pub fn response_with_notifications(&mut self, id: i64) -> (Value, Vec<Value>) {
        let mut notifications = Vec::new();
        loop {
            let message = self.recv();
            if message["id"] == id {
                return (message, notifications);
            }
            if message.get("method").is_some() {
                notifications.push(message);
            }
        }
    }

    pub fn response(&mut self, id: i64) -> Value {
        self.response_with_notifications(id).0
    }

    /// Reads until a notification with `method`; everything else stays available.
    pub fn notification(&mut self, method: &str) -> Value {
        loop {
            let message = self.recv();
            if message["method"] == method {
                return message;
            }
            self.pending.push(message);
        }
    }

    fn recv(&mut self) -> Value {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        self.messages
            .recv_timeout(TIMEOUT)
            .expect("timed out waiting for a message")
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

/// Creates a fresh, canonicalized workspace folder under the system temp directory.
pub fn temp_workspace(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nullscan-lsp-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create workspace folder");
    std::fs::canonicalize(&dir).expect("canonicalize workspace folder")
}

/// Plain temp paths need no escaping beyond spaces.
pub fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display().to_string().replace(' ', "%20"))
}
