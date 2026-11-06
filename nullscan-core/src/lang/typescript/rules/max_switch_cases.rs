//! Flags a `switch` with more than `Config::max_switch_cases` `case` clauses.
//!
//! `default` clauses are not counted. A large switch is often a hand-written mapping that
//! a lookup table would express more clearly.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let threshold = ctx.config.max_switch_cases;
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), threshold, &mut out);
    out
}

fn walk(node: Node<'_>, threshold: usize, out: &mut Vec<Diagnostic>) {
    if node.kind() == "switch_statement" {
        if let Some(body) = node.child_by_field_name("body") {
            let cases = count_cases(body);
            if cases > threshold {
                out.push(report_at(
                    "max-switch-cases",
                    node,
                    format!(
                        "'switch' has {cases} 'case' clauses; the maximum is {threshold}, so use a lookup table"
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
