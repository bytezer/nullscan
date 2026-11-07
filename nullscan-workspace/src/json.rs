//! The machine-readable JSON contract shared by `--format json` and the editor.
//!
//! Diagnostics are emitted as LSP-shaped ranges: 0-based lines and 0-based UTF-16
//! characters. Core diagnostics are 1-based byte positions, so each column is converted
//! against the line it belongs to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nullscan_core::{
    Diagnostic, Duplicate, DuplicationStats, Severity, byte_col_to_utf16, severity_of,
};
use serde::Serialize;

use crate::scan::{FileResult, ScanReport};

const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonReport {
    schema_version: u32,
    tool: ToolInfo,
    config: ConfigInfo,
    summary: SummaryInfo,
    diagnostics: Vec<DiagnosticEntry>,
    duplicates: Vec<DuplicateEntry>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ToolInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigInfo {
    path: Option<String>,
    disabled_rules: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SummaryInfo {
    files_scanned: usize,
    files_with_issues: usize,
    duration_ms: u64,
    by_severity: BySeverity,
    by_rule: Vec<RuleCount>,
    duplicates: usize,
    /// Line-density metric of the duplicate pass, so editors can show one percentage.
    duplication: DuplicationInfo,
    truncated: bool,
}

/// One percentage plus the raw counts behind it; `density` is already rounded to one
/// decimal, and the `duplicated*` fields include every file in the duplicate pass.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicationInfo {
    density: f64,
    duplicated_lines: usize,
    total_lines: usize,
    duplicated_blocks: usize,
    duplicated_files: usize,
    largest_block_lines: usize,
}

impl From<&DuplicationStats> for DuplicationInfo {
    fn from(stats: &DuplicationStats) -> Self {
        Self {
            density: stats.density(),
            duplicated_lines: stats.duplicated_lines,
            total_lines: stats.total_lines,
            duplicated_blocks: stats.duplicated_blocks,
            duplicated_files: stats.duplicated_files,
            largest_block_lines: stats.largest_block_lines,
        }
    }
}

#[derive(Default)]
struct BySeverity {
    error: usize,
    warning: usize,
    info: usize,
}

impl Serialize for BySeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("BySeverity", 3)?;
        state.serialize_field("error", &self.error)?;
        state.serialize_field("warning", &self.warning)?;
        state.serialize_field("info", &self.info)?;
        state.end()
    }
}

#[derive(Serialize)]
struct RuleCount {
    rule: &'static str,
    count: usize,
}

#[derive(Serialize)]
struct DiagnosticEntry {
    file: String,
    rule: &'static str,
    severity: &'static str,
    message: String,
    range: Range,
}

#[derive(Serialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Serialize)]
struct Position {
    line: usize,
    character: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateEntry {
    rule: &'static str,
    severity: &'static str,
    token_count: usize,
    line_count: usize,
    a: DuplicateSide,
    b: DuplicateSide,
}

/// A duplicate span keeps core's 1-based line numbers: unlike diagnostic ranges it is a
/// line range, not an LSP position.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateSide {
    file: String,
    start_line: usize,
    end_line: usize,
}

/// Renders a report as pretty JSON with a trailing newline.
pub fn render(report: &ScanReport) -> String {
    let document = JsonReport {
        schema_version: SCHEMA_VERSION,
        tool: ToolInfo {
            name: "nullscan",
            version: env!("CARGO_PKG_VERSION"),
        },
        config: ConfigInfo {
            path: report.config_path.as_deref().map(absolute_path),
            disabled_rules: report.disabled_rules.clone(),
        },
        summary: summarize(report),
        diagnostics: diagnostic_entries(report),
        duplicates: report.duplicates.iter().map(duplicate_entry).collect(),
        warnings: report.warnings.clone(),
    };

    let mut text =
        serde_json::to_string_pretty(&document).expect("scan report is always serializable");
    text.push('\n');
    text
}

/// The `summary` object of the JSON contract on its own, so the LSP can answer
/// `nullscan/analyzeWorkspace` with exactly what `--format json` shows under `summary`.
pub fn summary_json(report: &ScanReport) -> serde_json::Value {
    serde_json::to_value(summarize(report)).expect("the summary is always serializable")
}

fn summarize(report: &ScanReport) -> SummaryInfo {
    let mut files_with_issues = 0;
    let mut by_severity = BySeverity::default();
    let mut rule_counts: HashMap<&'static str, usize> = HashMap::new();

    for file in &report.files {
        if !file.diagnostics.is_empty() {
            files_with_issues += 1;
        }
        for diagnostic in &file.diagnostics {
            count_severity(&mut by_severity, diagnostic.severity);
            *rule_counts.entry(diagnostic.rule).or_insert(0) += 1;
        }
    }

    let mut by_rule: Vec<RuleCount> = rule_counts
        .into_iter()
        .map(|(rule, count)| RuleCount { rule, count })
        .collect();
    by_rule.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.rule.cmp(right.rule))
    });

    SummaryInfo {
        files_scanned: report.files.len(),
        files_with_issues,
        duration_ms: report.duration.as_millis() as u64,
        by_severity,
        by_rule,
        duplicates: report.duplicates.len(),
        duplication: DuplicationInfo::from(&report.duplication),
        truncated: report.truncated,
    }
}

