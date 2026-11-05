//! Flags an `if`/`else` whose two branches are single `return true;` / `return false;`
//! statements, which can be replaced by returning the condition directly.
//!
//! Both block bodies (`{ return true; }`) and unbraced statement bodies are supported.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement" {
        check(node, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn check(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let Some(alternative) = node.child_by_field_name("alternative") else {
        return;
    };
    let Some(alternative_branch) = alternative.named_child(0) else {
        return;
    };
    // `else if` is a separate conditional; only a direct `else` branch counts here.
    if alternative_branch.kind() == "if_statement" {
        return;
    }
    let Some(consequence) = node.child_by_field_name("consequence") else {
        return;
    };
    let (Some(consequence_value), Some(alternative_value)) = (
        single_boolean_return(consequence),
        single_boolean_return(alternative_branch),
    ) else {
        return;
    };
    if consequence_value == alternative_value {
        return;
    }

    out.push(report_at(
        "prefer-single-boolean-return",
        node,
        "return the boolean condition directly instead of an if/else that returns true/false",
    ));
}

fn single_boolean_return(node: Node<'_>) -> Option<bool> {
    let return_node = match node.kind() {
        "statement_block" => {
            if node.named_child_count() != 1 {
                return None;
            }
            node.named_child(0)?
        }
        "return_statement" => node,
        _ => return None,
    };
    if return_node.kind() != "return_statement" || return_node.named_child_count() != 1 {
        return None;
    }
    let value = return_node.named_child(0)?;
    match value.kind() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
