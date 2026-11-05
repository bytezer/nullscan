//! Flags conditionals with two branches that have the same implementation.
//!
//! The all-branches-identical case is left to `no-all-duplicated-branches` to avoid two
//! diagnostics for the same mistake. Empty branch bodies are ignored; other rules cover
//! empty catch/if bodies. Branch comparison is trimmed source text.

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
            check(&if_chain_branches(node), source, out);
        }
        "ternary_expression" => {
            check(&ternary_branches(node), source, out);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check(branches: &[Node<'_>], source: &str, out: &mut Vec<Diagnostic>) {
    if branches.len() < 2 {
        return;
    }

    let texts: Vec<&str> = branches
        .iter()
        .map(|branch| trimmed_text(*branch, source))
        .collect();

    // `no-all-duplicated-branches` reports this shape; don't double-report it here.
    let first = texts[0];
    if !first.is_empty() && texts.iter().all(|text| *text == first) {
        return;
    }

    let mut duplicate = None;
    'search: for current in 1..texts.len() {
        if texts[current].is_empty() {
            continue;
        }
        for earlier in 0..current {
            if texts[earlier] == texts[current] {
                duplicate = Some(branches[current]);
                break 'search;
            }
        }
    }

    if let Some(branch) = duplicate {
        out.push(report_at(
            "no-duplicated-branches",
            branch,
            "this branch duplicates another branch's implementation",
        ));
    }
}
