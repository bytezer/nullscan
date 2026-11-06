//! Converts core diagnostics (1-based byte positions) into LSP diagnostics.
//!
//! LSP ranges are 0-based lines and 0-based UTF-16 characters, so each column is converted
//! against the source line it belongs to. Out-of-range positions are clamped instead of
//! panicking: a diagnostic can never point outside the buffer.

use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range,
};
use nullscan_core::{Diagnostic, Severity, byte_col_to_utf16};

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
}
