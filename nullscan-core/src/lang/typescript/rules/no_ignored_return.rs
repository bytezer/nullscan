//! Flags a call whose pure return value is discarded.
//!
//! Conservative subset: only an expression statement that is exactly a call to an
//! allowlisted global function or static method is reported. There is no type
//! information, so instance methods are never considered pure. `await` calls, `void`
//! expressions, and calls nested inside other expressions are also ignored.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

const PURE_GLOBALS: &[&str] = &[
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "Number",
    "String",
    "Boolean",
    "encodeURI",
    "decodeURI",
    "encodeURIComponent",
    "decodeURIComponent",
];

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "expression_statement" {
        check_statement(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_statement(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some(expression) = node.named_child(0) else {
        return;
    };
    if expression.kind() != "call_expression" {
        return;
    }
    let Some(function) = expression.child_by_field_name("function") else {
        return;
    };
    let Some(name) = pure_function_name(function, source) else {
        return;
    };

    out.push(report_at(
        "no-ignored-return",
        expression,
        format!("return value of '{name}' is ignored"),
    ));
}

fn pure_function_name(function: Node<'_>, source: &str) -> Option<String> {
    match function.kind() {
        "identifier" => {
            let name = node_text(function, source).trim();
            PURE_GLOBALS.contains(&name).then(|| name.to_string())
        }
        "member_expression" => {
            let object = function.child_by_field_name("object")?;
            let property = function.child_by_field_name("property")?;
            if object.kind() != "identifier" || property.kind() != "property_identifier" {
                return None;
            }
            let object = node_text(object, source).trim();
            let property = node_text(property, source).trim();

            let pure = match object {
                "Math" => true,
                "JSON" => matches!(property, "parse" | "stringify"),
                "Object" => matches!(
                    property,
                    "keys" | "values" | "entries" | "assign" | "create" | "freeze" | "seal"
                ),
                "Array" => matches!(property, "isArray" | "from" | "of"),
                _ => false,
            };
            pure.then(|| format!("{object}.{property}"))
        }
        _ => None,
    }
}
