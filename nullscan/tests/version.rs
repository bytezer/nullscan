//! `nullscan --version` must expose the crate version to release tooling and editors.

use std::process::Command;

#[test]
fn version_prints_the_crate_version_and_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_nullscan"))
        .arg("--version")
        .output()
        .expect("run nullscan binary");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("nullscan {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}
