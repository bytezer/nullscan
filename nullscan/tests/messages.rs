//! Human-facing duplicate messages: the text and SARIF renderers must report the block
//! size in lines, with the token count kept as a secondary detail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "nullscan-messages-{label}-{}-{id}",
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

/// Writes two files whose 40-line function bodies are identical, so the clone spans the
/// whole file: line 1 through the closing brace.
fn duplicate_pair(dir: &Path) {
    let block = (0..40)
        .map(|i| format!("call{i}();"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(dir.join("a.ts"), format!("function a() {{\n{block}\n}}\n")).expect("write a.ts");
    fs::write(dir.join("b.ts"), format!("function b() {{\n{block}\n}}\n")).expect("write b.ts");
}

#[test]
fn text_format_reports_duplicates_in_lines_and_tokens() {
    let dir = temp_dir("text");
    duplicate_pair(&dir);

    let output = run_in(&dir, &["."]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(": [duplicate-code] duplicated block of 42 lines ("),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(" tokens), also found at "),
        "stdout: {stdout}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn sarif_format_reports_duplicates_in_lines_and_tokens() {
    let dir = temp_dir("sarif");
    duplicate_pair(&dir);

    let output = run_in(&dir, &["--format", "sarif", "."]);

    assert_eq!(output.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&output.stdout).expect("SARIF is JSON");
    let messages: Vec<&str> = json["runs"][0]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .filter(|result| result["ruleId"] == "duplicate-code")
        .map(|result| result["message"]["text"].as_str().expect("message text"))
        .collect();

    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(
        messages[0].starts_with("duplicated block of 42 lines ("),
        "message: {}",
        messages[0]
    );
    assert!(
        messages[0].contains(" tokens), also found at "),
        "message: {}",
        messages[0]
    );

    let _ = fs::remove_dir_all(dir);
}
