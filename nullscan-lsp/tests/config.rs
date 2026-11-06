//! `--config`: one explicit config file for every opened document.
//!
//! The flag replaces the per-folder `nullscan.toml` resolution; a missing or broken file
//! is reported to the client and the server keeps running with the default rules.

mod common;

use std::fs;

use common::{
    PUBLISH_METHOD, ServerProcess, TIMEOUT, diagnostic_codes, did_open, file_uri, temp_workspace,
};

#[test]
fn explicit_config_disables_a_rule_for_opened_documents() {
    let folder = temp_workspace("config-explicit");
    let file = folder.join("a.ts");
    let config = folder.join("custom.toml");
    fs::write(&config, "[rules]\ndisabled = [\"no-console\"]\n").expect("write config");
    let config_arg = config.display().to_string();

    let mut process = ServerProcess::spawn_with_args(&["--config", &config_arg]);
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&file), "console.log(value);\n");

    let published = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&published);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "no-console must be disabled, codes: {codes:?}"
    );

    let _ = fs::remove_dir_all(folder);
}

#[test]
fn the_equals_form_also_disables_the_rule() {
    let folder = temp_workspace("config-equals");
    let file = folder.join("a.ts");
    let config = folder.join("custom.toml");
    fs::write(&config, "[rules]\ndisabled = [\"no-console\"]\n").expect("write config");
    let config_arg = format!("--config={}", config.display());

    let mut process = ServerProcess::spawn_with_args(&[&config_arg]);
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&file), "console.log(value);\n");

    let published = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&published);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "no-console must be disabled, codes: {codes:?}"
    );

    let _ = fs::remove_dir_all(folder);
}

#[test]
fn without_the_flag_the_default_rule_set_applies() {
    let folder = temp_workspace("config-default");
    let file = folder.join("a.ts");

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&file), "console.log(value);\n");

    let published = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&published);
    assert!(
        codes.contains(&"no-console".to_owned()),
        "no-console expected, codes: {codes:?}"
    );

    let _ = fs::remove_dir_all(folder);
}

#[test]
fn a_missing_explicit_config_warns_and_falls_back_to_defaults() {
    let folder = temp_workspace("config-missing");
    let file = folder.join("a.ts");
    let missing = format!("--config={}", folder.join("does-not-exist.toml").display());

    let mut process = ServerProcess::spawn_with_args(&[&missing]);
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&file), "console.log(value);\n");

    let warning = client
        .notification_within("window/logMessage", TIMEOUT)
        .expect("config warning");
    let message = warning["params"]["message"]
        .as_str()
        .expect("log message text");
    assert!(
        message.contains("nullscan config"),
        "unexpected warning: {message}"
    );

    let published = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&published);
    assert!(
        codes.contains(&"no-console".to_owned()),
        "defaults must still report no-console, codes: {codes:?}"
    );

    let _ = fs::remove_dir_all(folder);
}
