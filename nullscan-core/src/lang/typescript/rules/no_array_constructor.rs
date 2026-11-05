//! Flags `Array(...)` and `new Array(...)` calls with an ambiguous argument list.
//!
//! A single numeric argument (`new Array(500)`) intentionally creates a sparse array and
//! is allowed, as is `Array.from(...)` and any other member call. Zero arguments or more
//! than one argument are reported, as is one non-numeric argument (the classic
//! `new Array("x")` mistake).

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    match node.kind() {
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                if is_array_identifier(function, source) {
                    if let Some(arguments) = node.child_by_field_name("arguments") {
                        check_arguments(node, arguments, out);
                    }
                }
            }
        }
        "new_expression" => {
            if let Some(constructor) = node.child_by_field_name("constructor") {
                if is_array_identifier(constructor, source) {
                    match node.child_by_field_name("arguments") {
                        Some(arguments) => check_arguments(node, arguments, out),
                        // `new Array` without parentheses is a zero-argument call.
                        None => report(node, out),
                    }
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

fn is_array_identifier(node: Node<'_>, source: &str) -> bool {
    node.kind() == "identifier" && node_text(node, source) == "Array"
}

fn check_arguments(call: Node<'_>, arguments: Node<'_>, out: &mut Vec<Diagnostic>) {
    let count = arguments.named_child_count();
    let single_number = count == 1
        && arguments
            .named_child(0)
            .is_some_and(|argument| argument.kind() == "number");
    if !single_number {
        report(call, out);
    }
}

fn report(call: Node<'_>, out: &mut Vec<Diagnostic>) {
    out.push(report_at(
        "no-array-constructor",
        call,
        "use an array literal instead of the 'Array' constructor",
    ));
}
