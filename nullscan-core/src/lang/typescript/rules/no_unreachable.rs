//! Flags statements that can never execute because an earlier statement in the same
//! statement list always transfers control.
//!
//! Conservative subset: termination is only recognised for `return`, `throw`, `break`,
//! `continue`, a block whose last executing statement terminates, and an `if` whose
//! consequence and alternative both terminate. Loops and `switch` statements are never
//! considered terminating, so code after `while (true) { ... }` is not reported.
//!
//! A following hoisted `function_declaration` is not reported; the region stays
//! unreachable, and one diagnostic is emitted for the first real unreachable statement.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::{is_hoisted_declaration, statement_terminates, statements};
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_list(ctx.tree.root_node(), &mut out);
    out
}

fn walk_list(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let mut terminated = false;
    let mut reported = false;

    for statement in statements(node) {
        if terminated {
            if is_hoisted_declaration(statement) {
                // Hoisted declarations are reachable even after a terminating statement,
                // so still inspect their bodies for nested unreachable regions.
                walk(statement, out);
                continue;
            }
            if !reported {
                out.push(report_at(
                    "no-unreachable",
                    statement,
                    "unreachable code after a statement that always transfers control",
                ));
                reported = true;
            }
            // The rest of this list is in the same unreachable region; report it once.
            continue;
        }

        walk(statement, out);
        if statement_terminates(statement) {
            terminated = true;
        }
    }
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if matches!(node.kind(), "program" | "statement_block") {
        walk_list(node, out);
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}