fn count_severity(by_severity: &mut BySeverity, severity: Severity) {
    match severity {
        Severity::Error => by_severity.error += 1,
        Severity::Warning => by_severity.warning += 1,
        Severity::Info => by_severity.info += 1,
    }
}

fn diagnostic_entries(report: &ScanReport) -> Vec<DiagnosticEntry> {
    let mut entries = Vec::new();
    for file in &report.files {
        let lines: Vec<&str> = file.source.lines().collect();
        for diagnostic in &file.diagnostics {
            entries.push(diagnostic_entry(file, &lines, diagnostic));
        }
    }
    entries
}

fn diagnostic_entry(
    file: &FileResult,
    lines: &[&str],
    diagnostic: &Diagnostic,
) -> DiagnosticEntry {
    let start_line = diagnostic.line.saturating_sub(1);
    let end_line = diagnostic.end_line.saturating_sub(1);
    let start_text = lines.get(start_line).copied().unwrap_or("");
    let end_text = lines.get(end_line).copied().unwrap_or("");

    DiagnosticEntry {
        file: absolute_path(&file.file),
        rule: diagnostic.rule,
        severity: diagnostic.severity.as_str(),
        message: diagnostic.message.clone(),
        range: Range {
            start: Position {
                line: start_line,
                character: byte_col_to_utf16(start_text, diagnostic.column.saturating_sub(1)),
            },
            end: Position {
                line: end_line,
                character: byte_col_to_utf16(end_text, diagnostic.end_column.saturating_sub(1)),
            },
        },
    }
}

fn duplicate_entry(duplicate: &Duplicate) -> DuplicateEntry {
    DuplicateEntry {
        rule: "duplicate-code",
        severity: severity_of("duplicate-code").as_str(),
        token_count: duplicate.token_count,
        line_count: duplicate.line_count,
        a: duplicate_side(&duplicate.a),
        b: duplicate_side(&duplicate.b),
    }
}

fn duplicate_side(side: &nullscan_core::DuplicateLocation) -> DuplicateSide {
    DuplicateSide {
        file: absolute_path(&side.file),
        start_line: side.start_line,
        end_line: side.end_line,
    }
}

