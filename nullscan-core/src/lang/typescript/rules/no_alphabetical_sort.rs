//! Flags `array.sort()` and `array.toSorted()` calls without a compare function.
//!
//! The default comparison stringifies every element, so `[10, 9].sort()` yields
//! `[10, 9]` and accented text can land in an unexpected order. Passing `undefined`
//! explicitly is the same as passing nothing and is treated as the default comparison.
//! A real comparator (including `(a, b) => ...`) is accepted.
//!
//! The rule is not type-aware: any `.sort()`/`.toSorted()` method call without arguments
//! is reported, even on a custom object that implements its own `sort`.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, named_children_without_comments, report_at};

const SORT_METHODS: &[&str] = &["sort", "toSorted"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "call_expression" && lacks_comparator(node, source) {
        out.push(report_at(
            "no-alphabetical-sort",
            node,
            "provide a compare function; the default sort compares elements as strings",
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn lacks_comparator(call: Node<'_>, source: &str) -> bool {
    if !is_sort_method(call, source) {
        return false;
    }
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    match named_children_without_comments(arguments).as_slice() {
        [] => true,
        [only] => node_text(*only, source) == "undefined",
        _ => false,
    }
}

fn is_sort_method(call: Node<'_>, source: &str) -> bool {
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "member_expression" {
        return false;
    }
    function
        .child_by_field_name("property")
        .is_some_and(|property| SORT_METHODS.contains(&node_text(property, source)))
}
