use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::{FileContext, severity_of};

/// Source text for a node. Invalid UTF-8 only happens for malformed source, so an empty
/// string is a safe fallback and keeps the text-sensitive rules from panicking.
pub(crate) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Inner text of a static string-like literal: a `string` literal or a `template_string`
/// with no substitutions. Returns `None` for dynamic templates and every other node kind.
/// The surrounding quote/backtick delimiters are stripped but escape sequences are left
/// untouched, so callers see the raw source text rather than a decoded runtime value.
pub(crate) fn static_string_text<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    let text = match node.kind() {
        "string" => node_text(node, source),
        "template_string" if !has_template_substitution(node) => node_text(node, source),
        _ => return None,
    };
    Some(strip_delimiters(text))
}

fn has_template_substitution(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "template_substitution")
}

fn strip_delimiters(text: &str) -> &str {
    let text = text
        .strip_prefix('"')
        .or_else(|| text.strip_prefix('\''))
        .or_else(|| text.strip_prefix('`'))
        .unwrap_or(text);
    text.strip_suffix('"')
        .or_else(|| text.strip_suffix('\''))
        .or_else(|| text.strip_suffix('`'))
        .unwrap_or(text)
}

/// Named children with comments removed. tree-sitter exposes comments as named nodes, so
/// child-count and "last statement" checks must filter them explicitly.
pub(crate) fn named_children_without_comments<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect()
}

/// Builds a diagnostic covering a node's full 1-based byte range.
///
/// This is the single place that stamps the level and end position, so no rule can forget
/// them.
pub(crate) fn report_at(
    rule: &'static str,
    node: Node<'_>,
    message: impl Into<String>,
) -> Diagnostic {
    let start = node.start_position();
    let end = node.end_position();
    Diagnostic {
        rule,
        severity: severity_of(rule),
        line: start.row + 1,
        column: start.column + 1,
        end_line: end.row + 1,
        end_column: end.column + 1,
        message: message.into(),
    }
}

/// Builds a diagnostic for a byte range stored outside the tree, such as the offsets kept
/// in the scope graph.
///
/// The range is resolved back to the smallest node covering it so the diagnostic still
/// carries a real end position. Returns `None` if the range is not inside the tree, which
/// cannot happen when the graph and the tree come from the same source.
pub(crate) fn report_at_range(
    ctx: &FileContext<'_>,
    rule: &'static str,
    start_byte: usize,
    end_byte: usize,
    message: impl Into<String>,
) -> Option<Diagnostic> {
    let node = ctx
        .tree
        .root_node()
        .descendant_for_byte_range(start_byte, end_byte)?;
    Some(report_at(rule, node, message))
}

/// Runs a tree-sitter query and reports one diagnostic per match at the `@report` capture.
pub(crate) fn query_rule(
    ctx: &FileContext<'_>,
    rule: &'static str,
    message: &str,
    query_src: &str,
) -> Vec<Diagnostic> {
    let query = Query::new(&ctx.tree.language(), query_src)
        .unwrap_or_else(|e| panic!("invalid query for rule {rule}: {e}"));
    let report_idx = query
        .capture_index_for_name("report")
        .unwrap_or_else(|| panic!("rule {rule} query is missing a @report capture"));

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures().iter().find(|c| c.index == report_idx) {
            out.push(report_at(rule, capture.node, message));
        }
    }
    out
}
