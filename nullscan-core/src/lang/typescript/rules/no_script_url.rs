//! Flags string literals and substitution-free template strings that start with
//! `javascript:` (case-insensitive, ignoring surrounding whitespace).
//!
//! Such values are script URLs: navigating to them executes the embedded code. Only
//! literal text is inspected; a template with substitutions is dynamic and is skipped,
//! and comments are never string nodes so they cannot produce a diagnostic.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{report_at, static_string_text};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if matches!(node.kind(), "string" | "template_string") {
        if let Some(value) = static_string_text(node, source) {
            if value.trim().to_ascii_lowercase().starts_with("javascript:") {
                out.push(report_at(
                    "no-script-url",
                    node,
                    "do not use 'javascript:' URLs; they execute embedded code",
                ));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}
