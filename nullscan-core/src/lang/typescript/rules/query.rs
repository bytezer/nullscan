use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

/// Source text for a node. Invalid UTF-8 only happens for malformed source, so an empty
/// string is a safe fallback and keeps the text-sensitive rules from panicking.
pub(crate) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Named children with comments removed. tree-sitter exposes comments as named nodes, so
/// child-count and "last statement" checks must filter them explicitly.
pub(crate) fn named_children_without_comments<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect()
}

/// Builds a diagnostic at a node's 1-based start position.
pub(crate) fn report_at(
    rule: &'static str,
    node: Node<'_>,
    message: impl Into<String>,
) -> Diagnostic {
    let start = node.start_position();
    Diagnostic {
        rule,
        line: start.row + 1,
        column: start.column + 1,
        message: message.into(),
    }
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
            let start = capture.node.start_position();
            out.push(Diagnostic {
                rule,
                line: start.row + 1,
                column: start.column + 1,
                message: message.to_string(),
            });
        }
    }
    out
}
