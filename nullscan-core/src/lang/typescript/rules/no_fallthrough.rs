//! Flags a non-final `switch` case whose body can fall through into the next case.
//!
//! Conservative subset:
//! - Termination is `break`/`return`/`throw`/`continue`, a terminating block, or a
//!   terminating `if`/`else` (see `control_flow::statement_terminates`).
//! - An empty case body is treated as intentional grouping and never reported.
//! - The last case is never reported, and `default` is not checked as a falling case.
//! - A comment between the last statement and the next label containing
//!   `fallthrough` or `falls through` (case-insensitive) suppresses the diagnostic.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::{last_executable_statement, last_statement, statement_terminates};
use crate::rule::FileContext;

use super::{named_children_without_comments, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut out);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "switch_statement" {
        check_switch(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

fn check_switch(node: Node<'_>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let cases: Vec<Node<'_>> = named_children_without_comments(body)
        .into_iter()
        .filter(|child| matches!(child.kind(), "switch_case" | "switch_default"))
        .collect();

    for (index, case) in cases.iter().enumerate() {
        if case.kind() != "switch_case" {
            continue;
        }
        let Some(next) = cases.get(index + 1) else {
            continue;
        };
        if last_statement(*case).is_none() {
            // Empty (or comment-only) case body: intentional grouping.
            continue;
        }
        let Some(last) = last_executable_statement(*case) else {
            continue;
        };
        if statement_terminates(last) {
            continue;
        }
        if comment_marks_fallthrough(source, last.end_byte(), next.start_byte()) {
            continue;
        }

        out.push(report_at(
            "no-fallthrough",
            *case,
            "case falls through to the next case; add 'break' or mark the fallthrough in a comment",
        ));
    }
}

fn comment_marks_fallthrough(source: &str, start: usize, end: usize) -> bool {
    let Some(between) = source.get(start..end) else {
        return false;
    };
    let lowered = between.to_ascii_lowercase();
    lowered.contains("fallthrough") || lowered.contains("falls through")
}
