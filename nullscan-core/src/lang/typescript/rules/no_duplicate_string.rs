use std::collections::HashMap;

use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

/// Flags string literals repeated 3+ times in the same file (mirrors SonarJS S1192);
/// short strings are exempt since they're usually incidental (e.g. `""`, `"id"`), not
/// copy-pasted business text worth extracting into a constant.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let threshold = ctx.config.duplicate_string_threshold;
    let min_len = ctx.config.duplicate_string_min_length;
    let query = Query::new(&ctx.tree.language(), "(string) @report")
        .expect("invalid no-duplicate-string query");
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut occurrences: Vec<Node> = Vec::new();
    while let Some(m) = matches.next() {
        let capture = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        let text = capture.node.utf8_text(ctx.source.as_bytes()).unwrap_or("");
        // Strip the surrounding quotes before measuring length.
        if text.len().saturating_sub(2) >= min_len {
            occurrences.push(capture.node);
        }
    }
    occurrences.sort_by_key(|n| n.start_byte());

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for node in &occurrences {
        *counts
            .entry(node.utf8_text(ctx.source.as_bytes()).unwrap_or(""))
            .or_insert(0) += 1;
    }

    let mut seen_so_far: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for node in &occurrences {
        let text = node.utf8_text(ctx.source.as_bytes()).unwrap_or("");
        let total = counts[text];
        if total < threshold {
            continue;
        }
        let index = seen_so_far.entry(text).or_insert(0);
        *index += 1;
        if *index >= threshold {
            let start = node.start_position();
            out.push(Diagnostic {
                rule: "no-duplicate-string",
                line: start.row + 1,
                column: start.column + 1,
                message: format!(
                    "string literal {text} is duplicated {total} times, consider extracting a constant"
                ),
            });
        }
    }
    out
}
