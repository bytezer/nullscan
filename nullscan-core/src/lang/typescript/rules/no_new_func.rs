//! Flags `new Function(...)` and `Function(...)` constructions.
//!
//! Both forms compile a string into a function at runtime, equivalent to `eval`. The bare
//! name and the `window.`/`global.`/`globalThis.` member forms are recognized. The rule is
//! syntax-only: aliases, arbitrary objects (`obj.Function(...)`), and constructor
//! references that are never called are not resolved.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const GLOBAL_OBJECTS: &[&str] = &["window", "global", "globalThis"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    match node.kind() {
        "new_expression" => {
            if let Some(constructor) = node.child_by_field_name("constructor") {
                if is_function_constructor(constructor, source) {
                    report(node, out);
                }
            }
        }
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                if is_function_constructor(function, source) {
                    report(node, out);
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn is_function_constructor(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "identifier" => node_text(node, source).trim() == "Function",
        "member_expression" => {
            let (Some(object), Some(property)) = (
                node.child_by_field_name("object"),
                node.child_by_field_name("property"),
            ) else {
                return false;
            };
            if object.kind() != "identifier" || property.kind() != "property_identifier" {
                return false;
            }
            GLOBAL_OBJECTS.contains(&node_text(object, source).trim())
                && node_text(property, source).trim() == "Function"
        }
        _ => false,
    }
}

fn report(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    out.push(report_at(
        "no-new-func",
        node,
        "do not use the 'Function' constructor; it compiles a string into code",
    ));
}
