//! `nullscan/rules`: the server returns the whole rule catalog, params ignored.

mod common;

use serde_json::{Value, json};

use common::{ServerProcess, temp_workspace};

fn rules(client: &mut common::Client, id: i64, params: Value) -> Value {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "nullscan/rules",
        "params": params
    }));
    let response = client.response(id);
    assert!(response["error"].is_null(), "response: {response}");
    response["result"].clone()
}

#[test]
fn rules_request_returns_the_whole_catalog() {
    let folder = temp_workspace("rules");
    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    let catalog = rules(client, 2, Value::Null)
        .as_array()
        .cloned()
        .expect("catalog array");
    assert_eq!(catalog.len(), 56);

    let eqeqeq = catalog
        .iter()
        .find(|info| info["id"] == "eqeqeq")
        .expect("eqeqeq entry");
    assert_eq!(eqeqeq["title"], "Use strict equality");
    assert_eq!(eqeqeq["severity"], "warning");
    assert_eq!(
        eqeqeq["docs_url"],
        "https://github.com/bytezer/nullscan/blob/main/RULES.md#eqeqeq"
    );
    assert!(
        eqeqeq["why"]
            .as_str()
            .is_some_and(|why| !why.trim().is_empty()),
        "eqeqeq entry: {eqeqeq}"
    );

    // The request takes no parameters, so any payload is ignored.
    let ignored = rules(client, 3, json!({ "unknown": true }))
        .as_array()
        .cloned()
        .expect("catalog array");
    assert_eq!(ignored.len(), catalog.len());

    let _ = std::fs::remove_dir_all(folder);
}
