//! Flags `new String(...)`, `new Number(...)`, and `new Boolean(...)`.
//!
//! Wrapper objects are objects, not primitives: `new Boolean(false)` is truthy and
//! `typeof new String("x")` is `"object"`. Calling the same names without `new` performs
//! the intended conversion and is not reported. Only the bare identifiers are covered;
//! aliased or member constructors are left alone.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const WRAPPERS: &[&str] = &["String", "Number", "Boolean"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "new_expression" {
        if let Some(constructor) = node.child_by_field_name("constructor") {
            if constructor.kind() == "identifier" {
                let name = node_text(constructor, source);
                if WRAPPERS.contains(&name) {
                    out.push(report_at(
                        "no-new-wrappers",
                        node,
                        format!(
                            "do not use 'new {name}(...)'; use the primitive value or a call without 'new'"
                        ),
                    ));
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}
