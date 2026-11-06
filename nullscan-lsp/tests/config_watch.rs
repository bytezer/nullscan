//! `workspace/didChangeWatchedFiles`: a `nullscan.toml` change re-analyzes open buffers.

mod common;

use std::time::Duration;

use serde_json::json;

use common::{
    PUBLISH_METHOD, ServerProcess, diagnostic_codes, did_open, file_uri, temp_workspace,
};

#[test]
fn changing_nullscan_toml_republishes_open_documents_under_the_new_config() {
    let folder = temp_workspace("config-watch");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &uri, "console.log(1);\n");
    let opened = client
        .notification_within(PUBLISH_METHOD, Duration::from_secs(5))
        .expect("publish on open");
    assert!(
        diagnostic_codes(&opened).contains(&"no-console".to_owned()),
        "diagnostics: {opened}"
    );

    let config = folder.join("nullscan.toml");
    std::fs::write(&config, "[rules]\ndisabled = [\"no-console\"]\n").expect("write config");
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "workspace/didChangeWatchedFiles",
        "params": {
            "changes": [{ "uri": file_uri(&config), "type": 2 }]
        }
    }));

    let republished = client
        .notification_within(PUBLISH_METHOD, Duration::from_secs(2))
        .expect("config change republishes the open document");
    assert_eq!(republished["params"]["uri"], uri);
    let codes = diagnostic_codes(&republished);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "the new config disables no-console, got {codes:?}"
    );

    let _ = std::fs::remove_dir_all(folder);
}
