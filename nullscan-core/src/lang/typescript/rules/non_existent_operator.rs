//! Flags `=+`, `=-`, and `=!` typos where the intended operator was likely `+=`, `-=`,
//! or `!=`.
//!
//! The check is deliberately whitespace-sensitive: the unary `+`, `-`, or `!` must start
//! immediately after the assignment `=`. `a = -b` is normal code and is not reported.

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
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    if right.kind() != "unary_expression" {
        return;
    }
    let Some(operator) = right.child_by_field_name("operator") else {
        return;
    };
    let operator_text = node_text(operator, source);
    if !matches!(operator_text, "+" | "-" | "!") {
        return;
    }

    let operator_start = operator.start_byte();
    if operator_start == 0 || source.as_bytes()[operator_start - 1] != b'=' {
        return;
    }

    let suggestion = if operator_text == "!" {
        "!=".to_string()
    } else {
        format!("{operator_text}=")
    };
    out.push(report_at(
        "non-existent-operator",
        node,
        format!("'={operator_text}' looks like a typo; did you mean '{suggestion}'?"),
    ));
}
