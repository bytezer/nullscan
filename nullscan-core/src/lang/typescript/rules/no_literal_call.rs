//! Flags a call whose callee is a literal value.
//!
//! Calling a literal always throws `TypeError: ... is not a function` at runtime, so
//! every match is a real bug.
//!
//! Syntax-only and deliberately narrow: only a *directly called* literal is reported.
//! A literal stored in a variable and called later (`const f = "x"; f();`) is ignored,
//! because without type information nullscan cannot know whether `f` holds a function.
//! A member expression on a literal (`"abc".toUpperCase()`) is fine and never reported.
//! Template literals with substitutions are dynamic, so only a substitution-free
//! template counts as a literal.
//!
//! `new` expressions are not calls and are never reported, even though
//! `new "X"()` is not valid JavaScript.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at;

/// Node kinds that are literal values and therefore not callable.
///
/// `template_string` is handled separately because a template with substitutions is
/// dynamic. `true`/`false`/`null`/`undefined` are keyword nodes in the TypeScript
/// grammar, not identifiers.
const LITERAL_KINDS: &[&str] = &[
    "string",
    "number",
    "true",
    "false",
    "null",
    "undefined",
    "object",
    "array",
    "regex",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "call_expression" {
        check_call(node, out);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn check_call(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let Some(callee) = node.child_by_field_name("function") else {
        return;
    };
    if !is_literal(callee) {
        return;
    }
    out.push(report_at(
        "no-literal-call",
        node,
        "this literal is called as a function; it is not callable at runtime",
    ));
}

/// Whether the callee is a literal value.
///
/// Parentheses are unwrapped first, so `("x")()` is reported just like `"x"()`.
fn is_literal(node: Node<'_>) -> bool {
    let node = unwrap_parentheses(node);
    match node.kind() {
        "template_string" => !has_substitution(node),
        kind => LITERAL_KINDS.contains(&kind),
    }
}

/// The expression inside the parentheses of a `parenthesized_expression`, or the node
/// itself when it is not parenthesized or has no inner expression.
fn unwrap_parentheses(node: Node<'_>) -> Node<'_> {
    if node.kind() != "parenthesized_expression" {
        return node;
    }
    let mut cursor = node.walk();
    let inner = node
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment");
    match inner {
        Some(child) => unwrap_parentheses(child),
        None => node,
    }
}

/// Whether a template literal interpolates an expression, which makes its value
/// dynamic and therefore not a literal.
fn has_substitution(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "template_substitution")
}
