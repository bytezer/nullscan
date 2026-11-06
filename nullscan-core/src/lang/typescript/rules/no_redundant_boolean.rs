use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{query_rule, report_at};

/// Flags `x == true`, `x !== false`, etc. and `!!x`.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = comparison_rule(ctx);
    out.extend(query_rule(
        ctx,
        "no-redundant-boolean",
        "double negation ('!!') is redundant, use Boolean(...) or the value directly",
        r#"
            (unary_expression operator: "!" argument: (unary_expression operator: "!")) @report
        "#,
    ));
    out
}

/// ponytail: combining an `operator:` field constraint with another field constraint
/// (e.g. `left:`) on the same `binary_expression` pattern silently matches nothing in
/// this tree-sitter binding, even though the query compiles without error — confirmed
/// in isolation, see FINDINGS.md. So this matches on `left`/`right` alone and checks the
/// operator's text manually, same workaround as the other text-sensitive rules.
fn comparison_rule(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for field in ["left", "right"] {
        let query_src = format!("(binary_expression {field}: [(true) (false)]) @report");
        let query = Query::new(&ctx.tree.language(), &query_src)
            .expect("invalid no-redundant-boolean query");
        let report_idx = query.capture_index_for_name("report").unwrap();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
        while let Some(m) = matches.next() {
            let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
            let node = report.node;
            let operator = node
                .child_by_field_name("operator")
                .and_then(|op| op.utf8_text(ctx.source.as_bytes()).ok())
                .unwrap_or("");
            if !matches!(operator, "==" | "===" | "!=" | "!==") {
                continue;
            }
            out.push(report_at(
                "no-redundant-boolean",
                node,
                "remove the comparison to a boolean literal and use the value directly",
            ));
        }
    }
    out
}
