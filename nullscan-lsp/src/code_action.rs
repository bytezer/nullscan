//! `textDocument/codeAction`: quick fixes that disable a rule in `nullscan.toml` or
//! suppress it with an inline comment.
//!
//! The server owns the published diagnostics, the open buffers and the workspace folders;
//! this module only turns rule ids into actions and one document into a workspace edit.
//! The edits are never applied here, they are returned to the client.

use lsp_types::{
    CodeAction, CodeActionKind, CreateFile, CreateFileOptions, Diagnostic as LspDiagnostic,
    DocumentChangeOperation, DocumentChanges, NumberOrString, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp, TextDocumentEdit,
    TextEdit, Uri, WorkspaceEdit,
};
use nullscan_core::byte_col_to_utf16;

use crate::hover::contains;

/// The marker core recognizes for suppressing the line after the comment.
const NEXT_LINE_MARKER: &str = "nullscan-ignore-next-line";

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

/// The source document an inline-suppression quick fix will edit.
pub struct TargetDocument<'a> {
    pub uri: &'a Uri,
    pub text: &'a str,
    pub version: Option<i32>,
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
        title: disable_title(hit.rule),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![hit.diagnostic.clone()]),
        edit: Some(workspace_edit(config, new_text)),
        is_preferred: Some(false),
        ..CodeAction::default()
    }
}

/// Builds the quick fix that adds `rule` to a `nullscan-ignore-next-line` comment above
/// the diagnostic.
///
/// There is nothing to offer when the previous line already suppresses every rule (the
/// diagnostic could not have been reported) or already names this rule.
pub fn suppress_action(hit: &RuleHit<'_>, target: &TargetDocument<'_>) -> Option<CodeAction> {
    let edit = suppression_edit(hit.rule, target.text, hit.diagnostic.range.start.line)?;
    Some(CodeAction {
        title: suppress_title(hit.rule),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![hit.diagnostic.clone()]),
        edit: Some(document_edit(target, edit)),
        is_preferred: Some(false),
        ..CodeAction::default()
    })
}

/// The single insertion that makes `rule` suppressed on the 0-based `line`.
///
/// An existing marker on the preceding line grows its rule list; otherwise a new comment
/// line is inserted with the diagnostic line's indentation and the document's newline.
pub fn suppression_edit(rule: &str, text: &str, line: u32) -> Option<TextEdit> {
    if let Some(previous) = previous_line(text, line) {
        let comments = next_line_comments(previous);
        let already_suppressed = comments
            .iter()
            .any(|comment| comment.suppresses_every_rule() || comment.lists(rule));
        if already_suppressed {
            return None;
        }
        if let Some(comment) = comments.first() {
            let column = byte_col_to_utf16(previous, comment.append_at) as u32;
            let position = Position::new(line - 1, column);
            return Some(TextEdit::new(
                Range::new(position, position),
                format!(", {rule}"),
            ));
        }
    }
    Some(fresh_insertion(rule, text, line))
}

/// The whole-current-content range a replacement edit uses.
pub fn document_range(content: &str) -> Range {
    Range {
        start: Position::new(0, 0),
        end: document_end(content),
    }
}

/// The action title shown in the client's quick-fix menu.
fn disable_title(rule: &str) -> String {
    format!("Disable {rule} in nullscan.toml")
}

/// The suppress action title shown in the client's quick-fix menu.
fn suppress_title(rule: &str) -> String {
    format!("Suppress {rule} on the next line")
}

fn fresh_insertion(rule: &str, text: &str, line: u32) -> TextEdit {
    let indent = line_at(text, line)
        .map(leading_whitespace)
        .unwrap_or_default();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let position = Position::new(line, 0);
    TextEdit::new(
        Range::new(position, position),
        format!("{indent}// {NEXT_LINE_MARKER} {rule}{newline}"),
    )
}

fn previous_line(text: &str, line: u32) -> Option<&str> {
    if line == 0 {
        return None;
    }
    text.split('\n').nth(line as usize - 1)
}

fn line_at(text: &str, line: u32) -> Option<&str> {
    text.split('\n').nth(line as usize)
}

