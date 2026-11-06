//! Flags a loop that unconditionally leaves on its first iteration.
//!
//! Conservative subset: only a top-level `break`, `return`, or `throw` in the loop body
//! counts, and only when it appears before any top-level `continue`. Exits nested inside
//! `if`/`else`, another loop, a `switch`, a block, or a function are ignored. This keeps
//! the rule from guessing about conditional early exits.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::{body_statements, is_loop};
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if is_loop(node) {
        check_loop(node, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn check_loop(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    for statement in body_statements(body) {
        match statement.kind() {
            "break_statement" | "return_statement" | "throw_statement" => {
                out.push(report_at(
                    "no-one-iteration-loop",
                    node,
                    "replace this loop with its body; it exits on the first iteration",
                ));
                return;
            }
            "continue_statement" => return,
            _ => {}
        }
    }
}
