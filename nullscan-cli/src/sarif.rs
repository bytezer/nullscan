//! Minimal SARIF 2.1.0 output (https://sarifweb.azurewebsites.net/), enough for GitHub
//! Code Scanning and editor SARIF viewers to render our diagnostics as inline
//! annotations. Only the fields those consumers actually read are modeled — SARIF has
//! plenty more optional structure we don't need yet.

use std::path::{Path, PathBuf};

use nullscan_core::{Diagnostic, Duplicate};
use serde::Serialize;

/// Short, rule-agnostic descriptions for the SARIF rule catalog. Diagnostic messages
/// carry the specific detail (a count, a name); this is the fixed "what is this rule"
/// blurb every occurrence shares.
pub(crate) const RULE_DESCRIPTIONS: &[(&str, &str)] = &[
    ("no-console", "Unexpected console call"),
    ("no-debugger", "Unexpected 'debugger' statement"),
    ("no-var", "Use 'let' or 'const' instead of 'var'"),
    ("eqeqeq", "Use '==='/'!==' instead of '=='/'!='"),
    ("no-explicit-any", "Avoid the explicit 'any' type"),
    ("no-empty-catch", "Empty 'catch' block swallows errors"),
    ("cognitive-complexity", "Function is too cognitively complex"),
    ("no-duplicate-string", "String literal duplicated across the file"),
    ("no-identical-conditions", "Branch condition duplicates an earlier one in the same chain"),
    ("no-redundant-boolean", "Redundant boolean comparison or double negation"),
    ("prefer-immediate-return", "Variable declared only to be returned immediately"),
    ("no-collapsible-if", "Nested 'if' can be merged with the outer condition"),
    ("duplicate-code", "Copy-pasted block of code"),
];

#[derive(Serialize)]
struct Log {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct Run {
    tool: Tool,
    results: Vec<ResultEntry>,
}

#[derive(Serialize)]
struct Tool {
    driver: Driver,
}

#[derive(Serialize)]
struct Driver {
    name: &'static str,
    version: &'static str,
    rules: Vec<Rule>,
}

#[derive(Serialize)]
struct Rule {
    id: &'static str,
    #[serde(rename = "shortDescription")]
    short_description: Text,
}

#[derive(Serialize)]
struct Text {
    text: &'static str,
}

#[derive(Serialize)]
struct ResultEntry {
    #[serde(rename = "ruleId")]
    rule_id: String,
    message: MessageText,
    locations: Vec<Location>,
    #[serde(rename = "relatedLocations", skip_serializing_if = "Vec::is_empty")]
    related_locations: Vec<Location>,
}

#[derive(Serialize)]
struct MessageText {
    text: String,
}

#[derive(Serialize)]
struct Location {
    #[serde(rename = "physicalLocation")]
    physical_location: PhysicalLocation,
}

#[derive(Serialize)]
struct PhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: ArtifactLocation,
    region: Region,
}

#[derive(Serialize)]
struct ArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
struct Region {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
}

fn location(file: &Path, line: usize, column: usize) -> Location {
    Location {
        physical_location: PhysicalLocation {
            artifact_location: ArtifactLocation { uri: file.to_string_lossy().replace('\\', "/") },
            region: Region { start_line: line, start_column: column },
        },
    }
}

/// Renders every file's diagnostics plus the cross-file duplicates as one SARIF log.
pub fn render(diagnostics: &[(PathBuf, Vec<Diagnostic>)], duplicates: &[Duplicate]) -> String {
    let mut results = Vec::new();

    for (file, file_diagnostics) in diagnostics {
        for d in file_diagnostics {
            results.push(ResultEntry {
                rule_id: d.rule.to_string(),
                message: MessageText { text: d.message.clone() },
                locations: vec![location(file, d.line, d.column)],
                related_locations: Vec::new(),
            });
        }
    }

    for dup in duplicates {
        results.push(ResultEntry {
            rule_id: "duplicate-code".to_string(),
            message: MessageText {
                text: format!(
                    "duplicated block of {} tokens, also found at {}:{}-{}",
                    dup.token_count,
                    dup.b.file.display(),
                    dup.b.start_line,
                    dup.b.end_line
                ),
            },
            locations: vec![location(&dup.a.file, dup.a.start_line, 1)],
            related_locations: vec![location(&dup.b.file, dup.b.start_line, 1)],
        });
    }

    let rules = RULE_DESCRIPTIONS
        .iter()
        .map(|(id, desc)| Rule { id, short_description: Text { text: desc } })
        .collect();

    let log = Log {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        version: "2.1.0",
        runs: vec![Run {
            tool: Tool {
                driver: Driver { name: "nullscan", version: env!("CARGO_PKG_VERSION"), rules },
            },
            results,
        }],
    };

    serde_json::to_string_pretty(&log).expect("SARIF log is always serializable")
}
