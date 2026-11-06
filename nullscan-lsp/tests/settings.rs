//! Settings the client pushes: analyze-on-open gating and debounced analyze-on-type.

mod common;

use std::time::Duration;

use serde_json::json;

use common::{
    PUBLISH_METHOD, ServerProcess, configure, diagnostic_codes, did_change, did_open, file_uri,
    temp_workspace,
};

#[test]
fn initial_settings_from_initialize_suppress_the_open_publish() {
    let folder = temp_workspace("settings-initial");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize_with_options(&[&folder], json!({ "nullscan": { "analyzeOnOpen": false } }));

    did_open(client, &uri, "console.log(1);\n");

    assert!(
        client
            .notification_within(PUBLISH_METHOD, Duration::from_millis(500))
            .is_none(),
        "initializationOptions analyzeOnOpen=false must not publish on open"
    );

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn configuration_changes_toggle_publishing_on_open() {
    let folder = temp_workspace("settings-change");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    configure(client, json!({ "analyzeOnOpen": false }));
    did_open(client, &uri, "console.log(1);\n");
    assert!(
        client
            .notification_within(PUBLISH_METHOD, Duration::from_millis(500))
            .is_none(),
        "analyzeOnOpen=false must not publish on open"
    );

    configure(client, json!({ "analyzeOnOpen": true }));
    let republished = client
        .notification_within(PUBLISH_METHOD, Duration::from_secs(1))
        .expect("enabling analyzeOnOpen republishes the open document");
    assert_eq!(republished["params"]["uri"], uri);
    assert!(
        diagnostic_codes(&republished).contains(&"no-console".to_owned()),
        "diagnostics: {republished}"
    );

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn analyze_on_type_publishes_the_changed_version_after_a_quiet_period() {
    let folder = temp_workspace("settings-type");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    configure(
        client,
        json!({ "analyzeOnOpen": false, "analyzeOnType": true }),
    );
    did_open(client, &uri, "const value = 1;\n");
    assert!(
        client
            .notification_within(PUBLISH_METHOD, Duration::from_millis(400))
            .is_none(),
        "analyzeOnOpen=false keeps didOpen silent"
    );

    did_change(client, &uri, 3, "console.log(1);\n");

    let published = client
        .notification_within(PUBLISH_METHOD, Duration::from_secs(1))
        .expect("analyzeOnType publishes after the debounce interval");
    assert_eq!(published["params"]["uri"], uri);
    assert_eq!(published["params"]["version"], 3);
    assert!(
        diagnostic_codes(&published).contains(&"no-console".to_owned()),
        "diagnostics: {published}"
    );

    let _ = std::fs::remove_dir_all(folder);
}
