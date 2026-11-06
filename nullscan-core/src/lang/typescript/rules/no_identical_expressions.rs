//! Flags binary expressions whose two operands are textually identical.
//!
//! `+` and `*` are deliberately excluded: repeating an operand there can be intentional
//! (for example `x + x` for doubling). Comparison is trimmed source text, not semantic
//! equivalence, so `a === a` fires but `a === (a)` does not.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const OPERATORS: &[&str] = &[
    "==", "===", "!=", "!==", "<", "<=", ">", ">=", "&&", "||", "??", "-", "/", "%", "&", "|",
    "^", "<<", ">>", ">>>",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "binary_expression" {
        check(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let (Some(left), Some(right), Some(operator)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
        node.child_by_field_name("operator"),
    ) else {
        return;
    };

    let operator = node_text(operator, source);
    if !OPERATORS.contains(&operator) {
        return;
    }
    if node_text(left, source).trim() != node_text(right, source).trim() {
        return;
    }

    out.push(report_at(
        "no-identical-expressions",
        node,
        format!("both sides of '{operator}' are the same; fix the copied operand"),
    ));
}
