//! Flags an `if` that starts on the same line as the immediately preceding statement.
//!
//! Only statement-list positions are checked (`program`, `statement_block`, `switch_case`,
//! `switch_default`, `labeled_statement`). An `else if` is never flagged: it belongs to
//! its parent `if`, not to a preceding statement.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{named_children_without_comments, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement"
        && node.parent().is_none_or(|parent| parent.kind() != "else_clause")
    {
        if let Some(previous) = previous_statement(node) {
            if previous.end_position().row == node.start_position().row {
                out.push(report_at(
                    "no-same-line-conditional",
                    node,
                    "conditional starts on the same line as the previous statement",
                ));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn previous_statement(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    let siblings = statement_siblings(parent);
    let position = siblings
        .iter()
        .position(|sibling| sibling.start_byte() == node.start_byte())?;
    if position == 0 {
        None
    } else {
        siblings.get(position - 1).copied()
    }
}

fn statement_siblings(parent: Node<'_>) -> Vec<Node<'_>> {
    match parent.kind() {
        "program" | "statement_block" | "switch_default" => {
            named_children_without_comments(parent)
        }
        "switch_case" => {
            let value = parent.child_by_field_name("value");
            named_children_without_comments(parent)
                .into_iter()
                .filter(|child| {
                    value.is_none_or(|value| {
                        child.start_byte() != value.start_byte()
                            || child.end_byte() != value.end_byte()
                    })
                })
                .collect()
        }
        "labeled_statement" => parent
            .child_by_field_name("body")
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}
