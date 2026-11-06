//! Fixture-based regression harness for every file-scoped rule.
//!
//! Layout:
//!
//! ```text
//! tests/fixtures/<rule-id>/valid.ts    -> zero diagnostics for <rule-id>
//! tests/fixtures/<rule-id>/invalid.ts  -> at least one diagnostic for <rule-id>
//! ```
//!
//! `.tsx` fixtures are supported and parsed with the TSX grammar. Other rules may
//! legitimately fire on a fixture; only the rule named by the directory is asserted.

use std::fs;
use std::path::{Path, PathBuf};

use nullscan_core::{analyze, Config, Diagnostic, Language, RuleScope, RULES};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(fixtures_dir())
        .expect("fixtures directory is missing")
        .map(|entry| {
            entry
                .expect("failed to read fixture directory entry")
                .path()
        })
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read fixture entry").path())
        .filter(|path| path.is_file() && Language::from_path(path).is_some())
        .collect();
    files.sort();
    files
}

fn diagnostics_for(rule_id: &str, path: &Path) -> Vec<Diagnostic> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let language = Language::from_path(path)
        .unwrap_or_else(|| panic!("unsupported fixture extension: {}", path.display()));
    analyze(&source, language, &Config::default())
        .into_iter()
        .filter(|diagnostic| diagnostic.rule == rule_id)
        .collect()
}

/// `Some(true)` for `valid.*`, `Some(false)` for `invalid.*`, `None` otherwise.
fn fixture_kind(path: &Path) -> Option<bool> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("valid.") {
        Some(true)
    } else if name.starts_with("invalid.") {
        Some(false)
    } else {
        None
    }
}

#[test]
fn fixtures_match_expected_diagnostics() {
    for dir in fixture_dirs() {
        let rule_id = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("invalid fixture directory name: {}", dir.display()));

        let files = source_files(&dir);
        assert!(!files.is_empty(), "no source fixtures in {}", dir.display());

        for path in files {
            let Some(valid) = fixture_kind(&path) else {
                continue;
            };
            let diagnostics = diagnostics_for(rule_id, &path);
            let rule_ids: Vec<_> = diagnostics.iter().map(|d| d.rule).collect();
            if valid {
                assert!(
                    diagnostics.is_empty(),
                    "{} should be clean for '{}', got {:?}",
                    path.display(),
                    rule_id,
                    rule_ids
                );
            } else {
                assert!(
                    !diagnostics.is_empty(),
                    "{} should trigger '{}' but produced no diagnostics",
                    path.display(),
                    rule_id
                );
            }
        }
    }
}

/// Every rule row carries `<a id="<rule-id>"></a>` so `RULES.md#<rule-id>` stays a stable
/// GitHub anchor. The LSP builds `codeDescription` and hover links from the same pattern.
#[test]
fn every_rule_has_a_rules_md_anchor() {
    let catalog = include_str!("../../RULES.md");
    for rule in RULES {
        let anchor = format!("<a id=\"{}\"></a>", rule.meta.id);
        assert!(
            catalog.contains(&anchor),
            "RULES.md is missing the anchor {anchor} for rule '{}'",
            rule.meta.id
        );
    }
}

#[test]
fn every_file_rule_has_valid_and_invalid_fixtures() {
    for rule in RULES.iter().filter(|rule| rule.scope == RuleScope::File) {
        let dir = fixtures_dir().join(rule.meta.id);
        assert!(
            dir.is_dir(),
            "missing fixture directory for '{}': {}",
            rule.meta.id,
            dir.display()
        );

        let files = source_files(&dir);
        let has_fixture = |prefix: &str| {
            files.iter().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
            })
        };
        assert!(
            has_fixture("valid."),
            "'{}' has no valid.* fixture in {}",
            rule.meta.id,
            dir.display()
        );
        assert!(
            has_fixture("invalid."),
            "'{}' has no invalid.* fixture in {}",
            rule.meta.id,
            dir.display()
        );
    }
}
