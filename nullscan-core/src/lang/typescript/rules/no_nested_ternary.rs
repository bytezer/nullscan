//! Flags a ternary expression nested inside another ternary expression.
//!
//! A function boundary between the two ternaries makes the nesting readable again, so
//! function-like ancestors stop the search. Only the inner ternary is reported; the
//! outer expression is not.
//!
//! This catches chains in any field of the outer ternary: `a ? b : c ? d : e`,
//! `a ? b ? c : d : e`, and `(a ? b : c) ? d : e`.

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
    if node.kind() == "ternary_expression" && is_nested(node) {
        out.push(report_at(
            "no-nested-ternary",
            node,
            "extract this nested ternary into an independent statement",
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
        if current.kind() == "ternary_expression" {
            return true;
        }
        ancestor = current.parent();
    }
    false
}
