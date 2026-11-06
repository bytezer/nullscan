//! Flags conditionals where every branch has the same implementation.
//!
//! Covers `if`/`else if`/`else` chains and ternary expressions. All-empty branch bodies
//! are not reported: they are not interesting duplication and are usually a different
//! problem. This rule reports once per conditional; `no-duplicated-branches` suppresses
//! itself for the same shape.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::branches::{if_chain_branches, ternary_branches, trimmed_text};
use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    match node.kind() {
        "if_statement" if node.parent().is_none_or(|parent| parent.kind() != "else_clause") => {
            check(node, &if_chain_branches(node), source, out);
        }
        "ternary_expression" => {
            check(node, &ternary_branches(node), source, out);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check(conditional: Node<'_>, branches: &[Node<'_>], source: &str, out: &mut Vec<Diagnostic>) {
    if branches.len() < 2 {
        return;
    }

    let first = trimmed_text(branches[0], source);
    if first.is_empty() {
        return;
    }
    if branches
        .iter()
        .all(|branch| trimmed_text(*branch, source) == first)
    {
        out.push(report_at(
            "no-all-duplicated-branches",
            conditional,
            "all branches do the same thing; keep one branch and remove the condition",
        ));
    }
}
