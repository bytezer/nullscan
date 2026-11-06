//! Workspace-wide scan tests: the custom requests, per-folder config and summary cache.

mod common;

use serde_json::{Value, json};

use common::{ServerProcess, file_uri, temp_workspace};

fn open(client: &mut common::Client, uri: &str, text: &str) {
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "typescript",
                "version": 1,
                "text": text
            }
        }
    }));
}

/// Runs `nullscan/analyzeWorkspace` with `id` and returns the response plus the
/// diagnostics published for `uri` while the scan ran.
fn analyze_workspace(client: &mut common::Client, id: i64, uri: &str) -> (Value, Value) {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "nullscan/analyzeWorkspace",
        "params": {}
    }));
    let (response, notifications) = client.response_with_notifications(id);
    let published = notifications
        .into_iter()
        .find(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == uri
        })
        .unwrap_or_else(|| panic!("no publishDiagnostics for {uri}"));
    (response, published)
}

fn request_summary(client: &mut common::Client, id: i64) -> Value {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "nullscan/summary",
        "params": null
    }));
    client.response(id)
}

fn diagnostic_codes(published: &Value) -> Vec<String> {
    published["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .filter_map(|diagnostic| diagnostic["code"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn analyze_workspace_publishes_diagnostics_and_caches_the_summary() {
    let folder = temp_workspace("analyze");
    let file = folder.join("a.ts");
    std::fs::write(&file, "console.log(1);\n").expect("write a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    open(client, &uri, "console.log(1);\n");
    assert_eq!(
        client.notification("textDocument/publishDiagnostics")["params"]["uri"],
        uri
    );

    let (response, published) = analyze_workspace(client, 2, &uri);
    assert_eq!(published["params"]["version"], Value::Null);
    let codes = diagnostic_codes(&published);
    assert!(codes.contains(&"no-console".to_owned()), "codes: {codes:?}");

    let summary = &response["result"];
    assert_eq!(summary["filesScanned"], 1);
    assert_eq!(summary["filesWithIssues"], 1);
    assert_eq!(summary["duplicates"], 0);
    assert!(
        summary["byRule"]
            .as_array()
            .expect("byRule array")
            .iter()
            .any(|entry| entry["rule"] == "no-console"),
        "summary: {summary}"
    );

    let cached = request_summary(client, 3);
    assert_eq!(cached["result"]["stale"], false);
    assert_eq!(cached["result"]["summary"]["filesScanned"], 1);
    assert_eq!(cached["result"]["summary"]["byRule"], summary["byRule"]);

    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "const value = 1;\n" }]
        }
    }));

    let stale = request_summary(client, 4);
    assert_eq!(stale["result"]["stale"], true);
    // The cached summary survives the edit; only its freshness changes.
    assert_eq!(stale["result"]["summary"]["filesScanned"], 1);

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_multi_root_workspace_scans_and_merges_every_folder() {
    let first = temp_workspace("multi-a");
    let second = temp_workspace("multi-b");
    let a = first.join("a.ts");
    let b = second.join("b.ts");
    std::fs::write(&a, "console.log(1);\n").expect("write a.ts");
    std::fs::write(&b, "console.log(2);\n").expect("write b.ts");
    let a_uri = file_uri(&a);
    let b_uri = file_uri(&b);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&first, &second]);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "nullscan/analyzeWorkspace",
        "params": {}
    }));
    let (response, notifications) = client.response_with_notifications(2);

    let published: Vec<String> = notifications
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .filter_map(|message| message["params"]["uri"].as_str().map(str::to_owned))
        .collect();
    assert!(published.contains(&a_uri), "published: {published:?}");
    assert!(published.contains(&b_uri), "published: {published:?}");

    let summary = &response["result"];
    assert_eq!(summary["filesScanned"], 2);
    assert_eq!(summary["byRule"][0]["rule"], "no-console");
    assert_eq!(summary["byRule"][0]["count"], 2);

    let _ = std::fs::remove_dir_all(first);
    let _ = std::fs::remove_dir_all(second);
}

#[test]
fn folder_config_disables_rules_for_the_workspace_scan() {
    let folder = temp_workspace("config");
    let file = folder.join("a.ts");
    std::fs::write(&file, "console.log(1);\n").expect("write a.ts");
    std::fs::write(
        folder.join("nullscan.toml"),
        "[rules]\ndisabled = [\"no-console\"]\n",
    )
    .expect("write config");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    let (response, published) = analyze_workspace(client, 2, &uri);
    let codes = diagnostic_codes(&published);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "config should disable no-console, got {codes:?}"
    );
    assert_eq!(response["result"]["filesScanned"], 1);

    let _ = std::fs::remove_dir_all(folder);
}