fn leading_whitespace(line: &str) -> &str {
    let trimmed = line.trim_start_matches([' ', '\t']);
    &line[..line.len() - trimmed.len()]
}

/// One `nullscan-ignore-next-line` comment found on a line.
struct ExistingComment {
    /// Rule names written after the marker; empty means every rule.
    rules: Vec<String>,
    /// Byte offset in the line where `, <rule>` can be appended.
    append_at: usize,
}

impl ExistingComment {
    fn suppresses_every_rule(&self) -> bool {
        self.rules.is_empty()
    }

    fn lists(&self, rule: &str) -> bool {
        self.rules.iter().any(|candidate| candidate == rule)
    }
}

#[derive(Clone, Copy)]
enum CommentKind {
    Line,
    Block,
}

/// Every `nullscan-ignore-next-line` comment on `line`, in source order.
///
/// This is deliberately line-based: only a comment that ends on the previous line targets
/// the diagnostic line, so a block comment spanning several lines is not considered.
fn next_line_comments(line: &str) -> Vec<ExistingComment> {
    let mut comments = Vec::new();
    let mut from = 0;
    while let Some((start, kind)) = next_comment_start(line, from) {
        match kind {
            CommentKind::Line => {
                if let Some(comment) = line_comment(&line[start..], line) {
                    comments.push(comment);
                }
                break;
            }
            CommentKind::Block => match block_comment(&line[start..], start, line) {
                Some((comment, end)) => {
                    comments.push(comment);
                    from = end;
                }
                None => from = start + 2,
            },
        }
    }
    comments
}

/// The earliest `//` or `/*` at or after `from`.
fn next_comment_start(line: &str, from: usize) -> Option<(usize, CommentKind)> {
    let line_comment = line[from..]
        .find("//")
        .map(|index| (from + index, CommentKind::Line));
    let block_comment = line[from..]
        .find("/*")
        .map(|index| (from + index, CommentKind::Block));
    match (line_comment, block_comment) {
        (Some(line_comment), Some(block_comment)) => Some(if line_comment.0 <= block_comment.0 {
            line_comment
        } else {
            block_comment
        }),
        (line_comment, block_comment) => line_comment.or(block_comment),
    }
}

/// A `//` comment that starts with the marker and runs to the end of the line.
fn line_comment(rest: &str, line: &str) -> Option<ExistingComment> {
    let body = rest.strip_prefix("//")?.trim_start();
    let rules_text = strip_marker(body)?;
    Some(ExistingComment {
        rules: rule_names(rules_text),
        append_at: line.trim_end().len(),
    })
}

/// A closed `/* */` comment that starts with the marker, plus the offset after `*/`.
fn block_comment(rest: &str, start: usize, line: &str) -> Option<(ExistingComment, usize)> {
    let body = rest.strip_prefix("/*")?;
    let close = body.find("*/")?;
    let rules_text = strip_marker(body[..close].trim_start())?;
    let close_start = start + 2 + close;
    let comment = ExistingComment {
        rules: rule_names(rules_text),
        append_at: line[..close_start].trim_end().len(),
    };
    Some((comment, close_start + 2))
}

/// The text after `NEXT_LINE_MARKER`, or `None` when the marker is absent or continues
/// into another word (for example `nullscan-ignore-next-lines`).
fn strip_marker(body: &str) -> Option<&str> {
    let rest = body.strip_prefix(NEXT_LINE_MARKER)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(next) if !is_word_char(next) => Some(rest),
        Some(_) => None,
    }
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}

