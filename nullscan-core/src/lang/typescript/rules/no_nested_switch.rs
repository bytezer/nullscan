//! Flags a `switch` statement nested inside another `switch` statement.
//!
//! A function boundary between the two switches makes the nesting acceptable (the inner
//! switch is then easier to reason about), so function-like ancestors stop the search.
//! The inner switch is reported once; the outer switch itself is not reported.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "generator_function",
    "generator_function_declaration",
    "arrow_function",
    "method_definition",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "switch_statement" && is_nested(node) {
        out.push(report_at(
            "no-nested-switch",
            node,
            "move this 'switch' into its own function or flatten the nesting",
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn is_nested(node: Node<'_>) -> bool {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if FUNCTION_KINDS.contains(&current.kind()) {
            return false;
        }
        if current.kind() == "switch_statement" {
            return true;
        }
        ancestor = current.parent();
    }
    false
}
