//! Flags an `Error` object that is created and then discarded without being thrown.
//!
//! Two shapes are reported:
//!
//! 1. `new Error(...)` or `Error(...)` used as a standalone expression statement. The
//!    error is created and immediately dropped, so nothing can ever throw it.
//! 2. A variable declarator initialized with `new Error(...)` or `Error(...)` whose
//!    binding is never thrown and never used in a way that could lead to a throw.
//!
//! The second shape needs the scope graph: the declarator's binding is resolved and
//! every reference to it is classified. The binding is reported only when all of its
//! references are inert, that is, when they neither throw the error nor hand it to
//! code that might throw it later.
//!
//! Deliberate limits:
//! - Only the plain `Error` global is recognized. `AggregateError` and custom error
//!   classes such as `new MyError(...)` are out of scope, because without type
//!   information there is no way to know that they extend `Error`.
//! - An error that is returned, passed as an argument, stored in an array or object,
//!   assigned to a property, or otherwise escapes its binding is never reported: some
//!   other code may throw it later.
//! - The rule is not path-sensitive. An error thrown on one path makes the whole
//!   binding quiet, even when other paths drop it.
//! - A binding that is written to is never reported, because the original error may no
//!   longer be reachable through it.
//! - Only the outermost `new Error(...)`/`Error(...)` expression is checked. An error
//!   created inside a larger expression, such as `new Error("x").message`, is ignored.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;
use crate::scope::ScopeGraph;

use super::{named_children_without_comments, node_text, report_at};

/// The only global constructor this rule understands.
const ERROR_GLOBAL: &str = "Error";

/// The message both shapes report.
const MESSAGE: &str = "this error is never thrown; throw it or remove the creation";

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut identifiers = HashMap::new();
    collect_identifiers(ctx.tree.root_node(), &mut identifiers);

    let mut declarators = HashMap::new();
    collect_declarators(ctx.tree.root_node(), &mut declarators);

    let nodes = Nodes {
        identifiers,
        declarators,
    };

    let mut out = Vec::new();
    collect_dropped_creations(ctx, &mut out);
    collect_unthrown_bindings(ctx, scopes, &nodes, &mut out);

    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}

/// Tree nodes the rule looks up by byte offset.
///
/// The scope graph stores only offsets, so the rule keeps its own map from an offset
/// back to the node it belongs to.
struct Nodes<'tree> {
    identifiers: HashMap<usize, Node<'tree>>,
    declarators: HashMap<usize, Node<'tree>>,
}

/// Reports `new Error(...)` / `Error(...)` used as a standalone expression statement.
fn collect_dropped_creations(ctx: &FileContext<'_>, out: &mut Vec<Diagnostic>) {
    walk(ctx.tree.root_node(), &mut |node| {
        if node.kind() != "expression_statement" {
            return;
        }
        let Some(expression) = named_children_without_comments(node).first().copied() else {
            return;
        };
        if !is_error_creation(expression, ctx.source) {
            return;
        }
        out.push(report_at("no-unthrown-error", expression, MESSAGE));
    });
}

/// Reports a declarator whose `Error` value is never thrown and never escapes.
fn collect_unthrown_bindings(
    ctx: &FileContext<'_>,
    scopes: &ScopeGraph,
    nodes: &Nodes<'_>,
    out: &mut Vec<Diagnostic>,
) {
    for declarator in &scopes.declarators {
        if declarator.bindings.len() != 1 || !declarator.has_initializer {
            continue;
        }
        let binding_index = declarator.bindings[0];

        let Some(declarator_node) = nodes.declarators.get(&declarator.start_byte) else {
            continue;
        };
        let Some(value) = declarator_node.child_by_field_name("value") else {
            continue;
        };
        if !is_error_creation(value, ctx.source) {
            continue;
        }

        if !binding_is_inert(scopes, binding_index, &nodes.identifiers) {
            continue;
        }

        out.push(report_at("no-unthrown-error", value, MESSAGE));
    }
}

/// Whether every reference to `binding_index` is inert.
///
/// A reference escapes when it is thrown, returned, passed to a call, stored in a
/// collection or object, assigned to a property, or otherwise leaves the binding. Any
/// other use is inert: it cannot result in the error being thrown.
fn binding_is_inert(
    scopes: &ScopeGraph,
    binding_index: usize,
    identifiers: &HashMap<usize, Node<'_>>,
) -> bool {
    scopes
        .references
        .iter()
        .filter(|reference| reference.binding == Some(binding_index))
        .all(|reference| {
            if reference.write {
                // The binding may hold a different value later, so the original error
                // is no longer reachable through it.
                return false;
            }
            let Some(node) = identifiers.get(&reference.offset) else {
                // A reference we cannot place in the tree is treated as escaping, so the
                // binding stays quiet.
                return false;
            };
            reference_is_inert(*node)
        })
}

/// Whether a read of the error binding cannot result in the error being thrown.
fn reference_is_inert(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        // A bare `error;` statement discards the value.
        "expression_statement" => named_children_without_comments(parent)
            .first()
            .is_some_and(|child| child.id() == node.id()),
        // `error.message` reads a property, which does not hand out the error.
        "member_expression" => parent
            .child_by_field_name("object")
            .is_some_and(|object| object.id() == node.id()),
        // `typeof error`, `!error` and `void error` produce a primitive.
        "unary_expression" => parent
            .child_by_field_name("argument")
            .is_some_and(|argument| argument.id() == node.id()),
        // `error instanceof Error` and comparisons produce a primitive.
        "binary_expression" => ["left", "right"].iter().any(|field| {
            parent
                .child_by_field_name(field)
                .is_some_and(|operand| operand.id() == node.id())
        }),
        _ => false,
    }
}

/// Whether `node` creates a plain `Error` object.
fn is_error_creation(node: Node<'_>, source: &str) -> bool {
    let constructor = match node.kind() {
        "new_expression" => node.child_by_field_name("constructor"),
        "call_expression" => node.child_by_field_name("function"),
        _ => return false,
    };
    let Some(constructor) = constructor else {
        return false;
    };
    constructor.kind() == "identifier" && node_text(constructor, source).trim() == ERROR_GLOBAL
}

fn walk<'tree>(node: Node<'tree>, visit: &mut dyn FnMut(Node<'tree>)) {
    visit(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, visit);
    }
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
