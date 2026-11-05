//! Heuristic detection of hard-coded secrets.
//!
//! A string value is reported when it is assigned to a secret-like name and looks like a
//! real value rather than a placeholder. Checked shapes:
//!
//! - `const password = "..."` (a variable declarator with an identifier name),
//! - `{ apiKey: "..." }` (an object-literal pair with a matching key),
//! - `target.token = "..."` / `token = "..."` (an assignment target).
//!
//! Secret-like names are matched case-insensitively after removing `_` and `-`, so
//! `apiKey`, `API_KEY`, and `api-key` are equivalent. Values must be string literals or
//! substitution-free templates, at least 8 trimmed characters long, and not obvious
//! placeholders.
//!
//! This is **not** a secret scanner and deliberately has no entropy analysis, taint
//! tracking, or type information. False positives are possible for documentation,
//! fixtures, or placeholder values outside the allowlist; false negatives are expected
//! for computed values, concatenation, destructuring, class fields, computed property
//! access (`obj["password"]`), aliases, and any name outside the fixed list below.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at, static_string_text};

const SECRET_NAMES: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "token",
    "apikey",
    "accesskey",
    "privatekey",
    "clientsecret",
    "auth",
    "credential",
    "credentials",
];

const PLACEHOLDER_PREFIXES: &[&str] = &[
    "changeme",
    "change_me",
    "password",
    "secret",
    "token",
    "example",
    "test",
    "dummy",
    "placeholder",
    "your_",
    "xxx",
    "***",
];

const MIN_SECRET_LENGTH: usize = 8;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    match node.kind() {
        "variable_declarator" => check_declarator(node, source, out),
        "pair" => check_pair(node, source, out),
        "assignment_expression" => check_assignment(node, source, out),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_declarator(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let (Some(name_node), Some(value)) = (
        node.child_by_field_name("name"),
        node.child_by_field_name("value"),
    ) else {
        return;
    };
    if name_node.kind() != "identifier" {
        return;
    }
    let name = node_text(name_node, source).trim();
    if !is_secret_name(name) || !is_secret_value(value, source) {
        return;
    }
    report(name_node, name, out);
}

fn check_pair(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    // Only object literals. `pair` also appears in object patterns such as
    // `const { password: p } = config`, which is not an assignment of a literal.
    if node.parent().is_none_or(|parent| parent.kind() != "object") {
        return;
    }
    let (Some(key), Some(value)) = (
        node.child_by_field_name("key"),
        node.child_by_field_name("value"),
    ) else {
        return;
    };
    let Some(name) = key_name(key, source) else {
        return;
    };
    if !is_secret_name(name) || !is_secret_value(value, source) {
        return;
    }
    report(key, name, out);
}

fn check_assignment(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) else {
        return;
    };
    let name_node = match left.kind() {
        "identifier" => left,
        "member_expression" => {
            let Some(property) = left.child_by_field_name("property") else {
                return;
            };
            if property.kind() != "property_identifier" {
                return;
            }
            property
        }
        _ => return,
    };
    let name = node_text(name_node, source).trim();
    if !is_secret_name(name) || !is_secret_value(right, source) {
        return;
    }
    report(name_node, name, out);
}

fn key_name<'a>(key: Node<'_>, source: &'a str) -> Option<&'a str> {
    match key.kind() {
        "property_identifier" | "identifier" => Some(node_text(key, source).trim()),
        "string" => static_string_text(key, source).map(str::trim),
        _ => None,
    }
}

fn is_secret_value(value: Node<'_>, source: &str) -> bool {
    let Some(text) = static_string_text(value, source) else {
        return false;
    };
    let trimmed = text.trim();
    trimmed.len() >= MIN_SECRET_LENGTH && !looks_like_placeholder(trimmed)
}

fn is_secret_name(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|character| !matches!(character, '_' | '-'))
        .collect();
    SECRET_NAMES.contains(&normalized.to_ascii_lowercase().as_str())
}

fn looks_like_placeholder(value: &str) -> bool {
    if value.trim().is_empty() || value.contains("${") {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    PLACEHOLDER_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn report(node: Node<'_>, name: &str, out: &mut Vec<Diagnostic>) {
    out.push(report_at(
        "hardcoded-secrets",
        node,
        format!("possible hard-coded secret in '{name}'"),
    ));
}
