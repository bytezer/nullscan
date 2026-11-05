use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

/// Flags calls whose object is the `console` identifier.
///
/// ponytail: `#eq?` text predicates aren't evaluated by `QueryCursor::matches`, so the
/// object text is checked manually — see `no_explicit_any` and FINDINGS.md.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let query = Query::new(
        &ctx.tree.language(),
        r#"
        (call_expression
          function: (member_expression
            object: (identifier) @obj)) @report
        "#,
    )
    .expect("invalid no-console query");
    let obj_idx = query.capture_index_for_name("obj").unwrap();
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let obj = m.captures().iter().find(|c| c.index == obj_idx).unwrap();
        if obj.node.utf8_text(ctx.source.as_bytes()).unwrap_or("") != "console" {
            continue;
        }
        let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        let start = report.node.start_position();
        out.push(Diagnostic {
            rule: "no-console",
            line: start.row + 1,
            column: start.column + 1,
            message: "unexpected console call".to_string(),
        });
    }
    out
}
