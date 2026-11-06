//! Inline suppression: the server reports whatever core keeps after applying the markers.

mod common;

use common::{
    PUBLISH_METHOD, ServerProcess, diagnostic_codes, did_open, file_uri, temp_workspace,
};

#[test]
fn a_next_line_marker_hides_only_the_rules_it_names() {
    let folder = temp_workspace("suppression");
    let file = folder.join("a.ts");
    let text = "// nullscan-ignore-next-line no-console\nconsole.log(x); if (x == 1) { }\n";

    let mut process = ServerProcess::spawn();
    let client = &mut process.client;
    client.initialize(&[&folder]);

    did_open(client, &file_uri(&file), text);

    let published = client.notification(PUBLISH_METHOD);
    let codes = diagnostic_codes(&published);
    assert!(
        !codes.contains(&"no-console".to_owned()),
        "the listed rule is suppressed, codes: {codes:?}"
    );
    assert!(
        codes.contains(&"eqeqeq".to_owned()),
        "an unlisted rule on the same line is still reported, codes: {codes:?}"
    );

    let _ = std::fs::remove_dir_all(folder);
}
