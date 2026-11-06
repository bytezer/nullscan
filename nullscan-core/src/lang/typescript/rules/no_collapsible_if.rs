use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

/// Flags `if (a) { if (b) { ... } }` with no `else` on either `if` — it can be merged
/// into `if (a && b) { ... }`.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_collapsible_if(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk_for_collapsible_if(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement" && node.child_by_field_name("alternative").is_none() {
        if let Some(consequence) = node.child_by_field_name("consequence") {
            let inner = match consequence.kind() {
                "statement_block" if consequence.named_child_count() == 1 => {
                    consequence.named_child(0)
                }
                "if_statement" => Some(consequence),
                _ => None,
            };
            if let Some(inner) = inner {
                if inner.kind() == "if_statement"
                    && inner.child_by_field_name("alternative").is_none()
                {
                    out.push(report_at(
                        "no-collapsible-if",
                        node,
                        "merge this nested 'if' into the outer condition with '&&'",
                    ));
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_collapsible_if(child, source, out);
    }
}
