//! Quick fixes: disable a rule in `nullscan.toml` or suppress it inline.

mod common;

use serde_json::{Value, json};

use common::{PUBLISH_METHOD, ServerProcess, did_open, file_uri, temp_workspace};

/// Sends `textDocument/codeAction` and returns its `result`.
fn code_action(
    client: &mut common::Client,
    id: i64,
    uri: &str,
    range: Value,
    diagnostics: Value,
) -> Value {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "textDocument/codeAction",
        "params": {
            "textDocument": { "uri": uri },
            "range": range,
            "context": { "diagnostics": diagnostics }
        }
    }));
    client.response(id)["result"].clone()
}

/// The action with `title`, or a panic listing what was offered instead.
fn action_titled<'a>(actions: &'a [Value], title: &str) -> &'a Value {
    actions
        .iter()
        .find(|action| action["title"] == title)
        .unwrap_or_else(|| panic!("action {title} missing from {actions:?}"))
}

/// The `documentChanges` operations of one action.
fn operations(action: &Value) -> &Vec<Value> {
    action["edit"]["documentChanges"]
        .as_array()
        .expect("document change operations")
}

/// The text an action would write into the config.
fn new_text(action: &Value) -> &str {
    operations(action)
        .last()
        .expect("text edit operation")["edits"][0]["newText"]
        .as_str()
        .expect("new text")
}

/// Opens `a.ts` with a console call and returns the publish notification.
fn open_console_file(client: &mut common::Client, uri: &str) -> Value {
    did_open(client, uri, "console.log(x);\n");
    client.notification(PUBLISH_METHOD)
}

#[test]
fn the_quick_fix_appends_the_rule_to_an_existing_config() {
    let folder = temp_workspace("code-action-edit");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);
    std::fs::write(
        folder.join("nullscan.toml"),
        "# keep this header\n[rules]\ndisabled = [\"no-var\"]\n",
    )
    .expect("write config");

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    let initialize = client.initialize(&[&folder]);
    assert_eq!(
        initialize["result"]["capabilities"]["codeActionProvider"]["codeActionKinds"][0],
        "quickfix"
    );

    let published = open_console_file(client, &uri);
    let diagnostics = published["params"]["diagnostics"].clone();
    let range = diagnostics[0]["range"].clone();

    let actions = code_action(client, 2, &uri, range, diagnostics);
    let actions = actions.as_array().expect("code action array");
    assert_eq!(actions.len(), 2, "actions: {actions:?}");

    let action = action_titled(actions, "Disable no-console in nullscan.toml");
    assert_eq!(action["kind"], "quickfix");
    assert_eq!(action["isPreferred"], false);
    assert_eq!(action["diagnostics"][0]["code"], "no-console");

    let suppress = action_titled(actions, "Suppress no-console on the next line");
    assert_eq!(suppress["kind"], "quickfix");
    assert_eq!(suppress["isPreferred"], false);
    assert_eq!(suppress["diagnostics"][0]["code"], "no-console");

    assert_eq!(
        operations(action).len(),
        1,
        "an existing config needs no CreateFile"
    );
    let text = new_text(action);
    assert!(
        text.contains("# keep this header"),
        "comments must survive: {text}"
    );
    assert!(
        text.contains("disabled = [\"no-var\", \"no-console\"]"),
        "the rule must be appended: {text}"
    );
    assert_eq!(
        action["edit"]["documentChanges"][0]["textDocument"]["version"],
        Value::Null
    );

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_missing_config_is_created_by_the_edit() {
    let folder = temp_workspace("code-action-create");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);
    let config_uri = file_uri(&folder.join("nullscan.toml"));

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    let published = open_console_file(client, &uri);
    let diagnostics = published["params"]["diagnostics"].clone();
    let range = diagnostics[0]["range"].clone();

    let actions = code_action(client, 2, &uri, range, diagnostics);
    let actions = actions.as_array().expect("code action array");
    let action = action_titled(actions, "Disable no-console in nullscan.toml");

    let operations = operations(action);
    assert_eq!(operations.len(), 2, "create then edit: {operations:?}");
    assert_eq!(operations[0]["kind"], "create");
    assert_eq!(operations[0]["uri"], config_uri);
    assert!(operations[1]["textDocument"]["uri"] == config_uri);
    assert_eq!(new_text(action), "[rules]\ndisabled = [\"no-console\"]\n");

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_cursor_only_request_offers_the_published_diagnostic() {
    let folder = temp_workspace("code-action-cursor");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    open_console_file(client, &uri);
    let cursor = json!({
        "start": { "line": 0, "character": 3 },
        "end": { "line": 0, "character": 3 }
    });

    let actions = code_action(client, 2, &uri, cursor, json!([]));
    let actions = actions.as_array().expect("code action array");

    assert_eq!(actions.len(), 2, "actions: {actions:?}");
    let disable = action_titled(actions, "Disable no-console in nullscan.toml");
    assert_eq!(disable["kind"], "quickfix");
    assert!(new_text(disable).contains("no-console"));
    action_titled(actions, "Suppress no-console on the next line");

    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_suppress_action_inserts_the_comment_above_the_diagnostic() {
    let folder = temp_workspace("code-action-suppress");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &uri, "function f() {\n  console.log(x);\n}\n");
    let published = client.notification(PUBLISH_METHOD);
    let diagnostics = published["params"]["diagnostics"].clone();
    let range = diagnostics[0]["range"].clone();

    let actions = code_action(client, 2, &uri, range, diagnostics);
    let actions = actions.as_array().expect("code action array");
    let action = action_titled(actions, "Suppress no-console on the next line");

    assert_eq!(action["kind"], "quickfix");
    assert_eq!(action["isPreferred"], false);
    assert_eq!(action["diagnostics"][0]["code"], "no-console");

    let operations = operations(action);
    assert_eq!(operations.len(), 1, "the source document is the only target");
    assert_eq!(operations[0]["textDocument"]["uri"], uri);
    assert_eq!(operations[0]["textDocument"]["version"], 1);
    let edit = &operations[0]["edits"][0];
    assert_eq!(
        edit["range"],
        json!({
            "start": { "line": 1, "character": 0 },
            "end": { "line": 1, "character": 0 }
        })
    );
    assert_eq!(edit["newText"], "  // nullscan-ignore-next-line no-console\n");

    let _ = std::fs::remove_dir_all(folder);
}
