//! `textDocument/codeAction`: quick fixes that disable a rule in `nullscan.toml`.
//!
//! The server owns the published diagnostics, the open buffers and the workspace folders;
//! this module only turns rule ids into actions and one config document into a workspace
//! edit. The edit is never applied here, it is returned to the client.

use lsp_types::{
    CodeAction, CodeActionKind, CreateFile, CreateFileOptions, Diagnostic as LspDiagnostic,
    DocumentChangeOperation, DocumentChanges, NumberOrString, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp, TextDocumentEdit,
    TextEdit, Uri, WorkspaceEdit,
};
use nullscan_core::byte_col_to_utf16;

use crate::hover::contains;

/// One rule to disable together with the diagnostic that asked for it.
pub struct RuleHit<'a> {
    pub rule: &'a str,
    pub diagnostic: &'a LspDiagnostic,
}

/// The `nullscan.toml` a quick fix will edit, as read from an open buffer or from disk.
pub struct ConfigDocument {
    pub uri: Uri,
    pub text: String,
    pub version: Option<i32>,
    /// Whether the file already exists on disk or as an open buffer.
    pub exists: bool,
}

/// Distinct rules in `diagnostics`, in first-seen order.
///
/// Diagnostics without a string `code` cannot name a rule and are skipped.
pub fn rule_hits(diagnostics: &[LspDiagnostic]) -> Vec<RuleHit<'_>> {
    let mut hits: Vec<RuleHit<'_>> = Vec::new();
    for diagnostic in diagnostics {
        let Some(rule) = rule_id(diagnostic) else {
            continue;
        };
        if hits.iter().any(|hit| hit.rule == rule) {
            continue;
        }
        hits.push(RuleHit { rule, diagnostic });
    }
    hits
}

/// The diagnostics that intersect `range`; a zero-width range is a cursor.
pub fn diagnostics_in_range<'a>(
    diagnostics: &'a [LspDiagnostic],
    range: Range,
) -> Vec<&'a LspDiagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| intersects(diagnostic.range, range))
        .collect()
}

/// Builds the quick fix that replaces the whole config document with `new_text`.
pub fn disable_rule_action(
    hit: &RuleHit<'_>,
    config: &ConfigDocument,
    new_text: String,
) -> CodeAction {
    CodeAction {
        title: title(hit.rule),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![hit.diagnostic.clone()]),
        edit: Some(workspace_edit(config, new_text)),
        is_preferred: Some(false),
        ..CodeAction::default()
    }
}

/// The whole-current-content range a replacement edit uses.
pub fn document_range(content: &str) -> Range {
    Range {
        start: Position::new(0, 0),
        end: document_end(content),
    }
}

/// The action title shown in the client's quick-fix menu.
fn title(rule: &str) -> String {
    format!("Disable {rule} in nullscan.toml")
}

fn workspace_edit(config: &ConfigDocument, new_text: String) -> WorkspaceEdit {
    let edit = TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier {
            uri: config.uri.clone(),
            version: config.version,
        },
        edits: vec![OneOf::Left(TextEdit::new(document_range(&config.text), new_text))],
    };
    let mut operations = Vec::new();
    if !config.exists {
        operations.push(DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
            uri: config.uri.clone(),
            options: Some(CreateFileOptions {
                overwrite: None,
                ignore_if_exists: Some(true),
            }),
            annotation_id: None,
        })));
    }
    operations.push(DocumentChangeOperation::Edit(edit));
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(operations)),
        ..WorkspaceEdit::default()
    }
}

/// Line and UTF-16 column one past the last character, so a trailing newline is covered.
fn document_end(content: &str) -> Position {
    let line = content.matches('\n').count() as u32;
    let last = content.rsplit('\n').next().unwrap_or_default();
    Position::new(line, byte_col_to_utf16(last, last.len()) as u32)
}

fn intersects(diagnostic: Range, requested: Range) -> bool {
    if requested.start == requested.end {
        return contains(diagnostic, requested.start);
    }
    requested.start <= diagnostic.end && diagnostic.start <= requested.end
}

