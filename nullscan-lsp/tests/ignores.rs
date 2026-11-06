//! `[files].ignore`: an open document the filter matches publishes no diagnostics.

mod common;

use std::fs;

use common::{
    PUBLISH_METHOD, ServerProcess, diagnostic_codes, did_open, file_uri, temp_workspace,
};

#[test]
fn an_ignored_document_reports_nothing_but_its_sibling_still_does() {
    let folder = temp_workspace("ignore-open");
    let src = folder.join("src");
    fs::create_dir_all(&src).expect("create src");
    fs::write(
        folder.join("nullscan.toml"),
        "[files]\nignore = [\"**/*.spec.ts\"]\n",
    )
    .expect("write config");

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&src.join("app.spec.ts")), "console.log(x);\n");
    let ignored = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&ignored);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "ignored document must publish nothing, codes: {codes:?}"
    );

    did_open(client, &file_uri(&src.join("app.ts")), "console.log(x);\n");
    let reported = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&reported);
    assert!(
        codes.contains(&"no-console".to_owned()),
        "an unignored sibling still reports, codes: {codes:?}"
    );

    let _ = fs::remove_dir_all(folder);
}