fn absolute_path(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
    absolute.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use serde_json::Value;

    use crate::scan::{FileResult, ScanReport};

    use nullscan_core::{Duplicate, DuplicateLocation, Language};

    fn diagnostic(
        rule: &'static str,
        severity: Severity,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Diagnostic {
        Diagnostic {
            rule,
            severity,
            line,
            column,
            end_line,
            end_column,
            message: "message".to_string(),
        }
    }

    fn file(name: &str, source: &str, diagnostics: Vec<Diagnostic>) -> FileResult {
        FileResult {
            file: PathBuf::from(name),
            language: Language::TypeScript,
            source: source.to_string(),
            diagnostics,
        }
    }

    fn report(files: Vec<FileResult>) -> ScanReport {
        ScanReport {
            files,
            duplicates: Vec::new(),
            duplication: DuplicationStats::default(),
            warnings: Vec::new(),
            duration: Duration::from_millis(12),
            truncated: false,
            cancelled: false,
            config_path: None,
            disabled_rules: vec!["no-var".to_string()],
        }
    }

    fn parse(report: &ScanReport) -> Value {
        serde_json::from_str(&render(report)).expect("valid JSON")
    }

    #[test]
    fn positions_are_zero_based_and_utf16() {
        let source = "const ñ = 1;\n";
        let diagnostic = diagnostic("no-console", Severity::Info, 1, 9, 1, 12);
        let json = parse(&report(vec![file("a.ts", source, vec![diagnostic])]));

        let entry = &json["diagnostics"][0];
        assert_eq!(entry["range"]["start"]["line"], 0);
        // "const ñ" is 8 bytes but 7 UTF-16 code units before the next character.
        assert_eq!(entry["range"]["start"]["character"], 7);
        assert_eq!(entry["range"]["end"]["line"], 0);
        assert_eq!(entry["range"]["end"]["character"], 10);
    }

    #[test]
    fn summary_counts_and_order_are_stable() {
        let files = vec![
            file(
                "a.ts",
                "a\n",
                vec![
                    diagnostic("no-var", Severity::Info, 1, 1, 1, 2),
                    diagnostic("eqeqeq", Severity::Warning, 1, 1, 1, 2),
                ],
            ),
            file(
                "b.ts",
                "b\n",
                vec![
                    diagnostic("no-var", Severity::Info, 1, 1, 1, 2),
                    diagnostic("eqeqeq", Severity::Warning, 1, 1, 1, 2),
                ],
            ),
        ];
        let json = parse(&report(files));

        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["tool"]["name"], "nullscan");
        assert_eq!(json["tool"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["config"]["path"], Value::Null);
        assert_eq!(json["config"]["disabledRules"][0], "no-var");

        let summary = &json["summary"];
        assert_eq!(summary["filesScanned"], 2);
        assert_eq!(summary["filesWithIssues"], 2);
        assert_eq!(summary["durationMs"], 12);
        assert_eq!(summary["bySeverity"]["error"], 0);
        assert_eq!(summary["bySeverity"]["warning"], 2);
        assert_eq!(summary["bySeverity"]["info"], 2);
        assert_eq!(summary["byRule"][0]["rule"], "eqeqeq");
        assert_eq!(summary["byRule"][0]["count"], 2);
        assert_eq!(summary["byRule"][1]["rule"], "no-var");
        assert_eq!(summary["duplicates"], 0);
        assert_eq!(summary["duplication"]["density"], 0.0);
        assert_eq!(summary["duplication"]["duplicatedLines"], 0);
        assert_eq!(summary["duplication"]["totalLines"], 0);
        assert_eq!(summary["duplication"]["duplicatedBlocks"], 0);
        assert_eq!(summary["duplication"]["duplicatedFiles"], 0);
        assert_eq!(summary["duplication"]["largestBlockLines"], 0);
        assert_eq!(summary["truncated"], false);
    }

    #[test]
    fn by_rule_breaks_ties_by_rule_id() {
        let files = vec![file(
            "a.ts",
            "a\n",
            vec![
                diagnostic("no-var", Severity::Info, 1, 1, 1, 2),
                diagnostic("eqeqeq", Severity::Warning, 1, 1, 1, 2),
            ],
        )];
        let json = parse(&report(files));

        assert_eq!(json["summary"]["byRule"][0]["rule"], "eqeqeq");
        assert_eq!(json["summary"]["byRule"][1]["rule"], "no-var");
    }

    #[test]
    fn severity_and_truncated_flags_are_serialized() {
        let files = vec![file(
            "a.ts",
            "a\n",
            vec![
                diagnostic("no-eval", Severity::Error, 1, 1, 1, 2),
                diagnostic("eqeqeq", Severity::Warning, 1, 1, 1, 2),
                diagnostic("no-console", Severity::Info, 1, 1, 1, 2),
            ],
        )];
        let mut report = report(files);
        report.truncated = true;
        let json = parse(&report);

        assert_eq!(json["diagnostics"][0]["severity"], "error");
        assert_eq!(json["diagnostics"][1]["severity"], "warning");
        assert_eq!(json["diagnostics"][2]["severity"], "info");
        assert_eq!(json["summary"]["truncated"], true);
        assert!(render(&report).ends_with('\n'));
    }

    #[test]
    fn summary_json_matches_the_embedded_summary_object() {
        let files = vec![file(
            "a.ts",
            "a\n",
            vec![diagnostic("no-console", Severity::Info, 1, 1, 1, 2)],
        )];
        let report = report(files);

        assert_eq!(summary_json(&report), parse(&report)["summary"]);
    }

    #[test]
    fn duplicates_use_the_registry_severity() {
        let mut report = report(vec![file("a.ts", "a\n", Vec::new())]);
        report.duplicates = vec![Duplicate::new(
            30,
            DuplicateLocation {
                file: PathBuf::from("a.ts"),
                start_line: 1,
                end_line: 3,
            },
            DuplicateLocation {
                file: PathBuf::from("b.ts"),
                start_line: 5,
                end_line: 7,
            },
        )];
        let json = parse(&report);

        assert_eq!(json["summary"]["duplicates"], 1);
        let duplicate = &json["duplicates"][0];
        assert_eq!(duplicate["rule"], "duplicate-code");
        assert_eq!(duplicate["severity"], "warning");
        assert_eq!(duplicate["tokenCount"], 30);
        assert_eq!(duplicate["lineCount"], 3);
        assert_eq!(duplicate["a"]["startLine"], 1);
        assert_eq!(duplicate["b"]["endLine"], 7);
    }

    #[test]
    fn summary_carries_the_duplication_metric() {
        let mut report = report(vec![file("a.ts", "a\n", Vec::new())]);
        report.duplication = DuplicationStats {
            duplicated_lines: 25,
            total_lines: 40,
            duplicated_blocks: 2,
            duplicated_files: 3,
            largest_block_lines: 12,
        };
        let json = parse(&report);
        let duplication = &json["summary"]["duplication"];

        assert_eq!(duplication["density"], 62.5);
        assert_eq!(duplication["duplicatedLines"], 25);
        assert_eq!(duplication["totalLines"], 40);
        assert_eq!(duplication["duplicatedBlocks"], 2);
        assert_eq!(duplication["duplicatedFiles"], 3);
        assert_eq!(duplication["largestBlockLines"], 12);
    }
}
