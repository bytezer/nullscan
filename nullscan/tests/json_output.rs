//! End-to-end checks for `--format json`: the CLI must expose the workspace contract,
//! including exit codes, to anything that shells out to it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "nullscan-json-{label}-{}-{id}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nullscan"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run nullscan binary")
}

fn parse_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn json_reports_a_known_finding_and_exits_one() {
    let dir = temp_dir("finding");
    let file = dir.join("a.ts");
    fs::write(&file, "const value = 1;\nconsole.log(value);\n").expect("write source");

    let output = run_in(&dir, &["--format", "json", file.to_str().expect("utf-8 path")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty(), "JSON warnings belong in the document");

    let json = parse_stdout(&output);
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["summary"]["filesScanned"], 1);
    assert_eq!(json["summary"]["filesWithIssues"], 1);
    assert_eq!(json["summary"]["bySeverity"]["info"], 1);

    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["rule"], "no-console");
    assert_eq!(diagnostic["severity"], "info");
    assert_eq!(diagnostic["range"]["start"]["line"], 1);
    assert_eq!(diagnostic["range"]["start"]["character"], 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn json_clean_file_exits_zero() {
    let dir = temp_dir("clean");
    let file = dir.join("clean.ts");
    fs::write(&file, "export const value = 1;\n").expect("write source");

    let output = run_in(&dir, &["--format", "json", file.to_str().expect("utf-8 path")]);

    assert_eq!(output.status.code(), Some(0));
    let json = parse_stdout(&output);
    assert_eq!(json["diagnostics"], Value::Array(Vec::new()));
    assert_eq!(json["summary"]["filesScanned"], 1);
    assert_eq!(json["summary"]["filesWithIssues"], 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn invalid_config_is_a_tool_error() {
    let dir = temp_dir("config");
    let file = dir.join("a.ts");
    let config = dir.join("nullscan.toml");
    fs::write(&file, "export const value = 1;\n").expect("write source");
    fs::write(&config, "this is not = toml [").expect("write config");

    let output = run_in(
        &dir,
        &[
            "--format",
            "json",
            "--config",
            config.to_str().expect("utf-8 path"),
            file.to_str().expect("utf-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid config file"));

    let _ = fs::remove_dir_all(dir);
}
