use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

/// Flags `any` used as an explicit type.
///
/// ponytail: `#eq?` text predicates aren't evaluated by `QueryCursor::matches`, so the
/// captured type's text is checked manually instead of constraining it in the query.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let query = Query::new(
        &ctx.tree.language(),
        "(predefined_type) @report",
    )
    .expect("invalid no-explicit-any query");
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        if report.node.utf8_text(ctx.source.as_bytes()).unwrap_or("") != "any" {
            continue;
        }
        let start = report.node.start_position();
        out.push(Diagnostic {
            rule: "no-explicit-any",
            line: start.row + 1,
            column: start.column + 1,
            message: "unexpected 'any' type".to_string(),
        });
    }
    out
}
