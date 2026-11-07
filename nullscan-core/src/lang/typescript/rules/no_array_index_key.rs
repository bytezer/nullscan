//! Flags JSX `key` attributes that use the index parameter of a `map`/`flatMap` callback.
//!
//! The rule resolves identifiers through the scope graph, so it is not a name heuristic:
//! a reference only counts when it binds to the second parameter of a callback passed
//! directly to `.map(...)` or `.flatMap(...)`. That keeps shadowed names, a local
//! `index`, and a callback parameter named `key` or `i` all correct.
//!
//! Deliberate limitations:
//! - Only callbacks written inline as an arrow function or function expression are
//!   recognized; `items.map(renderItem)` cannot be traced through the callback body.
//! - The first argument of the call must be the callback; `.map(fn, thisArg)` still
//!   resolves to the named `fn` and is therefore ignored.
//! - A key expression that reads the index through a helper call
//!   (`key={makeKey(index)}`) is still reported, because the identifier still binds to
//!   the index parameter.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::ScopeKind;
use crate::rule::FileContext;
use crate::scope::ScopeGraph;

use super::{named_children_without_comments, node_text, report_at};

const INDEX_METHODS: &[&str] = &["map", "flatMap"];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let index_bindings = collect_index_bindings(ctx.tree.root_node(), ctx.source, scopes);
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx, scopes, &index_bindings, &mut out);
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}

/// Binding indices of every second callback parameter of a `map`/`flatMap` call.
fn collect_index_bindings(
    root: Node<'_>,
    source: &str,
    scopes: &ScopeGraph,
) -> HashSet<usize> {
    let mut bindings = HashSet::new();
    collect_map_callbacks(root, source, &mut |callback| {
        let Some(scope) = scopes.scopes.iter().find(|scope| {
            scope.kind == ScopeKind::Function && scope.start_byte == callback.start_byte()
        }) else {
            return;
        };
        let Some(parameter) = second_parameter(callback) else {
            return;
        };
        for (index, binding) in scopes.bindings.iter().enumerate() {
            if binding.scope == scope.id
                && binding.offset >= parameter.start_byte()
                && binding.offset < parameter.end_byte()
            {
                bindings.insert(index);
            }
        }
    });
    bindings
}

fn collect_map_callbacks<'tree>(
    node: Node<'tree>,
    source: &str,
    visit: &mut dyn FnMut(Node<'tree>),
) {
    if node.kind() == "call_expression"
        && let Some(callback) = map_callback(node, source)
    {
        visit(callback);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_map_callbacks(child, source, visit);
    }
}

fn map_callback<'tree>(call: Node<'tree>, source: &str) -> Option<Node<'tree>> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "member_expression" {
        return None;
    }
    let property = function.child_by_field_name("property")?;
    if !INDEX_METHODS.contains(&node_text(property, source)) {
        return None;
    }
    let arguments = call.child_by_field_name("arguments")?;
    named_children_without_comments(arguments)
        .into_iter()
        .find(|argument| is_function_like(argument.kind()))
}

fn is_function_like(kind: &str) -> bool {
    matches!(kind, "arrow_function" | "function_expression")
}

fn second_parameter(callback: Node<'_>) -> Option<Node<'_>> {
    let parameters = callback.child_by_field_name("parameters")?;
    named_children_without_comments(parameters).get(1).copied()
}

fn walk(
    node: Node<'_>,
    ctx: &FileContext<'_>,
    scopes: &ScopeGraph,
    index_bindings: &HashSet<usize>,
    out: &mut Vec<Diagnostic>,
) {
    if node.kind() == "jsx_attribute"
        && is_key_attribute(node, ctx.source)
        && let Some(expression) = key_expression(node)
        && uses_index(expression, scopes, index_bindings)
    {
        out.push(report_at(
            "no-array-index-key",
            expression,
            "do not use the array index as a key; use a stable identifier from the item",
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, ctx, scopes, index_bindings, out);
    }
}

fn is_key_attribute(attribute: Node<'_>, source: &str) -> bool {
    named_children_without_comments(attribute)
        .first()
        .is_some_and(|name| {
            name.kind() == "property_identifier" && node_text(*name, source) == "key"
        })
}

fn key_expression(attribute: Node<'_>) -> Option<Node<'_>> {
    named_children_without_comments(attribute)
        .into_iter()
        .find(|child| child.kind() == "jsx_expression")
}

fn uses_index(
    expression: Node<'_>,
    scopes: &ScopeGraph,
    index_bindings: &HashSet<usize>,
) -> bool {
    let start = expression.start_byte();
    let end = expression.end_byte();
    scopes.references.iter().any(|reference| {
        reference.offset >= start
            && reference.offset < end
            && reference
                .binding
                .is_some_and(|binding| index_bindings.contains(&binding))
    })
}
