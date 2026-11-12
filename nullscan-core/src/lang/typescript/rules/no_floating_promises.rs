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
//! - an async IIFE;
//! - a call whose callee has a declared function type returning a promise, found only
//!   through explicit annotations: an annotated parameter or `const x: T`, object
//!   destructuring of such a value, and `a.b` / `a?.b` member access, following `type`
//!   aliases, interfaces and relative imports (see `type_summary`); plus functions
//!   imported by name, or through `import * as ns`, that the other file exports as
//!   `async`/`Promise<...>`.
//!
//! `await`, `void`, `.catch(...)`, `.then(a, b)`, and any value that is not a bare
//! statement are never reported. Statements built from `,`, `&&`, `||` or `?:` are
//! skipped. There is no inference: generics, `ReturnType<...>`, unannotated values, class
//! instances and bare-specifier or `node_modules` modules are not followed.

use std::collections::HashMap;
use std::sync::Arc;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;
use crate::scope::ScopeGraph;
use crate::type_summary::{ModuleSummary, Typed, Types, declares_promise};

use super::{named_children_without_comments, node_text, report_at};

const MESSAGE: &str = "promises must be awaited, end with a call to .catch, end with a call to .then with a rejection handler or be explicitly marked as ignored with the `void` operator";

const PROMISE_STATICS: &[&str] = &["all", "allSettled", "race", "any", "resolve", "reject"];

struct Checker<'a> {
    root: Node<'a>,
    source: &'a str,
    /// Reference offset -> binding offset, for references that resolved in this file.
    bound: HashMap<usize, usize>,
    types: Types<'a>,
}

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };
    let checker = Checker {
        root: ctx.tree.root_node(),
        source: ctx.source,
        bound: bound_offsets(scopes),
        types: Types::new(local_summary(ctx), ctx.resolver),
    };
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &checker, &mut out);
    out
}

fn local_summary(ctx: &FileContext<'_>) -> Arc<ModuleSummary> {
    let path = ctx.file.map(|p| p.to_path_buf());
    Arc::new(ModuleSummary::from_tree(ctx.tree, ctx.source, path))
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
            "identifier" => {
                self.is_global(callee, "fetch")
                    || self.local_returns_promise(callee)
                    || self.imported_returns_promise(callee)
                    || self.is_typed_promise_fn(callee)
            }
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
            _ => {
                self.is_promise_static(object, name)
                    || (object.kind() == "this" && self.class_method_returns_promise(callee, name))
                    || self.is_namespace_promise_fn(object, name)
                    || self.is_typed_promise_fn(callee)
            }
        }
    }

    fn is_promise_static(&self, object: Node<'a>, name: &str) -> bool {
        object.kind() == "identifier"
            && PROMISE_STATICS.contains(&name)
            && self.is_global(object, "Promise")
    }

    /// The identifier's binding when it is an import of this file.
    fn import_name(&self, identifier: Node<'a>) -> Option<&'a str> {
        let name = self.binding_name(identifier)?;
        matches!(name.parent()?.kind(), "import_specifier" | "namespace_import" | "import_clause")
            .then(|| node_text(identifier, self.source))
    }

    fn imported_returns_promise(&self, identifier: Node<'a>) -> bool {
        self.import_name(identifier)
            .is_some_and(|local| self.types.imported_fn_returns_promise(local))
    }

    fn is_namespace_promise_fn(&self, object: Node<'a>, member: &str) -> bool {
        object.kind() == "identifier"
            && self
                .import_name(object)
                .is_some_and(|ns| self.types.namespace_fn_returns_promise(ns, member))
    }

    /// Whether the callee's declared type is a function returning a promise.
    fn is_typed_promise_fn(&self, callee: Node<'a>) -> bool {
        self.type_of(callee).is_some_and(|t| self.types.is_promise_fn(&t))
    }

    /// The node declaring the binding a reference resolved to.
    fn binding_name(&self, identifier: Node<'a>) -> Option<Node<'a>> {
        let offset = *self.bound.get(&identifier.start_byte())?;
        self.root.descendant_for_byte_range(offset, offset)
    }

    /// The declared type of an expression, read only from explicit annotations.
    fn type_of(&self, expr: Node<'a>) -> Option<Typed> {
        match expr.kind() {
            "identifier" => self.name_type(self.binding_name(expr)?),
            "parenthesized_expression" | "non_null_expression" => self.type_of(expr.named_child(0)?),
            "member_expression" => {
                let object = self.type_of(expr.child_by_field_name("object")?)?;
                let property = expr.child_by_field_name("property")?;
                self.types.member(&object, node_text(property, self.source))
            }
            _ => None,
        }
    }

    /// The declared type of the binding introduced by `name`.
    fn name_type(&self, name: Node<'a>) -> Option<Typed> {
        let parent = name.parent()?;
        match parent.kind() {
            "required_parameter" | "optional_parameter" | "variable_declarator" => self.declared(parent),
            "object_pattern" => self.pattern_member(parent, node_text(name, self.source)),
            "object_assignment_pattern" => {
                self.pattern_member(parent.parent()?, node_text(name, self.source))
            }
            "pair_pattern" => {
                let key = node_text(parent.child_by_field_name("key")?, self.source);
                self.pattern_member(parent.parent()?, key)
            }
            _ => None,
        }
    }

    /// An annotated parameter or declarator; an unannotated `const` takes the declared
    /// type of its initializer when that is itself an annotated reference.
    fn declared(&self, declaration: Node<'a>) -> Option<Typed> {
        if let Some(annotation) = declaration.child_by_field_name("type") {
            return Some(self.types.from_node(annotation, self.source));
        }
        self.type_of(declaration.child_by_field_name("value")?)
    }

    fn pattern_member(&self, pattern: Node<'a>, key: &str) -> Option<Typed> {
        let base = self.pattern_type(pattern)?;
        self.types.member(&base, key)
    }

    fn pattern_type(&self, pattern: Node<'a>) -> Option<Typed> {
        let parent = pattern.parent()?;
        match parent.kind() {
            "required_parameter" | "optional_parameter" | "variable_declarator" => self.declared(parent),
            "pair_pattern" => {
                let key = node_text(parent.child_by_field_name("key")?, self.source);
                self.pattern_member(parent.parent()?, key)
            }
            _ => None,
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
                declares_promise(parent, self.source)
            }
            "variable_declarator" => parent
                .child_by_field_name("value")
                .is_some_and(|v| {
                    matches!(v.kind(), "arrow_function" | "function_expression" | "function")
                        && declares_promise(v, self.source)
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
                        && declares_promise(m, self.source)
                });
            }
            current = node.parent();
        }
        false
    }
}
