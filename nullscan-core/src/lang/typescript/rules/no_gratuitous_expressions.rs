//! Flags `&&`/`||` expressions inside conditions whose operands make the result constant:
//! both operands have identical text, or one is the `!` negation of the other.
//!
//! Conservative subset: only expressions used as `if`, ternary, `while`, `do`/`while`, or
//! `for` conditions are inspected. Broader always-true/always-false analysis (constant
//! folding, short-circuit reasoning, type narrowing) is deliberately out of scope.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::is_function_like;
use crate::rule::FileContext;

use super::{node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "binary_expression" {
        check_binary(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_binary(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let (Some(left), Some(right), Some(operator)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
        node.child_by_field_name("operator"),
    ) else {
        return;
    };
    let operator_text = node_text(operator, source);
    if !matches!(operator_text, "&&" | "||") || !is_in_condition(node) {
        return;
    }

    let left_text = node_text(left, source).trim();
    let right_text = node_text(right, source).trim();
    let identical = left_text == right_text;
    let negated = is_negation_of(left, right, source) || is_negation_of(right, left, source);
    if !identical && !negated {
        return;
    }

    let reason = if identical {
        "both operands are identical"
    } else {
        "one operand negates the other"
    };
    out.push(report_at(
        "no-gratuitous-expressions",
        node,
        format!("'{operator_text}' expression is constant because {reason}"),
    ));
}

/// Whether `candidate` is `!other`, comparing trimmed source text.
fn is_negation_of(candidate: Node<'_>, other: Node<'_>, source: &str) -> bool {
    if candidate.kind() != "unary_expression" {
        return false;
    }
    let Some(operator) = candidate.child_by_field_name("operator") else {
        return false;
    };
    if node_text(operator, source) != "!" {
        return false;
    }
    let Some(argument) = candidate.child_by_field_name("argument") else {
        return false;
    };
    node_text(argument, source).trim() == node_text(other, source).trim()
}

/// Whether the expression is (part of) an `if`/ternary/loop condition. The walk stops at
/// the nearest condition owner, so a binary in a ternary's consequence is not treated as
/// part of an enclosing condition.
fn is_in_condition(node: Node<'_>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if is_function_like(parent) {
            return false;
        }
        if matches!(
            parent.kind(),
            "if_statement"
                | "while_statement"
                | "do_statement"
                | "for_statement"
                | "ternary_expression"
        ) {
            return parent
                .child_by_field_name("condition")
                .is_some_and(|condition| {
                    condition.start_byte() == current.start_byte()
                        && condition.end_byte() == current.end_byte()
                });
        }
        current = parent;
    }
    false
}