fn rule_id(diagnostic: &LspDiagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => Some(code.as_str()),
        NumberOrString::Number(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(rule: Option<NumberOrString>, start: u32, end: u32) -> LspDiagnostic {
        LspDiagnostic {
            range: Range {
                start: Position::new(0, start),
                end: Position::new(0, end),
            },
            severity: None,
            code: rule,
            code_description: None,
            source: Some("nullscan".to_owned()),
            message: "message".to_owned(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    fn rule(name: &str) -> Option<NumberOrString> {
        Some(NumberOrString::String(name.to_owned()))
    }

    fn uri() -> Uri {
        "file:///repo/nullscan.toml".parse().expect("valid uri")
    }

    #[test]
    fn one_hit_per_distinct_rule_in_first_seen_order() {
        let diagnostics = [
            diagnostic(rule("no-console"), 0, 5),
            diagnostic(rule("no-var"), 6, 9),
            diagnostic(rule("no-console"), 10, 12),
        ];

        let hits = rule_hits(&diagnostics);

        let rules: Vec<&str> = hits.iter().map(|hit| hit.rule).collect();
        assert_eq!(rules, vec!["no-console", "no-var"]);
        assert_eq!(hits[0].diagnostic.range.start, Position::new(0, 0));
    }

    #[test]
    fn diagnostics_without_a_string_code_are_skipped() {
        let diagnostics = [
            diagnostic(None, 0, 5),
            diagnostic(Some(NumberOrString::Number(7)), 6, 9),
        ];

        assert!(rule_hits(&diagnostics).is_empty());
    }

    #[test]
    fn only_diagnostics_intersecting_the_range_are_kept() {
        let diagnostics = [
            diagnostic(rule("no-console"), 0, 5),
            diagnostic(rule("no-var"), 6, 10),
        ];

        let hits = diagnostics_in_range(&diagnostics, Range::new(
            Position::new(0, 4),
            Position::new(0, 7),
        ));

        let rules: Vec<&str> = hits
            .iter()
            .filter_map(|diagnostic| rule_id(diagnostic))
            .collect();
        assert_eq!(rules, vec!["no-console", "no-var"]);
    }

    #[test]
    fn a_cursor_matches_only_the_diagnostic_that_contains_it() {
        let diagnostics = [
            diagnostic(rule("no-console"), 0, 5),
            diagnostic(rule("no-var"), 6, 10),
        ];
        let cursor = Range::new(Position::new(0, 7), Position::new(0, 7));

        let hits = diagnostics_in_range(&diagnostics, cursor);

        assert_eq!(hits.len(), 1);
        assert_eq!(rule_id(hits[0]), Some("no-var"));
    }

    #[test]
    fn the_document_range_covers_a_trailing_newline() {
        assert_eq!(document_range(""), Range::new(
            Position::new(0, 0),
            Position::new(0, 0),
        ));
        assert_eq!(document_range("a\nb\n"), Range::new(
            Position::new(0, 0),
            Position::new(2, 0),
        ));
        assert_eq!(document_range("a\nb"), Range::new(
            Position::new(0, 0),
            Position::new(1, 1),
        ));
        assert_eq!(document_range("año\n"), Range::new(
            Position::new(0, 0),
            Position::new(1, 0),
        ));
    }

    #[test]
    fn an_existing_config_gets_only_the_text_edit() {
        let config = ConfigDocument {
            uri: uri(),
            text: "[rules]\n".to_owned(),
            version: Some(3),
            exists: true,
        };
        let hit = RuleHit {
            rule: "no-console",
            diagnostic: &diagnostic(rule("no-console"), 0, 5),
        };

        let new_text = "[rules]\ndisabled = [\"no-console\"]\n".to_owned();
        let action = disable_rule_action(&hit, &config, new_text);

        assert_eq!(action.title, "Disable no-console in nullscan.toml");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.is_preferred, Some(false));
        let edit = action.edit.expect("edit");
        let Some(DocumentChanges::Operations(operations)) = edit.document_changes else {
            panic!("expected document change operations");
        };
        assert_eq!(operations.len(), 1);
        assert!(matches!(operations[0], DocumentChangeOperation::Edit(_)));
    }

    #[test]
    fn a_missing_config_gets_a_create_file_operation() {
        let config = ConfigDocument {
            uri: uri(),
            text: String::new(),
            version: None,
            exists: false,
        };
        let hit = RuleHit {
            rule: "no-console",
            diagnostic: &diagnostic(rule("no-console"), 0, 5),
        };

        let action = disable_rule_action(&hit, &config, "[rules]\n".to_owned());

        let edit = action.edit.expect("edit");
        let Some(DocumentChanges::Operations(operations)) = edit.document_changes else {
            panic!("expected document change operations");
        };
        assert!(matches!(operations[0], DocumentChangeOperation::Op(ResourceOp::Create(_))));
        assert!(matches!(operations[1], DocumentChangeOperation::Edit(_)));
    }
}
