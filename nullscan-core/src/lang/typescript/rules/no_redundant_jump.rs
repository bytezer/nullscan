//! Flags jumps that don't change control flow:
//!
//! - a bare `return;` as the last statement of a function body;
//! - a bare `continue;` as the last statement of a loop body.
//!
//! `break` is deliberately not flagged (it is required in many `switch` cases and is not
//! always redundant in loops), and valued returns are always kept. Only jumps directly in
//! the function/loop body are checked; a `return;` at the end of a nested `if` is not
//! reported even when the enclosing function ends there too.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{named_children_without_comments, report_at};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "generator_function",
    "generator_function_declaration",
    "arrow_function",
    "method_definition",
];

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if FUNCTION_KINDS.contains(&node.kind()) {
        if let Some(body) = node.child_by_field_name("body") {
            if body.kind() == "statement_block" {
                if let Some(last) = last_named_child(body) {
                    if last.kind() == "return_statement" && last.named_child_count() == 0 {
                        out.push(report_at(
                            "no-redundant-jump",
                            last,
                            "remove this redundant 'return;'; the function ends here anyway",
                        ));
                    }
                }
            }
        }
    }

    if LOOP_KINDS.contains(&node.kind()) {
        if let Some(body) = node.child_by_field_name("body") {
            let last = if body.kind() == "statement_block" {
                last_named_child(body)
            } else {
                Some(body)
            };
            if let Some(last) = last {
                if last.kind() == "continue_statement"
                    && last.child_by_field_name("label").is_none()
                    && last.named_child_count() == 0
                {
                    out.push(report_at(
                        "no-redundant-jump",
                        last,
                        "remove this redundant 'continue;'; the loop ends here anyway",
                    ));
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn last_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children_without_comments(node).pop()
}
