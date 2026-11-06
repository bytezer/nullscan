//! End-to-end protocol test: spawn the real binary and speak JSON-RPC over pipes.

mod common;

use std::process::Command;

use serde_json::{Value, json};

use common::{ServerProcess, TIMEOUT, file_uri, temp_workspace};

#[test]
fn publishes_diagnostics_on_open_and_exits_cleanly() {
    let folder = temp_workspace("protocol");
    let file = folder.join("a.ts");
    let file_uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;

    let initialize = client.initialize(&[&folder]);
    let capabilities = &initialize["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"]["openClose"], true);
    assert_eq!(capabilities["textDocumentSync"]["change"], 1);
    assert_eq!(capabilities["textDocumentSync"]["save"]["includeText"], false);
    assert_eq!(capabilities["positionEncoding"], "utf-16");
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(initialize["result"]["serverInfo"]["name"], "nullscan-lsp");

    client.send(json!({
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
    }));

    let published = client.notification("textDocument/publishDiagnostics");
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

    client.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }));
    let shutdown: Value = client.response(2);
    assert!(shutdown["error"].is_null());

    client.send(json!({ "jsonrpc": "2.0", "method": "exit", "params": null }));

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
