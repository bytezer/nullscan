use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

/// Flags an `if`/`else if` whose condition text exactly repeats an earlier condition in
/// the same chain — the later branch can never run, since the earlier identical
/// condition already caught it.
///
/// ponytail: comparison is exact source text (trimmed), not semantic equivalence, so
/// `a === b` and `a===b` are treated as different conditions. Add real AST comparison
/// if that gap shows up in practice.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_if_chains(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk_for_if_chains(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement" {
        let is_chain_start = node.parent().is_none_or(|p| p.kind() != "else_clause");
        if is_chain_start {
            check_if_chain(node, source, out);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_if_chains(child, source, out);
    }
}

fn check_if_chain(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    let mut seen: Vec<&str> = Vec::new();
    let mut current = Some(node);
    while let Some(n) = current {
        if let Some(condition) = n.child_by_field_name("condition") {
            let text = condition.utf8_text(source.as_bytes()).unwrap_or("").trim();
            if seen.contains(&text) {
                out.push(report_at(
                    "no-identical-conditions",
                    condition,
                    "remove this branch: it repeats an earlier condition and can never run",
                ));
            } else {
                seen.push(text);
            }
        }
        current = n
            .child_by_field_name("alternative")
            .and_then(|alt| alt.named_child(0))
            .filter(|inner| inner.kind() == "if_statement");
    }
}
