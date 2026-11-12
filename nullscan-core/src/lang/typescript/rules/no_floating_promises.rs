//! Flags an expression statement that produces a promise nobody handles.
//!
//! Syntax-only: no type information. A statement is reported when its expression
//! (parentheses unwrapped) is one of:
//!
//! - `new Promise(...)`, `Promise.all|allSettled|race|any|resolve|reject(...)`, or an
//!   unshadowed global `fetch(...)`;
//! - a call to a same-file function (resolved through the scope graph) declared `async`
//!   or annotated `Promise<...>` (also inside a union), or `this.m()` on such a method
//!   of the enclosing class;
//! - a chain ending in `.then(onFulfilled)` with one argument or in `.finally(...)`,
//!   whatever the receiver;
//! - an async IIFE.
//!
//! `await`, `void`, `.catch(...)`, `.then(a, b)`, and any value that is not a bare
//! statement are never reported. Statements built from `,`, `&&`, `||` or `?:` are
//! skipped, and members or functions imported from other files are not detected.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;
use crate::scope::ScopeGraph;

use super::{named_children_without_comments, node_text, report_at};

const MESSAGE: &str = "promises must be awaited, end with a call to .catch, end with a call to .then with a rejection handler or be explicitly marked as ignored with the `void` operator";

const PROMISE_STATICS: &[&str] = &["all", "allSettled", "race", "any", "resolve", "reject"];

struct Checker<'a> {
    root: Node<'a>,
    source: &'a str,
    /// Reference offset -> binding offset, for references that resolved in this file.
    bound: HashMap<usize, usize>,
}

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };
    let checker = Checker {
        root: ctx.tree.root_node(),
        source: ctx.source,
        bound: bound_offsets(scopes),
    };
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &checker, &mut out);
    out
}

fn bound_offsets(scopes: &ScopeGraph) -> HashMap<usize, usize> {
    scopes
        .references
        .iter()
        .filter_map(|r| Some((r.offset, scopes.bindings[r.binding?].offset)))
        .collect()
}

fn walk(node: Node<'_>, checker: &Checker<'_>, out: &mut Vec<Diagnostic>) {
    if node.kind() == "expression_statement" {
        if let Some(expression) = node.named_child(0) {
            let expression = unwrap_parens(expression);
            if checker.is_promise(expression) {
                out.push(report_at("no-floating-promises", expression, MESSAGE.to_string()));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, checker, out);
    }
}

fn unwrap_parens(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "parenthesized_expression" {
        match node.named_child(0) {
            Some(inner) => node = inner,
            None => break,
        }
    }
    node
}

fn is_async(function: Node<'_>) -> bool {
    let mut cursor = function.walk();
    function.children(&mut cursor).any(|c| c.kind() == "async")
}

fn is_promise_type(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "type_annotation" | "parenthesized_type" => {
            node.named_child(0).is_some_and(|n| is_promise_type(n, source))
        }
        "union_type" => named_children_without_comments(node)
            .into_iter()
            .any(|n| is_promise_type(n, source)),
        "generic_type" => node
            .child_by_field_name("name")
            .is_some_and(|n| node_text(n, source) == "Promise"),
        _ => false,
    }
}

/// Whether a function-like node is `async` or declares a `Promise<...>` return type.
fn returns_promise(function: Node<'_>, source: &str) -> bool {
    is_async(function)
        || function
            .child_by_field_name("return_type")
            .is_some_and(|t| is_promise_type(t, source))
}

fn argument_count(call: Node<'_>) -> usize {
    call.child_by_field_name("arguments")
        .map_or(0, |a| named_children_without_comments(a).len())
}

impl<'a> Checker<'a> {
    fn is_promise(&self, expr: Node<'a>) -> bool {
        match expr.kind() {
            "new_expression" => expr
                .child_by_field_name("constructor")
                .is_some_and(|c| c.kind() == "identifier" && self.is_global(c, "Promise")),
            "call_expression" => expr
                .child_by_field_name("function")
                .is_some_and(|f| self.is_promise_callee(f, expr)),
            _ => false,
        }
    }

    fn is_global(&self, identifier: Node<'_>, name: &str) -> bool {
        node_text(identifier, self.source) == name && !self.bound.contains_key(&identifier.start_byte())
    }

    fn is_promise_callee(&self, callee: Node<'a>, call: Node<'a>) -> bool {
        match callee.kind() {
            "identifier" => self.is_global(callee, "fetch") || self.local_returns_promise(callee),
            "parenthesized_expression" => {
                let inner = unwrap_parens(callee);
                matches!(inner.kind(), "arrow_function" | "function_expression" | "function")
                    && is_async(inner)
            }
            "member_expression" => self.is_promise_member(callee, call),
            _ => false,
        }
    }

    fn is_promise_member(&self, callee: Node<'a>, call: Node<'a>) -> bool {
        let (Some(object), Some(property)) = (
            callee.child_by_field_name("object"),
            callee.child_by_field_name("property"),
        ) else {
            return false;
        };
        let name = node_text(property, self.source);
        match name {
            "finally" => true,
            "then" => argument_count(call) == 1,
            _ if object.kind() == "identifier" => {
                PROMISE_STATICS.contains(&name) && self.is_global(object, "Promise")
            }
            _ if object.kind() == "this" => self.class_method_returns_promise(callee, name),
            _ => false,
        }
    }

    fn local_returns_promise(&self, identifier: Node<'a>) -> bool {
        let Some(&offset) = self.bound.get(&identifier.start_byte()) else {
            return false;
        };
        let Some(name) = self.root.descendant_for_byte_range(offset, offset) else {
            return false;
        };
        let Some(parent) = name.parent() else {
            return false;
        };
        match parent.kind() {
            "function_declaration" | "generator_function_declaration" => {
                returns_promise(parent, self.source)
            }
            "variable_declarator" => parent
                .child_by_field_name("value")
                .is_some_and(|v| {
                    matches!(v.kind(), "arrow_function" | "function_expression" | "function")
                        && returns_promise(v, self.source)
                }),
            _ => false,
        }
    }

    fn class_method_returns_promise(&self, member: Node<'a>, name: &str) -> bool {
        let mut current = member.parent();
        while let Some(node) = current {
            if node.kind() == "class_body" {
                return named_children_without_comments(node).into_iter().any(|m| {
                    m.kind() == "method_definition"
                        && m.child_by_field_name("name")
                            .is_some_and(|n| node_text(n, self.source) == name)
                        && returns_promise(m, self.source)
                });
            }
            current = node.parent();
        }
        false
    }
}
