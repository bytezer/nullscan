//! Flags constant conditions in `if` statements and ternary expressions.
//!
//! Recognized constants are literal `true`/`false`/number/string/`null`/`undefined`,
//! object and array literals, substitution-free template literals, and unary/binary
//! expressions composed entirely of those. Any identifier makes the condition non-constant.
//!
//! Loop conditions are intentionally not checked, so `while (true)` and `for (;;)` remain
//! valid patterns.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    match node.kind() {
        "if_statement" => {
            if let Some(condition) = node.child_by_field_name("condition") {
                report_if_constant(condition, source, out);
            }
        }
        "ternary_expression" => {
            if let Some(condition) = node.child_by_field_name("condition") {
                report_if_constant(condition, source, out);
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn report_if_constant(condition: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if is_constant(condition, source) {
        out.push(report_at(
            "no-constant-condition",
            condition,
            "condition is constant and always evaluates the same way",
        ));
    }
}

fn is_constant(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "true" | "false" | "number" | "string" | "null" | "undefined" | "object" | "array" => {
            true
        }
        // A template with substitutions depends on runtime values.
        "template_string" => node.named_child_count() == 0,
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|inner| is_constant(inner, source)),
        "unary_expression" => node
            .child_by_field_name("argument")
            .is_some_and(|argument| is_constant(argument, source)),
        "binary_expression" => {
            node.child_by_field_name("left")
                .is_some_and(|left| is_constant(left, source))
                && node
                    .child_by_field_name("right")
                    .is_some_and(|right| is_constant(right, source))
        }
        _ => false,
    }
}
