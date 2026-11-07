//! Duplicate lifecycle: both sides get a linked diagnostic, and an edit drops only the
//! edited file's stale duplicates until the next workspace scan restores them.

mod common;

use serde_json::Value;

use common::{PUBLISH_METHOD, ServerProcess, did_change, did_open, file_uri, temp_workspace};

/// A block well above the default 30-token duplicate threshold.
fn duplicate_block() -> String {
    (0..40)
        .map(|i| format!("call{i}();"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn published_for<'a>(notifications: &'a [Value], uri: &str) -> &'a Value {
    notifications
        .iter()
        .find(|message| message["method"] == PUBLISH_METHOD && message["params"]["uri"] == uri)
        .unwrap_or_else(|| panic!("no publishDiagnostics for {uri}"))
}

fn duplicate_code(published: &Value) -> &Value {
    published["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["code"] == "duplicate-code")
        .unwrap_or_else(|| panic!("no duplicate-code diagnostic in {published}"))
}

fn codes(published: &Value) -> Vec<String> {
    common::diagnostic_codes(published)
}

fn analyze_workspace(client: &mut common::Client, id: i64) -> Vec<Value> {
    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "nullscan/analyzeWorkspace",
        "params": {}
    }));
    client.response_with_notifications(id).1
}

#[test]
fn duplicates_are_linked_on_both_sides_and_an_edit_drops_one_side() {
    let folder = temp_workspace("duplicates-lifecycle");
    let block = duplicate_block();
    let a_text = format!("function a() {{\n{block}\n}}\n");
    let b_text = format!("function b() {{\n{block}\n}}\n");
    let a = folder.join("a.ts");
    let b = folder.join("b.ts");
    std::fs::write(&a, &a_text).expect("write a.ts");
    std::fs::write(&b, &b_text).expect("write b.ts");
    let a_uri = file_uri(&a);
    let b_uri = file_uri(&b);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);
    did_open(client, &a_uri, &a_text);
    client.notification(PUBLISH_METHOD);
    did_open(client, &b_uri, &b_text);
    client.notification(PUBLISH_METHOD);

    let notifications = analyze_workspace(client, 2);
    let a_duplicate = duplicate_code(published_for(&notifications, &a_uri));
    let b_duplicate = duplicate_code(published_for(&notifications, &b_uri));

    assert_eq!(a_duplicate["source"], "nullscan");
    assert_eq!(a_duplicate["severity"], 2);
    assert_eq!(b_duplicate["severity"], 2);
    assert!(
        a_duplicate["message"]
            .as_str()
            .expect("message")
            .starts_with("duplicated block of 42 lines ("),
        "message: {}",
        a_duplicate["message"]
    );
    assert!(
        a_duplicate["message"]
            .as_str()
            .expect("message")
            .contains("also found at"),
        "message: {}",
        a_duplicate["message"]
    );
    assert_eq!(
        a_duplicate["relatedInformation"][0]["location"]["uri"],
        b_uri
    );
    assert_eq!(
        b_duplicate["relatedInformation"][0]["location"]["uri"],
        a_uri
    );

    // The edit keeps the duplicated block, so a later scan finds it again, but the
    // published ranges now belong to a different text and must disappear immediately.
    did_change(client, &a_uri, 2, &format!("// edited\n{a_text}"));
    let republished = client
        .notification_within(PUBLISH_METHOD, std::time::Duration::from_secs(1))
        .expect("didChange republishes the edited file");
    assert_eq!(republished["params"]["uri"], a_uri);
    assert!(
        !codes(&republished).contains(&"duplicate-code".to_owned()),
        "edited file keeps stale duplicates: {republished}"
    );
    assert!(
        client
            .notification_within(PUBLISH_METHOD, std::time::Duration::from_millis(300))
            .is_none(),
        "another file must keep its diagnostics until the next scan"
    );

    let notifications = analyze_workspace(client, 3);
    let a_restored = duplicate_code(published_for(&notifications, &a_uri));
    let b_restored = duplicate_code(published_for(&notifications, &b_uri));
    assert_eq!(
        a_restored["relatedInformation"][0]["location"]["uri"],
        b_uri
    );
    assert_eq!(
        b_restored["relatedInformation"][0]["location"]["uri"],
        a_uri
    );

    let _ = std::fs::remove_dir_all(folder);
}
