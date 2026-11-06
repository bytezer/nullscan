//! Converts core diagnostics (1-based byte positions) into LSP diagnostics.
//!
//! LSP ranges are 0-based lines and 0-based UTF-16 characters, so each column is converted
//! against the source line it belongs to. Out-of-range positions are clamped instead of
//! panicking: a diagnostic can never point outside the buffer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    NumberOrString, Position, Range,
};
use nullscan_core::{
    Diagnostic, DuplicateLocation, Severity, byte_col_to_utf16, severity_of,
};
use nullscan_workspace::ScanReport;

use crate::uri::from_file_path;

/// The project-scoped rule that emits one diagnostic per duplicated side.
const DUPLICATE_CODE_RULE: &str = "duplicate-code";

/// Maps one core diagnostic onto an LSP diagnostic for `source`.
pub fn to_lsp_diagnostic(source: &str, diagnostic: &Diagnostic) -> LspDiagnostic {
    LspDiagnostic {
        range: to_range(source, diagnostic),
        severity: Some(severity(diagnostic.severity)),
        code: Some(NumberOrString::String(diagnostic.rule.to_owned())),
        code_description: None,
        source: Some("nullscan".to_owned()),
        message: diagnostic.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Builds a 0-based UTF-16 range from a diagnostic's 1-based byte positions.
pub fn to_range(source: &str, diagnostic: &Diagnostic) -> Range {
    let lines = source_lines(source);
    let start = position_at(&lines, diagnostic.line.saturating_sub(1), diagnostic.column);
    let end_line = diagnostic
        .end_line
        .saturating_sub(1)
        .max(start.line as usize);
    let end = position_at(&lines, end_line, diagnostic.end_column);
    Range { start, end }
}

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

/// Turns every duplicate into one diagnostic per side, grouped by the file that receives
/// it. Both copies are flagged and linked to each other through `relatedInformation`.
pub fn duplicate_diagnostics(report: &ScanReport) -> HashMap<PathBuf, Vec<LspDiagnostic>> {
    let sources: HashMap<&Path, &str> = report
        .files
        .iter()
        .map(|file| (file.file.as_path(), file.source.as_str()))
        .collect();
    let mut grouped: HashMap<PathBuf, Vec<LspDiagnostic>> = HashMap::new();

    for duplicate in &report.duplicates {
        for (side, other) in [
            (&duplicate.a, &duplicate.b),
            (&duplicate.b, &duplicate.a),
        ] {
            let (Some(source), Some(other_source)) = (
                sources.get(side.file.as_path()),
                sources.get(other.file.as_path()),
            ) else {
                continue;
            };
            let diagnostic = duplicate_diagnostic(
                duplicate.token_count,
                DuplicateSide {
                    location: side,
                    source,
                },
                DuplicateSide {
                    location: other,
                    source: other_source,
                },
            );
            grouped
                .entry(side.file.clone())
                .or_default()
                .push(diagnostic);
        }
    }

    grouped
}

/// One end of a duplicate together with the source that gives its line its UTF-16 length.
struct DuplicateSide<'a> {
    location: &'a DuplicateLocation,
    source: &'a str,
}

fn duplicate_diagnostic(
    token_count: usize,
    side: DuplicateSide<'_>,
    other: DuplicateSide<'_>,
) -> LspDiagnostic {
    let message = duplicate_message(token_count, other.location);
    LspDiagnostic {
        range: duplicate_range(side.source, side.location),
        severity: Some(severity(severity_of(DUPLICATE_CODE_RULE))),
        code: Some(NumberOrString::String(DUPLICATE_CODE_RULE.to_owned())),
        code_description: None,
        source: Some("nullscan".to_owned()),
        message: message.clone(),
        related_information: Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: from_file_path(&other.location.file),
                range: duplicate_range(other.source, other.location),
            },
            message,
        }]),
        tags: None,
        data: None,
    }
}

