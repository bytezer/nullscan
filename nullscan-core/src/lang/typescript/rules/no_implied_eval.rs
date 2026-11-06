//! Flags `setTimeout`/`setInterval`/`setImmediate`/`execScript` calls whose first
//! argument is a string, because that string is evaluated as code.
//!
//! Syntax-only and conservative: only a string literal or a substitution-free template
//! string counts. A function, arrow function, identifier, dynamic template, or any other
//! expression is left alone. The bare names and the `window.`/`global.`/`globalThis.`
//! member forms are recognized; aliases and arbitrary objects are not.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{named_children_without_comments, node_text, report_at, static_string_text};

const IMPLIED_EVAL_FUNCTIONS: &[&str] = &["setTimeout", "setInterval", "setImmediate", "execScript"];
const GLOBAL_OBJECTS: &[&str] = &["window", "global", "globalThis"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "call_expression" {
        check_call(node, source, out);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_call(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let Some(name) = implied_eval_name(function, source) else {
        return;
    };
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let Some(first) = named_children_without_comments(arguments).into_iter().next() else {
        return;
    };
    if static_string_text(first, source).is_none() {
        return;
    }

    out.push(report_at(
        "no-implied-eval",
        node,
        format!("pass a function to '{name}' instead of a string that is evaluated as code"),
    ));
}

fn implied_eval_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    match node.kind() {
        "identifier" => {
            let name = node_text(node, source).trim();
            IMPLIED_EVAL_FUNCTIONS.contains(&name).then_some(name)
        }
        "member_expression" => {
            let object = node.child_by_field_name("object")?;
            let property = node.child_by_field_name("property")?;
            if object.kind() != "identifier" || property.kind() != "property_identifier" {
                return None;
            }
            if !GLOBAL_OBJECTS.contains(&node_text(object, source).trim()) {
                return None;
            }
            let name = node_text(property, source).trim();
            IMPLIED_EVAL_FUNCTIONS.contains(&name).then_some(name)
        }
        _ => None,
    }
}
