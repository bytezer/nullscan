//! Flags a plain assignment whose two sides are textually identical, such as `x = x` or
//! `obj.value = obj.value`.
//!
//! Only `=` assignments are checked; compound assignments (`+=`, `||=`, ...) are different
//! node types and are ignored. Comparison is trimmed source text, not resolved symbols.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "assignment_expression" {
        check(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) else {
        return;
    };
    if node_text(left, source).trim() != node_text(right, source).trim() {
        return;
    }

    out.push(report_at(
        "no-self-assign",
        node,
        "this assignment assigns a value to itself",
    ));
}
