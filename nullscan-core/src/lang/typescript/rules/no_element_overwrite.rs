//! Flags a second adjacent assignment to the same textual target whose value is never
//! read in between.
//!
//! Conservative subset: only two consecutive `assignment_expression` statements in one
//! statement list are compared (`a[0] = 1; a[0] = 2;`). There is deliberately no general
//! dataflow: intervening statements, aliases, and assignments in different branches are
//! not considered.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::statements;
use crate::rule::FileContext;

use super::{node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if matches!(
        node.kind(),
        "program" | "statement_block" | "switch_case" | "switch_default"
    ) {
        check_list(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_list(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    for pair in statements(node).windows(2) {
        let (Some(first), Some(second)) = (assignment(pair[0]), assignment(pair[1])) else {
            continue;
        };
        let (Some(first_target), Some(second_target)) = (
            first.child_by_field_name("left"),
            second.child_by_field_name("left"),
        ) else {
            continue;
        };
        if node_text(first_target, source).trim() == node_text(second_target, source).trim() {
            out.push(report_at(
                "no-element-overwrite",
                second,
                "the previous assignment to the same target is overwritten without being read",
            ));
        }
    }
}

fn assignment(statement: Node<'_>) -> Option<Node<'_>> {
    if statement.kind() != "expression_statement" {
        return None;
    }
    let expression = statement.named_child(0)?;
    (expression.kind() == "assignment_expression").then_some(expression)
}
