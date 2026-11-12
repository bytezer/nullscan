//! Flags an increment or decrement whose result is never used, so the statement has no
//! effect: `count++`, `--count`, `count += 1` and `count -= 1`.
//!
//! Only increments that are the whole expression of an expression statement are reported.
//! An increment used as a value (`const next = count++`), nested inside another expression,
//! or used as the update clause of a `for` loop is not reported. Compound assignments
//! other than `+=` and `-=` are out of scope.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const RULE: &str = "no-useless-increment";
const MESSAGE: &str = "this increment has no effect; remove it or use its value";

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "expression_statement" {
        check_statement(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

/// Reports the statement when its only expression is a discarded increment.
fn check_statement(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    let expressions: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect();
    let [expression] = expressions.as_slice() else {
        return;
    };

    if !is_useless_increment(*expression, source) {
        return;
    }
    out.push(report_at(RULE, *expression, MESSAGE));
}

/// Whether the expression is an increment whose value is discarded.
fn is_useless_increment(expression: Node<'_>, source: &str) -> bool {
    match expression.kind() {
        "update_expression" => true,
        "augmented_assignment_expression" => is_plus_or_minus_assign(expression, source),
        _ => false,
    }
}

/// Whether the augmented assignment uses exactly the `+=` or `-=` operator.
fn is_plus_or_minus_assign(expression: Node<'_>, source: &str) -> bool {
    let Some(operator) = expression.child_by_field_name("operator") else {
        return false;
    };
    matches!(node_text(operator, source), "+=" | "-=")
}