fn rule_names(rest: &str) -> Vec<String> {
    rest.split(|character: char| character == ',' || character.is_whitespace())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn workspace_edit(config: &ConfigDocument, new_text: String) -> WorkspaceEdit {
    let edit = TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier {
            uri: config.uri.clone(),
            version: config.version,
        },
        edits: vec![OneOf::Left(TextEdit::new(
            document_range(&config.text),
            new_text,
        ))],
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

/// A single insertion in the document the client already has open.
fn document_edit(target: &TargetDocument<'_>, edit: TextEdit) -> WorkspaceEdit {
    let text_document = OptionalVersionedTextDocumentIdentifier {
        uri: target.uri.clone(),
        version: target.version,
    };
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
            text_document,
            edits: vec![OneOf::Left(edit)],
        }])),
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

    fn diagnostic_at(name: &str, line: u32) -> LspDiagnostic {
        LspDiagnostic {
            range: Range::new(Position::new(line, 0), Position::new(line, 5)),
            ..diagnostic(rule(name), 0, 5)
        }
    }

    fn rule(name: &str) -> Option<NumberOrString> {
        Some(NumberOrString::String(name.to_owned()))
    }

    fn config_uri() -> Uri {
        "file:///repo/nullscan.toml".parse().expect("valid uri")
    }

    fn source_uri() -> Uri {
        "file:///repo/a.ts".parse().expect("valid uri")
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
            uri: config_uri(),
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
            uri: config_uri(),
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

    #[test]
    fn a_fresh_suppression_keeps_the_diagnostic_indentation() {
        let text = "function f() {\n  console.log(x);\n}\n";

        let edit = suppression_edit("no-console", text, 1).expect("fresh insertion");

        assert_eq!(edit.range, Range::new(Position::new(1, 0), Position::new(1, 0)));
        assert_eq!(edit.new_text, "  // nullscan-ignore-next-line no-console\n");
    }

    #[test]
    fn appending_to_a_line_comment_keeps_the_existing_rules() {
        let text = "// nullscan-ignore-next-line no-console\nconsole.log(x);\n";
        let column = "// nullscan-ignore-next-line no-console".len() as u32;

        let edit = suppression_edit("no-var", text, 1).expect("append to line comment");

        assert_eq!(
            edit.range,
            Range::new(Position::new(0, column), Position::new(0, column))
        );
        assert_eq!(edit.new_text, ", no-var");
    }

    #[test]
    fn appending_to_a_block_comment_inserts_before_the_closing_delimiter() {
        let text = "  /* nullscan-ignore-next-line no-console */\nconsole.log(x);\n";
        let column = "  /* nullscan-ignore-next-line no-console".len() as u32;

        let edit = suppression_edit("no-var", text, 1).expect("append to block comment");

        assert_eq!(
            edit.range,
            Range::new(Position::new(0, column), Position::new(0, column))
        );
        assert_eq!(edit.new_text, ", no-var");
    }

    #[test]
    fn a_crlf_document_inserts_a_crlf_line() {
        let text = "const a = 1;\r\nconsole.log(x);\r\n";

        let edit = suppression_edit("no-console", text, 1).expect("crlf insertion");

        assert_eq!(edit.range, Range::new(Position::new(1, 0), Position::new(1, 0)));
        assert_eq!(edit.new_text, "// nullscan-ignore-next-line no-console\r\n");
    }

    #[test]
    fn an_existing_all_rules_suppression_offers_no_action() {
        let text = "// nullscan-ignore-next-line\nconsole.log(x);\n";

        assert!(suppression_edit("no-console", text, 1).is_none());
    }

    #[test]
    fn a_rule_already_listed_offers_no_action() {
        let text = "  /* nullscan-ignore-next-line no-console */\nconsole.log(x);\n";

        assert!(suppression_edit("no-console", text, 1).is_none());
    }

    #[test]
    fn the_suppress_action_targets_the_source_document() {
        let diagnostic = diagnostic_at("no-console", 1);
        let hit = RuleHit {
            rule: "no-console",
            diagnostic: &diagnostic,
        };
        let uri = source_uri();
        let target = TargetDocument {
            uri: &uri,
            text: "function f() {\n  console.log(x);\n}\n",
            version: Some(4),
        };

        let action = suppress_action(&hit, &target).expect("suppress action");

        assert_eq!(action.title, "Suppress no-console on the next line");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.is_preferred, Some(false));
        assert_eq!(
            action.diagnostics.as_ref().map(Vec::len),
            Some(1),
            "the action carries the triggering diagnostic"
        );
        let edit = action.edit.expect("edit");
        let Some(DocumentChanges::Edits(edits)) = edit.document_changes else {
            panic!("expected text document edits");
        };
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].text_document.uri, uri);
        assert_eq!(edits[0].text_document.version, Some(4));
    }
}
