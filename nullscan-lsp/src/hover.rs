//! Rule hover: turns the published diagnostic under the cursor into Markdown.
//!
//! The server owns the published groups; this module only knows how a range contains a
//! position and how a diagnostic becomes a short, English hover.

use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Hover, HoverContents, MarkupContent,
    MarkupKind, NumberOrString, Position, Range,
};
use nullscan_core::RULES;

/// Builds the hover for the first diagnostic whose range contains `position`.
pub fn hover_at(diagnostics: &[LspDiagnostic], position: Position) -> Option<Hover> {
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| contains(diagnostic.range, position))?;
    Some(hover_for(diagnostic))
}

/// Start inclusive, end exclusive; a zero-width range matches only its start.
pub fn contains(range: Range, position: Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    range.start <= position && position < range.end
}

fn hover_for(diagnostic: &LspDiagnostic) -> Hover {
    let rule = rule_id(diagnostic).unwrap_or("nullscan");
    let description = description_of(rule).unwrap_or(&diagnostic.message);
    let severity = severity_label(diagnostic.severity);
    let message = &diagnostic.message;
    let value = format!("**{rule}** — {severity}\n\n{description}\n\n{message}");

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(diagnostic.range),
    }
}

fn rule_id(diagnostic: &LspDiagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => Some(code.as_str()),
        NumberOrString::Number(_) => None,
    }
}

fn description_of(rule: &str) -> Option<&'static str> {
    RULES
        .iter()
        .find(|registered| registered.meta.id == rule)
        .map(|registered| registered.meta.description)
}

fn severity_label(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        _ => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: Option<NumberOrString>, message: &str) -> LspDiagnostic {
        LspDiagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 4,
                },
                end: Position {
                    line: 0,
                    character: 9,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code,
            code_description: None,
            source: Some("nullscan".to_owned()),
            message: message.to_owned(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    fn value(hover: &Hover) -> &str {
        match &hover.contents {
            HoverContents::Markup(content) => &content.value,
            other => panic!("expected markdown contents, got {other:?}"),
        }
    }

    #[test]
    fn a_position_inside_the_range_hits() {
        let diagnostics = [
            diagnostic(
                Some(NumberOrString::String("no-console".to_owned())),
                "unexpected console call",
            ),
        ];
        let hover = hover_at(&diagnostics, Position {
            line: 0,
            character: 6,
        })
        .expect("hover");

        assert_eq!(
            hover.range,
            Some(diagnostics[0].range),
            "the hover highlights the diagnostic range"
        );
        assert_eq!(
            value(&hover),
            "**no-console** — warning\n\nUnexpected console call\n\nunexpected console call"
        );
    }

    #[test]
    fn positions_outside_the_range_miss() {
        let diagnostics = [diagnostic(
            Some(NumberOrString::String("no-console".to_owned())),
            "unexpected console call",
        )];

        assert!(hover_at(&diagnostics, Position {
            line: 0,
            character: 3,
        })
        .is_none());
        assert!(hover_at(&diagnostics, Position {
            line: 0,
            character: 9,
        })
        .is_none());
        assert!(hover_at(&diagnostics, Position {
            line: 1,
            character: 6,
        })
        .is_none());
    }

    #[test]
    fn a_zero_width_range_matches_only_its_start() {
        let mut zero_width = diagnostic(None, "message");
        zero_width.range.end = zero_width.range.start;

        assert!(contains(zero_width.range, zero_width.range.start));
        assert!(!contains(zero_width.range, Position {
            line: 0,
            character: 5,
        }));
    }

    #[test]
    fn an_unknown_rule_falls_back_to_the_message() {
        let diagnostics = [diagnostic(
            Some(NumberOrString::String("not-a-rule".to_owned())),
            "custom message",
        )];
        let hover = hover_at(&diagnostics, Position {
            line: 0,
            character: 5,
        })
        .expect("hover");

        assert_eq!(
            value(&hover),
            "**not-a-rule** — warning\n\ncustom message\n\ncustom message"
        );
    }
}
