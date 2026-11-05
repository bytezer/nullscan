//! Flags a `switch` with fewer than `Config::no_small_switch_min_cases` `case` clauses.
//!
//! `default` clauses are not counted. A small switch is usually clearer as an `if`/`else`
//! chain.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let threshold = ctx.config.no_small_switch_min_cases;
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), threshold, &mut out);
    out
}

fn walk(node: Node<'_>, threshold: usize, out: &mut Vec<Diagnostic>) {
    if node.kind() == "switch_statement" {
        if let Some(body) = node.child_by_field_name("body") {
            let cases = count_cases(body);
            if cases < threshold {
                out.push(report_at(
                    "no-small-switch",
                    node,
                    format!(
                        "'switch' has {cases} 'case' clause(s); use an if/else chain instead of fewer than {threshold}"
                    ),
                ));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, threshold, out);
    }
}

fn count_cases(body: Node<'_>) -> usize {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| child.kind() == "switch_case")
        .count()
}
