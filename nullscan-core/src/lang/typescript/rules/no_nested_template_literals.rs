//! Flags a template literal nested inside another template literal's substitution.
//!
//! Each nested template is reported once (at the inner template), even when three or more
//! levels are nested.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "template_string" && has_template_ancestor(node) {
        out.push(report_at(
            "no-nested-template-literals",
            node,
            "template literal is nested inside another template literal",
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn has_template_ancestor(node: Node<'_>) -> bool {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if current.kind() == "template_string" {
            return true;
        }
        ancestor = current.parent();
    }
    false
}
