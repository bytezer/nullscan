//! Hover: the rule under the cursor becomes Markdown, and empty space returns null.

mod common;

use serde_json::{Value, json};

use common::{PUBLISH_METHOD, ServerProcess, did_open, file_uri, temp_workspace};

fn hover(client: &mut common::Client, id: i64, uri: &str, line: u32, character: u32) -> Value {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        }
    }));
    client.response(id)["result"].clone()
}

#[test]
fn hover_marks_up_the_rule_under_the_cursor() {
    let folder = temp_workspace("hover");
    let file = folder.join("a.ts");
    let uri = file_uri(&file);

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    let initialize = client.initialize(&[&folder]);
    assert_eq!(initialize["result"]["capabilities"]["hoverProvider"], true);

    did_open(client, &uri, "console.log(x);\n\n");
    client.notification(PUBLISH_METHOD);

    let contents = &hover(client, 2, &uri, 0, 3)["contents"];
    assert_eq!(contents["kind"], "markdown");
    let value = contents["value"].as_str().expect("markdown value");
    assert!(
        value.contains("**Remove console calls** (`no-console`) — Info"),
        "hover should include the title, id and level: {value}"
    );
    assert!(
        value.contains("Console calls are for local debugging only"),
        "hover should include the registry rationale: {value}"
    );
    assert!(
        value.contains(
            "[More](https://github.com/bytezer/nullscan/blob/main/RULES.md#no-console)"
        ),
        "hover should link the rule docs: {value}"
    );

    let empty = hover(client, 3, &uri, 1, 0);
    assert!(empty.is_null(), "hover over whitespace: {empty}");

    let _ = std::fs::remove_dir_all(folder);
}
