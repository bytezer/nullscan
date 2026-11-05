use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

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
