use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

/// Flags a `catch` block with no statements.
///
/// ponytail: the empty-block check needs a child count, which a query alone can't express.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let query = Query::new(
        &ctx.tree.language(),
        "(catch_clause body: (statement_block) @body) @report",
    )
    .expect("invalid no-empty-catch query");
    let body_idx = query.capture_index_for_name("body").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures().iter().find(|c| c.index == body_idx) {
            let body: Node = capture.node;
            if body.named_child_count() == 0 {
                out.push(report_at(
                    "no-empty-catch",
                    body,
                    "handle the error or document why it is safe to ignore",
                ));
            }
        }
    }
    out
}
