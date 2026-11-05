//! Flags `x == NaN`, `x === Number.NaN`, and the analogous `!=`/`!==` comparisons.
//!
//! Comparison with `NaN` is always false (or always true for `!=`/`!==`); `Number.isNaN`
//! expresses the intent. Detecting the operands is text-based, so aliases of `NaN` are not
//! covered.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const COMPARISON_OPERATORS: &[&str] = &["==", "!=", "===", "!=="];

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
    let Some(operator) = node.child_by_field_name("operator") else {
        return;
    };
    if !COMPARISON_OPERATORS.contains(&node_text(operator, source)) {
        return;
    }
    let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) else {
        return;
    };
    let is_nan = |operand: Node<'_>| {
        matches!(node_text(operand, source).trim(), "NaN" | "Number.NaN")
    };
    if !is_nan(left) && !is_nan(right) {
        return;
    }

    out.push(report_at(
        "use-isnan",
        node,
        "use Number.isNaN(...) instead of comparing with NaN",
    ));
}
