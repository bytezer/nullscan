//! Flags a generator function whose body never yields.
//!
//! A generator that never runs a `yield` expression finishes on the first `next()` call:
//! the iterator is already done and `next()` returns `{ value: undefined, done: true }`.
//! That is almost always a mistake, because the caller expects the generator to produce a
//! sequence.
//!
//! What counts as a generator: a `function*` declaration, a `function*` expression, and a
//! class or object method declared with `*` (including `async function*` and
//! `async *method()`).
//!
//! Deliberate limitations:
//!
//! - Only **direct** `yield` expressions count. A `yield` inside a nested function or a
//!   nested class body belongs to that inner scope, so it does not silence the report on
//!   the outer generator.
//! - `yield*` (a delegating yield) counts as yielding.
//! - A generator with an empty body is reported: it never yields either.
//! - The rule is syntax-only. It cannot tell whether a generator is intentionally empty,
//!   for example when it is only used to satisfy an interface or a mock.
//! - Type-level generator declarations (`declare function*`, `interface` methods) have no
//!   body and are never reported.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::is_function_like;
use crate::rule::FileContext;

use super::report_at;

const RULE_ID: &str = "no-generator-without-yield";

/// Node kinds that are always generators.
const GENERATOR_KINDS: &[&str] = &["generator_function_declaration", "generator_function"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if is_generator(node) {
        check_generator(node, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

/// Whether `node` declares a generator function.
///
/// A class or object generator method is a plain `method_definition` carrying a `*`
/// token; the tree-sitter TypeScript grammar has no separate node kind for it.
fn is_generator(node: Node<'_>) -> bool {
    if GENERATOR_KINDS.contains(&node.kind()) {
        return true;
    }
    node.kind() == "method_definition" && has_star_token(node)
}

fn has_star_token(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| child.kind() == "*")
}

fn check_generator(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "statement_block" {
        return;
    }
    if contains_yield(body) {
        return;
    }
    out.push(report_at(
        RULE_ID,
        node,
        "this generator never yields; it returns undefined on the first next()",
    ));
}

/// Whether `node` or one of its descendants is a `yield` expression that belongs to the
/// current scope.
///
/// The walk stops at nested function-like nodes and at class boundaries: a `yield` there
/// belongs to that inner scope, not to the generator being checked.
fn contains_yield(node: Node<'_>) -> bool {
    if node.kind() == "yield_expression" {
        return true;
    }
    if is_function_like(node) || is_class_like(node) {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_yield(child) {
            return true;
        }
    }
    false
}

/// Node kinds that open a class scope, whose bodies are separate from the enclosing one.
fn is_class_like(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "class_declaration" | "class_expression" | "class" | "abstract_class_declaration"
    )
}
