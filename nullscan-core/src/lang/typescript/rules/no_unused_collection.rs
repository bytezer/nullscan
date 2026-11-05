//! Flags a local collection that is only mutated, never read.
//!
//! Requires the scope graph. A binding is reported when it is a local `let`/`const`/`var`
//! initialized to an array literal, an object literal, `new Map(...)`, or `new Set(...)`,
//! and every resolved reference is either:
//!
//! - a standalone mutation method call such as `a.push(x)` or `map.set(k, v)`, or
//! - a direct index/property assignment such as `a[0] = x` or `o.k = x`.
//!
//! Any other reference (argument, return, `.length`, a used method result, a plain read,
//! a reassignment) is a read and suppresses the diagnostic. Module-level bindings are
//! skipped. This is not type-aware: an object literal with a method named `push` is
//! treated like a collection for the purposes of the mutation allowlist.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::ScopeKind;
use crate::rule::FileContext;

use super::{node_text, report_at};

const MUTATION_METHODS: &[&str] = &[
    "push",
    "unshift",
    "pop",
    "shift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
    "add",
    "set",
    "delete",
    "clear",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut identifiers = HashMap::new();
    collect_identifiers(ctx.tree.root_node(), &mut identifiers);

    let mut declarators = HashMap::new();
    collect_declarators(ctx.tree.root_node(), &mut declarators);

    let mut out = Vec::new();
    for declarator in &scopes.declarators {
        if !matches!(
            declarator.kind,
            crate::BindingKind::Var | crate::BindingKind::Let | crate::BindingKind::Const
        ) || !declarator.has_initializer
            || declarator.bindings.len() != 1
        {
            continue;
        }

        let binding_index = declarator.bindings[0];
        let binding = &scopes.bindings[binding_index];
        if scopes.scopes[binding.scope].kind == ScopeKind::Module {
            continue;
        }

        let Some(declarator_node) = declarators.get(&declarator.start_byte) else {
            continue;
        };
        let Some(name_node) = declarator_node.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        let Some(value) = declarator_node.child_by_field_name("value") else {
            continue;
        };
        if !is_collection_literal(value, ctx.source) {
            continue;
        }

        let only_mutations = scopes
            .references
            .iter()
            .filter(|reference| reference.binding == Some(binding_index))
            .all(|reference| {
                identifiers
                    .get(&reference.offset)
                    .is_some_and(|node| is_collection_mutation(*node, ctx.source))
            });
        if !only_mutations {
            continue;
        }

        out.push(report_at(
            "no-unused-collection",
            *declarator_node,
            format!(
                "collection '{}' is only mutated; its contents are never read",
                binding.name
            ),
        ));
    }

    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}

fn collect_identifiers<'tree>(node: Node<'tree>, out: &mut HashMap<usize, Node<'tree>>) {
    if node.kind() == "identifier" {
        out.insert(node.start_byte(), node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_identifiers(child, out);
    }
}

fn collect_declarators<'tree>(node: Node<'tree>, out: &mut HashMap<usize, Node<'tree>>) {
    if node.kind() == "variable_declarator" {
        out.insert(node.start_byte(), node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_declarators(child, out);
    }
}

fn is_collection_literal(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "array" | "object" => true,
        "new_expression" => node
            .child_by_field_name("constructor")
            .is_some_and(|constructor| {
                matches!(node_text(constructor, source).trim(), "Map" | "Set")
            }),
        _ => false,
    }
}

fn same_node(left: Node<'_>, right: Node<'_>) -> bool {
    left.start_byte() == right.start_byte() && left.end_byte() == right.end_byte()
}

fn is_collection_mutation(node: Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if parent.kind() == "subscript_expression" {
        let is_object = parent
            .child_by_field_name("object")
            .is_some_and(|object| same_node(object, node));
        let is_assignment_target = parent.parent().is_some_and(|grandparent| {
            grandparent.kind() == "assignment_expression"
                && grandparent
                    .child_by_field_name("left")
                    .is_some_and(|left| same_node(left, parent))
        });
        return is_object && is_assignment_target;
    }

    if parent.kind() != "member_expression" {
        return false;
    }
    let is_object = parent
        .child_by_field_name("object")
        .is_some_and(|object| same_node(object, node));
    if !is_object {
        return false;
    }

    let is_assignment_target = parent.parent().is_some_and(|grandparent| {
        grandparent.kind() == "assignment_expression"
            && grandparent
                .child_by_field_name("left")
                .is_some_and(|left| same_node(left, parent))
    });
    if is_assignment_target {
        return true;
    }

    let Some(call) = parent.parent() else {
        return false;
    };
    if call.kind() != "call_expression"
        || !call
            .child_by_field_name("function")
            .is_some_and(|function| same_node(function, parent))
    {
        return false;
    }
    let Some(method) = parent.child_by_field_name("property") else {
        return false;
    };
    if !MUTATION_METHODS.contains(&node_text(method, source)) {
        return false;
    }
    call.parent().is_some_and(|statement| {
        statement.kind() == "expression_statement"
            && statement
                .named_child(0)
                .is_some_and(|expression| same_node(expression, call))
    })
}
