//! Flags a `catch` clause whose only action is rethrowing the caught value.
//!
//! Conservative subset: the body must contain exactly one statement, a `throw` of the
//! catch parameter itself. A catch that logs, transforms, or throws anything else is not
//! reported. Comments are not named statements, so `catch (e) { /* ... */ throw e; }`
//! still matches.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{named_children_without_comments, node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "catch_clause" {
        check(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some(parameter) = node.child_by_field_name("parameter") else {
        return;
    };
    if parameter.kind() != "identifier" {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let statements = named_children_without_comments(body);
    if statements.len() != 1 {
        return;
    }
    let statement = statements[0];
    if statement.kind() != "throw_statement" {
        return;
    }
    let Some(thrown) = statement.named_child(0) else {
        return;
    };
    if thrown.kind() != "identifier" {
        return;
    }
    if node_text(thrown, source).trim() != node_text(parameter, source).trim() {
        return;
    }

    out.push(report_at(
        "no-useless-catch",
        node,
        "catch block only rethrows the caught error; remove the try/catch",
    ));
}