fn duplicate_message(token_count: usize, other: &DuplicateLocation) -> String {
    format!(
        "duplicated block of {token_count} tokens, also found at {}:{}-{}",
        other.file.display(),
        other.start_line,
        other.end_line
    )
}

/// The whole spanned lines, from character 0 to the UTF-16 length of the end line.
fn duplicate_range(source: &str, location: &DuplicateLocation) -> Range {
    let lines = source_lines(source);
    let last = lines.len() - 1;
    let start = location.start_line.saturating_sub(1).min(last);
    let end = location.end_line.saturating_sub(1).min(last).max(start);
    let end_text = lines[end];

    Range {
        start: Position {
            line: start as u32,
            character: 0,
        },
        end: Position {
            line: end as u32,
            character: byte_col_to_utf16(end_text, end_text.len()) as u32,
        },
    }
}

/// Source lines with a guaranteed non-empty result, so index clamping always has a line.
fn source_lines(source: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        lines.push("");
    }
    lines
}

/// Converts one 1-based `line`/`column` pair, clamping the line to the buffer and the
/// column to the line's length via [`byte_col_to_utf16`].
fn position_at(lines: &[&str], line: usize, column: usize) -> Position {
    let index = line.min(lines.len() - 1);
    Position {
        line: index as u32,
        character: byte_col_to_utf16(lines[index], column.saturating_sub(1)) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nullscan_core::Duplicate;

    fn diagnostic(
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Diagnostic {
        Diagnostic {
            rule: "no-console",
            severity: Severity::Info,
            line,
            column,
            end_line,
            end_column,
            message: "unexpected console call".to_owned(),
        }
    }

    #[test]
    fn ascii_positions_keep_the_same_columns() {
        let source = "console.log(x);\n";
        let range = to_range(source, &diagnostic(1, 1, 1, 15));

        assert_eq!(range.start, Position { line: 0, character: 0 });
        assert_eq!(range.end, Position { line: 0, character: 14 });
    }

    #[test]
    fn accented_characters_shift_columns_by_one_code_unit() {
        // "const ñ" is 8 bytes but 7 UTF-16 code units before the next character.
        let source = "const ñ = 1;\n";
        let range = to_range(source, &diagnostic(1, 9, 1, 12));

        assert_eq!(range.start, Position { line: 0, character: 7 });
        assert_eq!(range.end, Position { line: 0, character: 10 });
    }

    #[test]
    fn emoji_before_the_diagnostic_count_two_code_units() {
        // The emoji is four bytes and two UTF-16 code units, so "console" starts at
        // byte 6 but at character 4.
        let source = "😀; console.log(1);\n";
        let range = to_range(source, &diagnostic(1, 7, 1, 21));

        assert_eq!(range.start, Position { line: 0, character: 4 });
        assert_eq!(range.end, Position { line: 0, character: 18 });
    }

    #[test]
    fn a_different_end_line_uses_that_line_text() {
        let source = "const x = 1;\nconsole.log(x);\n";
        let range = to_range(source, &diagnostic(1, 1, 2, 15));

        assert_eq!(range.start, Position { line: 0, character: 0 });
        assert_eq!(range.end, Position { line: 1, character: 14 });
    }

    #[test]
    fn out_of_range_positions_are_clamped() {
        let source = "a\nb\n";
        let range = to_range(source, &diagnostic(9, 5, 20, 999));

        assert_eq!(range.start, Position { line: 1, character: 1 });
        assert_eq!(range.end, Position { line: 1, character: 1 });
    }

    #[test]
    fn empty_source_maps_to_origin() {
        let range = to_range("", &diagnostic(1, 1, 1, 2));

        assert_eq!(range.start, Position { line: 0, character: 0 });
        assert_eq!(range.end, Position { line: 0, character: 0 });
    }

    #[test]
    fn severity_and_metadata_are_mapped() {
        let source = "a\n";
        let info = to_lsp_diagnostic(source, &diagnostic(1, 1, 1, 2));
        assert_eq!(info.severity, Some(DiagnosticSeverity::INFORMATION));
        assert_eq!(info.source.as_deref(), Some("nullscan"));
        assert_eq!(
            info.code,
            Some(NumberOrString::String("no-console".to_owned()))
        );

        let mut error = diagnostic(1, 1, 1, 2);
        error.severity = Severity::Error;
        assert_eq!(
            to_lsp_diagnostic(source, &error).severity,
            Some(DiagnosticSeverity::ERROR)
        );

        let mut warning = diagnostic(1, 1, 1, 2);
        warning.severity = Severity::Warning;
        assert_eq!(
            to_lsp_diagnostic(source, &warning).severity,
            Some(DiagnosticSeverity::WARNING)
        );
    }

    fn duplicate_report() -> ScanReport {
        ScanReport {
            files: vec![
                nullscan_workspace::FileResult {
                    file: PathBuf::from("/repo/a.ts"),
                    language: nullscan_core::Language::TypeScript,
                    source: "one\ntwo\nthree\nfour\n".to_owned(),
                    diagnostics: Vec::new(),
                },
                nullscan_workspace::FileResult {
                    file: PathBuf::from("/repo/b.ts"),
                    language: nullscan_core::Language::TypeScript,
                    source: "alpha\nbeta\ngamma\ndelta\n".to_owned(),
                    diagnostics: Vec::new(),
                },
            ],
            duplicates: vec![Duplicate {
                token_count: 42,
                a: DuplicateLocation {
                    file: PathBuf::from("/repo/a.ts"),
                    start_line: 2,
                    end_line: 4,
                },
                b: DuplicateLocation {
                    file: PathBuf::from("/repo/b.ts"),
                    start_line: 1,
                    end_line: 3,
                },
            }],
            warnings: Vec::new(),
            duration: std::time::Duration::ZERO,
            truncated: false,
            cancelled: false,
            config_path: None,
            disabled_rules: Vec::new(),
        }
    }

    #[test]
    fn duplicate_diagnostics_cover_both_sides_symmetrically() {
        let grouped = duplicate_diagnostics(&duplicate_report());
        let a = &grouped[Path::new("/repo/a.ts")][0];
        let b = &grouped[Path::new("/repo/b.ts")][0];

        assert_eq!(a.range.start, Position { line: 1, character: 0 });
        assert_eq!(a.range.end, Position { line: 3, character: 4 });
        assert_eq!(b.range.start, Position { line: 0, character: 0 });
        assert_eq!(b.range.end, Position { line: 2, character: 5 });

        assert_eq!(
            a.code,
            Some(NumberOrString::String("duplicate-code".to_owned()))
        );
        assert_eq!(a.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(a.source.as_deref(), Some("nullscan"));
        assert_eq!(
            a.message,
            "duplicated block of 42 tokens, also found at /repo/b.ts:1-3"
        );

        let related = &a.related_information.as_ref().expect("related information")[0];
        assert_eq!(related.location.uri, from_file_path(Path::new("/repo/b.ts")));
        assert_eq!(related.location.range, b.range);
        assert_eq!(related.message, a.message);

        let back = &b.related_information.as_ref().expect("related information")[0];
        assert_eq!(back.location.uri, from_file_path(Path::new("/repo/a.ts")));
        assert_eq!(back.location.range, a.range);
        assert_eq!(
            b.message,
            "duplicated block of 42 tokens, also found at /repo/a.ts:2-4"
        );
    }

    #[test]
    fn duplicate_ranges_clamp_to_the_source_lines() {
        let mut report = duplicate_report();
        report.duplicates[0].a.start_line = 50;
        report.duplicates[0].a.end_line = 90;

        let grouped = duplicate_diagnostics(&report);
        let a = &grouped[Path::new("/repo/a.ts")][0];

        assert_eq!(a.range.start, Position { line: 3, character: 0 });
        assert_eq!(a.range.end, Position { line: 3, character: 4 });
    }
}
