//! Flags direct calls to `eval(...)` and its common global/member spellings.
//!
//! Syntax-only: a call expression is reported when its callee is the bare identifier
//! `eval`, or a `.eval` member on `window`, `global`, `globalThis`, `self`, or `this`.
//! References that are not calls (`const f = eval`), arbitrary objects (`obj.eval(...)`),
//! computed access (`window["eval"]`), and aliases are deliberately not resolved.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const GLOBAL_OBJECTS: &[&str] = &["window", "global", "globalThis", "self"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "call_expression" {
        if let Some(function) = node.child_by_field_name("function") {
            if is_eval_callee(function, source) {
                out.push(report_at(
                    "no-eval",
                    node,
                    "'eval' executes arbitrary code and should not be used",
                ));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn is_eval_callee(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "identifier" => node_text(node, source).trim() == "eval",
        "member_expression" => {
            let (Some(object), Some(property)) = (
                node.child_by_field_name("object"),
                node.child_by_field_name("property"),
            ) else {
                return false;
            };
            if node_text(property, source).trim() != "eval" {
                return false;
            }
            let object = node_text(object, source).trim();
            object == "this" || GLOBAL_OBJECTS.contains(&object)
        }
        _ => false,
    }
}
