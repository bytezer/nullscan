//! Shared helpers for the two duplicated-branch rules.
//!
//! Both rules compare branch implementations as trimmed source text. They intentionally
//! do not attempt statement-level normalization: two bodies that are semantically equal
//! but formatted differently are treated as different.

use tree_sitter::Node;

use super::named_children_without_comments;

/// Implementation node of each branch in an `if`/`else if`/`else` chain, starting at the
/// chain's first `if_statement`. An `else if` contributes its own consequence; only the
/// final `else` contributes a non-`if` branch.
pub(crate) fn if_chain_branches<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut branches = Vec::new();
    let mut current = Some(node);

    while let Some(current_node) = current {
        if let Some(consequence) = current_node.child_by_field_name("consequence") {
            branches.push(consequence);
        }

        current = current_node
            .child_by_field_name("alternative")
            .and_then(|alternative| alternative.named_child(0))
            .and_then(|inner| {
                if inner.kind() == "if_statement" {
                    Some(inner)
                } else {
                    branches.push(inner);
                    None
                }
            });
    }

    branches
}

/// The two branches of a ternary expression.
pub(crate) fn ternary_branches<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    ["consequence", "alternative"]
        .iter()
        .filter_map(|field| node.child_by_field_name(field))
        .collect()
}

/// Branch source text with surrounding whitespace removed. Empty blocks (`{}`) and empty
/// statements (`;`) normalize to `""` so the branch rules can ignore them consistently.
pub(crate) fn trimmed_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    if node.kind() == "empty_statement"
        || (node.kind() == "statement_block"
            && named_children_without_comments(node).is_empty())
    {
        return "";
    }
    node.utf8_text(source.as_bytes()).unwrap_or("").trim()
}
